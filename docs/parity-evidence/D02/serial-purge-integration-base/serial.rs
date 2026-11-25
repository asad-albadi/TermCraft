//! MS-RDPESP serial requests, backed by explicitly selected local TTYs.
use super::{DENIED, INVALID, MAX_IO, UNSUPPORTED, complete, length_data, status, u32_at, word};
use crate::rdp_options::SerialMapping;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::{
        fd::AsRawFd,
        unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt},
    },
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

const INVALID_HANDLE: u32 = 0xc0000008;
const CANCELLED: u32 = 0xc0000120;
const DEVICE_BUSY: u32 = 0x80000011;
const BUFFER_TOO_SMALL: u32 = 0xc0000023;
const IO_TIMEOUT: u32 = 0xc00000b5;
const DISCONNECTED: u32 = 0xc000009d;
#[cfg(test)]
const IO_LIMIT: Duration = Duration::from_secs(1);

const SET_BAUD: u32 = 0x001b0004;
const GET_BAUD: u32 = 0x001b0050;
const SET_LINE: u32 = 0x001b000c;
const GET_LINE: u32 = 0x001b0054;

type DeviceIdentity = (u64, u64, u64);

fn identity(m: &fs::Metadata) -> DeviceIdentity {
    (m.dev(), m.ino(), m.rdev())
}

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
    file: File,
    reads: bool,
    writes: bool,
    original: Option<libc::termios>,
    original_modem: Option<libc::c_int>,
    modem_changed: bool,
    flow_changed: bool,
    break_changed: bool,
    timeouts: [u32; 5],
}

impl Drop for Handle {
    fn drop(&mut self) {
        if self.break_changed {
            unsafe {
                libc::ioctl(self.file.as_raw_fd(), libc::TIOCCBRK as _);
            }
        }
        if self.flow_changed {
            unsafe {
                libc::tcflow(self.file.as_raw_fd(), libc::TCOON);
            }
        }
        if let Some(original) = &self.original {
            // TCSANOW avoids waiting for potentially stalled serial output on shutdown.
            let _ = set_attributes(&self.file, original);
        }
        if self.modem_changed {
            if let Some(bits) = self.original_modem {
                let mask = libc::TIOCM_DTR
                    | if self
                        .original
                        .as_ref()
                        .is_some_and(|t| t.c_cflag & libc::CRTSCTS != 0)
                    {
                        0
                    } else {
                        libc::TIOCM_RTS
                    };
                let _ = change_modem(&self.file, bits, mask);
            }
        }
    }
}

impl Port {
    fn open(&self, id: u32, body: &[u8]) -> Result<Handle, u32> {
        let access = u32_at(body, 0)?;
        let disposition = u32_at(body, 20)?;
        let options = u32_at(body, 24)?;
        let length = u32_at(body, 28)? as usize;
        let name = body.get(32..).ok_or(INVALID)?;
        if name.len() != length {
            return Err(INVALID);
        }
        // The device id identifies the port. MS-RDPESP requires an empty path.
        if length != 0 {
            return Err(DENIED);
        }
        if disposition != 1 || options & (1 | 0x1000) != 0 {
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
        // A devpts name and inode can be recycled. Duplicate only the open
        // descriptor bound when the mapping was initialized.
        let endpoint = self.binding.endpoint.as_ref().map_err(|error| *error)?;
        let file = endpoint.try_clone().map_err(status)?;
        let attributes = get_attributes(&file)?;
        let original = if writes {
            let mut raw = attributes;
            let line_flags = libc::CSIZE | libc::PARENB | libc::PARODD | libc::CSTOPB;
            // cfmakeraw also chooses 8/N/1. Retain the locally configured line format.
            unsafe { libc::cfmakeraw(&mut raw) };
            raw.c_cflag = (raw.c_cflag & !line_flags) | (attributes.c_cflag & line_flags);
            raw.c_cc[libc::VMIN] = 1;
            raw.c_cc[libc::VTIME] = 0;
            set_attributes(&file, &raw)?;
            Some(attributes)
        } else {
            None
        };
        let original_modem = if writes { modem(&file).ok() } else { None };
        Ok(Handle {
            id,
            file,
            reads,
            writes,
            original,
            original_modem,
            modem_changed: false,
            flow_changed: false,
            break_changed: false,
            timeouts: [u32::MAX, u32::MAX, 1000, 0, 1000],
        })
    }
}

enum Transfer {
    Read(Vec<u8>),
    Write { data: Vec<u8>, written: usize },
}

struct Pending {
    request: Vec<u8>,
    device: u32,
    handle: u32,
    transfer: Transfer,
    deadline: Option<Instant>,
    started: bool,
    received: usize,
    last_byte: Option<Instant>,
    timeouts: [u32; 5],
}

impl Pending {
    fn immediate(&self) -> bool {
        self.timeouts[..3] == [u32::MAX, 0, 0]
    }
    fn first_byte_mode(&self) -> bool {
        self.timeouts[0] == u32::MAX && self.timeouts[1] == u32::MAX
    }

