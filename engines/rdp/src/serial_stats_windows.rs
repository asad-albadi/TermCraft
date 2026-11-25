use super::{control, stats};
use std::{
    fs::File,
    io,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};
use windows_sys::Win32::{
    Foundation::{ERROR_IO_INCOMPLETE, ERROR_IO_PENDING, HANDLE},
    System::{
        IO::{CancelIoEx, DeviceIoControl, GetOverlappedResult, OVERLAPPED},
        Threading::CreateEventW,
    },
};

fn terminal(handle: HANDLE, operation: *const OVERLAPPED) -> Option<Result<u32, ()>> {
    let mut count = 0;
    if unsafe { GetOverlappedResult(handle, operation, &mut count, 0) } != 0 {
        Some(Ok(count))
    } else if io::Error::last_os_error().raw_os_error() == Some(ERROR_IO_INCOMPLETE as i32) {
        None
    } else {
        Some(Err(()))
    }
}

// Declared last on the dedicated owner's stack so unwinding cannot drop the
// event, output or OVERLAPPED before the kernel relinquishes the request.
struct Retirement {
    handle: HANDLE,
    op: *const OVERLAPPED,
    pending: bool,
}
impl Drop for Retirement {
    fn drop(&mut self) {
        if self.pending {
            unsafe {
                CancelIoEx(self.handle, self.op);
            }
            while terminal(self.handle, self.op).is_none() {
                std::thread::sleep(stats::POLL);
            }
        }
    }
}

// The issuing thread owns the file, event and stable stack storage until terminal
// completion, even if the session drops its receiver or the deadline expires.
pub(super) fn baseline(
    file: &File,
    cancel: &AtomicBool,
    deadline: Instant,
) -> Option<stats::Origin> {
    let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
    if event.is_null() {
        return None;
    }
    let event = unsafe { OwnedHandle::from_raw_handle(event) };
    let mut operation = OVERLAPPED {
        hEvent: event.as_raw_handle(),
        ..Default::default()
    };
    let mut output = [0; 24];
    let handle = file.as_raw_handle();
    let op = &raw mut operation;
    let mut retirement = Retirement {
        handle,
        op,
        pending: false,
    };
    let ok = unsafe {
        DeviceIoControl(
            handle,
            control::GET_STATS,
            std::ptr::null(),
            0,
            output.as_mut_ptr().cast(),
            24,
            std::ptr::null_mut(),
            op,
        )
    };
    if ok == 0 && io::Error::last_os_error().raw_os_error() != Some(ERROR_IO_PENDING as i32) {
        return None;
    }
    retirement.pending = true;
    let valid = stats::retire(
        || cancel.load(Ordering::Acquire) || Instant::now() >= deadline,
        || unsafe {
            CancelIoEx(handle, op);
        },
        || terminal(handle, op),
        || std::thread::sleep(stats::POLL),
    );
    retirement.pending = false;
    valid.then_some(output)
}
