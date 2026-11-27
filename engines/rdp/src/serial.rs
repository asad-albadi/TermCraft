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
const PURGE: u32 = 0x001b004c;

const GET_WAIT_MASK: u32 = 0x001b0040;
const SET_WAIT_MASK: u32 = 0x001b0044;
const WAIT_ON_MASK: u32 = 0x001b0048;
const GET_STATS: u32 = 0x001b008c;
const RXCHAR: u32 = 1;
// Linux exposes receive error/break counters through nonblocking TIOCGICOUNT.
// Reading modem counters does not arm modem-status interrupts. Never accept
// CTS/DSR/RLSD/RING without a separately established bounded capture provider.
// TXEMPTY requires physical transmitter status; a software queue count is insufficient.
const COUNTER_EVENTS: u32 = if cfg!(target_os = "linux") { 0x00c0 } else { 0 };

// Admission recognizes only a well-shaped, supported Unix mask set. The worker
// still validates the handle and access and remains the sole owner of TTY state.
pub(super) fn is_wait_mask_set(body: &[u8]) -> bool {
    body.len() == 36
        && u32_at(body, 0).is_ok_and(|size| size as usize <= MAX_IO)
        && u32_at(body, 4) == Ok(4)
        && u32_at(body, 8) == Ok(SET_WAIT_MASK)
        && u32_at(body, 32).is_ok_and(|mask| mask & !(RXCHAR | COUNTER_EVENTS) == 0)
}

#[derive(Default)]
struct Events {
    mask: u32,
    queued: usize,
    received: bool,
    counter_bits: u32,
    counters: Option<[u32; 4]>,
    wait: Option<Vec<u8>>,
}

fn queued_input(file: &File) -> Result<usize, u32> {
    let mut count: libc::c_int = 0;
    if unsafe { libc::ioctl(file.as_raw_fd(), libc::FIONREAD as _, &mut count) } < 0 {
        return Err(tty_status(io::Error::last_os_error()));
    }
    usize::try_from(count).map_err(|_| INVALID)
}

impl Events {
    fn observe(&mut self, file: &File) -> Result<(), u32> {
        if self.mask & RXCHAR != 0 {
            let current = queued_input(file)?;
            self.received |= current > self.queued;
            self.queued = current;
        }
        if let Some(previous) = self.counters {
            let current = event_counters(file)?;
            // The driver's receive path records these errors/breaks. Merely
            // reading a modem snapshot does not establish modem edge capture.
            // A wrapping u32 increment is still a change. Polling coalesces events.
            let changed = |index: usize| current[index].wrapping_sub(previous[index]) != 0;
            if changed(3) {
                self.counter_bits |= 64 & self.mask;
            }
            // EV_ERR is framing, hardware overrun or parity, not RX/TX counts,
            // BREAK, software buffer overrun or any reserved UAPI field.
            if (0..=2).any(changed) {
                self.counter_bits |= 128 & self.mask;
            }
            self.counters = Some(current);
        }
        Ok(())
    }