    fn try_io(&mut self, handle: &mut Handle) -> Option<Result<Vec<u8>, u32>> {
        let now = Instant::now();
        if !self.started {
            self.started = true;
            let (len, multiplier, constant) = match &self.transfer {
                Transfer::Read(buffer) => (buffer.len(), self.timeouts[1], self.timeouts[2]),
                Transfer::Write { data, .. } => (data.len(), self.timeouts[3], self.timeouts[4]),
            };
            let multiplier = if matches!(self.transfer, Transfer::Read(_)) && self.first_byte_mode()
            {
                0
            } else {
                multiplier
            };
            let milliseconds = (len as u64)
                .saturating_mul(multiplier as u64)
                .saturating_add(constant as u64);
            self.deadline = (milliseconds != 0).then(|| now + Duration::from_millis(milliseconds));
        }
        let interval_expired = self.last_byte.is_some_and(|last| {
            self.timeouts[0] != 0
                && self.timeouts[0] != u32::MAX
                && now.duration_since(last) >= Duration::from_millis(self.timeouts[0] as u64)
        });
        if self.deadline.is_some_and(|deadline| now >= deadline) || interval_expired {
            return Some(match &mut self.transfer {
                Transfer::Read(buffer) => {
                    buffer.truncate(self.received);
                    Ok(length_data(std::mem::take(buffer)))
                }
                Transfer::Write { written, .. } if *written != 0 => Ok(write_count(*written)),
                Transfer::Write { .. } => Err(IO_TIMEOUT),
            });
        }
        let immediate = self.immediate();
        let first_byte = self.first_byte_mode();
        let result = match &mut self.transfer {
            Transfer::Read(buffer) if buffer.is_empty() => {
                return Some(Ok(length_data(Vec::new())));
            }
            Transfer::Read(buffer) => match handle.file.read(&mut buffer[self.received..]) {
                Ok(0) => match has_hung_up(&handle.file) {
                    Ok(true) => Err(io::Error::from_raw_os_error(libc::EIO)),
                    Ok(false) => Ok(immediate.then(|| length_data(Vec::new()))),
                    Err(error) => Err(error),
                },
                Ok(n) => {
                    self.received += n;
                    self.last_byte = Some(now);
                    if immediate || first_byte || self.received == buffer.len() {
                        buffer.truncate(self.received);
                        Ok(Some(length_data(std::mem::take(buffer))))
                    } else {
                        Ok(None)
                    }
                }
                Err(error) if immediate && error.kind() == io::ErrorKind::WouldBlock => {
                    Ok(Some(length_data(Vec::new())))
                }
                Err(error) => Err(error),
            },
            Transfer::Write { data, written } => handle.file.write(&data[*written..]).map(|n| {
                *written += n;
                (*written == data.len()).then(|| write_count(*written))
            }),
        };
        match result {
            Ok(body) => body.map(Ok),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                None
            }
            Err(error) => Some(Err(tty_status(error))),
        }
    }
}

fn write_count(n: usize) -> Vec<u8> {
    let mut body = (n as u32).to_le_bytes().to_vec();
    body.push(0);
    body
}

pub(super) struct Serial {
    ports: BTreeMap<u32, Port>,
    pending: VecDeque<Pending>,
    next_handle: u32,
}

impl Serial {
    pub(super) fn new(start: u32, bindings: Vec<Binding>) -> Self {
        let ports = bindings
            .into_iter()
            .enumerate()
            .map(|(index, binding)| {
                (
                    start + index as u32,
                    Port {
                        binding,
                        handle: None,
                    },
                )
            })
            .collect();
        Self {
            ports,
            pending: VecDeque::new(),
            next_handle: 1,
        }
    }

    pub(super) fn contains(&self, device: u32) -> bool {
        self.ports.contains_key(&device)
    }

