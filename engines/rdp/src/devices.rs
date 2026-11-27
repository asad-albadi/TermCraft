//! RDP device redirection. Disk operations run on a bounded worker,
//! confined to capability directory handles. No ambient path from the peer is opened.
use crate::rdp_options::{DriveMapping, PrinterMapping, RdpOptions, SerialMapping};
#[cfg(target_os = "macos")]
#[path = "native_file_macos.rs"]
mod native_file_macos;
#[cfg(windows)]
#[path = "native_file_windows.rs"]
mod native_file_windows;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
#[path = "printer.rs"]
mod printer;
#[cfg(unix)]
#[path = "serial.rs"]
mod serial;
#[cfg(windows)]
#[path = "serial_windows.rs"]
mod serial;
#[cfg(any(windows, test))]
#[path = "serial_completion.rs"]
mod serial_completion;
use crate::smartcard;
use cap_std::fs::{Dir, File, OpenOptions};
use ironrdp_core::AsAny;
use ironrdp_pdu::{PduResult, gcc::ChannelName};
use ironrdp_svc::{SvcClientProcessor, SvcMessage, SvcProcessor};
use std::{
    collections::BTreeMap,
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};
const INVALID: u32 = 0xc000000d;
const DENIED: u32 = 0xc0000022;
const UNSUPPORTED: u32 = 0xc00000bb;
const MAX_IO: usize = 64 * 1024;
const DEVICE_IO_CREDITS: usize = 16;
fn u32_at(p: &[u8], at: usize) -> Result<u32, u32> {
    Ok(u32::from_le_bytes(
        p.get(at..at + 4).ok_or(INVALID)?.try_into().unwrap(),
    ))
}
fn u64_at(p: &[u8], at: usize) -> Result<u64, u32> {
    Ok(u64::from_le_bytes(
        p.get(at..at + 8).ok_or(INVALID)?.try_into().unwrap(),
    ))
}
fn word(p: &mut Vec<u8>, n: u32) {
    p.extend(n.to_le_bytes());
}
fn quad(p: &mut Vec<u8>, n: u64) {
    p.extend(n.to_le_bytes());
}
fn wide(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}
fn text(p: &[u8]) -> Result<String, u32> {
    if p.len() > 8192 || p.len() % 2 != 0 {
        return Err(INVALID);
    }
    let units: Vec<_> = p
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect();
    let units = units.strip_suffix(&[0]).unwrap_or(&units);
    if units.contains(&0) {
        return Err(INVALID);
    }
    String::from_utf16(units).map_err(|_| INVALID)
}
fn relative(s: &str) -> Result<PathBuf, u32> {
    // Reject NT namespaces, alternate streams, drive prefixes and traversal, on every OS.
    if s.len() > 4096 || s.contains([':', '\0']) || s.starts_with("\\\\") {
        return Err(DENIED);
    }
    let mut p = PathBuf::new();
    for part in s.split(['\\', '/']) {
        if part == ".." {
            return Err(DENIED);
        }
        if !part.is_empty() && part != "." {
            p.push(part);
        }
    }
    if p.as_os_str().is_empty() {
        p.push(".");
    }
    Ok(p)
}
fn status(e: std::io::Error) -> u32 {
    match e.kind() {
        std::io::ErrorKind::NotFound => 0xc0000034,
        std::io::ErrorKind::AlreadyExists => 0xc0000035,
        std::io::ErrorKind::PermissionDenied => DENIED,
        std::io::ErrorKind::DirectoryNotEmpty => 0xc0000101,
        std::io::ErrorKind::StorageFull => 0xc000007f,
        _ => 0xc0000001,
    }
}
struct Root {
    dir: Dir,
    read_only: bool,
}
struct Handle {
    device: u32,
    file: Option<File>,
    dir: Option<Dir>,
    access: u32,
    share: u32,
    identity: (u64, u128),
    entries: Option<cap_std::fs::ReadDir>,
    pattern: String,
    reservation: Option<std::sync::Arc<Reservation>>,
}
struct Reservation {
    file: File,
    top: std::sync::atomic::AtomicU64,
}
impl Reservation {
    fn release(&self) -> Result<(), u32> {
        #[cfg(target_os = "linux")]
        {
            use std::sync::atomic::Ordering;
            let end = self.file.metadata().map_err(status)?.len();
            let top = self.top.load(Ordering::Acquire);
            if top > end {
                // ext4 clamps hole punching to EOF, leaving KEEP_SIZE extents
                // beyond it allocated. Even a same-length truncate releases that
                // tail without changing EOF or data written into the reservation.
                self.file.set_len(end).map_err(status)?;
            }
            self.top.store(0, Ordering::Release);
        }
        Ok(())
    }
}
impl Drop for Reservation {
    fn drop(&mut self) {
        let _ = self.release();
    }
}
pub struct Drives {
    roots: BTreeMap<u32, Root>,
    handles: BTreeMap<u32, Handle>,
    next: u32,
}
impl Drives {
    pub fn new(mappings: &[DriveMapping]) -> Result<Self, String> {
        let mut roots = BTreeMap::new();
        for (i, m) in mappings.iter().enumerate() {
            let dir = Dir::open_ambient_dir(&m.path, cap_std::ambient_authority())
                .map_err(|_| format!("Cannot open redirected folder {}", m.name))?;
            roots.insert(
                i as u32 + 1,
                Root {
                    dir,
                    read_only: m.read_only,
                },
            );
        }
        Ok(Self {
            roots,
            handles: BTreeMap::new(),
            next: 1,
        })
    }
    pub fn process(&mut self, p: &[u8]) -> Result<Vec<u8>, String> {
        if p.len() < 24 || p[..4] != [0x72, 0x44, 0x52, 0x49] || p.len() > MAX_IO + 128 {
            return Err("Invalid RDP device request".into());
        }
        let device = u32_at(p, 4).unwrap();
        let file = u32_at(p, 8).unwrap();
        let major = u32_at(p, 16).unwrap();
        let minor = u32_at(p, 20).unwrap();
        Ok(complete(
            p,
            self.operation(device, file, major, minor, &p[24..]),
        ))
    }
    fn operation(
        &mut self,
        device: u32,
        file: u32,
        major: u32,
        minor: u32,
        b: &[u8],
    ) -> Result<Vec<u8>, u32> {
        let root = self.roots.get(&device).ok_or(INVALID)?;
        if major == 0 {
            if self.handles.len() >= 128 {
                return Err(0xc000009a);
            }
            let access = u32_at(b, 0)?;
            let share = u32_at(b, 16)?;
            let disposition = u32_at(b, 20)?;
            let options = u32_at(b, 24)?;
            let len = u32_at(b, 28)? as usize;
            let path = relative(&text(
                b.get(32..32usize.checked_add(len).ok_or(INVALID)?)
                    .ok_or(INVALID)?,
            )?)?;
            if disposition > 5 || share & !7 != 0 {
                return Err(INVALID);
            }
            // POSIX cannot bind unlink/rename to this opened object's identity.
            // Refuse before any create/truncate side effect, rather than deleting a replacement.
            if options & 0x1000 != 0 {
                return Err(UNSUPPORTED);
            }
            let writing = access & (0x40000000 | 0x10000000 | 2 | 4) != 0;
            if root.read_only
                && (writing || access & (0x10000 | 0x10 | 0x100) != 0 || disposition != 1)
            {
                return Err(DENIED);
            }
            let existing = match root.dir.symlink_metadata(&path) {
                Ok(m) => Some(m),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => return Err(status(e)),
            };
            #[cfg(windows)]
            if existing.is_none() && disposition == 1 && access == 0x00100080 && options == 0 {
                // xrdp's FUSE lookup requires STATUS_NO_SUCH_FILE before it can create the file.
                return Err(0xc000000f);
            }
            if existing
                .as_ref()
                .is_some_and(|m| !m.is_file() && !m.is_dir())
            {
                return Err(UNSUPPORTED);
            }
            let directory = options & 1 != 0 || existing.as_ref().is_some_and(|m| m.is_dir());
            if directory && options & 0x40 != 0 {
                return Err(INVALID);
            }
            let (file, dir, metadata) = if directory {
                if disposition == 2 || disposition == 3 && existing.is_none() {
                    root.dir.create_dir(&path).map_err(status)?;
                } else if disposition != 1 && disposition != 3 {
                    return Err(UNSUPPORTED);
                }
                let dir = root.dir.open_dir(&path).map_err(status)?;
                let metadata = dir.dir_metadata().map_err(status)?;
                (None, Some(dir), metadata)
            } else {
                if disposition != 1 && !writing {
                    return Err(DENIED);
                }
                let mut opt = OpenOptions::new();
                opt.read(access & (0x80000000 | 0x10000000 | 1) != 0 || !writing)
                    .write(writing);
                #[cfg(unix)]
                {
                    use cap_std::fs::OpenOptionsExt;
                    opt.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
                }
                #[cfg(windows)]
                {
                    use cap_std::fs::OpenOptionsExt;
                    // Request DELETE only when the remote handle owns that right.
                    // FILE_GENERIC_READ/WRITE include SYNCHRONIZE for synchronous files.
                    let native_access = if access & (0x80000000 | 0x10000000 | 1) != 0 || !writing {
                        0x120089
                    } else {
                        0
                    } | if writing { 0x120116 } else { 0 }
                        | if access & (0x10000 | 0x10000000) != 0 {
                            0x10000
                        } else {
                            0
                        };
                    opt.access_mode(native_access).share_mode(share);
                }
                match disposition {
                    0 | 3 | 5 => {
                        opt.create(true);
                    }
                    2 => {
                        opt.create_new(true);
                    }
                    _ => {}
                }
                let file = root.dir.open_with(&path, &opt).map_err(status)?;
                let metadata = file.metadata().map_err(status)?;
                if !metadata.is_file() {
                    return Err(UNSUPPORTED);
                }
                (Some(file), None, metadata)
            };
            #[cfg(not(windows))]
            let identity = identity(&metadata)?;
            #[cfg(windows)]
            let identity = if let Some(file) = &file {
                native_file_windows::identity(file)
            } else {
                native_file_windows::identity(dir.as_ref().ok_or(INVALID)?)
            }
            .map_err(status)?;
            let requested = access_categories(access);
            for handle in self.handles.values().filter(|h| h.identity == identity) {
                if requested & !handle.share != 0 || access_categories(handle.access) & !share != 0
                {
                    return Err(0xc0000043);
                }
            }
            if [0, 4, 5].contains(&disposition) {
                if access & (0x40000000 | 0x10000000 | 2) == 0 {
                    return Err(DENIED);
                }
                file.as_ref().ok_or(INVALID)?.set_len(0).map_err(status)?;
            }
            let info = match disposition {
                3 => 1,
                5 => 3,
                _ => 0,
            };
            let id = self.next;
            self.next = self.next.checked_add(1).ok_or(0xc000009au32)?;
            let reservation = self
                .handles
                .values()
                .find(|h| h.identity == identity)
                .and_then(|h| h.reservation.clone());
            self.handles.insert(
                id,
                Handle {
                    device,
                    file,
                    dir,
                    access,
                    share,
                    identity,
                    entries: None,
                    pattern: "*".into(),
                    reservation,
                },
            );
            let mut out = Vec::new();
            word(&mut out, id);
            out.push(info);
            return Ok(out);
        }
        let h = self
            .handles
            .get_mut(&file)
            .filter(|h| h.device == device)
            .ok_or(0xc0000008u32)?;
        match major {
            2 => {
                if let Some(reservation) = &h.reservation {
                    if std::sync::Arc::strong_count(reservation) == 1 {
                        reservation.release()?;
                    }
                }
                self.handles.remove(&file);
                Ok(vec![0; 5])
            }
            3 | 4 => {
                let len = u32_at(b, 0)? as usize;
                let offset = u64_at(b, 4)?;
                if len > MAX_IO || offset > i64::MAX as u64 {
                    return Err(INVALID);
                }
                if major == 3 && h.access & (0x80000000 | 0x10000000 | 1) == 0 {
                    return Err(DENIED);
                }
                if major == 4 && h.access & (0x40000000 | 0x10000000 | 2) == 0 {
                    return Err(DENIED);
                }
                let f = h.file.as_mut().ok_or(INVALID)?;
                f.seek(SeekFrom::Start(offset)).map_err(status)?;
                if major == 3 {
                    let mut data = vec![0; len];
                    let n = f.read(&mut data).map_err(status)?;
                    data.truncate(n);
                    let mut out = Vec::new();
                    word(&mut out, n as u32);
                    out.extend(data);
                    Ok(out)
                } else {
                    if root.read_only {
                        return Err(DENIED);
                    }
                    let data = b.get(32..32 + len).ok_or(INVALID)?;
                    f.write_all(data).map_err(status)?;
                    let mut out = Vec::new();
                    word(&mut out, len as u32);
                    out.push(0);
                    Ok(out)
                }
            }
            5 => {
                let class = u32_at(b, 0)?;
                let meta = if let Some(file) = &h.file {
                    file.metadata()
                } else {
                    h.dir.as_ref().ok_or(INVALID)?.dir_metadata()
                }
                .map_err(status)?;
                let data = file_info(&meta, class)?;
                #[cfg(windows)]
                let data = if class == 5 {
                    let mut data = data;
                    if let Some(file) = &h.file {
                        data[..8].copy_from_slice(
                            &native_file_windows::allocation_size(file)
                                .map_err(status)?
                                .to_le_bytes(),
                        );
                    }
                    data
                } else {
                    data
                };
                Ok(length_data(data))
            }
            6 => {
                if root.read_only {
                    return Err(DENIED);
                }
                let class = u32_at(b, 0)?;
                let len = u32_at(b, 4)? as usize;
                let data = b
                    .get(32..32usize.checked_add(len).ok_or(INVALID)?)
                    .ok_or(INVALID)?;
                match class {
                    19 | 20 => {
                        if h.access & (0x40000000 | 0x10000000 | 2) == 0 {
                            return Err(DENIED);
                        }
                        if data.len() != 8 {
                            return Err(INVALID);
                        }
                        let size = u64_at(data, 0)?;
                        if size > i64::MAX as u64 {
                            return Err(INVALID);
                        }
                        let file = h.file.as_mut().ok_or(INVALID)?;
                        if class == 20 {
                            file.set_len(size).map_err(status)?;
                        } else {
                            let reservation = match &h.reservation {
                                Some(reservation) => reservation.clone(),
                                None => std::sync::Arc::new(Reservation {
                                    file: file.try_clone().map_err(status)?,
                                    top: std::sync::atomic::AtomicU64::new(size),
                                }),
                            };
                            allocate(file, size)?;
                            reservation
                                .top
                                .fetch_max(size, std::sync::atomic::Ordering::AcqRel);
                            let identity = h.identity;
                            for handle in
                                self.handles.values_mut().filter(|h| h.identity == identity)
                            {
                                handle.reservation = Some(reservation.clone());
                            }
                        }
                    }
                    #[cfg(windows)]
                    10 | 13 => {
                        if h.access & (0x10000 | 0x10000000) == 0 {
                            return Err(DENIED);
                        }
                        let file = h.file.as_ref().ok_or(UNSUPPORTED)?;
                        if class == 13 {
                            if !data.is_empty() {
                                return Err(INVALID);
                            }
                            native_file_windows::disposition(file, true).map_err(status)?;
                        } else {
                            // MS-RDPEFS encodes RootDirectory as one reserved zero byte.
                            if data.len() < 6 || data[0] > 1 || data[1] != 0 {
                                return Err(INVALID);
                            }
                            let name_length = u32_at(data, 2)? as usize;
                            if data.len() != 6 + name_length {
                                return Err(INVALID);
                            }
                            let destination = relative(&text(&data[6..])?)?;
                            let name = destination.file_name().ok_or(INVALID)?;
                            let parent = destination
                                .parent()
                                .filter(|path| !path.as_os_str().is_empty())
                                .unwrap_or(std::path::Path::new("."));
                            let parent = root.dir.open_dir(parent).map_err(status)?;
                            native_file_windows::rename(
                                file,
                                &parent,
                                std::path::Path::new(name),
                                data[0] != 0,
                            )
                            .map_err(status)?;
                        }
                    }
                    _ => return Err(UNSUPPORTED),
                }
                let mut out = Vec::new();
                word(&mut out, len as u32);
                Ok(out)
            }
            10 => {
                let class = u32_at(b, 0)?;
                let data = volume_info(class, root)?;
                Ok(length_data(data))
            }
            12 if minor == 1 => {
                if h.dir.is_none() {
                    return Err(INVALID);
                }
                let class = u32_at(b, 0)?;
                let initial = *b.get(4).ok_or(INVALID)? != 0;
                if initial {
                    let len = u32_at(b, 5)? as usize;
                    let pattern = text(
                        b.get(32..32usize.checked_add(len).ok_or(INVALID)?)
                            .ok_or(INVALID)?,
                    )?;
                    h.pattern = pattern.rsplit(['\\', '/']).next().unwrap_or("*").into();
                    h.entries = Some(h.dir.as_ref().ok_or(INVALID)?.entries().map_err(status)?);
                }
                let entries = h.entries.as_mut().ok_or(INVALID)?;
                // Stream a single result without retaining a directory-sized allocation.
                // This scan runs on the filesystem worker, never the protocol or GUI task.
                for entry in entries.by_ref() {
                    let entry = entry.map_err(status)?;
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if !matches_pattern(&h.pattern, &name) {
                        continue;
                    }
                    let metadata = entry.metadata().map_err(status)?;
                    return Ok(length_data(directory_info(&name, &metadata, class)?));
                }
                Err(0x80000006)
            }
            14 => Err(UNSUPPORTED),
            9 => {
                h.file.as_mut().ok_or(INVALID)?.sync_all().map_err(status)?;
                Ok(vec![0; 4])
            }
            _ => Err(UNSUPPORTED),
        }
    }
}
fn allocate(file: &File, size: u64) -> Result<(), u32> {
    // Operate on the original opened object, never a peer-supplied path.
    // Allocation reserves storage without extending logical EOF; shrinking below
    // EOF truncates per MS-FSCC FileAllocationInformation.
    #[cfg(target_os = "linux")]
    {
        let old = file.metadata().map_err(status)?.len();
        if size < old {
            file.set_len(size).map_err(status)?;
        } else if size != 0 {
            rustix::fs::fallocate(file, rustix::fs::FallocateFlags::KEEP_SIZE, 0, size)
                .map_err(|e| status(e.into()))?;
        }
        Ok(())
    }
    #[cfg(windows)]
    {
        native_file_windows::allocate(file, size).map_err(status)
    }
    #[cfg(target_os = "macos")]
    {
        native_file_macos::allocate(file, size).map_err(status)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = (file, size);
        Err(UNSUPPORTED)
    }
}
fn length_data(data: Vec<u8>) -> Vec<u8> {
    let mut out = Vec::new();
    word(&mut out, data.len() as u32);
    out.extend(data);
    out
}
fn attributes(m: &cap_std::fs::Metadata) -> u32 {
    if m.is_dir() { 0x10 } else { 0x80 }
}
fn time(t: Result<cap_std::time::SystemTime, std::io::Error>) -> u64 {
    t.ok()
        .and_then(|t| t.into_std().duration_since(std::time::UNIX_EPOCH).ok())
        .map(|t| {
            t.as_secs()
                .saturating_add(11644473600)
                .saturating_mul(10000000)
                + t.subsec_nanos() as u64 / 100
        })
        .unwrap_or(0)
}
fn basic(m: &cap_std::fs::Metadata) -> Vec<u8> {
    let mut out = Vec::new();
    for t in [
        time(m.created()),
        time(m.accessed()),
        time(m.modified()),
        time(m.modified()),
    ] {
        quad(&mut out, t);
    }
    word(&mut out, attributes(m));
    out
}
fn allocation_size(m: &cap_std::fs::Metadata) -> u64 {
    #[cfg(unix)]
    {
        use cap_std::fs::MetadataExt;
        m.blocks().saturating_mul(512)
    }
    #[cfg(not(unix))]
    {
        m.len().div_ceil(4096).saturating_mul(4096)
    }
}
fn file_info(m: &cap_std::fs::Metadata, class: u32) -> Result<Vec<u8>, u32> {
    match class {
        4 => Ok(basic(m)),
        5 => {
            let mut out = Vec::new();
            quad(&mut out, allocation_size(m));
            quad(&mut out, m.len());
            word(&mut out, 1);
            out.extend([0, u8::from(m.is_dir())]);
            Ok(out)
        }
        35 => {
            let mut out = Vec::new();
            word(&mut out, attributes(m));
            word(&mut out, 0);
            Ok(out)
        }
        _ => Err(UNSUPPORTED),
    }
}
fn directory_info(name: &str, m: &cap_std::fs::Metadata, class: u32) -> Result<Vec<u8>, u32> {
    let name = wide(name);
    let mut out = vec![0; 8];
    if class == 12 {
        word(&mut out, name.len() as u32);
    } else {
        if ![1, 2, 3].contains(&class) {
            return Err(UNSUPPORTED);
        }
        out.extend(&basic(m)[..32]);
        quad(&mut out, m.len());
        quad(&mut out, allocation_size(m));
        word(&mut out, attributes(m));
        word(&mut out, name.len() as u32);
        if class == 2 || class == 3 {
            word(&mut out, 0);
        }
        if class == 3 {
            out.extend([0; 25]);
        }
    }
    out.extend(name);
    Ok(out)
}
fn volume_info(class: u32, root: &Root) -> Result<Vec<u8>, u32> {
    let mut out = Vec::new();
    match class {
        1 => {
            quad(&mut out, 0);
            word(&mut out, 1);
            let label = wide("TermCraft");
            word(&mut out, label.len() as u32);
            out.push(0);
            out.extend(label)
        }
        #[cfg(unix)]
        3 | 7 => {
            let stats = rustix::fs::fstatvfs(&root.dir).map_err(|e| status(e.into()))?;
            let sectors = (stats.f_frsize / 512).max(1).min(u32::MAX as u64);
            quad(&mut out, stats.f_blocks);
            quad(&mut out, stats.f_bavail);
            if class == 7 {
                quad(&mut out, stats.f_bfree);
            }
            word(&mut out, sectors as u32);
            word(&mut out, 512);
        }
        4 => {
            word(&mut out, 7);
            word(&mut out, 0);
        }
        5 => {
            word(&mut out, if root.read_only { 0x80000 } else { 0 });
            word(&mut out, 255);
            let name = wide("TermCraft");
            word(&mut out, name.len() as u32);
            out.extend(name)
        }
        _ => return Err(UNSUPPORTED),
    }
    Ok(out)
}
fn matches_pattern(pattern: &str, name: &str) -> bool {
    let p = pattern.to_lowercase();
    let n = name.to_lowercase();
    let p = if p == "*.*" { "*" } else { &p };
    let (p, n) = (p.as_bytes(), n.as_bytes());
    let (mut a, mut b, mut star, mut retry) = (0, 0, None, 0);
    while b < n.len() {
        if a < p.len() && (p[a] == b'?' || p[a] == n[b]) {
            a += 1;
            b += 1;
        } else if a < p.len() && p[a] == b'*' {
            star = Some(a);
            a += 1;
            retry = b;
        } else if let Some(s) = star {
            retry += 1;
            b = retry;
            a = s + 1;
        } else {
            return false;
        }
    }
    while a < p.len() && p[a] == b'*' {
        a += 1
    }
    a == p.len()
}
fn header(id: u16) -> Vec<u8> {
    let mut p = vec![0x72, 0x44];
    p.extend(id.to_le_bytes());
    p
}

