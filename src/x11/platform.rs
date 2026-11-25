#[cfg(target_os = "macos")]
pub(super) use super::native_macos::start;
#[cfg(windows)]
pub(super) use super::native_windows::start;
#[cfg(windows)]
pub(super) const DEFAULT_ARTIFACT: &str = super::native_policy::WINDOWS_ARTIFACT;
#[cfg(target_os = "macos")]
pub(super) const DEFAULT_ARTIFACT: &str = super::native_policy::MACOS_ARTIFACT;
#[cfg(not(any(windows, target_os = "macos")))]
pub(super) const DEFAULT_ARTIFACT: &str = super::LINUX_XWAYLAND_ARTIFACT;
#[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
pub(super) use linux::{DisplayReservation, start};

pub(crate) fn bundled_runtime(
    root: &std::path::Path,
) -> Result<(std::path::PathBuf, String), String> {
    #[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
    return linux::bundled_runtime(root);
    #[cfg(windows)]
    return super::native_windows::bundled_runtime(root);
    #[cfg(target_os = "macos")]
    return super::native_macos::bundled_runtime(root);
    #[cfg(not(any(
        all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"),
        windows,
        target_os = "macos"
    )))]
    {
        let _ = root;
        Err("The packaged X server is unavailable on this platform".into())
    }
}

#[cfg(not(any(
    all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"),
    windows,
    target_os = "macos"
)))]
pub(super) fn start(
    _: &std::path::Path,
    _: super::XServerOptions,
    _: &std::sync::atomic::AtomicBool,
) -> Result<super::XServerLease, String> {
    Err("The packaged X server is unavailable on this platform".into())
}

