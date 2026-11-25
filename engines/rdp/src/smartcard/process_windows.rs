//! Suspended launch and explicit handle allowlist: no driver runs before Job ownership.
use std::{
    fs::File,
    io,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    Security::SECURITY_ATTRIBUTES,
    Storage::FileSystem::*,
    System::{
        JobObjects::*, Pipes::CreatePipe, SystemInformation::GetWindowsDirectoryW, Threading::*,
    },
};
#[path = "process_command.rs"]
mod command;
pub(super) struct Process {
    process: OwnedHandle,
    job: OwnedHandle,
    stopped: bool,
}
fn pipe() -> io::Result<(OwnedHandle, OwnedHandle)> {
    let security = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: null_mut(),
        bInheritHandle: 1,
    };
    let (mut read, mut write) = (null_mut(), null_mut());
    if unsafe { CreatePipe(&mut read, &mut write, &security, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe {
        (
            OwnedHandle::from_raw_handle(read),
            OwnedHandle::from_raw_handle(write),
        )
    })
}
fn no_inherit(handle: &OwnedHandle) -> io::Result<()> {
    if unsafe { SetHandleInformation(handle.as_raw_handle(), HANDLE_FLAG_INHERIT, 0) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
struct Attributes {
    bytes: Vec<usize>,
    initialized: bool,
}
impl Attributes {
    fn new(handles: &mut [HANDLE], job: &mut HANDLE) -> io::Result<Self> {
        let mut size = 0;
        unsafe {
            InitializeProcThreadAttributeList(null_mut(), 2, 0, &mut size);
        }
        if size == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut value = Self {
            bytes: vec![0; size.div_ceil(size_of::<usize>())],
            initialized: false,
        };
        if unsafe { InitializeProcThreadAttributeList(value.ptr(), 2, 0, &mut size) } == 0 {
            return Err(io::Error::last_os_error());
        }
        value.initialized = true;
        if unsafe {
            UpdateProcThreadAttribute(
                value.ptr(),
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                handles.as_mut_ptr().cast(),
                size_of_val(handles),
                null_mut(),
                null(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        if unsafe {
            UpdateProcThreadAttribute(
                value.ptr(),
                0,
                PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
                (job as *mut HANDLE).cast(),
                size_of::<HANDLE>(),
                null_mut(),
                null(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(value)
    }
    fn ptr(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.bytes.as_mut_ptr().cast()
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        if self.initialized {
            unsafe {
                DeleteProcThreadAttributeList(self.ptr());
            }
        }
    }
}
impl Process {
    pub(super) fn spawn() -> io::Result<(Self, File, File)> {
        let executable: Vec<u16> = std::env::current_exe()?
            .as_os_str()
            .encode_wide()
            .chain([0])
            .collect();
        if executable[..executable.len() - 1].contains(&0) {
            return Err(io::Error::other("NUL in PC/SC helper path"));
        }
        // Windows file paths cannot contain double quotes. Fixed argument, no shell.
        let mut command = command::command_line(&executable[..executable.len() - 1])
            .map_err(|_| io::Error::other("Invalid helper path"))?;
        let (parent_read, child_write) = pipe()?;
        let (child_read, parent_write) = pipe()?;
        no_inherit(&parent_read)?;
        no_inherit(&parent_write)?;
        let mut handles = [child_read.as_raw_handle(), child_write.as_raw_handle()];

        // A clean environment; the system directory comes from the OS API.
        let mut directory = vec![0u16; 32768];
        let length = unsafe { GetWindowsDirectoryW(directory.as_mut_ptr(), directory.len() as u32) }
            as usize;
        if length == 0 || length >= directory.len() {
            return Err(io::Error::last_os_error());
        }
        let environment: Vec<u16> = "SystemRoot="
            .encode_utf16()
            .chain(directory[..length].iter().copied())
            .chain([0, 0])
            .collect();
        unsafe {
            let raw_job = CreateJobObjectW(null(), null());
            if raw_job.is_null() {
                return Err(io::Error::last_os_error());
            }
            let job = OwnedHandle::from_raw_handle(raw_job);
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                raw_job,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of_val(&limits) as u32,
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            // Job membership is an atomic CreateProcess attribute (Windows 10+).
            // Even parent death between creation and resume cannot orphan this worker.
            let mut job_raw = raw_job;
            let mut attributes = Attributes::new(&mut handles, &mut job_raw)?;
            let startup = STARTUPINFOEXW {
                StartupInfo: STARTUPINFOW {
                    cb: size_of::<STARTUPINFOEXW>() as u32,
                    dwFlags: STARTF_USESTDHANDLES,
                    hStdInput: child_read.as_raw_handle(),
                    hStdOutput: child_write.as_raw_handle(),
                    hStdError: child_write.as_raw_handle(),
                    ..Default::default()
                },
                lpAttributeList: attributes.ptr(),
            };
            let mut info = PROCESS_INFORMATION::default();
            if CreateProcessW(
                executable.as_ptr(),
                command.as_mut_ptr(),
                null(),
                null(),
                1,
                CREATE_SUSPENDED
                    | CREATE_NO_WINDOW
                    | CREATE_UNICODE_ENVIRONMENT
                    | EXTENDED_STARTUPINFO_PRESENT,
                environment.as_ptr().cast(),
                null(),
                &startup.StartupInfo,
                &mut info,
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            let process = OwnedHandle::from_raw_handle(info.hProcess);
            let thread = OwnedHandle::from_raw_handle(info.hThread);
            let mut owned = Self {
                process,
                job,
                stopped: false,
            };
            if ResumeThread(thread.as_raw_handle()) == u32::MAX {
                let error = io::Error::last_os_error();
                owned.stop();
                return Err(error);
            }
            drop(child_read);
            drop(child_write);
            Ok((owned, File::from(parent_read), File::from(parent_write)))
        }
    }
    pub(super) fn stop(&mut self) {
        if self.stopped {
            return;
        }
        // Native handles anchor exact ownership; no PID lookup or global service kill.
        unsafe {
            TerminateJobObject(self.job.as_raw_handle(), 1);
            WaitForSingleObject(self.process.as_raw_handle(), INFINITE);
        }
        self.stopped = true;
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        self.stop();
    }
}
