//! Explicitly started, bounded live capture through the operating system's libpcap.

use libloading::Library;
use std::{
    ffi::{CStr, CString, c_char, c_int, c_uint, c_void},
    fs::{self, File},
    io::{self, BufReader, BufWriter, Read, Seek, Write},
    path::{Path, PathBuf},
    ptr::{self, NonNull},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

mod staging;

const MAX_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_PACKETS: u64 = 1_000_000;
const MAX_SNAPLEN: u32 = 262_144;
const FILE_HEADER: u64 = 24;
const PACKET_HEADER: u64 = 16;
const POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureInterface {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CaptureConfig {
    pub interface: String,
    pub filter: String,
    pub output: PathBuf,
    pub snaplen: u32,
    pub max_duration: Duration,
    pub max_bytes: u64,
    pub max_packets: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    Cancelled,
    TimeLimit,
    ByteLimit,
    PacketLimit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PacketTimestamp {
    pub seconds: u32,
    pub nanoseconds: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PcapMetadata {
    pub bytes: u64,
    pub packets: u64,
    pub snaplen: u32,
    pub linktype: u16,
    pub first_timestamp: Option<PacketTimestamp>,
}

#[derive(Debug)]
pub struct CaptureReport {
    pub output: PathBuf,
    pub reason: StopReason,
    pub metadata: PcapMetadata,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CaptureProgress {
    pub ready: bool,
    pub packets: u64,
    pub bytes: u64,
}

#[derive(Default)]
struct WorkerState {
    cancel: AtomicBool,
    progress: Mutex<CaptureProgress>,
}

pub struct CaptureHandle {
    state: Arc<WorkerState>,
    worker: Option<JoinHandle<Result<CaptureReport, String>>>,
}

impl CaptureHandle {
    pub fn progress(&self) -> CaptureProgress {
        *self
            .state
            .progress
            .lock()
            .unwrap_or_else(|err| err.into_inner())
    }

    pub fn cancel(&self) {
        self.state.cancel.store(true, Ordering::Relaxed);
    }

    pub fn is_finished(&self) -> bool {
        self.worker.as_ref().is_none_or(JoinHandle::is_finished)
    }

    pub fn wait(mut self) -> Result<CaptureReport, String> {
        self.worker
            .take()
            .expect("capture worker")
            .join()
            .map_err(|_| "Capture worker stopped unexpectedly")?
    }
}

impl Drop for CaptureHandle {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub struct PacketCapture;

impl PacketCapture {
    pub fn start(config: CaptureConfig) -> Result<CaptureHandle, String> {
        config.start()
    }
}

impl CaptureConfig {
    pub fn start(self) -> Result<CaptureHandle, String> {
        if self.interface.trim().is_empty()
            || self.interface == "any"
            || self.interface.contains('\0')
        {
            return Err("Choose a single capture interface".into());
        }
        if self.filter.len() > 4096 || self.filter.contains('\0') {
            return Err("Enter a capture filter of up to 4096 bytes".into());
        }
        if self.output.file_name().is_none() {
            return Err("Choose a capture output file".into());
        }
        if !(1..=MAX_SNAPLEN).contains(&self.snaplen)
            || !(Duration::from_millis(1)..=Duration::from_secs(3600)).contains(&self.max_duration)
            || !(FILE_HEADER..=MAX_BYTES).contains(&self.max_bytes)
            || !(1..=MAX_PACKETS).contains(&self.max_packets)
        {
            return Err("Capture limits must be 1–262144 bytes per packet, 1 ms–1 hour, 24 bytes–1 GiB, and 1–1000000 packets".into());
        }
        let state = Arc::new(WorkerState::default());
        let worker_state = Arc::clone(&state);
        let worker = thread::Builder::new()
            .name("live-packet-capture".into())
            .spawn(move || run(self, &worker_state))
            .map_err(|err| format!("Could not start capture worker: {err}"))?;
        Ok(CaptureHandle {
            state,
            worker: Some(worker),
        })
    }
}

/// Load the local provider and enumerate interfaces. Call from a background worker.
pub fn enumerate_interfaces() -> Result<Vec<CaptureInterface>, String> {
    let api = Api::load()?;
    let mut error = [0; 256];
    let mut head = ptr::null_mut();
    if unsafe { (api.pcap_findalldevs)(&mut head, error.as_mut_ptr()) } != 0 {
        return Err(format!(
            "Could not list capture interfaces: {}",
            error_text(&error)
        ));
    }
    let list = InterfaceList { api: &api, head };
    let mut next = list.head;
    let mut interfaces = Vec::new();
    while let Some(device) = unsafe { next.as_ref() } {
        let name = unsafe { string(device.name) };
        if let Some(name) = name.filter(|name| !name.is_empty() && name != "any") {
            interfaces.push(CaptureInterface {
                name,
                description: unsafe { string(device.description) },
            });
        }
        next = device.next;
    }
    interfaces.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(interfaces)
}

pub fn inspect_pcap(path: &Path) -> Result<PcapMetadata, String> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(pcap_read_error)?;
    read_metadata(file)
}

fn pcap_read_error(error: io::Error) -> String {
    format!("Could not read capture file: {error}")
}

fn read_metadata(file: File) -> Result<PcapMetadata, String> {
    let info = file.metadata().map_err(pcap_read_error)?;
    if !info.is_file() || !(FILE_HEADER..=MAX_BYTES).contains(&info.len()) {
        return Err("Expected a classic PCAP file between 24 bytes and 1 GiB".into());
    }
    let mut reader = BufReader::new(file);
    let mut header = [0; FILE_HEADER as usize];
    reader.read_exact(&mut header).map_err(pcap_read_error)?;
    let (little, nanos) = match &header[..4] {
        [0xd4, 0xc3, 0xb2, 0xa1] => (true, false),
        [0xa1, 0xb2, 0xc3, 0xd4] => (false, false),
        [0x4d, 0x3c, 0xb2, 0xa1] => (true, true),
        [0xa1, 0xb2, 0x3c, 0x4d] => (false, true),
        _ => return Err("Unrecognized classic PCAP signature".into()),
    };
    let read16 = |bytes: &[u8]| {
        let value = bytes.try_into().expect("two-byte PCAP field");
        if little {
            u16::from_le_bytes(value)
        } else {
            u16::from_be_bytes(value)
        }
    };
    let read32 = |bytes: &[u8]| {
        let value = bytes.try_into().expect("four-byte PCAP field");
        if little {
            u32::from_le_bytes(value)
        } else {
            u32::from_be_bytes(value)
        }
    };
    let snaplen = read32(&header[16..20]);
    if read16(&header[4..6]) != 2 || read16(&header[6..8]) != 4 || snaplen == 0 {
        return Err("Unsupported classic PCAP header".into());
    }
    let mut metadata = PcapMetadata {
        bytes: info.len(),
        packets: 0,
        snaplen,
        linktype: (read32(&header[20..24]) & 0xffff) as u16,
        first_timestamp: None,
    };
    let mut offset = FILE_HEADER;
    let mut record = [0; PACKET_HEADER as usize];
    while offset < metadata.bytes {
        if metadata.packets == MAX_PACKETS {
            return Err("PCAP exceeds the 1000000-packet inspection limit".into());
        }
        reader.read_exact(&mut record).map_err(pcap_read_error)?;
        let fraction = read32(&record[4..8]);
        let captured = read32(&record[8..12]);
        let wire = read32(&record[12..16]);
        offset += PACKET_HEADER + u64::from(captured);
        let fraction_limit = if nanos { 1_000_000_000 } else { 1_000_000 };
        if fraction >= fraction_limit
            || captured > snaplen
            || captured > wire
            || offset > metadata.bytes
        {
            return Err("Invalid or truncated PCAP packet record".into());
        }
        metadata.first_timestamp.get_or_insert(PacketTimestamp {
            seconds: read32(&record[..4]),
            nanoseconds: if nanos { fraction } else { fraction * 1000 },
        });
        metadata.packets += 1;
        reader
            .seek_relative(i64::from(captured))
            .map_err(pcap_read_error)?;
    }
    Ok(metadata)
}

fn run(config: CaptureConfig, state: &WorkerState) -> Result<CaptureReport, String> {
    if state.cancel.load(Ordering::Relaxed) {
        return Err("Capture cancelled before opening the interface".into());
    }
    let mut stage = staging::Stage::create(&config.output)
        .map_err(|err| format!("Could not stage capture output: {err}"))?;
    let api = Api::load()?;
    let live = open_capture(&api, &config)?;
    let network = savefile_linktype(&live)?;
    let mut writer = BufWriter::new(stage.file());
    // Keep file and pseudo-header byte ordering consistent with libpcap.
    let mut header = [0; FILE_HEADER as usize];
    header[..4].copy_from_slice(&0xa1b2c3d4u32.to_ne_bytes());
    header[4..6].copy_from_slice(&2u16.to_ne_bytes());
    header[6..8].copy_from_slice(&4u16.to_ne_bytes());
    header[16..20].copy_from_slice(&config.snaplen.to_ne_bytes());
    header[20..].copy_from_slice(&network.to_ne_bytes());
    writer.write_all(&header).map_err(pcap_write_error)?;
    #[cfg(unix)]
    let selectable = unsafe { (api.pcap_get_selectable_fd)(live.raw.as_ptr()) };
    #[cfg(not(unix))]
    let selectable = -1;
    let mut progress = CaptureProgress {
        ready: true,
        packets: 0,
        bytes: FILE_HEADER,
    };
    publish(state, progress);
    let deadline = Instant::now() + config.max_duration;
    let reason = loop {
        if state.cancel.load(Ordering::Relaxed) {
            break StopReason::Cancelled;
        }
        if Instant::now() >= deadline {
            break StopReason::TimeLimit;
        }
        if progress.packets == config.max_packets {
            break StopReason::PacketLimit;
        }
        if config.max_bytes - progress.bytes < PACKET_HEADER {
            break StopReason::ByteLimit;
        }

        let mut header = ptr::null();
        let mut packet = ptr::null();
        match unsafe { (api.pcap_next_ex)(live.raw.as_ptr(), &mut header, &mut packet) } {
            1 => {
                let info = unsafe { header.as_ref() }
                    .ok_or("Capture provider returned a missing packet header")?;
                let (seconds, micros) = info.savefile_timestamp(config.snaplen)?;
                if packet.is_null() {
                    return Err("Capture provider returned missing packet bytes".into());
                }
                let record_bytes = PACKET_HEADER + u64::from(info.caplen);
                if record_bytes > config.max_bytes - progress.bytes {
                    break StopReason::ByteLimit;
                }
                let fields = [seconds, micros, info.caplen, info.length];
                let mut record = [0; PACKET_HEADER as usize];
                for (field, destination) in fields.into_iter().zip(record.chunks_exact_mut(4)) {
                    destination.copy_from_slice(&field.to_ne_bytes());
                }
                writer.write_all(&record).map_err(pcap_write_error)?;
                // libpcap keeps the buffer live until the next packet read.
                let bytes = unsafe { std::slice::from_raw_parts(packet, info.caplen as usize) };
                writer.write_all(bytes).map_err(pcap_write_error)?;
                progress.packets += 1;
                progress.bytes += record_bytes;
                publish(state, progress);
            }
            0 => {
                let delay = POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now()));
                wait_for_packet(selectable, delay)?;
            }
            _ => return Err(live.error("Could not read a live packet")),
        }
    };
    // Release capture permissions and the kernel buffer before reading the staged file.
    drop(live);
    let file = writer
        .into_inner()
        .map_err(|err| pcap_write_error(err.into_error()))?;
    file.rewind().map_err(pcap_read_error)?;
    let metadata = read_metadata(file.try_clone().map_err(pcap_read_error)?)?;
    if metadata.bytes != progress.bytes || metadata.packets != progress.packets {
        return Err("Capture output is incomplete".into());
    }
    file.sync_all().map_err(pcap_write_error)?;
    let output = stage.publish()?;
    Ok(CaptureReport {
        output,
        reason,
        metadata,
    })
}

fn pcap_write_error(err: io::Error) -> String {
    format!("Could not write capture output: {err}")
}

fn savefile_linktype(live: &Live<'_>) -> Result<u32, String> {
    let raw = live.raw.as_ptr();
    let dlt = unsafe { (live.api.pcap_datalink)(raw) };
    let name = unsafe { string((live.api.pcap_datalink_val_to_name)(dlt)) };
    // Native DLT codes match the PCAP registry in the low and high matching
    // ranges, with platform-specific exceptions identified by libpcap's names.
    let linktype = match name.as_deref() {
        Some("ATM_RFC1483") => 100,
        Some("RAW") => 101,
        Some("SLIP_BSDOS") => 102,
        Some("PPP_BSDOS") => 103,
        Some("FR") => 107,
        Some("LOOP") => 108,
        Some("ENC") => 109,
        Some("HDLC") => 112,
        Some("C_HDLC") => 104,
        Some("ATM_CLIP") => 106,
        Some("PFSYNC") => 246,
        Some("PKTAP") => 258,
        Some(_) if matches!(dlt, 0..=10 | 32 | 50 | 51 | 99 | 104..=305) => dlt as u32,
        _ => {
            return Err(format!(
                "Capture link type {dlt} cannot be written to a classic PCAP file"
            ));
        }
    };
    let extensions = live
        .api
        .pcap_datalink_ext
        .map_or(0, |read| unsafe { read(raw) } as u32);
    Ok(linktype | extensions)
}

fn wait_for_packet(selectable: c_int, delay: Duration) -> Result<(), String> {
    if delay.is_zero() {
        return Ok(());
    }
    #[cfg(unix)]
    if selectable >= 0 {
        let mut fd = libc::pollfd {
            fd: selectable,
            events: libc::POLLIN,
            revents: 0,
        };
        // A short timeout bounds idle cancellation and also services providers
        // that require periodic reads even when their descriptor is quiet.
        let status = unsafe { libc::poll(&mut fd, 1, delay.as_millis().max(1) as c_int) };
        if status < 0 {
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(format!("Could not wait for live packets: {error}"));
            }
        } else if fd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            return Err("Capture interface stopped responding".into());
        }
        return Ok(());
    }
    #[cfg(not(unix))]
    let _ = selectable;
    thread::sleep(delay);
    Ok(())
}

fn publish(state: &WorkerState, progress: CaptureProgress) {
    *state.progress.lock().unwrap_or_else(|err| err.into_inner()) = progress;
}

fn open_capture<'a>(api: &'a Api, config: &CaptureConfig) -> Result<Live<'a>, String> {
    let interface =
        CString::new(config.interface.as_bytes()).map_err(|_| "Invalid capture interface")?;
    let filter = CString::new(config.filter.as_bytes()).map_err(|_| "Invalid capture filter")?;
    let mut error = [0; 256];
    let raw = unsafe { (api.pcap_create)(interface.as_ptr(), error.as_mut_ptr()) };
    let live = Live {
        api,
        raw: NonNull::new(raw)
            .ok_or_else(|| format!("Could not open capture interface: {}", error_text(&error)))?,
    };
    let raw = live.raw.as_ptr();
    for (setting, result) in [
        ("snapshot length", unsafe {
            (api.pcap_set_snaplen)(raw, config.snaplen as c_int)
        }),
        ("read timeout", unsafe { (api.pcap_set_timeout)(raw, 20) }),
        ("immediate mode", unsafe {
            (api.pcap_set_immediate_mode)(raw, 1)
        }),
    ] {
        if result != 0 {
            return Err(live.error(&format!("Could not configure capture {setting}")));
        }
    }
    let active = unsafe { (api.pcap_activate)(raw) };
    if matches!(active, -8 | -11) {
        return Err(live.error("Capture permission denied by the operating system"));
    }
    if active < 0 {
        return Err(live.error("Could not activate capture interface"));
    }
    let mut filter_program = BpfProgram {
        length: 0,
        instructions: ptr::null_mut(),
    };
    // Even an empty filter is compiled and applied by the provider.
    if unsafe { (api.pcap_compile)(raw, &mut filter_program, filter.as_ptr(), 1, u32::MAX) } != 0 {
        return Err(live.error("Invalid capture filter"));
    }
    let applied = unsafe { (api.pcap_setfilter)(raw, &mut filter_program) };
    unsafe { (api.pcap_freecode)(&mut filter_program) };
    if applied != 0 {
        return Err(live.error("Could not apply capture filter"));
    }
    if unsafe { (api.pcap_setnonblock)(raw, 1, error.as_mut_ptr()) } != 0 {
        return Err(format!(
            "Could not enable nonblocking capture: {}",
            error_text(&error)
        ));
    }
    Ok(live)
}