#[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
mod linux {
    use super::super::{LINUX_XWAYLAND_ARTIFACT, XDisplayMode, XServerLease, XServerOptions};
    use crate::x11::engine::OwnedProcess;
    use sha2::{Digest, Sha256};
    use std::fs::{File, OpenOptions};
    use std::io::{self, Read, Seek, Write};
    use std::os::fd::{AsFd, AsRawFd, FromRawFd};
    use std::os::linux::net::SocketAddrExt;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
    use std::os::unix::net::{SocketAddr, UnixListener, UnixStream};
    use std::os::unix::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};
    use zeroize::Zeroizing;

    const ENGINE_RELATIVE: &str = "x11/linux-x86_64/Xwayland";
    const ENGINE_SHA256: &str = "cbedd90fbc960d4cc799792cedf89b0afa1f1f74a93ba9fb05bfd4da2f27b2b8";
    const COOKIE_PROTOCOL: &[u8] = b"MIT-MAGIC-COOKIE-1";
    const START_TIMEOUT: Duration = Duration::from_secs(8);

    pub(in crate::x11) fn start(
        root: &Path,
        options: XServerOptions,
        cancelled: &AtomicBool,
    ) -> Result<XServerLease, String> {
        let deadline = Instant::now() + START_TIMEOUT;
        let (lease, cookie) = launch(root, options, deadline, cancelled)?;
        finish_startup(lease, &cookie, deadline, cancelled)
    }

    fn finish_startup(
        mut lease: XServerLease,
        cookie: &[u8; 16],
        deadline: Instant,
        cancelled: &AtomicBool,
    ) -> Result<XServerLease, String> {
        let display = lease.display.trim_start_matches(':');
        let ready = verify_display(&lease._reservation.socket.path, cookie, deadline, cancelled)
            .and_then(|()| write_authority(&lease.authority_path, display, cookie));
        if let Err(error) = ready {
            return Err(format!("Packaged X server readiness check failed: {error}"));
        }
        check_cancelled(cancelled)?;
        if !lease.is_running()? {
            return Err("The packaged X server exited during startup".into());
        }
        Ok(lease)
    }

    fn launch(
        root: &Path,
        options: XServerOptions,
        deadline: Instant,
        cancelled: &AtomicBool,
    ) -> Result<(XServerLease, Zeroizing<[u8; 16]>), String> {
        check_cancelled(cancelled)?;
        if options.artifact_id != LINUX_XWAYLAND_ARTIFACT {
            return Err(format!(
                "Unsupported packaged X server artifact: {}",
                options.artifact_id
            ));
        }
        if options.monitor.is_some() {
            return Err("Explicit monitor selection is unavailable for packaged Xwayland".into());
        }
        let (binary, verified_engine) = verify_engine(root)?;
        if std::env::var_os("WAYLAND_DISPLAY").is_none_or(|value| value.is_empty()) {
            return Err("Packaged Xwayland requires a Wayland desktop session".into());
        }
        let runtime = tempfile::Builder::new()
            .prefix("termcraft-x11-")
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()
            .map_err(|error| format!("Could not create the private X server directory: {error}"))?;
        let server_auth = runtime.path().join("server-authority");
        let mut cookie = Zeroizing::new([0; 16]);
        getrandom::fill(cookie.as_mut())
            .map_err(|error| format!("Could not generate X server credentials: {error}"))?;
        write_authority(&server_auth, "", &cookie)
            .map_err(|error| format!("Could not write private X server credentials: {error}"))?;

        let (reservation, listener, abstract_listener) = DisplayReservation::create(cancelled)?;
        let display = reservation.display;
        let listener_fd = listener.as_raw_fd();
        let abstract_fd = abstract_listener.as_raw_fd();
        // The executable image is sealed, including against in-place writes
        // by another holder of the original package file.
        let mut command = Command::new(format!("/proc/self/fd/{}", verified_engine.as_raw_fd()));
        command.arg0(&binary);
        command
            .arg(format!(":{display}"))
            .args(["-displayfd", "1", "-listenfd"])
            .arg(listener_fd.to_string())
            .arg("-listenfd")
            .arg(abstract_fd.to_string())
            .args(["-nolisten", "tcp", "-noreset", "-auth"])
            .arg(server_auth)
            // Do not inherit a compositor's preconnected descriptor or the user's X credentials.
            .env_remove("WAYLAND_SOCKET")
            .env_remove("DISPLAY")
            .env_remove("XAUTHORITY")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if options.display_mode == XDisplayMode::Fullscreen {
            command.arg("-fullscreen");
        }
        // Only this child inherits these already bound sockets. The X server never
        // probes, opens, or unlinks paths belonging to a different display.
        unsafe {
            command.pre_exec(move || {
                for fd in [listener_fd, abstract_fd] {
                    if libc::fcntl(fd, libc::F_SETFD, 0) < 0 {
                        return Err(io::Error::last_os_error());
                    }
                }
                Ok(())
            });
        }
        check_cancelled(cancelled)?;
        let mut child = command.spawn().map_err(|error| {
            format!(
                "Could not launch packaged Xwayland at {}: {error}",
                binary.display()
            )
        })?;
        drop(verified_engine);
        let mut display_pipe = child.stdout.take().expect("piped stdout");
        let mut log_pipe = child.stderr.take().expect("piped stderr");
        let mut process = OwnedProcess::new(child);
        nonblocking(&display_pipe)
            .and_then(|()| nonblocking(&log_pipe))
            .map_err(|error| format!("Could not monitor packaged Xwayland startup: {error}"))?;

        let confirmed = wait_for_display(
            &mut process,
            &mut display_pipe,
            &mut log_pipe,
            deadline,
            cancelled,
        )?;
        if confirmed != display {
            return Err(
                "Packaged Xwayland announced a different display than it was assigned".into(),
            );
        }
        drop((listener, abstract_listener));
        // Xwayland still writes diagnostics after startup. Keep draining them until exit.
        std::thread::Builder::new()
            .name("termcraft-xserver-diagnostics".into())
            .spawn(move || drain_diagnostics(log_pipe))
            .map_err(|error| format!("Could not start X server diagnostics worker: {error}"))?;

        let authority_path = runtime.path().join("Xauthority");
        let lease = XServerLease {
            alive: tokio::sync::watch::Sender::new(true),
            display: format!(":{display}"),
            authority_path,
            process,
            _reservation: reservation,
            _runtime: runtime,
        };
        Ok((lease, cookie))
    }

    fn nonblocking(pipe: &impl AsFd) -> io::Result<()> {
        let fd = pipe.as_fd().as_raw_fd();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    fn read_available(pipe: &mut impl Read, buffer: &mut [u8]) -> io::Result<usize> {
        match pipe.read(buffer) {
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                Ok(0)
            }
            result => result,
        }
    }

    fn drain_diagnostics(mut pipe: impl AsFd + Read) {
        let mut buffer = [0; 4096];
        let mut poll = libc::pollfd {
            fd: pipe.as_fd().as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        loop {
            let result = unsafe { libc::poll(&mut poll, 1, -1) };
            if result < 0 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            }
            if result <= 0 {
                return;
            }
            match pipe.read(&mut buffer) {
                Ok(0) => return,
                Ok(_) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) => {}
                Err(_) => return,
            }
        }
    }

    fn check_cancelled(cancelled: &AtomicBool) -> Result<(), String> {
        if cancelled.load(Ordering::Relaxed) {
            return Err("Packaged X server startup was cancelled".into());
        }
        Ok(())
    }

    fn wait_for_display(
        process: &mut OwnedProcess,
        display_pipe: &mut impl Read,
        log_pipe: &mut impl Read,
        deadline: Instant,
        cancelled: &AtomicBool,
    ) -> Result<u16, String> {
        let mut response = Vec::new();
        let mut diagnostics = Vec::new();
        let mut buffer = [0; 4096];
        loop {
            check_cancelled(cancelled)?;
            let read = read_available(log_pipe, &mut buffer)
                .map_err(|error| format!("Could not read Xwayland diagnostics: {error}"))?;
            diagnostics.extend_from_slice(&buffer[..read]);
            if diagnostics.len() > 4096 {
                diagnostics.drain(..diagnostics.len() - 4096);
            }
            let read = read_available(display_pipe, &mut buffer[..16])
                .map_err(|error| format!("Could not read the Xwayland display number: {error}"))?;
            response.extend_from_slice(&buffer[..read]);
            if response.ends_with(b"\n") {
                return std::str::from_utf8(&response[..response.len() - 1])
                    .ok()
                    .and_then(|value| value.parse().ok())
                    .ok_or_else(|| "Packaged Xwayland returned an invalid display number".into());
            }
            if response.len() > 5 {
                return Err("Packaged Xwayland returned an invalid display number".into());
            }
            let reason = if !process.is_running().map_err(|error| error.to_string())? {
                "Packaged Xwayland exited before opening a display"
            } else if Instant::now() >= deadline {
                "Packaged Xwayland timed out opening its display"
            } else {
                std::thread::sleep(Duration::from_millis(15));
                continue;
            };
            let detail = String::from_utf8_lossy(&diagnostics);
            return Err(format!(
                "{reason}. Check the Wayland session and package dependencies. {}",
                detail.trim()
            ));
        }
    }

    pub(super) fn bundled_runtime(root: &Path) -> Result<(PathBuf, String), String> {
        let (path, _) = verify_engine(root)?;
        Ok((path, "24.1.15-1".into()))
    }

    fn verify_engine(root: &Path) -> Result<(PathBuf, File), String> {
        let binary = root.join(ENGINE_RELATIVE);
        let image = verify_executable(&binary, ENGINE_SHA256)?;
        Ok((binary, image))
    }

    fn verify_executable(binary: &Path, expected_checksum: &str) -> Result<File, String> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(binary)
            .map_err(|error| {
                format!("Missing packaged X server at {}: {error}", binary.display())
            })?;
        let metadata = file.metadata().map_err(|error| error.to_string())?;
        if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
            return Err("The packaged X server must be a regular executable file".into());
        }
        if metadata.len() > 16 * 1024 * 1024 {
            return Err("The packaged X server exceeds its expected size".into());
        }
        let mut artifact = Vec::with_capacity(metadata.len() as usize);
        (&file)
            .take(metadata.len() + 1)
            .read_to_end(&mut artifact)
            .map_err(|error| format!("Could not verify the packaged X server checksum: {error}"))?;
        let checksum: String = Sha256::digest(&artifact)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if checksum != expected_checksum {
            return Err(
                "Packaged X server checksum mismatch. Restore the verified installation artifact."
                    .into(),
            );
        }
        seal_executable(&artifact)
            .map_err(|error| format!("Could not seal the verified X server executable: {error}"))
    }

    fn seal_executable(verified_bytes: &[u8]) -> io::Result<File> {
        let flags = libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING;
        let name = c"termcraft-xwayland";
        // MFD_EXEC makes the requirement explicit on kernels with memfd_noexec.
        let mut fd = unsafe { libc::memfd_create(name.as_ptr(), flags | libc::MFD_EXEC) };
        if fd < 0 && io::Error::last_os_error().raw_os_error() == Some(libc::EINVAL) {
            // Older kernels create executable memfds by default.
            fd = unsafe { libc::memfd_create(name.as_ptr(), flags) };
        }
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut image = unsafe { File::from_raw_fd(fd) };
        image.write_all(verified_bytes)?;
        image.rewind()?;
        image.set_permissions(std::fs::Permissions::from_mode(0o500))?;
        let seals =
            libc::F_SEAL_WRITE | libc::F_SEAL_GROW | libc::F_SEAL_SHRINK | libc::F_SEAL_SEAL;
        if unsafe { libc::fcntl(fd, libc::F_ADD_SEALS, seals) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(image)
    }

    fn write_authority(path: &Path, display: &str, cookie: &[u8; 16]) -> io::Result<()> {
        let mut file = OpenOptions::new()
            .mode(0o600)
            .write(true)
            .create_new(true)
            .open(path)?;
        // FamilyWild identifies the local credential independently of the hostname.
        file.write_all(&65535_u16.to_be_bytes())?;
        for field in [b"".as_slice(), display.as_bytes(), COOKIE_PROTOCOL, cookie] {
            file.write_all(&(field.len() as u16).to_be_bytes())?;
            file.write_all(field)?;
        }
        Ok(())
    }

    struct OwnedEndpoint {
        path: PathBuf,
        identity: (u64, u64),
    }

    impl OwnedEndpoint {
        fn from_metadata(path: PathBuf, metadata: std::fs::Metadata) -> Self {
            Self {
                path,
                identity: (metadata.dev(), metadata.ino()),
            }
        }
    }

    impl Drop for OwnedEndpoint {
        fn drop(&mut self) {
            if self
                .path
                .symlink_metadata()
                .is_ok_and(|metadata| self.identity == (metadata.dev(), metadata.ino()))
            {
                let _ = std::fs::remove_file(&self.path);
            }
        }
    }

    pub(in crate::x11) struct DisplayReservation {
        display: u16,
        socket: OwnedEndpoint,
        _lock: OwnedEndpoint,
    }

    impl DisplayReservation {
        fn create(cancelled: &AtomicBool) -> Result<(Self, UnixListener, UnixListener), String> {
            // Keep away from the low numbers commonly allocated by desktop compositors.
            const FIRST: u16 = 100;
            const COUNT: u16 = 900;
            let mut random = [0_u8; 2];
            getrandom::fill(&mut random).map_err(|error| error.to_string())?;
            let start = u16::from_ne_bytes(random) % COUNT;
            for offset in 0..COUNT {
                check_cancelled(cancelled)?;
                let display = FIRST + (start + offset) % COUNT;
                match Self::reserve(display) {
                    Ok(Some(reservation)) => return Ok(reservation),
                    Ok(None) => {}
                    Err(error) => {
                        return Err(format!("Could not reserve a local X display: {error}"));
                    }
                }
            }
            Err("No unused TermCraft X displays are available".into())
        }

        fn reserve(display: u16) -> io::Result<Option<(Self, UnixListener, UnixListener)>> {
            let lock_path = PathBuf::from(format!("/tmp/.X{display}-lock"));
            let mut lock_file = match OpenOptions::new()
                .mode(0o444)
                .write(true)
                .create_new(true)
                .open(&lock_path)
            {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return Ok(None),
                Err(error) => return Err(error),
            };
            let lock = OwnedEndpoint::from_metadata(lock_path, lock_file.metadata()?);
            writeln!(lock_file, "{:>10}", std::process::id())?;

            let path = PathBuf::from(format!("/tmp/.X11-unix/X{display}"));
            let abstract_addr =
                SocketAddr::from_abstract_name(path.as_os_str().as_encoded_bytes())?;
            let abstract_listener = match UnixListener::bind_addr(&abstract_addr) {
                Ok(listener) => listener,
                Err(error) if error.kind() == io::ErrorKind::AddrInUse => return Ok(None),
                Err(error) => return Err(error),
            };
            let listener = match UnixListener::bind(&path) {
                Ok(listener) => listener,
                Err(error) if error.kind() == io::ErrorKind::AddrInUse => return Ok(None),
                Err(error) => return Err(error),
            };
            let socket_metadata = path.symlink_metadata()?;
            let socket = OwnedEndpoint::from_metadata(path, socket_metadata);
            Ok(Some((
                Self {
                    display,
                    socket,
                    _lock: lock,
                },
                listener,
                abstract_listener,
            )))
        }
    }

    fn readiness_io<T>(
        deadline: Instant,
        cancelled: &AtomicBool,
        mut operation: impl FnMut() -> io::Result<T>,
    ) -> io::Result<T> {
        loop {
            if cancelled.load(Ordering::Relaxed) {
                return Err(io::Error::other("Packaged X server startup was cancelled"));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "X server readiness deadline expired",
                ));
            }
            match operation() {
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) =>
                {
                    std::thread::sleep(remaining.min(Duration::from_millis(10)))
                }
                result => return result,
            }
        }
    }

    fn connect_local_nonblocking(path: &Path) -> io::Result<UnixStream> {
        let path = path.as_os_str().as_encoded_bytes();
        let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
        if path.len() >= address.sun_path.len() || path.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid X display socket path",
            ));
        }
        address.sun_family = libc::AF_UNIX as libc::sa_family_t;
        for (destination, source) in address.sun_path.iter_mut().zip(path) {
            *destination = *source as libc::c_char;
        }
        let length =
            (std::mem::offset_of!(libc::sockaddr_un, sun_path) + path.len() + 1) as libc::socklen_t;
        let fd = unsafe {
            libc::socket(
                libc::AF_UNIX,
                libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
                0,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let stream = unsafe { UnixStream::from_raw_fd(fd) };
        if unsafe { libc::connect(fd, (&raw const address).cast(), length) } < 0 {
            // Linux AF_UNIX returns EAGAIN when a nonblocking connect would wait
            // for a listener queue slot. Drop that descriptor and try again.
            return Err(io::Error::last_os_error());
        }
        Ok(stream)
    }

    fn verify_display(
        socket: &Path,
        cookie: &[u8; 16],
        deadline: Instant,
        cancelled: &AtomicBool,
    ) -> io::Result<()> {
        let mut stream = readiness_io(deadline, cancelled, || connect_local_nonblocking(socket))?;
        let mut peer = std::mem::MaybeUninit::<libc::ucred>::uninit();
        let mut size = size_of::<libc::ucred>() as libc::socklen_t;
        // The listener is bound by TermCraft, so the peer PID is the parent even
        // when the connection is accepted by the child using the inherited FD.
        let result = unsafe {
            libc::getsockopt(
                stream.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                peer.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if result < 0 {
            return Err(io::Error::last_os_error());
        }
        if size as usize != size_of::<libc::ucred>()
            || unsafe { peer.assume_init() }.pid != std::process::id() as libc::pid_t
        {
            return Err(io::Error::other(
                "X display is owned by an unexpected process",
            ));
        }
        let mut setup = Zeroizing::new([0_u8; 48]);
        setup[0] = b'l';
        setup[2] = 11;
        setup[6] = COOKIE_PROTOCOL.len() as u8;
        setup[8] = cookie.len() as u8;
        setup[12..12 + COOKIE_PROTOCOL.len()].copy_from_slice(COOKIE_PROTOCOL);
        setup[32..].copy_from_slice(cookie);
        let mut sent = 0;
        while sent < setup.len() {
            let written = readiness_io(deadline, cancelled, || stream.write(&setup[sent..]))?;
            if written == 0 {
                return Err(io::Error::from(io::ErrorKind::WriteZero));
            }
            sent += written;
        }
        let mut accepted = [0];
        if readiness_io(deadline, cancelled, || stream.read(&mut accepted))? == 0 {
            return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
        }
        if accepted[0] != 1 {
            return Err(io::Error::other(
                "Packaged Xwayland rejected its private credential",
            ));
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::sync::{Arc, mpsc};

        #[test]
        fn verified_executable_survives_in_place_mutation() {
            let root = tempfile::tempdir().unwrap();
            let file = root.path().join("engine");
            let original = b"original verified bytes";
            std::fs::write(&file, original).unwrap();
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o700)).unwrap();
            let checksum: String = Sha256::digest(original)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            let mut verified = verify_executable(&file, &checksum).unwrap();
            std::fs::write(&file, b"modified after verification").unwrap();
            let mut bytes = Vec::new();
            verified.read_to_end(&mut bytes).unwrap();
            assert_eq!(bytes.as_slice(), original);
            assert_eq!(
                verified
                    .write_all(b"modified sealed image")
                    .unwrap_err()
                    .raw_os_error(),
                Some(libc::EPERM)
            );
        }

        #[test]
        fn full_listener_queue_cancels_and_cleans_owned_startup() {
            let runtime = tempfile::tempdir().unwrap();
            let directory = runtime.path().to_path_buf();
            let socket = directory.join("X");
            let listener = UnixListener::bind(&socket).unwrap();
            assert_eq!(unsafe { libc::listen(listener.as_raw_fd(), 0) }, 0);
            let queued = UnixStream::connect(&socket).unwrap();
            let lock = directory.join("display-lock");
            std::fs::write(&lock, []).unwrap();
            let reservation = DisplayReservation {
                display: 100,
                socket: OwnedEndpoint::from_metadata(socket.clone(), socket.metadata().unwrap()),
                _lock: OwnedEndpoint::from_metadata(lock.clone(), lock.metadata().unwrap()),
            };
            let child = Command::new("/usr/bin/sleep").arg("30").spawn().unwrap();
            let pid = child.id();
            let lease = XServerLease {
                alive: tokio::sync::watch::Sender::new(true),
                display: ":100".into(),
                authority_path: directory.join("Xauthority"),
                process: OwnedProcess::new(child),
                _reservation: reservation,
                _runtime: runtime,
            };
            let (results, outcome) = mpsc::channel();
            let cancelled = Arc::new(AtomicBool::new(false));
            let worker_cancelled = cancelled.clone();
            let worker = std::thread::spawn(move || {
                let result = finish_startup(
                    lease,
                    &[0; 16],
                    Instant::now() + Duration::from_secs(5),
                    &worker_cancelled,
                );
                assert!(results.send(result.err()).is_ok());
            });
            std::thread::sleep(Duration::from_millis(35));
            cancelled.store(true, Ordering::Relaxed);
            let result = outcome.recv_timeout(Duration::from_secs(1));
            if result.is_err() {
                // Free one queue slot so a regressed blocking-connect worker can exit too.
                let _ = listener.accept();
            }
            worker.join().unwrap();
            drop(queued);
            let error = result
                .expect("startup remained blocked after cancellation")
                .unwrap();
            assert!(error.contains("cancelled"), "{error}");
            assert!(!directory.exists());
            assert!(!socket.exists());
            assert!(!lock.exists());
            assert!(!Path::new(&format!("/proc/{pid}")).exists());
        }
    }
}
