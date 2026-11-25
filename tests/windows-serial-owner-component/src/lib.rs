//! Linux controlled-boundary execution of the exact Windows serial source.
//! Win32, OS handles and thread spawning are replaced only at their boundaries.
#![cfg(target_os = "linux")]
#![allow(dead_code, non_snake_case, non_camel_case_types, unused_imports)]
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs::File,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
const INVALID: u32 = 0xc000000d;
const DENIED: u32 = 0xc0000022;
const UNSUPPORTED: u32 = 0xc00000bb;
const MAX_IO: usize = 64 * 1024;
fn status(_: std::io::Error) -> u32 {
    0xc0000001
}
include!("helpers.rs");
#[path = "completion.rs"]
mod serial_completion;
mod rdp_options {
    #[derive(Clone)]
    pub struct SerialMapping {
        pub name: String,
        pub path: std::path::PathBuf,
        pub read_only: bool,
    }
}
#[path = "serial_windows.rs"]
mod serial;
mod native_io {
    use std::{
        fs::File,
        os::fd::{AsRawFd, FromRawFd},
    };
    pub trait AsRawHandle {
        fn as_raw_handle(&self) -> *mut std::ffi::c_void;
    }
    impl AsRawHandle for File {
        fn as_raw_handle(&self) -> *mut std::ffi::c_void {
            self.as_raw_fd() as usize as _
        }
    }
    pub trait FromRawHandle {
        unsafe fn from_raw_handle(raw: *mut std::ffi::c_void) -> Self;
    }
    impl FromRawHandle for File {
        unsafe fn from_raw_handle(raw: *mut std::ffi::c_void) -> Self {
            unsafe { Self::from_raw_fd(raw as i32) }
        }
    }
    pub struct OwnedHandle(pub usize);
    impl AsRawHandle for OwnedHandle {
        fn as_raw_handle(&self) -> *mut std::ffi::c_void {
            self.0 as _
        }
    }
    impl FromRawHandle for OwnedHandle {
        unsafe fn from_raw_handle(raw: *mut std::ffi::c_void) -> Self {
            Self(raw as usize)
        }
    }
    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            let mut s = super::state().lock().unwrap();
            if s.ops.values().any(|o| o.event == self.0) {
                s.early_drop = true;
            }
            if let Some(owner) = s.event_threads.get(&self.0) {
                if *owner != std::thread::current().id() {
                    s.wrong_thread = true;
                }
            }
            s.events_dropped += 1;
        }
    }
}
#[derive(Clone)]
struct Op {
    owner: std::thread::ThreadId,
    fill: u8,
    handle: usize,
    output: usize,
    len: u32,
    event: usize,
    terminal: Option<Result<u32, u32>>,
    cancels: usize,
    code: u32,
}
#[derive(Default)]
struct State {
    ops: BTreeMap<usize, Op>,
    early_drop: bool,
    events_dropped: usize,
    restores: usize,
    blocking_waits: usize,
    issued: usize,
    init_block: bool,
    init_entered: bool,
    stats_pending: bool,
    stats_calls: usize,
    submit_block: bool,
    submit_entered: bool,
    wrong_thread: bool,
    event_threads: BTreeMap<usize, std::thread::ThreadId>,
}
fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(Default::default)
}
unsafe extern "C" {
    fn __errno_location() -> *mut i32;
}
fn error(n: u32) {
    unsafe {
        *__errno_location() = n as i32;
    }
}
mod api {
    use super::*;
    pub type HANDLE = *mut std::ffi::c_void;
    pub const ERROR_IO_INCOMPLETE: u32 = 996;
    pub const ERROR_IO_PENDING: u32 = 997;
    pub const ERROR_OPERATION_ABORTED: u32 = 995;
    pub const ERROR_DEVICE_NOT_CONNECTED: u32 = 1167;
    pub const ERROR_INVALID_FUNCTION: u32 = 1;
    pub const ERROR_NOT_SUPPORTED: u32 = 50;
    pub const INVALID_HANDLE_VALUE: HANDLE = usize::MAX as _;
    pub const SETXON: u32 = 1;
    pub const FILE_FLAG_OVERLAPPED: u32 = 1;
    pub const OPEN_EXISTING: u32 = 3;
    #[derive(Default)]
    pub struct DCB {
        pub DCBlength: u32,
    }
    #[derive(Default)]
    pub struct COMMTIMEOUTS {
        pub a: u32,
    }
    #[derive(Default)]
    pub struct OVERLAPPED {
        pub hEvent: HANDLE,
    }
    pub struct SERIAL_STATUS {
        a: [u8; 20],
    }
    pub struct SERIAL_COMMPROP {
        a: [u8; 64],
    }
    pub struct SERIALPERF_STATS {
        a: [u8; 24],
    }
    pub unsafe fn CreateFileW(
        _: *const u16,
        _: u32,
        _: u32,
        _: *const (),
        _: u32,
        _: u32,
        _: HANDLE,
    ) -> HANDLE {
        use std::os::fd::IntoRawFd;
        File::open("/dev/null").unwrap().into_raw_fd() as usize as _
    }
    pub unsafe fn GetCommState(_: HANDLE, _: *mut DCB) -> i32 {
        loop {
            let mut s = state().lock().unwrap();
            s.init_entered = true;
            if !s.init_block {
                return 1;
            }
            drop(s);
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    pub unsafe fn GetCommTimeouts(_: HANDLE, _: *mut COMMTIMEOUTS) -> i32 {
        1
    }
    pub unsafe fn GetCommMask(_: HANDLE, m: *mut u32) -> i32 {
        unsafe {
            *m = 0;
        }
        1
    }
    pub unsafe fn ClearCommBreak(_: HANDLE) -> i32 {
        1
    }
    pub unsafe fn EscapeCommFunction(_: HANDLE, _: u32) -> i32 {
        1
    }
    pub unsafe fn SetCommState(handle: HANDLE, _: *const DCB) -> i32 {
        let mut s = state().lock().unwrap();
        if s.ops.values().any(|o| o.handle == handle as usize) {
            s.early_drop = true;
        }
        s.restores += 1;
        1
    }
    pub unsafe fn SetCommTimeouts(_: HANDLE, _: *const COMMTIMEOUTS) -> i32 {
        1
    }
    pub unsafe fn SetCommMask(_: HANDLE, _: u32) -> i32 {
        1
    }
    pub unsafe fn CreateEventW(_: *const (), _: i32, _: i32, _: *const u16) -> HANDLE {
        static NEXT: AtomicUsize = AtomicUsize::new(1000);
        NEXT.fetch_add(1, Ordering::SeqCst) as _
    }
    fn issue(handle: HANDLE, out: *mut u8, len: u32, op: *mut OVERLAPPED, code: u32) -> i32 {
        let event = unsafe { (*op).hEvent as usize };
        let mut s = state().lock().unwrap();
        s.issued += 1;
        s.event_threads.insert(event, std::thread::current().id());
        s.ops.insert(
            op as usize,
            Op {
                owner: std::thread::current().id(),
                fill: 7,
                handle: handle as usize,
                output: out as usize,
                len,
                event,
                terminal: None,
                cancels: 0,
                code,
            },
        );
        drop(s);
        error(ERROR_IO_PENDING);
        0
    }
    pub unsafe fn ReadFile(
        handle: HANDLE,
        out: *mut u8,
        n: u32,
        _: *mut u32,
        op: *mut OVERLAPPED,
    ) -> i32 {
        loop {
            let mut s = state().lock().unwrap();
            s.submit_entered = true;
            if !s.submit_block {
                break;
            }
            drop(s);
            std::thread::sleep(Duration::from_millis(1));
        }
        issue(handle, out, n, op, 3)
    }
    pub unsafe fn WriteFile(
        handle: HANDLE,
        _: *const u8,
        n: u32,
        _: *mut u32,
        op: *mut OVERLAPPED,
    ) -> i32 {
        issue(handle, std::ptr::null_mut(), n, op, 4)
    }
    pub unsafe fn DeviceIoControl(
        handle: HANDLE,
        code: u32,
        _: *const std::ffi::c_void,
        _: u32,
        out: *mut std::ffi::c_void,
        n: u32,
        _: *mut u32,
        op: *mut OVERLAPPED,
    ) -> i32 {
        if code == 0x001b008c {
            let result = issue(handle, out.cast(), n, op, code);
            let mut s = state().lock().unwrap();
            s.stats_calls += 1;
            let fill = if s.stats_calls == 1 { 40 } else { 43 };
            let pending = s.stats_pending;
            let o = s.ops.get_mut(&(op as usize)).unwrap();
            o.fill = fill;
            if !pending {
                o.terminal = Some(Ok(24));
            }
            drop(s);
            error(ERROR_IO_PENDING);
            return result;
        }
        if code == 0x001b0044 {
            let mut s = state().lock().unwrap();
            for o in s.ops.values_mut() {
                if o.code == 0x001b0048 {
                    o.terminal = Some(Ok(4));
                }
            }
            drop(s);
            unsafe {
                std::ptr::write_bytes(out, 0, n as usize);
            }
            return 1;
        }
        issue(handle, out.cast(), n, op, code)
    }
    pub unsafe fn CancelIoEx(_: HANDLE, op: *const OVERLAPPED) -> i32 {
        let mut s = state().lock().unwrap();
        if let Some(o) = s.ops.get_mut(&(op as usize)) {
            o.cancels += 1;
            if o.owner != std::thread::current().id() {
                s.wrong_thread = true;
            }
        }
        1
    }
    pub unsafe fn GetOverlappedResult(
        _: HANDLE,
        op: *const OVERLAPPED,
        n: *mut u32,
        wait: i32,
    ) -> i32 {
        loop {
            let mut s = state().lock().unwrap();
            if let Some(o) = s.ops.get(&(op as usize)).cloned() {
                if o.owner != std::thread::current().id() {
                    s.wrong_thread = true;
                }
                match o.terminal {
                    None => {
                        if wait != 0 {
                            s.blocking_waits += 1;
                            drop(s);
                            std::thread::sleep(Duration::from_millis(1));
                            continue;
                        }
                        error(ERROR_IO_INCOMPLETE);
                        return 0;
                    }
                    Some(result) => {
                        s.ops.remove(&(op as usize));
                        drop(s);
                        match result {
                            Ok(count) => {
                                if o.output != 0 {
                                    unsafe {
                                        std::ptr::write_bytes(
                                            o.output as *mut u8,
                                            o.fill,
                                            count.min(o.len) as usize,
                                        );
                                    }
                                }
                                unsafe {
                                    *n = count;
                                }
                                return 1;
                            }
                            Err(e) => {
                                error(e);
                                return 0;
                            }
                        }
                    }
                }
            } else {
                unsafe {
                    *n = 0;
                }
                return 1;
            }
        }
    }
}
mod windows_sys {
    pub mod Win32 {
        pub mod Foundation {
            pub use crate::api::*;
        }
        pub mod Devices {
            pub mod Communication {
                pub use crate::api::*;
            }
            pub mod SerialCommunication {
                pub use crate::api::*;
            }
        }
        pub mod Storage {
            pub mod FileSystem {
                pub use crate::api::*;
            }
        }
        pub mod System {
            pub mod IO {
                pub use crate::api::*;
            }
            pub mod Threading {
                pub use crate::api::*;
            }
        }
    }
}
fn req(device: u32, id: u32, major: u32, completion: u32) -> Vec<u8> {
    let mut r = vec![114, 68, 82, 73];
    for n in [device, id, completion, major, 0] {
        r.extend(n.to_le_bytes());
    }
    r.resize(56, 0);
    r
}
fn open(device: u32, completion: u32) -> Vec<u8> {
    let mut r = req(device, 0, 0, completion);
    r[24..28].copy_from_slice(&0xc0000000u32.to_le_bytes());
    r[44..48].copy_from_slice(&1u32.to_le_bytes());
    r
}
fn data(device: u32, id: u32, major: u32, completion: u32) -> Vec<u8> {
    let mut r = req(device, id, major, completion);
    r[24..28].copy_from_slice(&3u32.to_le_bytes());
    if major == 4 {
        r.extend([1, 2, 3]);
    }
    r
}
fn bindings(n: usize) -> Vec<serial::Binding> {
    serial::bind(
        &(1..=n)
            .map(|n| rdp_options::SerialMapping {
                name: format!("COM{n}"),
                path: format!(r"\\.\COM{n}").into(),
                read_only: false,
            })
            .collect::<Vec<_>>(),
    )
    .unwrap()
}
fn wait(mut f: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(3);
    while !f() {
        assert!(Instant::now() < end, "timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn finish(result: Result<u32, u32>) {
    for op in state().lock().unwrap().ops.values_mut() {
        op.terminal = Some(result);
    }
}
#[test]
fn stuck_close_does_not_block_shared_dispatch_and_terminal_write_wins() {
    reset();
    let bindings = bindings(2);
    let (tx, rx) = std::sync::mpsc::channel::<Option<Vec<u8>>>();
    let (done, results) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let mut s = serial::Serial::new(1, bindings);
        while let Ok(r) = rx.recv() {
            let v = match r {
                Some(r) if u32_at(&r, 4) == Ok(99) => vec![complete(
                    &r,
                    Ok(length_data(std::fs::read("/etc/hostname").unwrap())),
                )],
                Some(r) => s.process(r).into_iter().collect(),
                None => s.ready(),
            };
            done.send(v).unwrap();
        }
    });
    tx.send(Some(open(1, 1))).unwrap();
    results.recv().unwrap();
    let mut opened = Vec::new();
    wait(|| {
        tx.send(None).unwrap();
        opened.extend(results.recv().unwrap());
        !opened.is_empty()
    });
    tx.send(Some(data(1, 1, 4, 2))).unwrap();
    results.recv().unwrap();
    wait(|| !state().lock().unwrap().ops.is_empty());
    tx.send(Some(req(1, 1, 2, 3))).unwrap();
    let prompt = results.recv_timeout(Duration::from_millis(100));
    // Always unblock an old unsafe fixture before asserting, avoiding orphan test threads.
    if prompt.is_err() {
        finish(Ok(3));
        results.recv_timeout(Duration::from_secs(3)).unwrap();
        drop(tx);
        worker.join().unwrap();
        panic!("CLOSE blocked shared dispatcher while native cancellation was pending");
    }
    assert!(prompt.unwrap().is_empty());
    tx.send(Some(open(2, 4))).unwrap();
    results.recv().unwrap();
    let mut other = Vec::new();
    wait(|| {
        tx.send(None).unwrap();
        other.extend(results.recv().unwrap());
        other.iter().any(|r| u32_at(r, 8) == Ok(4))
    });
    tx.send(Some(req(99, 0, 3, 5))).unwrap();
    let file = results.recv_timeout(Duration::from_millis(100)).unwrap();
    assert_eq!(u32_at(&file[0], 8), Ok(5));
    assert_eq!(u32_at(&file[0], 12), Ok(0));
    assert!(u32_at(&file[0], 16).unwrap() > 0);
    assert!(!state().lock().unwrap().early_drop);
    assert!(!state().lock().unwrap().wrong_thread);
    finish(Ok(3));
    let mut completion = Vec::new();
    wait(|| {
        tx.send(None).unwrap();
        completion.extend(results.recv().unwrap());
        completion.iter().any(|r| u32_at(r, 8) == Ok(3))
    });
    let write = completion.iter().find(|r| u32_at(r, 8) == Ok(2)).unwrap();
    assert_eq!(&write[12..], &[0, 0, 0, 0, 3, 0, 0, 0, 0]);
    drop(tx);
    worker.join().unwrap();
    wait(|| state().lock().unwrap().ops.is_empty());
    assert!(!state().lock().unwrap().early_drop);
    assert!(!state().lock().unwrap().wrong_thread);
}
fn reset() {
    wait(|| state().lock().unwrap().ops.is_empty());
    std::thread::sleep(Duration::from_millis(20));
    *state().lock().unwrap() = State::default();
}
fn opened(s: &mut serial::Serial, device: u32, completion: u32) -> u32 {
    assert!(s.process(open(device, completion)).is_none());
    let mut r = Vec::new();
    wait(|| {
        r.extend(s.ready());
        !r.is_empty()
    });
    assert_eq!(u32_at(&r[0], 12), Ok(0));
    u32_at(&r[0], 16).unwrap()
}
fn close(s: &mut serial::Serial, device: u32, id: u32, completion: u32) -> Vec<Vec<u8>> {
    assert!(s.process(req(device, id, 2, completion)).is_none());
    let mut r = Vec::new();
    wait(|| {
        r.extend(s.ready());
        r.iter().any(|p| u32_at(p, 8) == Ok(completion))
    });
    r
}
#[test]
fn disconnect_quarantines_binding_buffers_and_late_abort_until_terminal() {
    reset();
    let b = bindings(1);
    let mut first = serial::Serial::new(1, b.clone());
    let id = opened(&mut first, 1, 10);
    assert!(first.process(data(1, id, 3, 11)).is_none());
    wait(|| !state().lock().unwrap().ops.is_empty());
    let before = Instant::now();
    drop(first);
    assert!(before.elapsed() < Duration::from_millis(100));
    wait(|| state().lock().unwrap().ops.values().all(|o| o.cancels == 1));
    let mut next = serial::Serial::new(1, b);
    let denied = next.process(open(1, 12)).unwrap();
    assert_eq!(u32_at(&denied, 12), Ok(0x80000011));
    assert_eq!(state().lock().unwrap().restores, 0);
    assert!(!state().lock().unwrap().early_drop);
    assert!(!state().lock().unwrap().wrong_thread);
    finish(Err(api::ERROR_OPERATION_ABORTED));
    wait(|| state().lock().unwrap().restores == 1);
    // Retry after quarantine release must succeed on the same pinned binding.
    let mut response = None;
    wait(|| {
        response = next.process(open(1, 13));
        response.is_none()
    });
    let mut r = Vec::new();
    wait(|| {
        r.extend(next.ready());
        !r.is_empty()
    });
    let id = u32_at(&r[0], 16).unwrap();
    close(&mut next, 1, id, 14);
    drop(next);
    assert!(!state().lock().unwrap().early_drop);
    assert!(!state().lock().unwrap().wrong_thread);
    assert_eq!(state().lock().unwrap().blocking_waits, 0);
}
#[test]
fn native_abort_is_reported_before_async_close_and_no_cancel_success_fabrication() {
    reset();
    let mut s = serial::Serial::new(1, bindings(1));
    let id = opened(&mut s, 1, 20);
    s.process(data(1, id, 3, 21));
    wait(|| !state().lock().unwrap().ops.is_empty());
    s.process(req(1, id, 2, 22));
    wait(|| state().lock().unwrap().ops.values().all(|o| o.cancels == 1));
    finish(Err(api::ERROR_OPERATION_ABORTED));
    let mut r = Vec::new();
    wait(|| {
        r.extend(s.ready());
        r.len() == 2
    });
    assert_eq!(u32_at(&r[0], 8), Ok(21));
    assert_eq!(u32_at(&r[0], 12), Ok(0xc0000120));
    assert_eq!(u32_at(&r[1], 8), Ok(22));
    assert_eq!(state().lock().unwrap().restores, 1);
    drop(s);
}
#[test]
fn process_capacity_is_sixteen_and_disconnect_retains_all_stuck_owners() {
    reset();
    let b = bindings(17);
    let mut s = serial::Serial::new(1, b.clone());
    for n in 1..=16 {
        assert!(s.process(open(n, 100 + n)).is_none());
    }
    let exhausted = s.process(open(17, 117)).unwrap();
    assert_eq!(u32_at(&exhausted, 12), Ok(0x80000011));
    let mut responses = Vec::new();
    wait(|| {
        responses.extend(s.ready());
        responses.len() == 16
    });
    for n in 1..=16 {
        assert!(s.process(data(n, n, 3, 200 + n)).is_none());
    }
    wait(|| state().lock().unwrap().ops.len() == 16);
    drop(s);
    wait(|| state().lock().unwrap().ops.values().all(|o| o.cancels == 1));
    let mut retry = serial::Serial::new(1, b);
    assert_eq!(
        u32_at(&retry.process(open(17, 300)).unwrap(), 12),
        Ok(0x80000011)
    );
    finish(Ok(3));
    wait(|| state().lock().unwrap().restores == 16);
    drop(retry);
    assert!(!state().lock().unwrap().early_drop);
    assert!(!state().lock().unwrap().wrong_thread);
}
fn ctl(device: u32, id: u32, code: u32, input: &[u8], capacity: u32, completion: u32) -> Vec<u8> {
    let mut r = req(device, id, 14, completion);
    r[24..28].copy_from_slice(&capacity.to_le_bytes());
    r[28..32].copy_from_slice(&(input.len() as u32).to_le_bytes());
    r[32..36].copy_from_slice(&code.to_le_bytes());
    r.extend(input);
    r
}
#[test]
fn saturated_native_requests_preserve_wait_mask_reset_escape_and_close() {
    reset();
    let mut s = serial::Serial::new(1, bindings(1));
    let id = opened(&mut s, 1, 400);
    s.process(ctl(1, id, 0x001b0048, &[], 4, 401));
    for n in 0..15 {
        s.process(data(1, id, 3, 402 + n));
    }
    wait(|| state().lock().unwrap().ops.len() == 16);
    // A second wait is rejected while the first wait remains live.
    s.process(ctl(1, id, 0x001b0048, &[], 4, 420));
    let mut response = Vec::new();
    wait(|| {
        response.extend(s.ready());
        response.iter().any(|r| u32_at(r, 8) == Ok(420))
    });
    assert_ne!(u32_at(response.last().unwrap(), 12), Ok(0));
    s.process(ctl(1, id, 0x001b0044, &0u32.to_le_bytes(), 0, 421));
    wait(|| {
        response.extend(s.ready());
        response.iter().any(|r| u32_at(r, 8) == Ok(401))
    });
    assert!(
        response
            .iter()
            .any(|r| u32_at(r, 8) == Ok(421) && u32_at(r, 12) == Ok(0))
    );
    s.process(req(1, id, 2, 422));
    wait(|| state().lock().unwrap().ops.values().all(|o| o.cancels == 1));
    finish(Err(api::ERROR_OPERATION_ABORTED));
    wait(|| {
        response.extend(s.ready());
        response.iter().any(|r| u32_at(r, 8) == Ok(422))
    });
    drop(s);
    assert!(!state().lock().unwrap().early_drop);
    assert!(!state().lock().unwrap().wrong_thread);
}
#[test]
fn blocked_logical_initialization_keeps_dispatch_and_nonblocking_queue_bounded() {
    reset();
    let b = bindings(1);
    {
        let mut s = state().lock().unwrap();
        s.init_block = true;
        s.init_entered = false;
    }
    let mut s = serial::Serial::new(1, b);
    let start = Instant::now();
    assert!(s.process(open(1, 500)).is_none());
    assert!(start.elapsed() < Duration::from_millis(100));
    wait(|| state().lock().unwrap().init_entered);
    assert!(s.ready().is_empty());
    let guessed = s.process(data(1, 1, 3, 501)).unwrap();
    assert_eq!(u32_at(&guessed, 12), Ok(0xc0000008));
    let start = Instant::now();
    drop(s);
    assert!(start.elapsed() < Duration::from_millis(100));
    state().lock().unwrap().init_block = false;
    wait(|| state().lock().unwrap().restores == 1);
    assert!(!state().lock().unwrap().early_drop);
    assert!(!state().lock().unwrap().wrong_thread);
}

mod spawn_boundary {
    pub static FAIL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    pub struct Builder(std::thread::Builder);
    impl Builder {
        pub fn new() -> Self {
            Self(std::thread::Builder::new())
        }
        pub fn name(self, name: String) -> Self {
            Self(self.0.name(name))
        }
        pub fn spawn<F, T>(self, f: F) -> std::io::Result<std::thread::JoinHandle<T>>
        where
            F: FnOnce() -> T + Send + 'static,
            T: Send + 'static,
        {
            if FAIL.swap(false, std::sync::atomic::Ordering::SeqCst) {
                return Err(std::io::Error::other("controlled spawn failure"));
            }
            self.0.spawn(f)
        }
    }
}
#[test]
fn spawn_failure_releases_binding_and_process_capacity_before_any_native_io() {
    reset();
    let b = bindings(1);
    let mut s = serial::Serial::new(1, b);
    state().lock().unwrap().init_entered = false;
    spawn_boundary::FAIL.store(true, Ordering::SeqCst);
    let failed = s.process(open(1, 600)).unwrap();
    assert_eq!(u32_at(&failed, 12), Ok(0xc000009a));
    assert_eq!(state().lock().unwrap().issued, 0);
    assert!(!state().lock().unwrap().init_entered);
    let id = opened(&mut s, 1, 601);
    close(&mut s, 1, id, 602);
    drop(s);
}
#[test]
fn stuck_probe_timeout_never_restores_or_reopens_until_native_probe_retires() {
    reset();
    let b = bindings(1);
    state().lock().unwrap().stats_pending = true;
    let mut s = serial::Serial::new(1, b.clone());
    let id = opened(&mut s, 1, 700);
    assert!(!state().lock().unwrap().ops.is_empty());
    assert!(s.process(req(1, id, 2, 701)).is_none());
    std::thread::sleep(Duration::from_millis(20));
    assert!(s.ready().is_empty());
    assert_eq!(state().lock().unwrap().restores, 0);
    let mut retry = serial::Serial::new(1, b);
    assert_eq!(
        u32_at(&retry.process(open(1, 702)).unwrap(), 12),
        Ok(0x80000011)
    );
    finish(Ok(24));
    let mut r = Vec::new();
    wait(|| {
        r.extend(s.ready());
        !r.is_empty()
    });
    assert_eq!(u32_at(&r[0], 8), Ok(701));
    assert_eq!(state().lock().unwrap().restores, 1);
    drop(s);
    drop(retry);
    assert!(!state().lock().unwrap().early_drop);
    assert!(!state().lock().unwrap().wrong_thread);
}
#[test]
fn bounded_nonblocking_request_queue_reports_every_rejected_id() {
    reset();
    let mut s = serial::Serial::new(1, bindings(1));
    let id = opened(&mut s, 1, 800);
    {
        let mut state = state().lock().unwrap();
        state.submit_block = true;
        state.submit_entered = false;
    }
    assert!(s.process(data(1, id, 3, 801)).is_none());
    wait(|| state().lock().unwrap().submit_entered);
    for n in 0..18 {
        assert!(s.process(data(1, id, 3, 802 + n)).is_none());
    }
    let busy = s.process(data(1, id, 3, 820)).unwrap();
    assert_eq!(u32_at(&busy, 8), Ok(820));
    assert_eq!(u32_at(&busy, 12), Ok(0x80000011));
    state().lock().unwrap().submit_block = false;
    let mut responses = vec![busy];
    wait(|| {
        responses.extend(s.ready());
        state().lock().unwrap().ops.len() == 16
    });
    finish(Ok(3));
    wait(|| {
        responses.extend(s.ready());
        responses.len() == 20
    });
    for response in &responses {
        if u32_at(response, 12) == Ok(0) {
            assert_eq!(&response[20..], &[7, 7, 7]);
        }
    }
    let ids: BTreeSet<_> = responses.iter().map(|r| u32_at(r, 8).unwrap()).collect();
    assert_eq!(ids, (801..=820).collect());
    close(&mut s, 1, id, 821);
    drop(s);
    assert!(!state().lock().unwrap().early_drop);
    assert!(!state().lock().unwrap().wrong_thread);
}
#[test]
fn successful_create_replies_stay_in_admission_order_and_ids_unpublished() {
    reset();
    let b = bindings(2);
    {
        let mut state = state().lock().unwrap();
        state.init_block = true;
        state.init_entered = false;
    }
    let mut s = serial::Serial::new(1, b);
    s.process(open(1, 900));
    wait(|| state().lock().unwrap().init_entered);
    s.process(open(2, 901));
    let duplicate = s.process(open(1, 902)).unwrap();
    assert_eq!(u32_at(&duplicate, 12), Ok(0x80000011));
    assert_eq!(
        u32_at(&s.process(data(2, 2, 3, 903)).unwrap(), 12),
        Ok(0xc0000008)
    );
    assert!(s.ready().is_empty());
    state().lock().unwrap().init_block = false;
    let mut r = Vec::new();
    wait(|| {
        r.extend(s.ready());
        r.len() == 2
    });
    assert_eq!(
        r.iter().map(|p| u32_at(p, 8).unwrap()).collect::<Vec<_>>(),
        vec![900, 901]
    );
    close(&mut s, 1, 1, 904);
    close(&mut s, 2, 2, 905);
    drop(s);
}
#[test]
fn actor_stats_completion_uses_its_captured_logical_open_origin() {
    reset();
    let mut s = serial::Serial::new(1, bindings(1));
    let id = opened(&mut s, 1, 1000);
    s.process(ctl(1, id, 0x001b008c, &[], 24, 1001));
    let mut r = Vec::new();
    wait(|| {
        r.extend(s.ready());
        !r.is_empty()
    });
    assert_eq!(&r[0][12..20], &[0, 0, 0, 0, 24, 0, 0, 0]);
    assert_eq!(&r[0][20..], &[3; 24]);
    close(&mut s, 1, id, 1002);
    drop(s);
    assert!(!state().lock().unwrap().wrong_thread);
}