#[repr(C)]
struct Device {
    next: *mut Device,
    name: *const c_char,
    description: *const c_char,
    addresses: *mut c_void,
    flags: c_uint,
}

#[repr(C)]
struct PacketHeader {
    #[cfg(unix)]
    timestamp: libc::timeval,
    #[cfg(not(unix))]
    timestamp: [std::ffi::c_long; 2],
    caplen: u32,
    length: u32,
}

impl PacketHeader {
    fn savefile_timestamp(&self, snaplen: u32) -> Result<(u32, u32), String> {
        #[cfg(unix)]
        let (seconds, micros) = (self.timestamp.tv_sec, self.timestamp.tv_usec);
        #[cfg(not(unix))]
        let [seconds, micros] = self.timestamp;
        if self.caplen > self.length || self.caplen > snaplen || !(0..1_000_000).contains(&micros) {
            return Err("Capture provider returned an invalid packet record".into());
        }
        let seconds = u32::try_from(seconds)
            .map_err(|_| "Packet timestamp is outside the classic PCAP range")?;
        Ok((seconds, micros as u32))
    }
}

#[repr(C)]
struct BpfProgram {
    length: c_uint,
    instructions: *mut c_void,
}

struct InterfaceList<'a> {
    api: &'a Api,
    head: *mut Device,
}