type DeviceCompletion = (Vec<u8>, Option<&'static str>);

/// The SVC processor never performs filesystem I/O on the protocol task.
pub struct Channel {
    tx: mpsc::SyncSender<Vec<u8>>,
    rx: mpsc::Receiver<Result<DeviceCompletion, String>>,
    notice: Option<&'static str>,
    announce: Vec<u8>,
    has_drives: bool,
    has_ports: bool,
    printer_id: Option<u32>,
    smartcard: Option<(u32, smartcard::Worker)>,
    smartcard_announced: bool,
    peer_supports_smartcard: bool,
    confirmed: bool,
    announced: bool,
    pending: usize,
    #[cfg(any(unix, windows))]
    serial_ids: std::ops::Range<u32>,
    closed: Arc<AtomicBool>,
}
impl std::fmt::Debug for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DeviceRedirectionChannel")
    }
}
impl AsAny for Channel {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
impl Drop for Channel {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Release);
    }
}

/// Serial devices selected for one connection, shared by its certificate attempts.
pub struct BoundSerialMappings {
    #[cfg(any(unix, windows))]
    ports: Vec<serial::Binding>,
}

impl BoundSerialMappings {
    pub fn new(mappings: &[SerialMapping]) -> Result<Self, String> {
        #[cfg(not(any(unix, windows)))]
        if !mappings.is_empty() {
            return Err("Embedded RDP serial redirection is unsupported on this platform".into());
        }
        Ok(Self {
            #[cfg(any(unix, windows))]
            ports: serial::bind(mappings)?,
        })
    }
}

