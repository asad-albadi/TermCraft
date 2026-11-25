//! Explicit Windows COM mappings pinned to one opened device for the connection.
//! Native overlapped operations and state restoration belong to bounded per-open owners.
#[path = "serial_windows_control.rs"]
mod control;
#[path = "serial_stats.rs"]
mod stats;
#[path = "serial_stats_windows.rs"]
mod stats_windows;
pub(super) use control::is_wait_mask_set;

use super::{DENIED, INVALID, MAX_IO, UNSUPPORTED, complete, length_data, status, u32_at};
use crate::rdp_options::SerialMapping;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs::File,
    io,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError},
    },
    thread,
};
use windows_sys::Win32::{
    Devices::Communication::{
        COMMTIMEOUTS, ClearCommBreak, DCB, EscapeCommFunction, GetCommMask, GetCommState,
        GetCommTimeouts, SETXON, SetCommMask, SetCommState, SetCommTimeouts,
    },
    Foundation::{
        ERROR_DEVICE_NOT_CONNECTED, ERROR_INVALID_FUNCTION, ERROR_IO_INCOMPLETE, ERROR_IO_PENDING,
        ERROR_NOT_SUPPORTED, ERROR_OPERATION_ABORTED, INVALID_HANDLE_VALUE,
    },
    Storage::FileSystem::{CreateFileW, FILE_FLAG_OVERLAPPED, OPEN_EXISTING, ReadFile, WriteFile},
    System::{
        IO::{CancelIoEx, DeviceIoControl, GetOverlappedResult, OVERLAPPED},
        Threading::CreateEventW,
    },
};
const _: () = {
    use windows_sys::Win32::Devices::SerialCommunication::{
        SERIAL_COMMPROP, SERIAL_STATUS, SERIALPERF_STATS,
    };
    assert!(size_of::<SERIAL_STATUS>() == control::STATUS_SIZE);
    assert!(size_of::<SERIAL_COMMPROP>() == control::PROPERTIES_SIZE);
    assert!(size_of::<SERIALPERF_STATS>() == control::STATS_SIZE);
};
const CANCELLED: u32 = 0xc0000120;
const INVALID_HANDLE: u32 = 0xc0000008;
const DEVICE_BUSY: u32 = 0x80000011;