    pub(super) fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    pub(super) fn process(&mut self, request: Vec<u8>) -> Option<Vec<u8>> {
        let device = u32_at(&request, 4).unwrap();
        let id = u32_at(&request, 8).unwrap();
        let major = u32_at(&request, 16).unwrap();
        let minor = u32_at(&request, 20).unwrap();
        if minor != 0 {
            return Some(complete(&request, Err(UNSUPPORTED)));
        }

        let port = self.ports.get_mut(&device).unwrap();
        let body = &request[24..];
        let result = match (major, port.handle.as_mut()) {
            (0, _) => {
                let result = port.open(self.next_handle, body).map(|handle| {
                    let mut response = handle.id.to_le_bytes().to_vec();
                    // MS-RDPEFS encodes FILE_OPEN as information 0.
                    response.push(0);
                    self.next_handle = self.next_handle.wrapping_add(1).max(1);
                    port.handle = Some(handle);
                    response
                });
                return Some(complete(&request, result));
            }
            (_, Some(handle)) if handle.id == id => match major {
                2 if body.len() == 32 => {
                    port.handle = None;
                    Ok(vec![0; 4])
                }
                2 => Err(INVALID),
                3 | 4 => match transfer(body, handle, major) {
                    Ok(transfer) => {
                        self.pending.push_back(Pending {
                            request,
                            device,
                            handle: id,
                            transfer,
                            deadline: None,
                            started: false,
                            received: 0,
                            last_byte: None,
                            timeouts: handle.timeouts,
                        });
                        return None;
                    }
                    Err(error) => Err(error),
                },
                14 => control(handle, body),
                _ => Err(UNSUPPORTED),
            },
            _ => Err(INVALID_HANDLE),
        };
        Some(complete(&request, result))
    }

    pub(super) fn ready(&mut self) -> Vec<Vec<u8>> {
        let mut responses = Vec::new();
        // Preserve per-port order within each direction. A read waiting for data
        // can run concurrently with writes and controls on the same port.
        let mut waiting = BTreeSet::new();
        for _ in 0..self.pending.len() {
            let mut job = self.pending.pop_front().unwrap();
            let direction = matches!(job.transfer, Transfer::Read(_));
            let port = &mut self.ports.get_mut(&job.device).unwrap().handle;
            let result = match port.as_mut().filter(|h| h.id == job.handle) {
                Some(handle) if !waiting.contains(&(job.device, direction)) => job.try_io(handle),
                Some(_) => None,
                None => Some(Err(CANCELLED)),
            };
            if let Some(result) = result {
                responses.push(complete(&job.request, result));
            } else {
                waiting.insert((job.device, direction));
                self.pending.push_back(job);
            }
        }
        responses
    }
}

fn transfer(body: &[u8], handle: &Handle, major: u32) -> Result<Transfer, u32> {
    let len = u32_at(body, 0)? as usize;
    if len > MAX_IO || body.len() != (32 + if major == 4 { len } else { 0 }) {
        return Err(INVALID);
    }
    match major {
        3 if handle.reads => Ok(Transfer::Read(vec![0; len])),
        4 if handle.writes => Ok(Transfer::Write {
            data: body[32..].to_vec(),
            written: 0,
        }),
        _ => Err(DENIED),
    }
}

pub(super) fn bind(mappings: &[SerialMapping]) -> Result<Vec<Binding>, String> {
    let mut bindings = Vec::with_capacity(mappings.len());
    let mut names = BTreeSet::new();
    let mut devices = BTreeSet::new();
    for map in mappings {
        let name = map.name.to_ascii_uppercase();
        let valid_name = name
            .strip_prefix("COM")
            .and_then(|n| n.parse::<u16>().ok())
            .is_some_and(|n| (1..=256).contains(&n) && name == format!("COM{n}"));
        if !valid_name || !names.insert(name.clone()) || !map.path.is_absolute() {
            return Err("Invalid or duplicate RDP serial mapping".into());
        }
        let path = fs::canonicalize(&map.path)
            .map_err(|_| format!("Cannot resolve redirected serial port {name}"))?;
        let metadata = fs::symlink_metadata(&path)
            .map_err(|_| format!("Cannot inspect redirected serial port {name}"))?;
        if !metadata.file_type().is_char_device() || !devices.insert(metadata.rdev()) {
            return Err(format!(
                "Redirected serial port {name} is not a distinct character device"
            ));
        }
        let endpoint = bind_endpoint(&path, identity(&metadata), map.read_only).map(Arc::new);
        bindings.push(Binding {
            name,
            endpoint,
            read_only: map.read_only,
        });
    }
    Ok(bindings)
}

