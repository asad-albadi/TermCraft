//! Bounded authenticated X11 startup exchange for an owned loopback transport.
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
const PROTOCOL: &[u8] = b"MIT-MAGIC-COOKIE-1";
pub fn cancelled(flag: &AtomicBool) -> Result<(), String> {
    if flag.load(Ordering::Relaxed) {
        Err("Packaged X server startup was cancelled".into())
    } else {
        Ok(())
    }
}
pub fn write_authority(path: &Path, display: u16, cookie: &[u8; 16]) -> Result<(), String> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|e| e.to_string())?;
    file.write_all(&65535u16.to_be_bytes())
        .map_err(|e| e.to_string())?;
    for field in [
        b"".as_slice(),
        display.to_string().as_bytes(),
        PROTOCOL,
        cookie,
    ] {
        file.write_all(&(field.len() as u16).to_be_bytes())
            .and_then(|()| file.write_all(field))
            .map_err(|e| e.to_string())?;
    }
    file.sync_all().map_err(|e| e.to_string())
}
pub fn setup(
    address: SocketAddr,
    cookie: Option<&[u8; 16]>,
    flag: &AtomicBool,
) -> Result<bool, String> {
    cancelled(flag)?;
    if !address.ip().is_loopback() {
        return Err("Native X transport must be loopback-only".into());
    }
    setup_tcp(address, cookie, flag)
}

#[cfg(windows)]
pub(super) fn setup_owned_tcp(
    address: SocketAddr,
    cookie: Option<&[u8; 16]>,
    flag: &AtomicBool,
) -> Result<bool, String> {
    // The Windows launcher verifies ownership of this exact local listener
    // with the IP Helper table before calling this path.
    setup_tcp(address, cookie, flag)
}