impl Drop for InterfaceList<'_> {
    fn drop(&mut self) {
        unsafe { (self.api.pcap_freealldevs)(self.head) };
    }
}

struct Live<'a> {
    api: &'a Api,
    raw: NonNull<c_void>,
}

impl Live<'_> {
    fn error(&self, context: &str) -> String {
        let detail = unsafe { string((self.api.pcap_geterr)(self.raw.as_ptr())) };
        match detail.filter(|detail| !detail.is_empty()) {
            Some(detail) => format!("{context}: {detail}"),
            None => context.into(),
        }
    }
}

impl Drop for Live<'_> {
    fn drop(&mut self) {
        unsafe { (self.api.pcap_close)(self.raw.as_ptr()) };
    }
}

unsafe fn string(raw: *const c_char) -> Option<String> {
    if raw.is_null() {
        return None;
    }
    Some(
        unsafe { CStr::from_ptr(raw) }
            .to_string_lossy()
            .into_owned(),
    )
}

fn error_text(buffer: &[c_char]) -> String {
    let bytes: Vec<u8> = buffer
        .iter()
        .take_while(|&&byte| byte != 0)
        .map(|&byte| byte as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(not(windows))]
fn load_provider() -> Result<Arc<Library>, String> {
    #[cfg(target_os = "linux")]
    let candidates = &["libpcap.so.1", "libpcap.so.0.8", "libpcap.so"][..];
    #[cfg(target_os = "macos")]
    let candidates = &["/usr/lib/libpcap.A.dylib", "/usr/lib/libpcap.dylib"][..];
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let candidates: &[&str] = &[];
    candidates
        .iter()
        .find_map(|name| unsafe { Library::new(name) }.ok())
        .map(Arc::new)
        .ok_or_else(|| {
            "Live capture provider unavailable. A compatible libpcap is required on Linux or macOS"
                .into()
        })
}

#[cfg(windows)]
fn load_provider() -> Result<Arc<Library>, String> {
    use libloading::os::windows::{
        LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32, Library as WindowsLibrary,
    };
    use std::{ffi::OsString, os::windows::ffi::OsStringExt, sync::OnceLock};
    use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;

    static PROVIDER: OnceLock<Result<Arc<Library>, String>> = OnceLock::new();
    PROVIDER.get_or_init(|| {
        let mut directory = vec![0; 32_768];
        let length = unsafe { GetSystemDirectoryW(directory.as_mut_ptr(), directory.len() as u32) } as usize;
        if length == 0 || length >= directory.len() {
            return Err("Could not locate the Windows system directory for Npcap".into());
        }
        let path = PathBuf::from(OsString::from_wide(&directory[..length]))
            .join("Npcap")
            .join("wpcap.dll");
        let library = unsafe {
            WindowsLibrary::load_with_flags(path, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32)
        }.map_err(|err| format!("Live capture provider unavailable. A compatible Npcap installation is required: {err}"))?;
        let library: Library = library.into();
        type Init = unsafe extern "C" fn(c_uint, *mut c_char) -> c_int;
        let initialize = unsafe { library.get::<Init>(b"pcap_init\0") }
            .map_err(|err| format!("Incompatible Npcap provider: {err}"))?;
        let mut error = [0; 256];
        const PCAP_CHAR_ENC_UTF_8: c_uint = 1;
        if unsafe { initialize(PCAP_CHAR_ENC_UTF_8, error.as_mut_ptr()) } != 0 {
            return Err(format!("Could not initialize Npcap: {}", error_text(&error)));
        }
        Ok(Arc::new(library))
    }).clone()
}

macro_rules! pcap_api {
    ($($(#[$meta:meta])* $name:ident: $ty:ty),+ $(,)?) => {
        struct Api {
            $($(#[$meta])* $name: $ty,)+
            pcap_datalink_ext: Option<unsafe extern "C" fn(*mut c_void) -> c_int>,
            _library: Arc<Library>,
        }
        impl Api {
            fn load() -> Result<Self, String> {
                let library = load_provider()?;
                Ok(Self {
                    $($(#[$meta])* $name: *unsafe { library.get::<$ty>(concat!(stringify!($name), "\0").as_bytes()) }
                        .map_err(|err| format!("Incompatible packet capture provider: {err}"))?,)+
                    pcap_datalink_ext: unsafe { library.get(b"pcap_datalink_ext\0") }.ok().map(|symbol| *symbol),
                    _library: library,
                })
            }
        }
    };
}

pcap_api! {
    pcap_findalldevs: unsafe extern "C" fn(*mut *mut Device, *mut c_char) -> c_int,
    pcap_freealldevs: unsafe extern "C" fn(*mut Device),
    pcap_create: unsafe extern "C" fn(*const c_char, *mut c_char) -> *mut c_void,
    pcap_set_snaplen: unsafe extern "C" fn(*mut c_void, c_int) -> c_int,
    pcap_set_timeout: unsafe extern "C" fn(*mut c_void, c_int) -> c_int,
    pcap_set_immediate_mode: unsafe extern "C" fn(*mut c_void, c_int) -> c_int,
    pcap_activate: unsafe extern "C" fn(*mut c_void) -> c_int,
    pcap_compile: unsafe extern "C" fn(*mut c_void, *mut BpfProgram, *const c_char, c_int, u32) -> c_int,
    pcap_setfilter: unsafe extern "C" fn(*mut c_void, *mut BpfProgram) -> c_int,
    pcap_freecode: unsafe extern "C" fn(*mut BpfProgram),
    pcap_setnonblock: unsafe extern "C" fn(*mut c_void, c_int, *mut c_char) -> c_int,
    pcap_geterr: unsafe extern "C" fn(*mut c_void) -> *const c_char,
    #[cfg(unix)]
    pcap_get_selectable_fd: unsafe extern "C" fn(*mut c_void) -> c_int,
    pcap_next_ex: unsafe extern "C" fn(*mut c_void, *mut *const PacketHeader, *mut *const u8) -> c_int,
    pcap_close: unsafe extern "C" fn(*mut c_void),
    pcap_datalink: unsafe extern "C" fn(*mut c_void) -> c_int,
    pcap_datalink_val_to_name: unsafe extern "C" fn(c_int) -> *const c_char,
}