    fn consumed(&mut self, count: usize) {
        if self.mask & RXCHAR != 0 {
            self.received |= count > self.queued;
            self.queued = self.queued.saturating_sub(count);
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
thread_local! {
    // Per-test-thread response at the native counter boundary. Real PTY tests leave it unset.
    static FIXTURE_COUNTERS: std::cell::Cell<Option<[libc::c_int; 20]>> = const { std::cell::Cell::new(None) };
}

type DriverCounters = [u32; 6];

#[cfg(target_os = "linux")]
fn native_counters(file: &File) -> Result<[libc::c_int; 20], u32> {
    #[cfg(test)]
    if let Some(counts) = FIXTURE_COUNTERS.get() {
        return Ok(counts);
    }
    // linux/serial.h serial_icounter_struct consists of 11 int fields and 9 reserved ints.
    let mut counts = [0 as libc::c_int; 20];
    if unsafe {
        libc::ioctl(
            file.as_raw_fd(),
            libc::TIOCGICOUNT as _,
            counts.as_mut_ptr(),
        )
    } < 0
    {
        return Err(tty_status(io::Error::last_os_error()));
    }
    Ok(counts)
}

#[cfg(target_os = "linux")]
fn driver_counters(file: &File) -> Result<DriverCounters, u32> {
    native_counters(file).map(|counts| select_counters(&counts))
}

#[cfg(target_os = "linux")]
fn event_counters(file: &File) -> Result<[u32; 4], u32> {
    // Only framing, hardware overrun, parity and input break. No modem counters.
    native_counters(file).map(|counts| [6, 7, 8, 9].map(|index| counts[index] as u32))
}

#[cfg(not(target_os = "linux"))]
fn event_counters(_: &File) -> Result<[u32; 4], u32> {
    Err(UNSUPPORTED)
}

#[cfg(target_os = "linux")]
fn select_counters(counts: &[libc::c_int; 20]) -> DriverCounters {
    // SERIALPERF_STATS: rx, tx, frame, overrun, buffer overrun, parity.
    [4, 5, 6, 7, 10, 8].map(|index| counts[index] as u32)
}

#[cfg(not(target_os = "linux"))]
fn driver_counters(_: &File) -> Result<DriverCounters, u32> {
    Err(UNSUPPORTED)
}

fn encode_stats(baseline: DriverCounters, current: DriverCounters) -> Vec<u8> {
    let mut data = Vec::with_capacity(24);
    for (before, after) in baseline.into_iter().zip(current) {
        word(&mut data, after.wrapping_sub(before));
    }
    length_data(data)
}

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
    events: Events,
    stats_baseline: Option<DriverCounters>,
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
        // The binding pins the native open across logical RDP opens.
        let stats_baseline = driver_counters(&file).ok();
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
            events: Events::default(),
            stats_baseline,
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
    cancelled: bool,
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

    // Keep deadline decisions separate from native I/O. Both read timeout modes
    // return the bytes already received without touching the descriptor again.
    fn complete_expired(&mut self, now: Instant) -> Option<Result<Vec<u8>, u32>> {
        let interval_expired = self.last_byte.is_some_and(|last| {
            self.timeouts[0] != 0
                && self.timeouts[0] != u32::MAX
                && now.duration_since(last) >= Duration::from_millis(self.timeouts[0] as u64)
        });
        if self.deadline.is_some_and(|deadline| now >= deadline) || interval_expired {
            Some(match &mut self.transfer {
                Transfer::Read(buffer) => {
                    buffer.truncate(self.received);
                    Ok(length_data(std::mem::take(buffer)))
                }
                Transfer::Write { written, .. } if *written != 0 => Ok(write_count(*written)),
                Transfer::Write { .. } => Err(IO_TIMEOUT),
            })
        } else {
            None
        }
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
        if let Some(result) = self.complete_expired(now) {
            return Some(result);
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
                    handle.events.consumed(n);
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
    finished_waits: Vec<Vec<u8>>,
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
            finished_waits: Vec::new(),
        }
    }

    pub(super) fn contains(&self, device: u32) -> bool {
        self.ports.contains_key(&device)
    }

    pub(super) fn has_pending(&self) -> bool {
        !self.pending.is_empty()
            || !self.finished_waits.is_empty()
            || self.ports.values().any(|port| {
                port.handle
                    .as_ref()
                    .is_some_and(|h| h.events.wait.is_some())
            })
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
                    if let Some(wait) = handle.events.wait.take() {
                        self.finished_waits.push(complete(&wait, Err(CANCELLED)));
                    }
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
                            cancelled: false,
                            received: 0,
                            last_byte: None,
                            timeouts: handle.timeouts,
                        });
                        return None;
                    }
                    Err(error) => Err(error),
                },
                14 => {
                    let result = control(handle, body);
                    if result.is_ok() {
                        match u32_at(body, 8) {
                            Ok(SET_WAIT_MASK) => {
                                if let Some(wait) = handle.events.wait.take() {
                                    self.finished_waits.push(complete(
                                        &wait,
                                        Ok(length_data(0u32.to_le_bytes().to_vec())),
                                    ));
                                }
                            }
                            Ok(WAIT_ON_MASK) => {
                                handle.events.wait = Some(request);
                                return None;
                            }
                            _ => {}
                        }
                    }
                    if result.is_ok() && u32_at(body, 8) == Ok(PURGE) {
                        // Only requests already admitted on this exact remote handle
                        // are aborted. No native I/O remains in flight on Unix.
                        let flags = u32_at(body, 32).unwrap();
                        for job in &mut self.pending {
                            let abort = match job.transfer {
                                Transfer::Read(_) => flags & 2 != 0,
                                Transfer::Write { .. } => flags & 1 != 0,
                            };
                            if job.device == device && job.handle == id && abort {
                                job.cancelled = true;
                            }
                        }
                    }
                    result
                }
                _ => Err(UNSUPPORTED),
            },
            _ => Err(INVALID_HANDLE),
        };
        Some(complete(&request, result))
    }

    pub(super) fn ready(&mut self) -> Vec<Vec<u8>> {
        let mut responses = std::mem::take(&mut self.finished_waits);
        for handle in self.ports.values_mut().filter_map(|p| p.handle.as_mut()) {
            if handle.events.wait.is_none() {
                continue;
            }
            let result = match has_hung_up(&handle.file) {
                Ok(true) => Err(DISCONNECTED),
                Err(err) => Err(tty_status(err)),
                Ok(false) => handle.events.observe(&handle.file),
            };
            if result.is_err() || handle.events.received || handle.events.counter_bits != 0 {
                let bits =
                    handle.events.counter_bits | if handle.events.received { RXCHAR } else { 0 };
                let body = result.map(|()| length_data(bits.to_le_bytes().to_vec()));
                responses.push(complete(&handle.events.wait.take().unwrap(), body));
                handle.events.received = false;
                handle.events.counter_bits = 0;
            }
        }
        // Preserve per-port order within each direction. A read waiting for data
        // can run concurrently with writes and controls on the same port.
        let mut waiting = BTreeSet::new();
        for _ in 0..self.pending.len() {
            let mut job = self.pending.pop_front().unwrap();
            let direction = matches!(job.transfer, Transfer::Read(_));
            let port = &mut self.ports.get_mut(&job.device).unwrap().handle;
            let result = match port.as_mut().filter(|h| h.id == job.handle) {
                _ if job.cancelled => Some(Err(CANCELLED)),
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

fn purge_queue(flags: u32) -> Result<Option<libc::c_int>, u32> {
    // SERIAL_PURGE_TXABORT/RXABORT affect the protocol request queue;
    // TXCLEAR/RXCLEAR discard only bytes still queued in the opened TTY.
    if flags == 0 || flags & !0x0f != 0 {
        return Err(INVALID);
    }
    Ok(match flags & 0x0c {
        0x04 => Some(libc::TCOFLUSH),
        0x08 => Some(libc::TCIFLUSH),
        0x0c => Some(libc::TCIOFLUSH),
        _ => None,
    })
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
        SET_BAUD | PURGE | SET_WAIT_MASK => (4, 0),
        GET_BAUD | GET_WAIT_MASK | WAIT_ON_MASK => (0, 4),
        GET_STATS => (0, 24),
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
        GET_WAIT_MASK
            | WAIT_ON_MASK
            | GET_STATS
            | GET_BAUD
            | GET_LINE
            | 0x001b0020
            | 0x001b005c
            | 0x001b0060
            | 0x001b0068
            | 0x001b0078
    );
    if !query && !handle.writes {
        return Err(DENIED);
    }
    match code {
        GET_WAIT_MASK => return Ok(length_data(handle.events.mask.to_le_bytes().to_vec())),
        SET_WAIT_MASK => {
            let mask = u32_at(input, 0)?;
            if mask & !0x1fff != 0 {
                return Err(INVALID);
            }
            // Successful CREATE counter probing fixes this logical handle's
            // capabilities; later successful queries cannot promote a PTY.
            let supported = RXCHAR
                | if handle.stats_baseline.is_some() {
                    COUNTER_EVENTS
                } else {
                    0
                };
            if mask & !supported != 0 {
                return Err(UNSUPPORTED);
            }
            let counters = if mask & COUNTER_EVENTS != 0 {
                Some(event_counters(&handle.file)?)
            } else {
                None
            };
            let queued = if mask & RXCHAR != 0 {
                queued_input(&handle.file)?
            } else {
                0
            };
            // Commit only after every required query succeeds. A rejected SET
            // leaves the old mask, origin and pending wait unchanged.
            handle.events.mask = mask;
            handle.events.queued = queued;
            handle.events.received = false;
            handle.events.counter_bits = 0;
            handle.events.counters = counters;
            return Ok(length_data(Vec::new()));
        }
        WAIT_ON_MASK => {
            if !handle.reads {
                return Err(DENIED);
            }
            if handle.events.mask == 0 || handle.events.wait.is_some() {
                return Err(INVALID);
            }
            return Ok(Vec::new()); // process retains the request until ready.
        }
        GET_STATS => {
            let baseline = handle.stats_baseline.ok_or(UNSUPPORTED)?;
            return Ok(encode_stats(baseline, driver_counters(&handle.file)?));
        }
        _ => {}
    }
    if code == PURGE {
        if let Some(queue) = purge_queue(u32_at(input, 0)?)? {
            // Preserve a previously arrived RXCHAR across a successful input flush.
            let clears_input = queue == libc::TCIFLUSH || queue == libc::TCIOFLUSH;
            let before = if clears_input && handle.events.mask & RXCHAR != 0 {
                queued_input(&handle.file).ok()
            } else {
                None
            };
            if unsafe { libc::tcflush(handle.file.as_raw_fd(), queue) } != 0 {
                return Err(tty_status(io::Error::last_os_error()));
            }
            if clears_input {
                handle.events.received |= before.is_some_and(|n| n > handle.events.queued);
                handle.events.queued = 0;
            }
        }
        return Ok(length_data(Vec::new()));
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
            events: Events::default(),
            stats_baseline: None,
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
            cancelled: false,
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
    fn partial_serial_reads_complete_at_interval_and_total_timeout_boundaries() {
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
            events: Events::default(),
            stats_baseline: None,
            timeouts: [25, 0, 250, 0, 250],
        };
        let mut pending = Pending {
            request: Vec::new(),
            device: 1,
            handle: 1,
            transfer: Transfer::Read(vec![0; 4]),
            deadline: None,
            started: false,
            cancelled: false,
            received: 0,
            last_byte: None,
            timeouts: handle.timeouts,
        };
        master.write_all(b"ab").unwrap();
        let limit = Instant::now() + Duration::from_secs(2);
        while pending.received != 2 {
            assert!(pending.try_io(&mut handle).is_none());
            assert!(
                Instant::now() < limit,
                "partial bytes did not reach the PTY"
            );
            std::thread::yield_now();
        }
        let last_byte = pending.last_byte.unwrap();
        // Exercise exact instants, independent of the test thread's scheduling.
        pending.deadline = Some(last_byte + Duration::from_millis(250));
        assert_eq!(
            pending.complete_expired(last_byte + Duration::from_millis(24)),
            None
        );
        assert_eq!(pending.received, 2);
        assert_eq!(
            pending.complete_expired(last_byte + Duration::from_millis(25)),
            Some(Ok(length_data(b"ab".to_vec())))
        );

        // Disabling the inter-byte interval leaves the total timeout in force.
        pending.transfer = Transfer::Read(b"ab\0\0".to_vec());
        pending.timeouts[0] = 0;
        assert_eq!(
            pending.complete_expired(last_byte + Duration::from_millis(249)),
            None
        );
        assert_eq!(
            pending.complete_expired(last_byte + Duration::from_millis(250)),
            Some(Ok(length_data(b"ab".to_vec())))
        );
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
            events: Events::default(),
            stats_baseline: None,
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
            cancelled: false,
            received: 0,
            last_byte: None,
            timeouts: [u32::MAX, u32::MAX, 1000, 0, 1000],
        };
        assert_eq!(pending.try_io(&mut handle), Some(Err(DISCONNECTED)));
        pending.transfer = Transfer::Read(Vec::new());
        assert_eq!(pending.try_io(&mut handle), Some(Ok(vec![0; 4])));
    }
    fn purge_body(flags: u32) -> Vec<u8> {
        let mut body = vec![0; 32];
        body[4..8].copy_from_slice(&4u32.to_le_bytes());
        body[8..12].copy_from_slice(&0x001b004cu32.to_le_bytes());
        body.extend(flags.to_le_bytes());
        body
    }

    fn event_request(
        device: u32,
        handle: u32,
        completion: u32,
        major: u32,
        body: &[u8],
    ) -> Vec<u8> {
        let mut request = Vec::new();
        for value in [0, device, handle, completion, major, 0] {
            word(&mut request, value);
        }
        request.extend(body);
        request
    }

    fn purge_fixture_access(read_only: bool) -> (File, File, Serial, u32) {
        let (master, slave, path) = pty();
        let bindings = bind(&[SerialMapping {
            name: "COM1".into(),
            path,
            read_only,
        }])
        .unwrap();
        let mut serial = Serial::new(1, bindings);
        let mut body = vec![0; 32];
        body[..4].copy_from_slice(
            &(if read_only {
                0x80000000u32
            } else {
                0xc0000000u32
            })
            .to_le_bytes(),
        );
        body[20..24].copy_from_slice(&1u32.to_le_bytes());
        let opened = serial.process(event_request(1, 0, 1, 0, &body)).unwrap();
        assert_eq!(u32_at(&opened, 12), Ok(0));
        let handle = u32_at(&opened, 16).unwrap();
        (master, slave, serial, handle)
    }

    fn purge_fixture() -> (File, File, Serial, u32) {
        purge_fixture_access(false)
    }

    fn queue_transfer(serial: &mut Serial, handle: u32, completion: u32, write: bool) {
        let mut body = vec![0; 32];
        body[..4].copy_from_slice(&4u32.to_le_bytes());
        if write {
            body.extend(b"kept");
        }
        assert!(
            serial
                .process(event_request(
                    1,
                    handle,
                    completion,
                    if write { 4 } else { 3 },
                    &body
                ))
                .is_none()
        );
    }

    #[test]
    fn serial_purge_flag_validation_precedes_any_queue_side_effect() {
        let (_master, _slave, mut serial, handle) = purge_fixture();
        for flags in [0, 16, 0x80000008, u32::MAX] {
            let reply = serial
                .process(event_request(1, handle, 2, 14, &purge_body(flags)))
                .unwrap();
            assert_eq!(u32_at(&reply, 12), Ok(INVALID), "flags {flags:#x}");
        }
        let mut malformed = purge_body(8);
        malformed.pop();
        let reply = serial
            .process(event_request(1, handle, 3, 14, &malformed))
            .unwrap();
        assert_eq!(u32_at(&reply, 12), Ok(INVALID));
    }

    #[test]
    fn serial_purge_rxclear_discards_actual_unread_pty_bytes() {
        let (mut master, slave, mut serial, handle) = purge_fixture();
        master.write_all(b"gone").unwrap();
        let deadline = Instant::now() + IO_LIMIT;
        loop {
            let mut queued: libc::c_int = 0;
            assert_eq!(
                unsafe { libc::ioctl(slave.as_raw_fd(), libc::FIONREAD, &mut queued) },
                0
            );
            if queued == 4 {
                break;
            }
            assert!(Instant::now() < deadline, "PTY input did not arrive");
            std::thread::yield_now();
        }
        let reply = serial
            .process(event_request(1, handle, 2, 14, &purge_body(8)))
            .unwrap();
        assert_eq!(u32_at(&reply, 12), Ok(0));
        let file = &mut serial
            .ports
            .get_mut(&1)
            .unwrap()
            .handle
            .as_mut()
            .unwrap()
            .file;
        assert_eq!(
            file.read(&mut [0; 4]).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        master.write_all(b"kept").unwrap();
        queue_transfer(&mut serial, handle, 3, false);
        let deadline = Instant::now() + IO_LIMIT;
        loop {
            let replies = serial.ready();
            if !replies.is_empty() {
                assert_eq!(&replies[0][16..], &[4, 0, 0, 0, b'k', b'e', b'p', b't']);
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
    }

    #[test]
    fn serial_purge_abort_cancels_only_selected_direction_and_existing_requests() {
        for (flags, cancel_write) in [(1, true), (2, false), (3, true)] {
            let (mut master, _slave, mut serial, handle) = purge_fixture();
            queue_transfer(&mut serial, handle, 20, false);
            queue_transfer(&mut serial, handle, 21, false);
            queue_transfer(&mut serial, handle, 22, true);
            let reply = serial
                .process(event_request(1, handle, 2, 14, &purge_body(flags)))
                .unwrap();
            assert_eq!(u32_at(&reply, 12), Ok(0));
            let replies = serial.ready();
            let cancelled: Vec<_> = replies
                .iter()
                .filter(|r| u32_at(r, 12) == Ok(CANCELLED))
                .map(|r| u32_at(r, 8).unwrap())
                .collect();
            assert_eq!(
                cancelled,
                if flags == 3 {
                    vec![20, 21, 22]
                } else if cancel_write {
                    vec![22]
                } else {
                    vec![20, 21]
                }
            );
            if flags == 1 {
                master.write_all(b"readmore").unwrap();
                let deadline = Instant::now() + IO_LIMIT;
                let mut count = 0;
                while count < 2 {
                    for reply in serial.ready() {
                        assert_eq!(u32_at(&reply, 12), Ok(0));
                        count += 1;
                    }
                    assert!(Instant::now() < deadline);
                    std::thread::yield_now();
                }
            } else if flags == 2 {
                assert!(
                    replies
                        .iter()
                        .any(|r| u32_at(r, 8) == Ok(22) && u32_at(r, 12) == Ok(0))
                );
            }
            // An abort affects the requests already present, never later work.
            queue_transfer(&mut serial, handle, 30, true);
            let replies = serial.ready();
            assert!(
                replies
                    .iter()
                    .any(|r| u32_at(r, 8) == Ok(30) && u32_at(r, 12) == Ok(0))
            );
        }
    }

    #[test]
    fn serial_purge_denies_readonly_and_stale_handle_without_cancelling_reads() {
        let (_master, _slave, mut serial, handle) = purge_fixture();
        queue_transfer(&mut serial, handle, 20, false);
        let reply = serial
            .process(event_request(1, handle + 1, 2, 14, &purge_body(15)))
            .unwrap();
        assert_eq!(u32_at(&reply, 12), Ok(INVALID_HANDLE));
        assert!(serial.ready().is_empty());
        assert_eq!(serial.pending.len(), 1);
        let (_master, _slave, mut serial, handle) = purge_fixture_access(true);
        queue_transfer(&mut serial, handle, 20, false);
        let reply = serial
            .process(event_request(1, handle, 3, 14, &purge_body(15)))
            .unwrap();
        assert_eq!(u32_at(&reply, 12), Ok(DENIED));
        assert!(serial.ready().is_empty());
        assert_eq!(serial.pending.len(), 1);
    }

    #[test]
    fn serial_purge_clear_does_not_abort_admitted_requests_or_partial_read() {
        let (mut master, _slave, mut serial, handle) = purge_fixture();
        serial
            .ports
            .get_mut(&1)
            .unwrap()
            .handle
            .as_mut()
            .unwrap()
            .timeouts = [0, 0, 1000, 0, 1000];
        // Part of the read is already in its owned protocol buffer.
        master.write_all(b"ha").unwrap();
        queue_transfer(&mut serial, handle, 20, false);
        let deadline = Instant::now() + IO_LIMIT;
        loop {
            assert!(serial.ready().is_empty());
            if serial.pending.front().unwrap().received == 2 {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        let reply = serial
            .process(event_request(1, handle, 2, 14, &purge_body(12)))
            .unwrap();
        assert_eq!(u32_at(&reply, 12), Ok(0));
        assert!(serial.ready().is_empty());
        master.write_all(b"nd").unwrap();
        let deadline = Instant::now() + IO_LIMIT;
        loop {
            let replies = serial.ready();
            if !replies.is_empty() {
                assert_eq!(u32_at(&replies[0], 12), Ok(0));
                assert_eq!(&replies[0][20..], b"hand");
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        // A write not yet handed to the terminal survives TXCLEAR.
        queue_transfer(&mut serial, handle, 30, true);
        let reply = serial
            .process(event_request(1, handle, 3, 14, &purge_body(4)))
            .unwrap();
        assert_eq!(u32_at(&reply, 12), Ok(0));
        let replies = serial.ready();
        assert_eq!(replies.len(), 1);
        assert_eq!(u32_at(&replies[0], 12), Ok(0));
        let mut bytes = [0; 4];
        master.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"kept");
    }

    #[test]
    fn serial_purge_abort_preserves_completed_write_result_and_unread_input() {
        let (mut master, _slave, mut serial, handle) = purge_fixture();
        queue_transfer(&mut serial, handle, 20, true);
        let completed = serial.ready();
        assert_eq!(completed.len(), 1);
        assert_eq!(&completed[0][12..], &[0, 0, 0, 0, 4, 0, 0, 0, 0]);
        master.write_all(b"kept").unwrap();
        queue_transfer(&mut serial, handle, 21, false);
        let reply = serial
            .process(event_request(1, handle, 2, 14, &purge_body(3)))
            .unwrap();
        assert_eq!(u32_at(&reply, 12), Ok(0));
        let cancelled = serial.ready();
        assert_eq!(cancelled.len(), 1);
        assert_eq!(u32_at(&cancelled[0], 8), Ok(21));
        assert_eq!(u32_at(&cancelled[0], 12), Ok(CANCELLED));
        queue_transfer(&mut serial, handle, 22, false);
        let deadline = Instant::now() + IO_LIMIT;
        loop {
            let replies = serial.ready();
            if !replies.is_empty() {
                assert_eq!(u32_at(&replies[0], 12), Ok(0));
                assert_eq!(&replies[0][20..], b"kept");
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(serial.ready().is_empty());
        assert_eq!(&completed[0][12..], &[0, 0, 0, 0, 4, 0, 0, 0, 0]);
    }

    #[test]
    fn serial_purge_does_not_touch_another_opened_pty_or_its_pending_read() {
        let (mut master, _slave, mut serial, handle) = purge_fixture();
        let (mut other_master, _other_slave, path) = pty();
        let binding = bind(&[SerialMapping {
            name: "COM2".into(),
            path,
            read_only: false,
        }])
        .unwrap()
        .remove(0);
        serial.ports.insert(
            2,
            Port {
                binding,
                handle: None,
            },
        );
        let mut body = vec![0; 32];
        body[..4].copy_from_slice(&0xc0000000u32.to_le_bytes());
        body[20..24].copy_from_slice(&1u32.to_le_bytes());
        let reply = serial.process(event_request(2, 0, 2, 0, &body)).unwrap();
        assert_eq!(u32_at(&reply, 12), Ok(0));
        let other_handle = u32_at(&reply, 16).unwrap();
        body[..4].copy_from_slice(&4u32.to_le_bytes());
        assert!(
            serial
                .process(event_request(2, other_handle, 40, 3, &body))
                .is_none()
        );
        queue_transfer(&mut serial, handle, 20, false);
        master.write_all(b"gone").unwrap();
        other_master.write_all(b"kept").unwrap();
        let reply = serial
            .process(event_request(1, handle, 3, 14, &purge_body(15)))
            .unwrap();
        assert_eq!(u32_at(&reply, 12), Ok(0));
        let mut got = Vec::new();
        let deadline = Instant::now() + IO_LIMIT;
        while got.len() < 2 {
            got.extend(serial.ready());
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        let cancelled = got.iter().find(|r| u32_at(r, 8) == Ok(20)).unwrap();
        assert_eq!(u32_at(cancelled, 12), Ok(CANCELLED));
        let kept = got.iter().find(|r| u32_at(r, 8) == Ok(40)).unwrap();
        assert_eq!(u32_at(kept, 12), Ok(0));
        assert_eq!(&kept[20..], b"kept");
        assert!(serial.ready().is_empty());
    }

    #[test]
    fn serial_purge_maps_all_valid_queue_and_abort_flag_combinations() {
        for (flags, queue) in [
            (1, None),
            (2, None),
            (3, None),
            (4, Some(libc::TCOFLUSH)),
            (5, Some(libc::TCOFLUSH)),
            (6, Some(libc::TCOFLUSH)),
            (7, Some(libc::TCOFLUSH)),
            (8, Some(libc::TCIFLUSH)),
            (9, Some(libc::TCIFLUSH)),
            (10, Some(libc::TCIFLUSH)),
            (11, Some(libc::TCIFLUSH)),
            (12, Some(libc::TCIOFLUSH)),
            (13, Some(libc::TCIOFLUSH)),
            (14, Some(libc::TCIOFLUSH)),
            (15, Some(libc::TCIOFLUSH)),
        ] {
            assert_eq!(purge_queue(flags), Ok(queue), "flags {flags:#x}");
        }
        for flags in [0, 16, 0x80000008, u32::MAX] {
            assert_eq!(purge_queue(flags), Err(INVALID));
        }
    }
    fn event_control(
        serial: &mut Serial,
        handle: u32,
        tag: u32,
        code: u32,
        input: &[u8],
        cap: u32,
    ) -> Option<Vec<u8>> {
        let mut body = vec![0; 32];
        body[0..4].copy_from_slice(&cap.to_le_bytes());
        body[4..8].copy_from_slice(&(input.len() as u32).to_le_bytes());
        body[8..12].copy_from_slice(&code.to_le_bytes());
        body.extend(input);
        serial.process(event_request(1, handle, tag, 14, &body))
    }

    #[test]
    fn unix_event_wait_tracks_new_pty_input_once_without_consuming_it() {
        let (mut master, _slave, mut serial, h) = purge_fixture();
        let set = event_control(&mut serial, h, 2, 0x1b0044, &1u32.to_le_bytes(), 0).unwrap();
        assert_eq!(u32_at(&set, 12), Ok(0));
        let get = event_control(&mut serial, h, 3, 0x1b0040, &[], 4).unwrap();
        assert_eq!(&get[16..], &[4, 0, 0, 0, 1, 0, 0, 0]);
        assert!(event_control(&mut serial, h, 4, 0x1b0048, &[], 4).is_none());
        assert!(serial.has_pending());
        assert!(serial.ready().is_empty());
        let duplicate = event_control(&mut serial, h, 5, 0x1b0048, &[], 4).unwrap();
        assert_eq!(u32_at(&duplicate, 12), Ok(INVALID));
        master.write_all(b"x").unwrap();
        let deadline = Instant::now() + IO_LIMIT;
        loop {
            if let Some(reply) = serial.ready().pop() {
                assert_eq!(u32_at(&reply, 8), Ok(4));
                assert_eq!(&reply[16..], &[4, 0, 0, 0, 1, 0, 0, 0]);
                break;
            }
            assert!(Instant::now() < deadline);
        }
        assert!(event_control(&mut serial, h, 6, 0x1b0048, &[], 4).is_none());
        assert!(serial.ready().is_empty());
        let reset = event_control(&mut serial, h, 7, 0x1b0044, &0u32.to_le_bytes(), 0).unwrap();
        assert_eq!(u32_at(&reset, 12), Ok(0));
        let completed = serial.ready().pop().unwrap();
        assert_eq!(u32_at(&completed, 8), Ok(6));
        assert_eq!(&completed[16..], &[4, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(
            u32_at(
                &event_control(&mut serial, h, 8, 0x1b0048, &[], 4).unwrap(),
                12
            ),
            Ok(INVALID)
        );
        let mut value = [0; 1];
        serial
            .ports
            .get_mut(&1)
            .unwrap()
            .handle
            .as_mut()
            .unwrap()
            .file
            .read_exact(&mut value)
            .unwrap();
        assert_eq!(&value, b"x");
    }

    #[test]
    fn unix_event_controls_validate_wire_access_and_stale_id() {
        let (_master, _slave, mut serial, h) = purge_fixture_access(true);
        for (id, code, input, cap, expected) in [
            (h, 0x1b0040, vec![], 3, BUFFER_TOO_SMALL),
            (h, 0x1b0040, vec![0], 4, INVALID),
            (h, 0x1b0044, vec![1, 0, 0, 0], 0, DENIED),
            (h + 1, 0x1b0040, vec![], 4, INVALID_HANDLE),
            (h, 0x1b008c, vec![], 23, BUFFER_TOO_SMALL),
            (h, 0x1b008c, vec![], 24, UNSUPPORTED),
        ] {
            let reply = event_control(&mut serial, id, 2, code, &input, cap).unwrap();
            assert_eq!(u32_at(&reply, 12), Ok(expected));
        }
    }

    #[test]
    fn unix_event_close_and_hangup_retire_only_the_admitted_wait() {
        for disconnect in [false, true] {
            let (master, _slave, mut serial, h) = purge_fixture();
            assert_eq!(
                u32_at(
                    &event_control(&mut serial, h, 2, 0x1b0044, &1u32.to_le_bytes(), 0).unwrap(),
                    12
                ),
                Ok(0)
            );
            assert!(event_control(&mut serial, h, 3, 0x1b0048, &[], 4).is_none());
            for (value, error) in [(4u32, UNSUPPORTED), (8192, INVALID)] {
                assert_eq!(
                    u32_at(
                        &event_control(&mut serial, h, 4, 0x1b0044, &value.to_le_bytes(), 0)
                            .unwrap(),
                        12
                    ),
                    Ok(error)
                );
                assert!(serial.ready().is_empty());
            }
            if disconnect {
                drop(master);
            } else {
                assert_eq!(
                    u32_at(
                        &serial.process(event_request(1, h, 5, 2, &[0; 32])).unwrap(),
                        12
                    ),
                    Ok(0)
                );
            }
            let replies = serial.ready();
            assert_eq!(replies.len(), 1);
            assert_eq!(u32_at(&replies[0], 8), Ok(3));
            assert_eq!(
                u32_at(&replies[0], 12),
                Ok(if disconnect { DISCONNECTED } else { CANCELLED })
            );
            assert!(!serial.has_pending());
        }
    }

    #[test]
    fn unix_event_remembers_input_consumed_by_a_protocol_read_before_wait() {
        let (mut master, _slave, mut serial, h) = purge_fixture();
        assert_eq!(
            u32_at(
                &event_control(&mut serial, h, 2, 0x1b0044, &1u32.to_le_bytes(), 0).unwrap(),
                12
            ),
            Ok(0)
        );
        queue_transfer(&mut serial, h, 3, false);
        master.write_all(b"q").unwrap();
        let deadline = Instant::now() + IO_LIMIT;
        loop {
            if let Some(reply) = serial.ready().pop() {
                assert_eq!(&reply[20..], b"q");
                break;
            }
            assert!(Instant::now() < deadline);
        }
        assert!(event_control(&mut serial, h, 4, 0x1b0048, &[], 4).is_none());
        assert_eq!(u32_at(&serial.ready()[0], 20), Ok(1));
        assert!(event_control(&mut serial, h, 5, 0x1b0048, &[], 4).is_none());
        assert!(serial.ready().is_empty());
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn unix_event_stats_use_kernel_uapi_field_order_and_bit_patterns() {
        let mut counters = [0; 20];
        for (i, field) in counters.iter_mut().enumerate() {
            *field = i as i32;
        }
        counters[4] = -1;
        let data = encode_stats([0; 6], select_counters(&counters));
        assert_eq!(data.len(), 28);
        for (i, value) in [24, u32::MAX, 5, 6, 7, 10, 8].into_iter().enumerate() {
            assert_eq!(u32_at(&data, 4 * i), Ok(value));
        }
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn unix_event_driver_stats_start_at_each_logical_create() {
        let mut counts = [100i32; 20];
        counts[4] = -2;
        let (_untracked_master, _untracked_slave, mut untracked, untracked_h) = purge_fixture();
        FIXTURE_COUNTERS.set(Some(counts));
        // A later successful counter query cannot replace a missing CREATE snapshot.
        let unsupported = event_control(&mut untracked, untracked_h, 6, 0x1b008c, &[], 24).unwrap();
        assert_eq!(u32_at(&unsupported, 12), Ok(UNSUPPORTED));
        let (_master, _slave, mut serial, h) = purge_fixture();
        let snapshot = |serial: &mut Serial, h| {
            let reply = event_control(serial, h, 7, 0x1b008c, &[], 24).unwrap();
            assert_eq!(u32_at(&reply, 12), Ok(0));
            assert_eq!(reply.len(), 44);
            assert_eq!(u32_at(&reply, 16), Ok(24));
            std::array::from_fn::<_, 6, _>(|i| u32_at(&reply, 20 + 4 * i).unwrap())
        };
        assert_eq!(snapshot(&mut serial, h), [0; 6]);
        for (i, index) in [4, 5, 6, 7, 10, 8].into_iter().enumerate() {
            counts[index] = counts[index].wrapping_add(i as i32 + 3);
        }
        FIXTURE_COUNTERS.set(Some(counts));
        assert_eq!(snapshot(&mut serial, h), [3, 4, 5, 6, 7, 8]);
        assert_eq!(
            u32_at(
                &serial.process(event_request(1, h, 8, 2, &[0; 32])).unwrap(),
                12
            ),
            Ok(0)
        );
        let mut body = [0; 32];
        body[..4].copy_from_slice(&0xc0000000u32.to_le_bytes());
        body[20] = 1;
        let opened = serial.process(event_request(1, 0, 9, 0, &body)).unwrap();
        let reopened = u32_at(&opened, 16).unwrap();
        assert_ne!(reopened, h);
        assert_eq!(snapshot(&mut serial, reopened), [0; 6]);
        FIXTURE_COUNTERS.set(None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn unix_counter_events_require_create_capability_and_fresh_mask_origin() {
        // PTYs do not expose TIOCGICOUNT: later successful probes must not promote
        // the already-opened logical handle's advertised support.
        let (_m, _s, mut unsupported, u) = purge_fixture();
        let mut counts = [17; 20];
        FIXTURE_COUNTERS.set(Some(counts));
        let reply =
            event_control(&mut unsupported, u, 2, 0x1b0044, &64u32.to_le_bytes(), 0).unwrap();
        assert_eq!(u32_at(&reply, 12), Ok(UNSUPPORTED));
        let (_m, _s, mut serial, h) = purge_fixture();
        counts[9] = counts[9].wrapping_add(10); // historic input breaks before SET
        counts[8] = counts[8].wrapping_add(10); // historic parity errors before SET
        FIXTURE_COUNTERS.set(Some(counts));
        let mask = 0xc0u32; // input BREAK and line-status ERR only
        let reply = event_control(&mut serial, h, 3, 0x1b0044, &mask.to_le_bytes(), 0).unwrap();
        assert_eq!(u32_at(&reply, 12), Ok(0));
        assert!(event_control(&mut serial, h, 4, 0x1b0048, &[], 4).is_none());
        assert!(
            serial.ready().is_empty(),
            "historic counts are not new edges"
        );
        for index in [9, 8] {
            counts[index] = counts[index].wrapping_add(1);
        }
        FIXTURE_COUNTERS.set(Some(counts));
        let replies = serial.ready();
        assert_eq!(replies.len(), 1);
        assert_eq!(u32_at(&replies[0], 20), Ok(mask));
        assert!(event_control(&mut serial, h, 5, 0x1b0048, &[], 4).is_none());
        assert!(
            serial.ready().is_empty(),
            "one transition is reported only once"
        );
        let reply = event_control(&mut serial, h, 6, 0x1b0044, &0u32.to_le_bytes(), 0).unwrap();
        assert_eq!(u32_at(&reply, 12), Ok(0));
        let replies = serial.ready();
        assert_eq!(replies.len(), 1);
        assert_eq!(u32_at(&replies[0], 8), Ok(5));
        assert_eq!(u32_at(&replies[0], 20), Ok(0));
        FIXTURE_COUNTERS.set(None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn unix_counter_events_filter_fields_wrap_and_preserve_wait_on_failed_set() {
        let mut counts = [0i32; 20];
        counts[9] = -1;
        FIXTURE_COUNTERS.set(Some(counts));
        let (_m, _s, mut serial, h) = purge_fixture();
        let mask = 64u32 | 128;
        assert_eq!(
            u32_at(
                &event_control(&mut serial, h, 2, 0x1b0044, &mask.to_le_bytes(), 0).unwrap(),
                12
            ),
            Ok(0)
        );
        assert!(event_control(&mut serial, h, 3, 0x1b0048, &[], 4).is_none());
        counts[4] += 1;
        counts[5] += 1;
        counts[0] += 1; // RX/TX/modem snapshots are not receive errors
        FIXTURE_COUNTERS.set(Some(counts));
        assert!(serial.ready().is_empty());
        for rejected in [4u32, 8, 16, 32, 256, 64 | 8, 0x200, 0x2000] {
            let expected = if rejected == 0x2000 {
                INVALID
            } else {
                UNSUPPORTED
            };
            assert_eq!(
                u32_at(
                    &event_control(&mut serial, h, 4, 0x1b0044, &rejected.to_le_bytes(), 0)
                        .unwrap(),
                    12
                ),
                Ok(expected)
            );
        }
        FIXTURE_COUNTERS.set(None); // a driver query failure cannot reset the wait
        assert_eq!(
            u32_at(
                &event_control(&mut serial, h, 5, 0x1b0044, &mask.to_le_bytes(), 0).unwrap(),
                12
            ),
            Ok(UNSUPPORTED)
        );
        FIXTURE_COUNTERS.set(Some(counts));
        assert!(serial.ready().is_empty());
        counts[9] = 0;
        counts[6] += 1; // real modulo-u32 transition + framing error
        FIXTURE_COUNTERS.set(Some(counts));
        let replies = serial.ready();
        assert_eq!(replies.len(), 1);
        assert_eq!(u32_at(&replies[0], 8), Ok(3));
        assert_eq!(u32_at(&replies[0], 20), Ok(mask));
        FIXTURE_COUNTERS.set(None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn unix_counter_event_mask_admission_tracks_supported_family_shape() {
        let mut body = vec![0; 32];
        body[4..8].copy_from_slice(&4u32.to_le_bytes());
        body[8..12].copy_from_slice(&0x1b0044u32.to_le_bytes());
        body.extend(0xc1u32.to_le_bytes());
        assert!(
            is_wait_mask_set(&body),
            "reset escape must admit newly supported masks"
        );
        body[32..36].copy_from_slice(&4u32.to_le_bytes());
        assert!(!is_wait_mask_set(&body));
        body[32..36].copy_from_slice(&0x2000u32.to_le_bytes());
        assert!(!is_wait_mask_set(&body));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn unix_counter_events_map_individual_kernel_fields_and_keep_between_waits() {
        let mut counts = [0i32; 20];
        FIXTURE_COUNTERS.set(Some(counts));
        let (_m, _s, mut serial, h) = purge_fixture();
        let mask = 0xc0u32;
        assert_eq!(
            u32_at(
                &event_control(&mut serial, h, 2, 0x1b0044, &mask.to_le_bytes(), 0).unwrap(),
                12
            ),
            Ok(0)
        );
        for (tag, (index, expected)) in [(9, 64u32), (6, 128), (7, 128), (8, 128)]
            .into_iter()
            .enumerate()
        {
            // Counter changes between waits must satisfy the NEXT wait.
            counts[index] += 1;
            FIXTURE_COUNTERS.set(Some(counts));
            assert!(event_control(&mut serial, h, 20 + tag as u32, 0x1b0048, &[], 4).is_none());
            let replies = serial.ready();
            assert_eq!(replies.len(), 1);
            assert_eq!(
                u32_at(&replies[0], 20),
                Ok(expected),
                "kernel field {index}"
            );
        }
        assert!(event_control(&mut serial, h, 40, 0x1b0048, &[], 4).is_none());
        for index in [0, 1, 2, 3, 4, 5, 10, 11, 19] {
            counts[index] += 1;
        }
        FIXTURE_COUNTERS.set(Some(counts));
        assert!(
            serial.ready().is_empty(),
            "RX/TX/buffer overrun/reserved are not line-status errors"
        );
        FIXTURE_COUNTERS.set(None);
        let replies = serial.ready();
        assert_eq!(replies.len(), 1);
        assert_eq!(u32_at(&replies[0], 8), Ok(40));
        assert_eq!(u32_at(&replies[0], 12), Ok(UNSUPPORTED));
        assert!(!serial.has_pending());
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn unix_counter_events_coexist_with_real_pty_reads_and_close_cleanup() {
        let mut counts = [0i32; 20];
        FIXTURE_COUNTERS.set(Some(counts));
        let (mut master, _slave, mut serial, h) = purge_fixture();
        let mask = 64u32;
        assert_eq!(
            u32_at(
                &event_control(&mut serial, h, 2, 0x1b0044, &mask.to_le_bytes(), 0).unwrap(),
                12
            ),
            Ok(0)
        );
        assert!(event_control(&mut serial, h, 3, 0x1b0048, &[], 4).is_none());
        master.write_all(b"kept").unwrap();
        queue_transfer(&mut serial, h, 4, false);
        let deadline = Instant::now() + IO_LIMIT;
        loop {
            let replies = serial.ready();
            if !replies.is_empty() {
                assert_eq!(replies.len(), 1);
                assert_eq!(u32_at(&replies[0], 8), Ok(4));
                assert_eq!(&replies[0][20..], b"kept");
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(
            serial.ready().is_empty(),
            "a real protocol read cannot fabricate RXCHAR outside the mask"
        );
        counts[9] += 1;
        FIXTURE_COUNTERS.set(Some(counts));
        let replies = serial.ready();
        assert_eq!(replies.len(), 1);
        assert_eq!(u32_at(&replies[0], 8), Ok(3));
        assert_eq!(u32_at(&replies[0], 20), Ok(64));
        assert!(event_control(&mut serial, h, 5, 0x1b0048, &[], 4).is_none());
        assert_eq!(
            u32_at(
                &serial.process(event_request(1, h, 6, 2, &[0; 32])).unwrap(),
                12
            ),
            Ok(0)
        );
        let replies = serial.ready();
        assert_eq!(replies.len(), 1);
        assert_eq!(u32_at(&replies[0], 8), Ok(5));
        assert_eq!(u32_at(&replies[0], 12), Ok(CANCELLED));
        assert!(!serial.has_pending());
        FIXTURE_COUNTERS.set(None);
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn unix_modem_snapshot_does_not_establish_active_interrupt_capture() {
        FIXTURE_COUNTERS.set(Some([0; 20]));
        let (_m, _s, mut serial, h) = purge_fixture();
        // A real driver can return a valid snapshot with modem interrupts off.
        // Snapshot success never authorizes modem masks, before or after local
        // flow/carrier configuration changes. Only this test-owned PTY changes.
        for mode in [0, libc::CLOCAL, libc::CRTSCTS, libc::CLOCAL | libc::CRTSCTS] {
            let handle = serial.ports.get_mut(&1).unwrap().handle.as_mut().unwrap();
            let mut attrs = get_attributes(&handle.file).unwrap();
            attrs.c_cflag = (attrs.c_cflag & !(libc::CLOCAL | libc::CRTSCTS)) | mode;
            set_attributes(&handle.file, &attrs).unwrap();
            let before = get_attributes(&handle.file).unwrap().c_cflag;
            for bit in [8u32, 16, 32, 256] {
                let reply =
                    event_control(&mut serial, h, 2, 0x1b0044, &bit.to_le_bytes(), 0).unwrap();
                assert_eq!(u32_at(&reply, 12), Ok(UNSUPPORTED), "modem event {bit:#x}");
                let mut body = vec![0; 32];
                body[4..8].copy_from_slice(&4u32.to_le_bytes());
                body[8..12].copy_from_slice(&0x1b0044u32.to_le_bytes());
                body.extend(bit.to_le_bytes());
                assert!(
                    !is_wait_mask_set(&body),
                    "no queue escape for unsupported modem event {bit:#x}"
                );
            }
            let handle = serial.ports.get(&1).unwrap().handle.as_ref().unwrap();
            assert_eq!(
                get_attributes(&handle.file).unwrap().c_cflag,
                before,
                "failed modem masks must not force arming through flow/carrier flags"
            );
        }
        FIXTURE_COUNTERS.set(None);
    }
}
