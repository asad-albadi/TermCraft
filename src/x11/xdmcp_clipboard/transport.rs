//! Bounded authenticated local-only X11 transport; never modifies process environment.
use std::{
    io::{self, IoSlice},
    os::fd::AsRawFd,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use x11rb::{
    rust_connection::{DefaultStream, PollMode, RustConnection, Stream},
    utils::RawFdContainer,
};
fn error(e: impl std::fmt::Display) -> io::Error {
    io::Error::other(e.to_string())
}
pub(super) struct Timed {
    stream: DefaultStream,
    deadline: Mutex<Instant>,
    cancelled: Arc<AtomicBool>,
}
fn active(cancelled: &AtomicBool, deadline: Instant) -> io::Result<Duration> {
    if cancelled.load(Ordering::Relaxed) {
        return Err(io::ErrorKind::Interrupted.into());
    }
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(io::ErrorKind::TimedOut.into());
    }
    Ok(remaining)
}
impl Timed {
    fn remaining(&self) -> io::Result<Duration> {
        active(&self.cancelled, *self.deadline.lock().unwrap())
    }
}
impl Stream for Timed {
    fn poll(&self, mode: PollMode) -> io::Result<()> {
        loop {
            let remain = self.remaining()?;
            let mut fd = libc::pollfd {
                fd: self.stream.as_raw_fd(),
                events: (if mode.readable() { libc::POLLIN } else { 0 })
                    | (if mode.writable() { libc::POLLOUT } else { 0 }),
                revents: 0,
            };
            let result = unsafe { libc::poll(&mut fd, 1, remain.as_millis().min(20) as i32) };
            if result > 0 {
                return Ok(());
            }
            if result < 0 {
                let e = io::Error::last_os_error();
                if e.kind() != io::ErrorKind::Interrupted {
                    return Err(e);
                }
            }
        }
    }
    fn read(&self, b: &mut [u8], fds: &mut Vec<RawFdContainer>) -> io::Result<usize> {
        self.remaining()?;
        self.stream.read(b, fds)
    }
    fn write(&self, b: &[u8], fds: &mut Vec<RawFdContainer>) -> io::Result<usize> {
        self.remaining()?;
        self.stream.write(b, fds)
    }
    fn write_vectored(
        &self,
        b: &[IoSlice<'_>],
        fds: &mut Vec<RawFdContainer>,
    ) -> io::Result<usize> {
        self.remaining()?;
        self.stream.write_vectored(b, fds)
    }
}
fn authority_cookie(
    path: &std::path::Path,
    display: u16,
    address: &[u8],
) -> io::Result<(Vec<u8>, Vec<u8>)> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)?;
    if !file.metadata()?.is_file() || file.metadata()?.len() > 65536 {
        return Err(error("Invalid Xauthority file"));
    }
    let mut bytes = zeroize::Zeroizing::new(Vec::new());
    file.take(65537).read_to_end(&mut bytes)?;
    if bytes.len() > 65536 {
        return Err(error("Xauthority exceeds limit"));
    }
    let mut rest = bytes.as_slice();
    fn field<'a>(rest: &mut &'a [u8]) -> io::Result<&'a [u8]> {
        if rest.len() < 2 {
            return Err(error("Truncated Xauthority"));
        }
        let n = u16::from_be_bytes([rest[0], rest[1]]) as usize;
        *rest = &rest[2..];
        if rest.len() < n {
            return Err(error("Truncated Xauthority"));
        }
        let (b, r) = rest.split_at(n);
        *rest = r;
        Ok(b)
    }
    while !rest.is_empty() {
        if rest.len() < 2 {
            return Err(error("Truncated Xauthority"));
        }
        let family = u16::from_be_bytes([rest[0], rest[1]]);
        rest = &rest[2..];
        let addr = field(&mut rest)?;
        let number = field(&mut rest)?;
        let name = field(&mut rest)?;
        let data = field(&mut rest)?;
        if (family == 65535 || family == 256 && addr == address)
            && (number.is_empty() || number == display.to_string().as_bytes())
            && name == b"MIT-MAGIC-COOKIE-1"
            && data.len() == 16
        {
            return Ok((name.to_vec(), data.to_vec()));
        }
    }
    Err(error("No matching Xauthority cookie"))
}