#[cfg(any(windows, target_os = "macos"))]
pub(crate) fn printer_child() -> Result<(), String> {
    printer::child()
}

/// Local printer selected once for a connection, including its certificate retries.
#[derive(Default)]
pub struct BoundPrinterMapping {
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    binding: Option<printer::Binding>,
}

impl BoundPrinterMapping {
    pub fn new(mapping: Option<&PrinterMapping>) -> Result<Self, String> {
        #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
        if mapping.is_some() {
            return Err("Embedded RDP printer redirection is unsupported on this platform".into());
        }
        Ok(Self {
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            binding: mapping.map(printer::Binding::new).transpose()?,
        })
    }

    #[cfg(all(test, target_os = "linux"))]
    pub fn at(mapping: &PrinterMapping, socket: &std::path::Path) -> Result<Self, String> {
        Ok(Self {
            binding: Some(printer::Binding::at(mapping, socket)?),
        })
    }

    pub async fn check_ready(&self) -> Result<(), String> {
        #[cfg(any(target_os = "linux", target_os = "macos", windows))]
        if let Some(binding) = &self.binding {
            binding.check_ready().await?;
        }
        Ok(())
    }

    fn announce(&self) -> Option<Vec<u8>> {
        #[cfg(any(target_os = "linux", target_os = "macos", windows))]
        return self.binding.as_ref().map(printer::Binding::announcement);
        #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
        None
    }
}