fn setup_tcp(
    address: SocketAddr,
    cookie: Option<&[u8; 16]>,
    flag: &AtomicBool,
) -> Result<bool, String> {
    cancelled(flag)?;
    let slice = Duration::from_millis(25);
    let deadline = Instant::now() + Duration::from_millis(150);
    let mut stream = TcpStream::connect_timeout(&address, slice).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(slice))
        .and_then(|()| stream.set_write_timeout(Some(slice)))
        .map_err(|e| e.to_string())?;
    exchange(&mut stream, cookie, deadline, flag)
}
fn exchange(
    stream: &mut (impl Read + Write),
    cookie: Option<&[u8; 16]>,
    deadline: Instant,
    flag: &AtomicBool,
) -> Result<bool, String> {
    let mut request = zeroize::Zeroizing::new(vec![0u8; if cookie.is_some() { 48 } else { 12 }]);
    request[0] = b'l';
    request[2..4].copy_from_slice(&11u16.to_le_bytes());
    if let Some(cookie) = cookie {
        request[6..8].copy_from_slice(&18u16.to_le_bytes());
        request[8..10].copy_from_slice(&16u16.to_le_bytes());
        request[12..30].copy_from_slice(PROTOCOL);
        request[32..48].copy_from_slice(cookie);
    }
    stream.write_all(&request).map_err(|e| e.to_string())?;
    let mut header = [0; 8];
    read_bounded(stream, &mut header, deadline, flag)?;
    if header[0] == 0 {
        return Ok(false);
    }
    if header[0] != 1
        || u16::from_le_bytes([header[2], header[3]]) != 11
        || u16::from_le_bytes([header[4], header[5]]) != 0
    {
        return Err("Invalid native X setup response".into());
    }
    let length = u16::from_le_bytes([header[6], header[7]]) as usize * 4;
    if !(32..=65536).contains(&length) {
        return Err("Invalid native X setup payload length".into());
    }
    let mut data = vec![0; length];
    read_bounded(stream, &mut data, deadline, flag)?;
    cancelled(flag)?;
    Ok(true)
}
fn read_bounded(
    stream: &mut impl Read,
    mut bytes: &mut [u8],
    deadline: Instant,
    flag: &AtomicBool,
) -> Result<(), String> {
    while !bytes.is_empty() {
        cancelled(flag)?;
        if Instant::now() >= deadline {
            return Err("Native X setup timed out".into());
        }
        match stream.read(bytes) {
            Ok(0) => return Err("Native X server closed setup".into()),
            Ok(count) => bytes = &mut bytes[count..],
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::Interrupted
                        | std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                ) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}

#[cfg(unix)]
pub fn setup_unix(
    path: &Path,
    cookie: Option<&[u8; 16]>,
    flag: &AtomicBool,
) -> Result<bool, String> {
    let mut stream = connect_unix(path, flag)?;
    stream
        .set_nonblocking(false)
        .and_then(|()| stream.set_read_timeout(Some(Duration::from_millis(25))))
        .and_then(|()| stream.set_write_timeout(Some(Duration::from_millis(25))))
        .map_err(|e| e.to_string())?;
    exchange(
        &mut stream,
        cookie,
        Instant::now() + Duration::from_millis(150),
        flag,
    )
}

#[cfg(any(target_os = "macos", all(test, unix)))]
pub(super) fn connect_unix(
    path: &Path,
    flag: &AtomicBool,
) -> Result<std::os::unix::net::UnixStream, String> {
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::net::UnixStream;
    cancelled(flag)?;
    let bytes = path.as_os_str().as_encoded_bytes();
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    if bytes.len() >= address.sun_path.len() || bytes.contains(&0) {
        return Err("Private X socket path exceeds native limit".into());
    }
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    for (to, from) in address.sun_path.iter_mut().zip(bytes) {
        *to = *from as libc::c_char;
    }
    let length =
        (std::mem::offset_of!(libc::sockaddr_un, sun_path) + bytes.len() + 1) as libc::socklen_t;
    #[cfg(target_os = "macos")]
    {
        address.sun_len = length as u8;
    }
    let raw = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
    if raw < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let stream = unsafe { UnixStream::from_raw_fd(raw) };
    if unsafe { libc::fcntl(raw, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    stream.set_nonblocking(true).map_err(|e| e.to_string())?;
    if unsafe { libc::connect(raw, (&address as *const libc::sockaddr_un).cast(), length) } < 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::EINPROGRESS) {
            return Err(error.to_string());
        }
        let mut descriptor = libc::pollfd {
            fd: raw,
            events: libc::POLLOUT,
            revents: 0,
        };
        if unsafe { libc::poll(&mut descriptor, 1, 25) } <= 0 {
            return Err("Private X socket connection timed out".into());
        }
        let mut result: libc::c_int = 0;
        let mut length = size_of_val(&result) as libc::socklen_t;
        if unsafe {
            libc::getsockopt(
                stream.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_ERROR,
                (&mut result as *mut libc::c_int).cast(),
                &mut length,
            )
        } < 0
            || result != 0
        {
            return Err("Private X socket connection failed".into());
        }
    }
    cancelled(flag)?;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };
    #[test]
    fn readiness_transmits_cookie_and_requires_a_real_setup_success() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0; 48];
            socket.read_exact(&mut request).unwrap();
            assert_eq!(&request[12..30], b"MIT-MAGIC-COOKIE-1");
            assert_eq!(&request[32..], &[7; 16]);
            let mut response = [0; 40];
            response[0] = 1;
            response[2..4].copy_from_slice(&11u16.to_le_bytes());
            response[6..8].copy_from_slice(&8u16.to_le_bytes());
            socket.write_all(&response).unwrap();
        });
        assert!(
            setup(
                address,
                Some(&[7; 16]),
                &std::sync::atomic::AtomicBool::new(false)
            )
            .unwrap()
        );
        server.join().unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn inherited_private_unix_socket_requires_cookie_setup() {
        use std::os::unix::net::UnixListener;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("display:0");
        let listener = UnixListener::bind(&path).unwrap();
        let worker = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0; 48];
            socket.read_exact(&mut request).unwrap();
            assert_eq!(&request[32..], &[5; 16]);
            let mut response = [0; 40];
            response[0] = 1;
            response[2] = 11;
            response[6] = 8;
            socket.write_all(&response).unwrap();
        });
        assert!(
            setup_unix(
                &path,
                Some(&[5; 16]),
                &std::sync::atomic::AtomicBool::new(false)
            )
            .unwrap()
        );
        worker.join().unwrap();
    }
    #[test]
    fn cancelled_readiness_never_connects() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        assert!(
            setup(
                listener.local_addr().unwrap(),
                Some(&[0; 16]),
                &std::sync::atomic::AtomicBool::new(true)
            )
            .is_err()
        );
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
    #[test]
    fn private_authority_is_exclusive_and_contains_only_the_owned_cookie() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("auth");
        write_authority(&path, 456, &[3; 16]).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..2], &65535u16.to_be_bytes());
        assert!(bytes.windows(3).any(|b| b == b"456"));
        assert_eq!(&bytes[bytes.len() - 16..], &[3; 16]);
        assert!(write_authority(&path, 456, &[9; 16]).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}