pub(super) type Conn = RustConnection<Timed>;
pub(super) fn local_display(display: &str) -> io::Result<(u16, usize)> {
    let body = display
        .strip_prefix(':')
        .ok_or_else(|| error("Select a local parent X11 display such as :0"))?;
    let mut parts = body.split('.');
    let number = parts.next().unwrap_or("");
    let screen = parts.next().unwrap_or("0");
    if number.is_empty()
        || screen.is_empty()
        || !number.bytes().all(|b| b.is_ascii_digit())
        || !screen.bytes().all(|b| b.is_ascii_digit())
        || parts.next().is_some()
    {
        return Err(error("Invalid local X11 display"));
    }
    let number: u16 = number.parse().map_err(error)?;
    let screen: u8 = screen.parse().map_err(error)?;
    if number > 59535 {
        return Err(error("Local display exceeds supported range"));
    }
    Ok((number, screen as usize))
}
pub(super) fn connect(
    stream: std::os::unix::net::UnixStream,
    screen: usize,
    auth: (Vec<u8>, Vec<u8>),
    cancelled: Arc<AtomicBool>,
    deadline: Instant,
) -> io::Result<Conn> {
    active(&cancelled, deadline)?;
    stream.set_nonblocking(true)?;
    let (stream, _) = DefaultStream::from_unix_stream(stream)?;
    RustConnection::connect_to_stream_with_auth_info(
        Timed {
            stream,
            deadline: Mutex::new(deadline),
            cancelled,
        },
        screen,
        auth.0,
        auth.1,
    )
    .map_err(error)
}
/// Initialization may narrow/set the phase deadline, never allocate another budget.
pub(super) fn initialize_until(conn: &Conn, deadline: Instant) -> io::Result<()> {
    let mut current = conn.stream().deadline.lock().unwrap();
    let phase_deadline = (*current).min(deadline);
    active(&conn.stream().cancelled, phase_deadline)?;
    *current = phase_deadline;
    Ok(())
}
/// A resumed parent request gets its ordinary I/O allowance within the reset budget.
pub(super) fn begin_recovery_request(conn: &Conn, recovery_deadline: Instant) -> io::Result<()> {
    let deadline = recovery_deadline.min(Instant::now() + Duration::from_millis(250));
    active(&conn.stream().cancelled, deadline)?;
    *conn.stream().deadline.lock().unwrap() = deadline;
    Ok(())
}
pub(super) fn renew(conn: &Conn) {
    *conn.stream().deadline.lock().unwrap() = Instant::now() + Duration::from_millis(250);
}
#[cfg(test)]
pub(super) fn parent(
    display: &str,
    authority: Option<&Path>,
    cancelled: Arc<AtomicBool>,
) -> io::Result<(Conn, i32)> {
    parent_until(
        display,
        authority,
        cancelled,
        Instant::now() + Duration::from_secs(2),
    )
}
pub(super) fn parent_until(
    display: &str,
    authority: Option<&Path>,
    cancelled: Arc<AtomicBool>,
    deadline: Instant,
) -> io::Result<(Conn, i32)> {
    active(&cancelled, deadline)?;
    let (number, screen) = local_display(display)?;
    let stream = super::super::xdmcp_engine::connect_ready_socket(
        Path::new(&format!("/tmp/.X11-unix/X{number}")),
        deadline,
        &cancelled,
    )
    .map_err(error)?;
    let peer = peer(&stream)?;
    if peer.pid <= 0 || !(peer.uid == unsafe { libc::geteuid() } || peer.uid == 0) {
        return Err(error("Selected parent X11 socket has an unexpected owner"));
    }
    let parsed = x11rb::reexports::x11rb_protocol::parse_display::parse_display(Some(display))
        .map_err(error)?;
    // x11rb uses gethostname for FamilyLocal Xauthority matching.
    let address = x11rb::rust_connection::DefaultStream::from_unix_stream(stream.try_clone()?)?
        .1
        .1;
    let selected = authority
        .map(Path::to_path_buf)
        .or_else(|| std::env::var_os("XAUTHORITY").map(Into::into))
        .or_else(|| {
            std::env::var_os("HOME").map(|p| std::path::PathBuf::from(p).join(".Xauthority"))
        });
    let auth = match selected {
        Some(p) if authority.is_some() || p.exists() => {
            authority_cookie(&p, parsed.display, &address)?
        }
        _ => (Vec::new(), Vec::new()),
    };
    Ok((
        connect(stream, screen, auth, cancelled, deadline)?,
        peer.pid,
    ))
}
pub(super) fn peer(stream: &std::os::unix::net::UnixStream) -> io::Result<libc::ucred> {
    let mut peer = std::mem::MaybeUninit::<libc::ucred>::uninit();
    let mut size = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    if unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            peer.as_mut_ptr().cast(),
            &mut size,
        )
    } < 0
        || size as usize != std::mem::size_of::<libc::ucred>()
    {
        return Err(error("Could not verify selected local X11 peer"));
    }
    Ok(unsafe { peer.assume_init() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, os::unix::fs::PermissionsExt, sync::mpsc};
    #[test]
    fn selected_authority_is_bounded_regular_and_matches_cookie_record() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth");
        let mut bytes = 65535u16.to_be_bytes().to_vec();
        for field in [b"".as_slice(), b"177", b"MIT-MAGIC-COOKIE-1", &[0x51; 16]] {
            bytes.extend_from_slice(&(field.len() as u16).to_be_bytes());
            bytes.extend_from_slice(field);
        }
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(&bytes).unwrap();
        drop(file);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            authority_cookie(&path, 177, b"host").unwrap().1,
            vec![0x51; 16]
        );
        assert!(authority_cookie(&path, 178, b"host").is_err());
        std::fs::write(&path, &bytes[..7]).unwrap();
        assert!(authority_cookie(&path, 177, b"host").is_err());
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(65537).unwrap();
        drop(file);
        assert!(authority_cookie(&path, 177, b"host").is_err());
        let fifo = dir.path().join("fifo");
        let c = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        let start = Instant::now();
        assert!(authority_cookie(&fifo, 177, b"host").is_err());
        assert!(start.elapsed() < Duration::from_millis(100));
    }
    #[test]
    fn stalled_x11_handshake_is_cancelled_and_worker_joined() {
        let (client, _server) = std::os::unix::net::UnixStream::pair().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let (send, receive) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            send.send(
                connect(
                    client,
                    0,
                    (Vec::new(), Vec::new()),
                    flag,
                    Instant::now() + Duration::from_secs(2),
                )
                .is_err(),
            )
            .unwrap();
        });
        std::thread::sleep(Duration::from_millis(30));
        let start = Instant::now();
        stop.store(true, Ordering::Relaxed);
        assert!(receive.recv_timeout(Duration::from_millis(500)).unwrap());
        worker.join().unwrap();
        assert!(start.elapsed() < Duration::from_millis(500));
    }
    #[test]
    fn unix_connection_identity_is_descriptor_based() {
        let (a, _b) = std::os::unix::net::UnixStream::pair().unwrap();
        let peer = peer(&a).unwrap();
        assert_eq!(peer.pid, std::process::id() as i32);
        assert_eq!(peer.uid, unsafe { libc::geteuid() });
    }
}
