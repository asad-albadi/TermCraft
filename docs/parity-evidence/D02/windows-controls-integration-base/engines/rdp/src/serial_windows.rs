//! Explicit Windows COM mappings pinned to one opened device for the connection.
//! Native overlapped operations run only on the bounded device worker.
use super::{DENIED, INVALID, MAX_IO, UNSUPPORTED, complete, length_data, status, u32_at};
use crate::rdp_options::SerialMapping;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs::File,
    io,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    sync::Arc,
};
use windows_sys::Win32::{
    Devices::Communication::{
        COMMTIMEOUTS, ClearCommBreak, DCB, EscapeCommFunction, GetCommState, GetCommTimeouts,
        SETXON, SetCommState, SetCommTimeouts,
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
const CANCELLED: u32 = 0xc0000120;
const INVALID_HANDLE: u32 = 0xc0000008;
const DEVICE_BUSY: u32 = 0x80000011;

#[derive(Clone)]
pub(super) struct Binding {
    pub(super) name: String,
    endpoint: Result<Arc<File>, u32>,
    read_only: bool,
}
struct Port {
    binding: Binding,
    handle: Option<Handle>,
}
struct Handle {
    id: u32,
    file: Arc<File>,
    reads: bool,
    writes: bool,
    original: Option<(DCB, COMMTIMEOUTS)>,
}
impl Drop for Handle {
    fn drop(&mut self) {
        if let Some((state, timeouts)) = &self.original {
            unsafe {
                ClearCommBreak(self.file.as_raw_handle());
                EscapeCommFunction(self.file.as_raw_handle(), SETXON);
                SetCommState(self.file.as_raw_handle(), state);
                SetCommTimeouts(self.file.as_raw_handle(), timeouts);
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
    finished: bool,
}
impl Pending {
    fn new(request: Vec<u8>, device: u32, handle: &Handle, major: u32) -> Result<Self, u32> {
        let body = &request[24..];
        let mut input = Vec::new();
        let mut output = Vec::new();
        let mut code = 0;
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
                let capacity = u32_at(body, 0)? as usize;
                let len = u32_at(body, 4)? as usize;
                code = u32_at(body, 8)?;
                if capacity > MAX_IO || len > MAX_IO || body.len() != 32 + len {
                    return Err(INVALID);
                }
                let (required_input, required_output, query) = match code {
                    0x001b0004 => (4, 0, false),
                    0x001b0050 => (0, 4, true),
                    0x001b000c => (3, 0, false),
                    0x001b0054 => (0, 3, true),
                    0x001b001c => (20, 0, false),
                    0x001b0020 => (0, 20, true),
                    0x001b0058 => (6, 0, false),
                    0x001b005c => (0, 6, true),
                    0x001b0064 => (16, 0, false),
                    0x001b0060 => (0, 16, true),
                    0x001b0068 | 0x001b0078 => (0, 4, true),
                    0x001b0024 | 0x001b0028 | 0x001b0030 | 0x001b0034 | 0x001b0038 | 0x001b003c
                    | 0x001b0010 | 0x001b0014 => (0, 0, false),
                    _ => return Err(UNSUPPORTED),
                };
                if len != required_input {
                    return Err(INVALID);
                }
                if capacity < required_output {
                    return Err(0xc0000023);
                }
                if !query && !handle.writes {
                    return Err(DENIED);
                }
                input.extend_from_slice(&body[32..]);
                output.resize(required_output, 0);
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
            finished: false,
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
        let result = super::serial_completion::terminal_data(
            self.major,
            &mut self.output,
            if ok == 0 {
                Err(last_error())
            } else {
                Ok(transferred)
            },
        );
        Some(complete(&self.request, result))
    }
    fn cancel_and_retire(&mut self) -> Vec<u8> {
        let handle = self.file.as_raw_handle();
        let operation = &*self.operation as *const OVERLAPPED;
        let response = super::serial_completion::cancel_and_collect(
            &self.request,
            self.major,
            &mut self.output,
            || unsafe {
                CancelIoEx(handle, operation);
            },
            || {
                // This wait retains self's buffers/event/handle until native
                // completion; CancelIoEx alone does not decide the outcome.
                let mut transferred = 0;
                let ok = unsafe { GetOverlappedResult(handle, operation, &mut transferred, 1) };
                if ok == 0 {
                    Err(last_error())
                } else {
                    Ok(transferred)
                }
            },
        );
        self.finished = true;
        response
    }
}
impl Drop for Pending {
    fn drop(&mut self) {
        if !self.finished {
            let mut transferred = 0;
            unsafe {
                CancelIoEx(self.file.as_raw_handle(), &*self.operation);
                // Only this filesystem/device worker can wait for a driver; the
                // protocol and UI threads never join it or own the native buffers.
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
pub(super) struct Serial {
    ports: BTreeMap<u32, Port>,
    pending: VecDeque<Pending>,
    completed: Vec<Vec<u8>>,
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
            pending: VecDeque::new(),
            completed: Vec::new(),
            next_handle: 1,
        }
    }
    pub(super) fn contains(&self, device: u32) -> bool {
        self.ports.contains_key(&device)
    }
    pub(super) fn has_pending(&self) -> bool {
        !self.pending.is_empty() || !self.completed.is_empty()
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
            port.open(self.next_handle, &request[24..])
                .and_then(|handle| {
                    let next = self.next_handle.checked_add(1).ok_or(0xc000009au32)?;
                    let mut body = handle.id.to_le_bytes().to_vec();
                    body.push(0);
                    self.next_handle = next;
                    port.handle = Some(handle);
                    Ok(body)
                })
        } else if port.handle.as_ref().is_some_and(|handle| handle.id == id) {
            if major == 2 {
                if request.len() != 56 {
                    Err(INVALID)
                } else {
                    let mut active = VecDeque::new();
                    for mut pending in self.pending.drain(..) {
                        if pending.device == device {
                            self.completed.push(pending.cancel_and_retire());
                            // Retire native buffers before restoring the port state.
                            drop(pending);
                        } else {
                            active.push_back(pending);
                        }
                    }
                    self.pending = active;
                    port.handle = None;
                    Ok(vec![0; 4])
                }
            } else {
                match Pending::new(
                    request.clone(),
                    device,
                    port.handle.as_ref().unwrap(),
                    major,
                ) {
                    Ok(pending) => {
                        self.pending.push_back(pending);
                        return None;
                    }
                    Err(error) => Err(error),
                }
            }
        } else {
            Err(INVALID_HANDLE)
        };
        Some(complete(&request, result))
    }
    pub(super) fn ready(&mut self) -> Vec<Vec<u8>> {
        let mut responses = std::mem::take(&mut self.completed);
        self.pending.retain_mut(|pending| match pending.ready() {
            Some(response) => {
                responses.push(response);
                false
            }
            None => true,
        });
        responses
    }
}
impl Drop for Serial {
    fn drop(&mut self) {
        self.pending.clear(); /* retire native buffers before handle state restoration */
    }
}
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