impl Channel {
    pub async fn attach_with_smartcard(
        connector: &mut ironrdp_connector::ClientConnector,
        options: &RdpOptions,
        serial: &BoundSerialMappings,
        printer: &BoundPrinterMapping,
    ) -> Result<(), String> {
        let smartcard = smartcard::Worker::start(options.smartcard_reader.as_deref()).await?;
        Self::attach_devices(connector, options, serial, printer, smartcard)
    }

    #[cfg(test)]
    pub fn attach(
        connector: &mut ironrdp_connector::ClientConnector,
        options: &RdpOptions,
        serial: &BoundSerialMappings,
        printer: &BoundPrinterMapping,
    ) -> Result<(), String> {
        Self::attach_devices(connector, options, serial, printer, None)
    }

    fn attach_devices(
        connector: &mut ironrdp_connector::ClientConnector,
        options: &RdpOptions,
        serial: &BoundSerialMappings,
        printer: &BoundPrinterMapping,
        smartcard: Option<smartcard::Worker>,
    ) -> Result<(), String> {
        if !options.drives.is_empty()
            || !options.serial_mappings.is_empty()
            || options.printer_mapping.is_some()
            || smartcard.is_some()
        {
            connector.attach_static_channel(Self::with_providers(
                &options.drives,
                serial,
                printer,
                smartcard,
            )?);
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn new(mappings: &[DriveMapping]) -> Result<Self, String> {
        Self::with_serial(mappings, &[])
    }

    #[cfg(test)]
    pub fn with_serial(drives: &[DriveMapping], ports: &[SerialMapping]) -> Result<Self, String> {
        Self::with_bound_serial(drives, &BoundSerialMappings::new(ports)?)
    }

    #[cfg(test)]
    pub fn with_bound_serial(
        drives: &[DriveMapping],
        serial: &BoundSerialMappings,
    ) -> Result<Self, String> {
        Self::with_bound_devices(drives, serial, &BoundPrinterMapping::default())
    }

    #[cfg(test)]
    pub fn with_bound_devices(
        drives: &[DriveMapping],
        serial: &BoundSerialMappings,
        printer: &BoundPrinterMapping,
    ) -> Result<Self, String> {
        Self::with_providers(drives, serial, printer, None)
    }

    fn with_providers(
        drives: &[DriveMapping],
        serial: &BoundSerialMappings,
        printer: &BoundPrinterMapping,
        smartcard: Option<smartcard::Worker>,
    ) -> Result<Self, String> {
        #[cfg(any(unix, windows))]
        let names: Vec<&str> = serial.ports.iter().map(|port| port.name.as_str()).collect();
        #[cfg(not(any(unix, windows)))]
        let names = {
            let _ = serial;
            Vec::new()
        };
        let has_drives = !drives.is_empty();
        let has_ports = !names.is_empty();
        let printer_data = printer.announce();
        let printer_id = printer_data
            .as_ref()
            .map(|_| (drives.len() + names.len()) as u32 + 1);
        let announce = announce_devices(drives, &names, printer_data.as_deref());
        let smartcard = smartcard.map(|worker| {
            let id = (drives.len() + names.len() + usize::from(printer_id.is_some())) as u32 + 1;
            (id, worker)
        });
        #[cfg(any(target_os = "linux", target_os = "macos", windows))]
        let mut printer = printer
            .binding
            .clone()
            .zip(printer_id)
            .map(|(binding, id)| printer::Printer::new(binding, id));
        #[cfg(any(unix, windows))]
        let serial_ids = (drives.len() as u32 + 1)..((drives.len() + names.len()) as u32 + 1);
        let drives = drives.to_vec();
        #[cfg(any(unix, windows))]
        let ports = serial.ports.clone();
        // Keep one close position beyond all other requests, including the serial
        // mask reset escape that can release a wait without closing its handle.
        let queue_limit = DEVICE_IO_CREDITS + 1;
        #[cfg(any(unix, windows))]
        let queue_limit = queue_limit + usize::from(!serial_ids.is_empty());
        let (tx, jobs) = mpsc::sync_channel::<Vec<u8>>(queue_limit);
        let (results, rx) = mpsc::sync_channel(queue_limit);
        let closed = Arc::new(AtomicBool::new(false));
        let cancelled = Arc::clone(&closed);
        std::thread::Builder::new()
            .name("rdp-devices".into())
            .spawn(move || {
                let mut drives = match Drives::new(&drives) {
                    Ok(drives) => drives,
                    Err(error) => {
                        let _ = results.send(Err(error));
                        return;
                    }
                };
                #[cfg(any(unix, windows))]
                let mut serial = serial::Serial::new(drives.roots.len() as u32 + 1, ports);
                loop {
                    if cancelled.load(Ordering::Acquire) {
                        break;
                    }
                    #[cfg(any(unix, windows))]
                    for response in serial.ready() {
                        if results.send(Ok((response, None))).is_err() {
                            return;
                        }
                    }
                    #[cfg(any(unix, windows))]
                    let request = if serial.has_pending() {
                        match jobs.recv_timeout(std::time::Duration::from_millis(5)) {
                            Ok(request) => Some(request),
                            Err(mpsc::RecvTimeoutError::Timeout) => None,
                            Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        }
                    } else {
                        let Ok(request) = jobs.recv() else { break };
                        Some(request)
                    };
                    #[cfg(not(any(unix, windows)))]
                    let request = match jobs.recv() {
                        Ok(request) => Some(request),
                        Err(_) => break,
                    };
                    // An mpsc receiver can still drain queued work after its sender drops.
                    if cancelled.load(Ordering::Acquire) {
                        break;
                    }
                    if let Some(request) = request {
                        #[cfg(any(target_os = "linux", target_os = "macos", windows))]
                        if let Some(printer) = printer
                            .as_mut()
                            .filter(|p| p.contains(u32_at(&request, 4).unwrap()))
                        {
                            if results
                                .send(Ok(printer.process(&request, &cancelled)))
                                .is_err()
                            {
                                return;
                            }
                            continue;
                        }
                        #[cfg(any(unix, windows))]
                        let response = if serial.contains(u32_at(&request, 4).unwrap()) {
                            serial.process(request).map(Ok)
                        } else {
                            Some(drives.process(&request))
                        };
                        #[cfg(not(any(unix, windows)))]
                        let response = Some(drives.process(&request));
                        if let Some(response) = response
                            && results.send(response.map(|packet| (packet, None))).is_err()
                        {
                            return;
                        }
                    }
                }
            })
            .map_err(|_| "Cannot start RDP device worker")?;
        Ok(Self {
            tx,
            rx,
            notice: None,
            announce,
            has_drives,
            has_ports,
            printer_id,
            smartcard,
            smartcard_announced: false,
            peer_supports_smartcard: false,
            confirmed: false,
            announced: false,
            pending: 0,
            #[cfg(any(unix, windows))]
            serial_ids,
            closed,
        })
    }
    pub fn poll(&mut self) -> Result<Vec<SvcMessage>, String> {
        let mut out = if let Some((_, worker)) = &mut self.smartcard {
            if self.confirmed && !self.peer_supports_smartcard {
                return Err("The RDP server did not advertise smartcard redirection".into());
            }
            worker.poll()?
        } else {
            Vec::new()
        };
        while let Ok(result) = self.rx.try_recv() {
            self.pending = self.pending.saturating_sub(1);
            let (packet, notice) = result?;
            if notice.is_some() {
                self.notice = notice;
            }
            out.push(packet.into());
        }
        Ok(out)
    }

    pub fn take_notice(&mut self) -> Option<&'static str> {
        self.notice.take()
    }

    fn packet(&mut self, p: &[u8]) -> Result<Vec<SvcMessage>, String> {
        #[cfg(any(target_os = "linux", target_os = "macos", windows))]
        if self.printer_id.is_some() && self.announced && p.starts_with(b"RP") {
            return printer::discard_cache(p).map(|()| Vec::new());
        }
        if p.len() < 4 || p[..2] != [0x72, 0x44] {
            return Err("Invalid device-channel packet".into());
        }
        let id = u16::from_le_bytes(p[2..4].try_into().unwrap());
        let mut out = Vec::new();
        match id {
            0x496e => {
                if self.smartcard_announced {
                    return Err(
                        "RDP device reinitialization requires a fresh smartcard session".into(),
                    );
                }
                if p.len() != 12 || p[4..6] != [1, 0] {
                    return Err("Unsupported RDP device protocol version".into());
                }
                let mut reply = header(0x4343);
                reply.extend(&p[4..12]);
                out.push(reply.into());
                let mut name = header(0x434e);
                word(&mut name, 1);
                word(&mut name, 0);
                let data = wide("TermCraft\0");
                word(&mut name, data.len() as u32);
                name.extend(data);
                out.push(name.into());
            }
            0x4343 => {
                if p.len() != 12 {
                    return Err("Invalid RDP device confirmation".into());
                }
                self.confirmed = true;
                if let Some((id, _)) = self.smartcard.as_ref()
                    && self.peer_supports_smartcard
                    && !self.smartcard_announced
                {
                    out.push(announce_smartcard(*id).into());
                    self.smartcard_announced = true;
                }
            }
            0x554c => {
                if !self.confirmed {
                    return Err("RDP device logon preceded confirmation".into());
                }
                if !self.announced {
                    if self.announce.len() > 8 || self.smartcard.is_none() {
                        out.push(self.announce.clone().into());
                    }
                    self.announced = true;
                }
            }
            0x6472 => {
                if p.len() != 12 {
                    return Err("Invalid RDP device reply".into());
                }
                if u32_at(p, 8).unwrap() != 0 {
                    return Err("Server rejected redirected device".into());
                }
            }
            0x5350 => {
                if p.len() < 8 {
                    return Err("Invalid RDP device capabilities".into());
                }
                let smartcard = if self.smartcard.is_some() {
                    self.peer_supports_smartcard = peer_smartcard_capability(p)?;
                    self.peer_supports_smartcard
                } else {
                    false
                };
                out.push(
                    capabilities(
                        self.has_drives,
                        self.has_ports,
                        self.printer_id.is_some(),
                        smartcard,
                    )
                    .into(),
                );
            }

            0x4952 => {
                if p.len() < 24 {
                    return Err("Invalid RDP device request".into());
                }
                if let Some((id, worker)) = &mut self.smartcard
                    && u32_at(p, 4) == Ok(*id)
                {
                    if !self.smartcard_announced {
                        return Err("RDP smartcard request preceded announcement".into());
                    }
                    out.extend(worker.enqueue(p)?);
                    return Ok(out);
                }
                if !self.announced {
                    return Err("RDP device request preceded announcement".into());
                }
                let major = u32_at(p, 16);
                let limit = DEVICE_IO_CREDITS + usize::from(major == Ok(2));
                #[cfg(any(unix, windows))]
                let limit = limit
                    + usize::from(
                        !self.serial_ids.is_empty()
                            && (major == Ok(2)
                                || (major == Ok(14)
                                    && u32_at(p, 20) == Ok(0)
                                    && self.serial_ids.contains(&u32_at(p, 4).unwrap())
                                    && serial::is_wait_mask_set(&p[24..]))),
                    );
                if p.len() > MAX_IO + 128 || self.pending >= limit {
                    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
                    if self.printer_id == u32_at(p, 4).ok() {
                        // Retire the connection if a printer write cannot be queued.
                        // A later close must never submit a document with dropped data.
                        if u32_at(p, 16) == Ok(4) {
                            self.closed.store(true, Ordering::Release);
                            return Err("RDP printer request exceeds the device I/O limits".into());
                        }
                        let error = if self.pending >= limit {
                            0x80000011
                        } else {
                            INVALID
                        };
                        out.push(printer::response(p, Err(error)).into());
                        return Ok(out);
                    }
                    out.push(
                        failure(
                            p,
                            if self.pending >= limit {
                                0x80000011
                            } else {
                                INVALID
                            },
                        )
                        .into(),
                    );
                    return Ok(out);
                }
                self.tx
                    .try_send(p.to_vec())
                    .map_err(|_| "RDP device queue is unavailable or full")?;
                self.pending += 1;
            }
            _ => return Err("Unsupported RDP device-channel packet".into()),
        }
        Ok(out)
    }
}
impl SvcProcessor for Channel {
    fn channel_name(&self) -> ChannelName {
        ChannelName::from_static(b"rdpdr\0\0\0")
    }
    fn process(&mut self, p: &[u8]) -> PduResult<Vec<SvcMessage>> {
        self.packet(p)
            .map_err(|_| ironrdp_pdu::pdu_other_err!("Invalid RDP device channel"))
    }
}
impl SvcClientProcessor for Channel {}
fn peer_smartcard_capability(packet: &[u8]) -> Result<bool, String> {
    let Some(header) = packet.get(4..8) else {
        return Err("Truncated RDP device capability header".into());
    };
    let count = u16::from_le_bytes([header[0], header[1]]);
    let mut sets = &packet[8..];
    let mut smartcard = false;
    for _ in 0..count {
        let Some(header) = sets.get(..8) else {
            return Err("Truncated RDP device capability".into());
        };
        let kind = u16::from_le_bytes([header[0], header[1]]);
        let length = u16::from_le_bytes([header[2], header[3]]) as usize;
        if !(8..=sets.len()).contains(&length) {
            return Err("Invalid RDP device capability length".into());
        }
        if kind == 5 {
            smartcard = length == 8 && u32_at(header, 4) == Ok(1);
        }
        sets = &sets[length..];
    }
    if !sets.is_empty() {
        return Err("Unexpected trailing RDP device capability data".into());
    }
    Ok(smartcard)
}

fn announce_smartcard(id: u32) -> Vec<u8> {
    let mut packet = header(0x4441);
    word(&mut packet, 1);
    announce_device(&mut packet, 0x20, id, "SCARD", &[]);
    packet
}

fn capabilities(drives: bool, ports: bool, printer: bool, smartcard: bool) -> Vec<u8> {
    let mut p = header(0x4350);
    p.extend(
        (1 + u16::from(drives) + u16::from(ports) + u16::from(printer) + u16::from(smartcard))
            .to_le_bytes(),
    );
    p.extend([0; 2]);
    p.extend(1u16.to_le_bytes());
    p.extend(44u16.to_le_bytes());
    word(&mut p, 2);
    word(&mut p, 0);
    word(&mut p, 0);
    p.extend(1u16.to_le_bytes());
    p.extend(12u16.to_le_bytes());
    word(&mut p, 0xffff);
    word(&mut p, 0);
    word(&mut p, if smartcard { 6 } else { 4 });
    word(&mut p, 0);
    word(&mut p, 0);
    word(&mut p, u32::from(smartcard)); // SpecialTypeDeviceCap counts the pre-logon SCARD entry.
    if printer {
        p.extend(2u16.to_le_bytes());
        p.extend(8u16.to_le_bytes());
        word(&mut p, 1);
    }
    if ports {
        p.extend(3u16.to_le_bytes());
        p.extend(8u16.to_le_bytes());
        word(&mut p, 1);
    }
    if drives {
        p.extend(4u16.to_le_bytes());
        p.extend(8u16.to_le_bytes());
        word(&mut p, 2);
    }
    if smartcard {
        p.extend(5u16.to_le_bytes());
        p.extend(8u16.to_le_bytes());
        word(&mut p, 1);
    }
    p
}

#[cfg(test)]
pub(crate) fn announce_mappings(mappings: &[DriveMapping]) -> Vec<u8> {
    announce_devices(mappings, &[], None)
}

fn announce_devices(drives: &[DriveMapping], ports: &[&str], printer: Option<&[u8]>) -> Vec<u8> {
    let mut p = header(0x4441);
    let count = drives.len() + ports.len();
    word(&mut p, (count + usize::from(printer.is_some())) as u32);
    for (i, mapping) in drives.iter().enumerate() {
        let data = wide(&format!("{}\0", mapping.name));
        announce_device(&mut p, 8, i as u32 + 1, &mapping.name, &data);
    }
    for (i, name) in ports.iter().enumerate() {
        let id = (drives.len() + i) as u32 + 1;
        announce_device(&mut p, 1, id, name, &[]);
    }
    if let Some(data) = printer {
        let port = (1..)
            .map(|n| format!("PRN{n}"))
            .find(|port| {
                !drives
                    .iter()
                    .any(|drive| drive.name.eq_ignore_ascii_case(port))
            })
            .unwrap();
        announce_device(&mut p, 4, count as u32 + 1, &port, data);
    }
    p
}

fn announce_device(p: &mut Vec<u8>, kind: u32, id: u32, name: &str, data: &[u8]) {
    word(p, kind);
    word(p, id);
    let mut preferred = [0; 8];
    let name = name.as_bytes();
    preferred[..name.len().min(7)].copy_from_slice(&name[..name.len().min(7)]);
    p.extend(preferred);
    word(p, data.len() as u32);
    p.extend(data);
}

fn access_categories(access: u32) -> u32 {
    let mut categories = 0;
    if access & (0x80000000 | 0x10000000 | 1) != 0 {
        categories |= 1;
    }
    if access & (0x40000000 | 0x10000000 | 2 | 4) != 0 {
        categories |= 2;
    }
    if access & (0x10000000 | 0x10000) != 0 {
        categories |= 4;
    }
    categories
}
#[cfg(not(windows))]
fn identity(metadata: &cap_std::fs::Metadata) -> Result<(u64, u128), u32> {
    #[cfg(unix)]
    {
        use cap_std::fs::MetadataExt;
        Ok((metadata.dev(), metadata.ino() as u128))
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        Err(UNSUPPORTED)
    }
}
fn complete(request: &[u8], result: Result<Vec<u8>, u32>) -> Vec<u8> {
    match result {
        Ok(body) => {
            let mut p = completion_header(request, 0);
            p.extend(body);
            p
        }
        Err(error) => failure(request, error),
    }
}

fn completion_header(request: &[u8], status: u32) -> Vec<u8> {
    let mut p = header(0x4943);
    p.extend(&request[4..8]);
    p.extend(&request[12..16]);
    word(&mut p, status);
    p
}

fn failure(request: &[u8], status: u32) -> Vec<u8> {
    let mut p = completion_header(request, status);
    word(&mut p, 0);
    if matches!(u32_at(request, 16), Ok(0) | Ok(4)) {
        p.push(0);
    }
    p
}

#[cfg(test)]
mod smartcard_tests {
    use super::*;