#[derive(Clone)]
pub(super) struct Binding {
    pub(super) name: String,
    endpoint: Result<Arc<File>, u32>,
    read_only: bool,
    probe_busy: Arc<AtomicBool>,
    owner_busy: Arc<AtomicBool>,
}
struct Port {
    binding: Binding,
    handle: Option<Session>,
}
struct Handle {
    id: u32,
    file: Arc<File>,
    reads: bool,
    writes: bool,
    original: Option<(DCB, COMMTIMEOUTS)>,
    original_mask: Option<u32>,
    origin: Option<stats::Origin>,
    probe_busy: Arc<AtomicBool>,
}
impl Drop for Handle {
    fn drop(&mut self) {
        // Handle exists only on its dedicated native owner. A timed-out read-only
        // probe still owns kernel buffers; keep restoration and reopen quarantined.
        while self.probe_busy.load(Ordering::Acquire) {
            thread::sleep(stats::POLL);
        }
        if let Some((state, timeouts)) = &self.original {
            unsafe {
                ClearCommBreak(self.file.as_raw_handle());
                EscapeCommFunction(self.file.as_raw_handle(), SETXON);
                SetCommState(self.file.as_raw_handle(), state);
                SetCommTimeouts(self.file.as_raw_handle(), timeouts);
            }
        }
        if let Some(mask) = self.original_mask {
            unsafe {
                SetCommMask(self.file.as_raw_handle(), mask);
            }
        }
    }
}
fn failure(error: io::Error) -> u32 {
    match error.raw_os_error().map(|value| value as u32) {
        Some(ERROR_OPERATION_ABORTED) => CANCELLED,
        Some(ERROR_DEVICE_NOT_CONNECTED) => 0xc000009d,
        Some(ERROR_INVALID_FUNCTION | ERROR_NOT_SUPPORTED) => UNSUPPORTED,
        _ => status(error),
    }
}
fn last_error() -> u32 {
    failure(io::Error::last_os_error())
}
fn com_name(name: &str) -> Option<String> {
    let name = name.to_ascii_uppercase();
    let n = name.strip_prefix("COM")?.parse::<u16>().ok()?;
    ((1..=256).contains(&n) && name == format!("COM{n}")).then_some(name)
}
pub(super) fn bind(mappings: &[SerialMapping]) -> Result<Vec<Binding>, String> {
    let mut names = BTreeSet::new();
    let mut devices = BTreeSet::new();
    mappings
        .iter()
        .map(|mapping| {
            let name = com_name(&mapping.name).ok_or("Invalid RDP serial port name")?;
            let local = mapping
                .path
                .to_str()
                .and_then(|name| name.strip_prefix("\\\\.\\"))
                .and_then(com_name)
                .ok_or("Select an explicit Windows serial device such as \\\\.\\COM3")?;
            if !names.insert(name.clone()) || !devices.insert(local.clone()) {
                return Err("Duplicate RDP serial mapping".into());
            }
            let path: Vec<u16> = format!("\\\\.\\{local}\0").encode_utf16().collect();
            let raw = unsafe {
                CreateFileW(
                    path.as_ptr(),
                    0x80000000 | if mapping.read_only { 0 } else { 0x40000000 },
                    0,
                    std::ptr::null(),
                    OPEN_EXISTING,
                    FILE_FLAG_OVERLAPPED,
                    std::ptr::null_mut(),
                )
            };
            let endpoint = if raw == INVALID_HANDLE_VALUE {
                Err(last_error())
            } else {
                // The binding keeps this device open across certificate retries and
                // remote close/reopen. A removed/reassigned COM name is never reopened.
                let file = unsafe { File::from_raw_handle(raw) };
                let mut state = DCB {
                    DCBlength: size_of::<DCB>() as u32,
                    ..Default::default()
                };
                if unsafe { GetCommState(raw, &mut state) } == 0 {
                    Err(last_error())
                } else {
                    Ok(Arc::new(file))
                }
            };
            Ok(Binding {
                name,
                endpoint,
                read_only: mapping.read_only,
                probe_busy: Arc::default(),
                owner_busy: Arc::default(),
            })
        })
        .collect()
}
impl Port {
    fn open(&self, id: u32, body: &[u8]) -> Result<Handle, u32> {
        let access = u32_at(body, 0)?;
        let len = u32_at(body, 28)? as usize;
        if body.len() != 32 + len {
            return Err(INVALID);
        }
        if len != 0 {
            return Err(DENIED);
        }
        if u32_at(body, 20)? != 1 || u32_at(body, 24)? & (1 | 0x1000) != 0 {
            return Err(UNSUPPORTED);
        }
        let reads = access & (0x80000000 | 0x10000000 | 1) != 0;
        let writes = access & (0x40000000 | 0x10000000 | 2 | 4) != 0;
        if self.binding.read_only && (writes || access & (0x10000 | 0x10 | 0x100) != 0) {
            return Err(DENIED);
        }
        if self.handle.is_some() {
            return Err(DEVICE_BUSY);
        }
        let file = self
            .binding
            .endpoint
            .as_ref()
            .map_err(|error| *error)?
            .clone();
        let mut state = DCB {
            DCBlength: size_of::<DCB>() as u32,
            ..Default::default()
        };
        let mut timeouts = COMMTIMEOUTS::default();
        if unsafe { GetCommState(file.as_raw_handle(), &mut state) } == 0
            || unsafe { GetCommTimeouts(file.as_raw_handle(), &mut timeouts) } == 0
        {
            return Err(last_error());
        }
        Ok(Handle {
            id,
            file,
            reads,
            writes,
            original: writes.then_some((state, timeouts)),
            original_mask: None,
            origin: None,
            probe_busy: self.binding.probe_busy.clone(),
        })
    }
}
// Buffer, OVERLAPPED and event retain stable addresses until native completion.
// Cancellation requests alone do not end the kernel's ownership of those buffers.
struct Pending {
    request: Vec<u8>,
    device: u32,
    file: Arc<File>,
    operation: Box<OVERLAPPED>,
    _event: OwnedHandle,
    input: Vec<u8>,
    output: Vec<u8>,
    major: u32,
    code: u32,
    wait: bool,
    finished: bool,
    cancelled: bool,
    origin: Option<stats::Origin>,
}
impl Pending {
    fn new(
        request: Vec<u8>,
        device: u32,
        handle: &mut Handle,
        major: u32,
        wait_pending: bool,
    ) -> Result<Self, u32> {
        let body = &request[24..];
        let mut input = Vec::new();
        let mut output = Vec::new();
        let mut code = 0;
        let mut wait = false;
        match major {
            3 | 4 => {
                let len = u32_at(body, 0)? as usize;
                if len > MAX_IO || body.len() != 32 + if major == 4 { len } else { 0 } {
                    return Err(INVALID);
                }
                if major == 3 {
                    if !handle.reads {
                        return Err(DENIED);
                    }
                    output.resize(len, 0);
                } else {
                    if !handle.writes {
                        return Err(DENIED);
                    }
                    input.extend_from_slice(&body[32..]);
                }
            }
            14 => {
                let validated = control::validate(body, handle.reads, handle.writes, wait_pending)?;
                code = validated.code;
                wait = validated.wait;
                if code == control::GET_STATS && handle.origin.is_none() {
                    return Err(UNSUPPORTED);
                }
                if code == control::SET_WAIT_MASK && handle.original_mask.is_none() {
                    // Capture lazily: a driver without wait-mask support must not
                    // prevent ordinary reads/writes from opening the port.
                    let mut mask = 0;
                    if unsafe { GetCommMask(handle.file.as_raw_handle(), &mut mask) } == 0 {
                        return Err(last_error());
                    }
                    handle.original_mask = Some(mask);
                }
                input.extend_from_slice(&body[32..]);
                output.resize(validated.output_len, 0);
            }
            _ => return Err(UNSUPPORTED),
        }
        let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        if event.is_null() {
            return Err(last_error());
        }
        let event = unsafe { OwnedHandle::from_raw_handle(event) };
        let mut operation = Box::new(OVERLAPPED::default());
        operation.hEvent = event.as_raw_handle();
        let mut pending = Self {
            request,
            device,
            file: handle.file.clone(),
            operation,
            _event: event,
            input,
            output,
            major,
            code,
            wait,
            finished: false,
            cancelled: false,
            origin: handle.origin.filter(|_| code == control::GET_STATS),
        };
        let ok = unsafe {
            match major {
                3 => ReadFile(
                    pending.file.as_raw_handle(),
                    pending.output.as_mut_ptr(),
                    pending.output.len() as u32,
                    std::ptr::null_mut(),
                    &mut *pending.operation,
                ),
                4 => WriteFile(
                    pending.file.as_raw_handle(),
                    pending.input.as_ptr(),
                    pending.input.len() as u32,
                    std::ptr::null_mut(),
                    &mut *pending.operation,
                ),
                _ => DeviceIoControl(
                    pending.file.as_raw_handle(),
                    code,
                    pending.input.as_ptr().cast(),
                    pending.input.len() as u32,
                    pending.output.as_mut_ptr().cast(),
                    pending.output.len() as u32,
                    std::ptr::null_mut(),
                    &mut *pending.operation,
                ),
            }
        };
        if ok == 0 && io::Error::last_os_error().raw_os_error() != Some(ERROR_IO_PENDING as i32) {
            pending.finished = true;
            return Err(last_error());
        }
        Ok(pending)
    }
    fn ready(&mut self) -> Option<Vec<u8>> {
        let mut transferred = 0;
        let ok = unsafe {
            GetOverlappedResult(
                self.file.as_raw_handle(),
                &*self.operation,
                &mut transferred,
                0,
            )
        };
        if ok == 0 && io::Error::last_os_error().raw_os_error() == Some(ERROR_IO_INCOMPLETE as i32)
        {
            return None;
        }
        self.finished = true;
        let outcome = if ok == 0 {
            Err(last_error())
        } else {
            stats::completed_count(
                self.origin.as_ref(),
                self.code,
                &mut self.output,
                transferred,
            )
        };
        let result = super::serial_completion::terminal_data(self.major, &mut self.output, outcome);
        Some(complete(&self.request, result))
    }
    fn request_cancel(&mut self) {
        if !self.finished && !self.cancelled {
            self.cancelled = true;
            unsafe {
                CancelIoEx(self.file.as_raw_handle(), &*self.operation);
            }
        }
    }
}
impl Drop for Pending {
    fn drop(&mut self) {
        if !self.finished {
            let mut transferred = 0;
            self.request_cancel();
            unsafe {
                // Unwind fallback runs only on the bounded issuing owner, never
                // on the shared device worker. Retain buffers until terminal.
                GetOverlappedResult(
                    self.file.as_raw_handle(),
                    &*self.operation,
                    &mut transferred,
                    1,
                );
            }
        }
    }
}
// Capacity is reserved before spawning and therefore before any logical-open
// native call. The binding flag is shared across certificate-attempt clones.
const OWNER_LIMIT: usize = 16;
const ORDINARY_LIMIT: usize = 16;
const QUEUE_LIMIT: usize = ORDINARY_LIMIT + 2; // SET_WAIT_MASK escape and CLOSE
static LIVE_OWNERS: AtomicUsize = AtomicUsize::new(0);
struct OwnerPermit(Arc<AtomicBool>);
impl OwnerPermit {
    fn reserve(binding: &Arc<AtomicBool>) -> Option<Self> {
        if binding.swap(true, Ordering::AcqRel) {
            return None;
        }
        if LIVE_OWNERS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < OWNER_LIMIT).then_some(n + 1)
            })
            .is_err()
        {
            binding.store(false, Ordering::Release);
            return None;
        }
        Some(Self(binding.clone()))
    }
}
impl Drop for OwnerPermit {
    fn drop(&mut self) {
        LIVE_OWNERS.fetch_sub(1, Ordering::AcqRel);
        self.0.store(false, Ordering::Release);
    }
}
enum OwnerEvent {
    Opened(Vec<u8>, bool),
    Response(Vec<u8>),
    Closed(Vec<u8>),
}
struct Session {
    id: u32,
    sender: SyncSender<Vec<u8>>,
    events: Receiver<OwnerEvent>,
    published: bool,
    closing: bool,
}
impl Session {
    fn start(binding: Binding, id: u32, request: Vec<u8>) -> Result<Self, u32> {
        let permit = OwnerPermit::reserve(&binding.owner_busy).ok_or(DEVICE_BUSY)?;
        let (sender, commands) = mpsc::sync_channel(QUEUE_LIMIT);
        let (responses, events) = mpsc::sync_channel(QUEUE_LIMIT + 1);
        // Spawn failure drops the closure (including its permit) before any
        // native data/control/open-initialization operation can have been issued.
        thread::Builder::new()
            .name("rdp-serial-owner".into())
            .spawn(move || {
                let terminal = run_owner(binding, id, request, commands, &responses);
                drop(permit);
                if let Some(event) = terminal {
                    let _ = responses.send(event);
                }
            })
            .map_err(|_| 0xc000009au32)?;
        Ok(Self {
            id,
            sender,
            events,
            published: false,
            closing: false,
        })
    }
    fn submit(&mut self, request: Vec<u8>) -> Result<(), Vec<u8>> {
        let close = u32_at(&request, 16) == Ok(2);
        match self.sender.try_send(request) {
            Ok(()) => {
                self.closing |= close;
                Ok(())
            }
            Err(mpsc::TrySendError::Full(request) | mpsc::TrySendError::Disconnected(request)) => {
                Err(request)
            }
        }
    }
}
// This is the issuing thread for every Native Pending on this logical handle.
// Stack declaration order retires pending requests before Handle restoration on
// unwind. Pending::drop is only an owner-thread fallback, never a worker join.
fn run_owner(
    binding: Binding,
    id: u32,
    request: Vec<u8>,
    commands: Receiver<Vec<u8>>,
    responses: &SyncSender<OwnerEvent>,
) -> Option<OwnerEvent> {
    let port = Port {
        binding,
        handle: None,
    };
    let mut handle = match port.open(id, &request[24..]) {
        Ok(handle) => handle,
        Err(error) => {
            return Some(OwnerEvent::Opened(complete(&request, Err(error)), false));
        }
    };
    let mut pending: VecDeque<Pending> = VecDeque::new();
    let file = handle.file.clone();
    let probe = stats::Probe::start(&port.binding.probe_busy, move |cancel, deadline| {
        stats_windows::baseline(&file, cancel, deadline)
    });
    handle.origin = loop {
        if let Err(TryRecvError::Disconnected) = commands.try_recv() {
            return None;
        }
        match &probe {
            None => break None,
            Some(probe) => {
                if let Some(origin) = probe.poll() {
                    break origin;
                }
            }
        }
        thread::sleep(stats::POLL);
    };
    drop(probe);
    let mut body = id.to_le_bytes().to_vec();
    body.push(0);
    if responses
        .send(OwnerEvent::Opened(complete(&request, Ok(body)), true))
        .is_err()
    {
        return None;
    }
    let mut close = None;
    let mut disconnected = false;
    loop {
        // Bound work per poll, preserving completion and cancellation progress.
        for _ in 0..QUEUE_LIMIT {
            let request = match commands.try_recv() {
                Ok(request) => request,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            };
            let major = u32_at(&request, 16).unwrap();
            if close.is_some() || disconnected {
                let _ = responses.send(OwnerEvent::Response(complete(&request, Err(DEVICE_BUSY))));
            } else if major == 2 {
                close = Some(request);
            } else {
                let escape = major == 14 && is_wait_mask_set(&request[24..]);
                let limit = ORDINARY_LIMIT + usize::from(escape);
                if pending.len() >= limit {
                    let _ =
                        responses.send(OwnerEvent::Response(complete(&request, Err(DEVICE_BUSY))));
                    continue;
                }
                let device = u32_at(&request, 4).unwrap();
                match Pending::new(
                    request.clone(),
                    device,
                    &mut handle,
                    major,
                    pending.iter().any(|p| p.wait),
                ) {
                    Ok(operation) => pending.push_back(operation),
                    Err(error) => {
                        let _ =
                            responses.send(OwnerEvent::Response(complete(&request, Err(error))));
                    }
                }
            }
        }
        if close.is_some() || disconnected {
            for operation in &mut pending {
                operation.request_cancel();
            }
        }
        pending.retain_mut(|operation| match operation.ready() {
            None => true,
            Some(response) => {
                let _ = responses.send(OwnerEvent::Response(response));
                false
            }
        });
        if (close.is_some() || disconnected) && pending.is_empty() {
            // May quarantine this owner for a stuck optional probe or restoration
            // call. Its binding/global permit remains held, without a worker wait.
            drop(handle);
            return close.map(|request| OwnerEvent::Closed(complete(&request, Ok(vec![0; 4]))));
        }
        thread::sleep(stats::POLL);
    }
}
struct Opening {
    request: Vec<u8>,
    device: u32,
}
pub(super) struct Serial {
    opening: VecDeque<Opening>,
    ports: BTreeMap<u32, Port>,
    next_handle: u32,
}
impl Serial {
    pub(super) fn new(start: u32, bindings: Vec<Binding>) -> Self {
        Self {
            ports: bindings
                .into_iter()
                .enumerate()
                .map(|(i, binding)| {
                    (
                        start + i as u32,
                        Port {
                            binding,
                            handle: None,
                        },
                    )
                })
                .collect(),
            opening: VecDeque::new(),
            next_handle: 1,
        }
    }
    pub(super) fn contains(&self, device: u32) -> bool {
        self.ports.contains_key(&device)
    }
    pub(super) fn has_pending(&self) -> bool {
        !self.opening.is_empty() || self.ports.values().any(|port| port.handle.is_some())
    }
    pub(super) fn process(&mut self, request: Vec<u8>) -> Option<Vec<u8>> {
        let device = u32_at(&request, 4).unwrap();
        let id = u32_at(&request, 8).unwrap();
        let major = u32_at(&request, 16).unwrap();
        if u32_at(&request, 20).unwrap() != 0 {
            return Some(complete(&request, Err(UNSUPPORTED)));
        }
        let port = self.ports.get_mut(&device).unwrap();
        let result = if major == 0 {
            if port.handle.is_some() {
                Err(DEVICE_BUSY)
            } else if let Some(next) = self.next_handle.checked_add(1) {
                match Session::start(port.binding.clone(), self.next_handle, request.clone()) {
                    Err(error) => Err(error),
                    Ok(session) => {
                        self.next_handle = next;
                        port.handle = Some(session);
                        self.opening.push_back(Opening { request, device });
                        return None;
                    }
                }
            } else {
                Err(0xc000009a)
            }
        } else if let Some(session) = port.handle.as_mut().filter(|s| s.published && s.id == id) {
            if session.closing {
                Err(DEVICE_BUSY)
            } else if major == 2 && request.len() != 56 {
                Err(INVALID)
            } else {
                return match session.submit(request) {
                    Ok(()) => None,
                    Err(request) => Some(complete(&request, Err(DEVICE_BUSY))),
                };
            }
        } else {
            Err(INVALID_HANDLE)
        };
        Some(complete(&request, result))
    }
    pub(super) fn ready(&mut self) -> Vec<Vec<u8>> {
        let mut responses = Vec::new();
        // CREATE responses preserve admission order; unpublished IDs reject I/O.
        while let Some(first) = self.opening.front() {
            let port = self.ports.get_mut(&first.device).unwrap();
            let session = port.handle.as_mut().unwrap();
            match session.events.try_recv() {
                Ok(OwnerEvent::Opened(response, success)) => {
                    responses.push(response);
                    if success {
                        session.published = true;
                    } else {
                        port.handle = None;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    responses.push(complete(&first.request, Err(CANCELLED)));
                    port.handle = None;
                }
                _ => unreachable!("owner must initialize before accepting requests"),
            }
            self.opening.pop_front();
        }
        for port in self.ports.values_mut() {
            let Some(session) = port.handle.as_mut().filter(|s| s.published) else {
                continue;
            };
            let mut closed = false;
            while let Ok(event) = session.events.try_recv() {
                match event {
                    OwnerEvent::Response(response) => responses.push(response),
                    OwnerEvent::Closed(response) => {
                        responses.push(response);
                        closed = true;
                        break;
                    }
                    OwnerEvent::Opened(..) => unreachable!(),
                }
            }
            if closed {
                port.handle = None;
            }
        }
        responses
    }
}
// Dropping Serial only disconnects channels. Its bounded native owners retain
// all kernel-owned resources and restore state after terminal completion.
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_com_names_exclude_ambient_files_and_device_namespaces() {
        for value in ["COM0", "COM01", "COM257", "COM1\\other", "NUL", "C:\\data"] {
            assert!(com_name(value).is_none());
        }
        assert_eq!(com_name("com256").as_deref(), Some("COM256"));
    }
}
