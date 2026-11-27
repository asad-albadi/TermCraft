//! Deadline-bound X11 connection over an already-owned Unix socket.
use std::{
    io::{self, IoSlice},
    os::fd::AsRawFd,
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

// Keep transport failures typed through x11rb's request/setup wrappers. Only
// disconnected transports may be retried; authorization and protocol errors stay fatal.
pub(super) fn x11_error(e: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    use std::any::Any;
    use x11rb::errors::{ConnectError, ConnectionError, ReplyError, ReplyOrIdError};
    let any = &e as &dyn Any;
    let connection = any
        .downcast_ref::<ConnectionError>()
        .or_else(|| match any.downcast_ref::<ReplyError>() {
            Some(ReplyError::ConnectionError(e)) => Some(e),
            _ => None,
        })
        .or_else(|| match any.downcast_ref::<ReplyOrIdError>() {
            Some(ReplyOrIdError::ConnectionError(e)) => Some(e),
            _ => None,
        });
    let kind = match connection {
        Some(ConnectionError::IoError(e)) => e.kind(),
        _ => match any.downcast_ref::<ConnectError>() {
            Some(ConnectError::IoError(e)) => e.kind(),
            Some(ConnectError::Incomplete { .. }) => io::ErrorKind::UnexpectedEof,
            _ => io::ErrorKind::Other,
        },
    };
    io::Error::new(kind, e)
}
pub(super) struct Timed {
    stream: DefaultStream,
    deadline: Mutex<Instant>,
    cancelled: Arc<AtomicBool>,
}
pub(super) fn active(cancelled: &AtomicBool, deadline: Instant) -> io::Result<Duration> {
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
pub(super) type Conn = RustConnection<Timed>;
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
    .map_err(x11_error)
}
/// Initialization may narrow/set the phase deadline, never allocate another budget.
#[cfg(target_os = "linux")]
pub(super) fn initialize_until(conn: &Conn, deadline: Instant) -> io::Result<()> {
    let mut current = conn.stream().deadline.lock().unwrap();
    let phase_deadline = (*current).min(deadline);
    active(&conn.stream().cancelled, phase_deadline)?;
    *current = phase_deadline;
    Ok(())
}
/// A resumed parent request gets its ordinary I/O allowance within the reset budget.
#[cfg(target_os = "linux")]
pub(super) fn begin_recovery_request(conn: &Conn, recovery_deadline: Instant) -> io::Result<()> {
    let deadline = recovery_deadline.min(Instant::now() + Duration::from_millis(250));
    active(&conn.stream().cancelled, deadline)?;
    *conn.stream().deadline.lock().unwrap() = deadline;
    Ok(())
}
pub(super) fn renew(conn: &Conn) {
    *conn.stream().deadline.lock().unwrap() = Instant::now() + Duration::from_millis(250);
}