    #[test]
    fn smartcard_capability_and_announcement_require_a_matching_peer_capability() {
        let mut peer = capabilities(false, false, false, true);
        peer[2..4].copy_from_slice(&0x5350u16.to_le_bytes());
        assert_eq!(peer_smartcard_capability(&peer), Ok(true));
        assert_eq!(
            peer_smartcard_capability(&capabilities(false, false, false, false)),
            Ok(false)
        );
        assert!(peer_smartcard_capability(&peer[..peer.len() - 1]).is_err());
        let packet = announce_smartcard(34);
        assert_eq!(&packet[..4], b"rDAD");
        assert_eq!(u32_at(&packet, 4), Ok(1));
        assert_eq!(u32_at(&packet, 8), Ok(0x20));
        assert_eq!(u32_at(&packet, 12), Ok(34));
        assert_eq!(&packet[16..24], b"SCARD\0\0\0");
        assert_eq!(u32_at(&packet, 24), Ok(0));
    }

    #[tokio::test]
    async fn default_does_not_attach_the_smartcard_or_device_channel() {
        let options = RdpOptions::default();
        let config = crate::wire::Config {
            version: crate::wire::VERSION,
            connect_to: None,
            carrier_token: None,
            host: "localhost".into(),
            port: 3389,
            user: String::new(),
            width: 640,
            height: 480,
            options: options.clone(),
            timeout_secs: 5,
            remembered_certificate: None,
            certificate_authority: None,
        };
        let mut connector = ironrdp_connector::ClientConnector::new(
            crate::config::connector(&config),
            "127.0.0.1:3389".parse().unwrap(),
        );
        Channel::attach_with_smartcard(
            &mut connector,
            &options,
            &BoundSerialMappings::new(&[]).unwrap(),
            &BoundPrinterMapping::default(),
        )
        .await
        .unwrap();
        assert!(
            connector
                .get_static_channel_processor::<Channel>()
                .is_none()
        );
    }
}