fn bind_endpoint(path: &Path, expected: DeviceIdentity, read_only: bool) -> Result<File, u32> {
    let file = OpenOptions::new()
        .read(true)
        .write(!read_only)
        .custom_flags(libc::O_NOCTTY | libc::O_NONBLOCK | libc::O_NOFOLLOW)
        .open(path)
        .map_err(tty_status)?;
    let metadata = file.metadata().map_err(status)?;
    if !metadata.file_type().is_char_device() || identity(&metadata) != expected {
        return Err(DENIED);
    }
    get_attributes(&file)?;
    Ok(file)
}

fn has_hung_up(file: &File) -> io::Result<bool> {
    let mut fd = libc::pollfd {
        fd: file.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    if unsafe { libc::poll(&mut fd, 1, 0) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(fd.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0)
}

fn tty_status(error: io::Error) -> u32 {
    match error.raw_os_error() {
        Some(libc::EIO | libc::ENODEV | libc::ENXIO) => DISCONNECTED,
        Some(libc::ENOTTY) => UNSUPPORTED,
        _ => status(error),
    }
}

fn get_attributes(file: &File) -> Result<libc::termios, u32> {
    let mut attributes = std::mem::MaybeUninit::uninit();
    if unsafe { libc::tcgetattr(file.as_raw_fd(), attributes.as_mut_ptr()) } != 0 {
        return Err(tty_status(io::Error::last_os_error()));
    }
    Ok(unsafe { attributes.assume_init() })
}

fn set_attributes(file: &File, attributes: &libc::termios) -> Result<(), u32> {
    if unsafe { libc::tcsetattr(file.as_raw_fd(), libc::TCSANOW, attributes) } != 0 {
        return Err(tty_status(io::Error::last_os_error()));
    }
    Ok(())
}

fn control(handle: &mut Handle, body: &[u8]) -> Result<Vec<u8>, u32> {
    let output = u32_at(body, 0)? as usize;
    let len = u32_at(body, 4)? as usize;
    let code = u32_at(body, 8)?;
    let input = body.get(32..).ok_or(INVALID)?;
    if input.len() != len || output > MAX_IO {
        return Err(INVALID);
    }
    let (input_size, output_size) = match code {
        SET_BAUD => (4, 0),
        GET_BAUD => (0, 4),
        SET_LINE => (3, 0),
        GET_LINE => (0, 3),
        0x001b001c => (20, 0),             // SET_TIMEOUTS
        0x001b0020 => (0, 20),             // GET_TIMEOUTS
        0x001b0058 => (6, 0),              // SET_CHARS
        0x001b005c => (0, 6),              // GET_CHARS
        0x001b0064 => (16, 0),             // SET_HANDFLOW
        0x001b0060 => (0, 16),             // GET_HANDFLOW
        0x001b0068 | 0x001b0078 => (0, 4), // MODEMSTATUS / DTRRTS
        0x001b0024 | 0x001b0028 | 0x001b0030 | 0x001b0034 | 0x001b0038 | 0x001b003c
        | 0x001b0010 | 0x001b0014 => (0, 0),
        _ => return Err(UNSUPPORTED),
    };
    if len != input_size {
        return Err(INVALID);
    }
    if output < output_size {
        return Err(BUFFER_TOO_SMALL);
    }
    let query = matches!(
        code,
        GET_BAUD | GET_LINE | 0x001b0020 | 0x001b005c | 0x001b0060 | 0x001b0068 | 0x001b0078
    );
    if !query && !handle.writes {
        return Err(DENIED);
    }
    if code == 0x001b001c {
        let mut values = [0; 5];
        for (index, value) in values.iter_mut().enumerate() {
            *value = u32_at(input, index * 4)?;
        }
        if values[0] == u32::MAX && values[2] == u32::MAX {
            return Err(INVALID);
        }
        handle.timeouts = values;
        return Ok(length_data(Vec::new()));
    }
    if code == 0x001b0020 {
        return Ok(length_data(
            handle
                .timeouts
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect(),
        ));
    }
    let modem_mutation = matches!(
        code,
        0x001b0064 | 0x001b0024 | 0x001b0028 | 0x001b0030 | 0x001b0034
    );
    if modem_mutation && handle.original_modem.is_none() {
        handle.original_modem = Some(modem(&handle.file)?);
    }
    let file = &handle.file;
    let mut attributes = get_attributes(file)?;
    let mut data = Vec::new();
    match code {
        SET_BAUD => {
            let baud = u32_at(input, 0)?;
            let speed = SPEEDS
                .iter()
                .find(|(rate, _)| *rate == baud)
                .ok_or(UNSUPPORTED)?
                .1;
            if unsafe { libc::cfsetispeed(&mut attributes, speed) } != 0
                || unsafe { libc::cfsetospeed(&mut attributes, speed) } != 0
            {
                return Err(INVALID);
            }
            set_attributes(file, &attributes)?;
            let actual = get_attributes(file)?;
            if unsafe { libc::cfgetospeed(&actual) } != speed {
                return Err(UNSUPPORTED);
            }
        }
        GET_BAUD => {
            let speed = unsafe { libc::cfgetospeed(&attributes) };
            let baud = SPEEDS
                .iter()
                .find(|(_, rate)| *rate == speed)
                .ok_or(UNSUPPORTED)?
                .0;
            word(&mut data, baud);
        }
        SET_LINE => {
            let [stop, parity, bits] = <[u8; 3]>::try_from(input).map_err(|_| INVALID)?;
            let stop = match (stop, bits) {
                (0, _) => 0,
                (1, 5) | (2, 6..=8) => libc::CSTOPB,
                _ => return Err(UNSUPPORTED),
            };
            let parity = match parity {
                0 => 0,
                1 => libc::PARENB | libc::PARODD,
                2 => libc::PARENB,
                _ => return Err(UNSUPPORTED),
            };
            let bits = match bits {
                5 => libc::CS5,
                6 => libc::CS6,
                7 => libc::CS7,
                8 => libc::CS8,
                _ => return Err(UNSUPPORTED),
            };
            let mask = libc::CSTOPB | libc::PARENB | libc::PARODD | libc::CSIZE;
            #[cfg(any(target_os = "linux", target_os = "android"))]
            let mask = mask | libc::CMSPAR;
            let previous = attributes;
            attributes.c_cflag = (attributes.c_cflag & !mask) | stop | parity | bits;
            set_attributes(file, &attributes)?;
            if get_attributes(file)?.c_cflag & mask != attributes.c_cflag & mask {
                let _ = set_attributes(file, &previous);
                return Err(UNSUPPORTED);
            }
        }
        GET_LINE => {
            let flags = attributes.c_cflag;
            #[cfg(any(target_os = "linux", target_os = "android"))]
            if flags & (libc::PARENB | libc::CMSPAR) == (libc::PARENB | libc::CMSPAR) {
                return Err(UNSUPPORTED);
            }
            let bits = match flags & libc::CSIZE {
                libc::CS5 => 5,
                libc::CS6 => 6,
                libc::CS7 => 7,
                libc::CS8 => 8,
                _ => return Err(UNSUPPORTED),
            };
            data.push(match (flags & libc::CSTOPB != 0, bits) {
                (false, _) => 0,
                (true, 5) => 1,
                (true, _) => 2,
            });
            data.push(if flags & libc::PARENB == 0 {
                0
            } else if flags & libc::PARODD != 0 {
                1
            } else {
                2
            });
            data.push(bits);
        }
        0x001b0058 => {
            // Unix termios cannot implement arbitrary EOF/error/break substitution.
            if input[..4] != [0; 4] || input[4] == input[5] {
                return Err(UNSUPPORTED);
            }
            attributes.c_cc[libc::VSTART] = input[4];
            attributes.c_cc[libc::VSTOP] = input[5];
            set_attributes(file, &attributes)?;
        }
        0x001b005c => {
            data.extend([
                0,
                0,
                0,
                0,
                attributes.c_cc[libc::VSTART],
                attributes.c_cc[libc::VSTOP],
            ]);
        }
        0x001b0064 => {
            let handshake = u32_at(input, 0)?;
            let flow = u32_at(input, 4)?;
            // POSIX provides coupled RTS/CTS and software XON/XOFF, but no
            // independent DSR/DCD handshake or programmable queue thresholds.
            if ![0, 1, 9].contains(&handshake)
                || flow & !0xc3 != 0
                || ![0, 0x40, 0x80].contains(&(flow & 0xc0))
                || (handshake & 8 != 0) != (flow & 0xc0 == 0x80)
                || input[8..] != [0; 8]
            {
                return Err(UNSUPPORTED);
            }
            let old_modem = modem(file)?; // Prove support before any termios mutation.
            let old = attributes;
            attributes.c_iflag &= !(libc::IXON | libc::IXOFF | libc::IXANY);
            attributes.c_cflag &= !libc::CRTSCTS;
            if flow & 1 != 0 {
                attributes.c_iflag |= libc::IXON;
            }
            if flow & 2 != 0 {
                attributes.c_iflag |= libc::IXOFF;
            }
            if handshake & 8 != 0 {
                attributes.c_cflag |= libc::CRTSCTS;
            }
            let mut next_modem = old_modem & !(libc::TIOCM_DTR | libc::TIOCM_RTS);
            if handshake & 1 != 0 {
                next_modem |= libc::TIOCM_DTR;
            }
            if flow & 0xc0 == 0x40 {
                next_modem |= libc::TIOCM_RTS;
            }
            set_attributes(file, &attributes)?;
            let actual = get_attributes(file)?;
            let mask = libc::IXON | libc::IXOFF | libc::IXANY;
            if actual.c_iflag & mask != attributes.c_iflag & mask
                || actual.c_cflag & libc::CRTSCTS != attributes.c_cflag & libc::CRTSCTS
            {
                let _ = set_attributes(file, &old);
                return Err(UNSUPPORTED);
            }
            handle.modem_changed = true;
            // Let the kernel own RTS when hardware handshaking is selected.
            let mask = libc::TIOCM_DTR
                | if handshake & 8 == 0 {
                    libc::TIOCM_RTS
                } else {
                    0
                };
            if let Err(error) = change_modem(file, next_modem, mask) {
                let _ = set_attributes(file, &old);
                let _ = change_modem(file, old_modem, mask);
                return Err(error);
            }
        }
        0x001b0060 => {
            let modem = modem(file)?;
            let hardware = attributes.c_cflag & libc::CRTSCTS != 0;
            word(
                &mut data,
                u32::from(modem & libc::TIOCM_DTR != 0) | if hardware { 8 } else { 0 },
            );
            word(
                &mut data,
                u32::from(attributes.c_iflag & libc::IXON != 0)
                    | (u32::from(attributes.c_iflag & libc::IXOFF != 0) << 1)
                    | if hardware {
                        0x80
                    } else if modem & libc::TIOCM_RTS != 0 {
                        0x40
                    } else {
                        0
                    },
            );
            data.extend([0; 8]);
        }
        0x001b0068 => {
            let modem = modem(file)?;
            word(
                &mut data,
                if modem & libc::TIOCM_CTS != 0 {
                    0x10
                } else {
                    0
                } | if modem & libc::TIOCM_DSR != 0 {
                    0x20
                } else {
                    0
                } | if modem & libc::TIOCM_RI != 0 { 0x40 } else { 0 }
                    | if modem & libc::TIOCM_CD != 0 { 0x80 } else { 0 },
            );
        }
        0x001b0078 => {
            let modem = modem(file)?;
            word(
                &mut data,
                u32::from(modem & libc::TIOCM_DTR != 0)
                    | (u32::from(modem & libc::TIOCM_RTS != 0) << 1),
            );
        }
        0x001b0024 | 0x001b0028 | 0x001b0030 | 0x001b0034 => {
            let (mask, enabled) = match code {
                0x001b0024 => (libc::TIOCM_DTR, true),
                0x001b0028 => (libc::TIOCM_DTR, false),
                0x001b0030 => (libc::TIOCM_RTS, true),
                _ => (libc::TIOCM_RTS, false),
            };
            if mask == libc::TIOCM_RTS && attributes.c_cflag & libc::CRTSCTS != 0 {
                return Err(INVALID);
            }
            handle.modem_changed = true;
            change_modem(file, if enabled { mask } else { 0 }, mask)?;
        }
        0x001b0038 | 0x001b003c => {
            handle.flow_changed = true;
            let action = if code == 0x001b0038 {
                libc::TCOOFF
            } else {
                libc::TCOON
            };
            if unsafe { libc::tcflow(file.as_raw_fd(), action) } != 0 {
                return Err(tty_status(io::Error::last_os_error()));
            }
        }
        0x001b0010 | 0x001b0014 => {
            handle.break_changed = true;
            let operation = if code == 0x001b0010 {
                libc::TIOCSBRK
            } else {
                libc::TIOCCBRK
            };
            if unsafe { libc::ioctl(file.as_raw_fd(), operation as _) } != 0 {
                return Err(tty_status(io::Error::last_os_error()));
            }
        }
        _ => unreachable!(),
    }
    Ok(length_data(data))
}

fn modem(file: &File) -> Result<libc::c_int, u32> {
    let mut bits = 0;
    if unsafe { libc::ioctl(file.as_raw_fd(), libc::TIOCMGET, &mut bits) } != 0 {
        return Err(tty_status(io::Error::last_os_error()));
    }
    Ok(bits)
}
fn change_modem(file: &File, bits: libc::c_int, mask: libc::c_int) -> Result<(), u32> {
    for (operation, value) in [
        (libc::TIOCMBIS, bits & mask),
        (libc::TIOCMBIC, !bits & mask),
    ] {
        if value != 0 && unsafe { libc::ioctl(file.as_raw_fd(), operation, &value) } != 0 {
            return Err(tty_status(io::Error::last_os_error()));
        }
    }
    Ok(())
}

const SPEEDS: &[(u32, libc::speed_t)] = &[
    (50, libc::B50),
    (75, libc::B75),
    (110, libc::B110),
    (134, libc::B134),
    (150, libc::B150),
    (200, libc::B200),
    (300, libc::B300),
    (600, libc::B600),
    (1200, libc::B1200),
    (1800, libc::B1800),
    (2400, libc::B2400),
    (4800, libc::B4800),
    (9600, libc::B9600),
    (19200, libc::B19200),
    (38400, libc::B38400),
    (57600, libc::B57600),
    (115200, libc::B115200),
    (230400, libc::B230400),
];

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use crate::devices_tests::serial::pty;

    #[test]
    fn expired_serial_operations_leave_pty_data_untouched() {
        let (mut master, slave, _) = pty();
        let mut attributes = get_attributes(&slave).unwrap();
        unsafe { libc::cfmakeraw(&mut attributes) };
        set_attributes(&slave, &attributes).unwrap();
        let mut handle = Handle {
            id: 1,
            file: slave,
            reads: true,
            writes: true,
            original: None,
            original_modem: None,
            modem_changed: false,
            flow_changed: false,
            break_changed: false,
            timeouts: [u32::MAX, u32::MAX, 1000, 0, 1000],
        };
        let mut pending = Pending {
            request: Vec::new(),
            device: 1,
            handle: 1,
            transfer: Transfer::Write {
                data: b"lost".to_vec(),
                written: 1,
            },
            deadline: Some(Instant::now() - Duration::from_secs(1)),
            started: true,
            received: 0,
            last_byte: None,
            timeouts: [u32::MAX, u32::MAX, 1000, 0, 1000],
        };
        assert_eq!(pending.try_io(&mut handle), Some(Ok(vec![1, 0, 0, 0, 0])));
        let mut bytes = [0; 4];
        assert_eq!(
            master.read(&mut bytes).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        master.write_all(b"kept").unwrap();
        pending.transfer = Transfer::Read(vec![0; 4]);
        assert_eq!(pending.try_io(&mut handle), Some(Ok(vec![0; 4])));
        handle.file.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"kept");
    }

    #[test]
    fn serial_peer_hangup_is_not_a_successful_empty_read() {
        let (master, slave, _) = pty();
        let mut handle = Handle {
            id: 1,
            file: slave,
            reads: true,
            writes: false,
            original: None,
            original_modem: None,
            modem_changed: false,
            flow_changed: false,
            break_changed: false,
            timeouts: [u32::MAX, u32::MAX, 1000, 0, 1000],
        };
        drop(master);
        let mut pending = Pending {
            request: Vec::new(),
            device: 1,
            handle: 1,
            transfer: Transfer::Read(vec![0; 1]),
            deadline: Some(Instant::now() + IO_LIMIT),
            started: true,
            received: 0,
            last_byte: None,
            timeouts: [u32::MAX, u32::MAX, 1000, 0, 1000],
        };
        assert_eq!(pending.try_io(&mut handle), Some(Err(DISCONNECTED)));
        pending.transfer = Transfer::Read(Vec::new());
        assert_eq!(pending.try_io(&mut handle), Some(Ok(vec![0; 4])));
    }
}
