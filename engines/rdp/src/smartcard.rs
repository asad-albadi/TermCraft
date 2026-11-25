//! Owned platform PC/SC provider processes. Native drivers never run on the RDP task.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "../../../src/desktop/loader_env.rs"]
mod loader_env;
#[cfg(any(target_os = "linux", target_os = "macos", windows, test))]
#[path = "smartcard/ndr.rs"]
mod ndr;
#[cfg(target_os = "linux")]
#[path = "smartcard/pcsc.rs"]
mod pcsc;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
#[path = "smartcard/session.rs"]
mod session;

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub struct Worker;
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
impl Worker {
    pub async fn start(reader: Option<&str>) -> Result<Option<Self>, String> {
        if reader.is_some() {
            return Err("Smartcard redirection requires Windows, macOS, or Linux PC/SC".into());
        }
        Ok(None)
    }

    pub fn enqueue(&mut self, _: &[u8]) -> Result<Option<ironrdp_svc::SvcMessage>, String> {
        unreachable!("PC/SC workers are unavailable on this platform")
    }

    pub fn poll(&mut self) -> Result<Vec<ironrdp_svc::SvcMessage>, String> {
        unreachable!("PC/SC workers are unavailable on this platform")
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{
        loader_env,
        ndr::{self, BUSY, INVALID, MAX_PACKET, word_at},
        pcsc::Api,
        session::Session,
    };
    use ironrdp_core::{Encode, EncodeResult, WriteCursor};
    use ironrdp_svc::{SvcEncode, SvcMessage};
    use std::{
        collections::BTreeSet,
        ffi::CString,
        io,
        os::{
            fd::{AsFd, FromRawFd, OwnedFd},
            unix::{net::UnixDatagram, process::CommandExt},
        },
        process::{Child, Command, Stdio},
        time::{Duration, Instant},
    };
    use zeroize::Zeroizing;

    const READY: &[u8] = b"PCSC ready";
    const HEARTBEAT: &[u8] = b"\0";
    const WORKER_TIMEOUT: Duration = Duration::from_secs(4);
    const TICK: Duration = Duration::from_millis(100);
    const MAX_PENDING: usize = 16;

    struct SensitivePdu(Zeroizing<Vec<u8>>);
    impl Encode for SensitivePdu {
        fn encode(&self, dst: &mut WriteCursor<'_>) -> EncodeResult<()> {
            ironrdp_core::ensure_size!(in: dst, size: self.size());
            dst.write_slice(&self.0);
            Ok(())
        }
        fn name(&self) -> &'static str {
            "RDP smartcard completion"
        }
        fn size(&self) -> usize {
            self.0.len()
        }
    }
    impl SvcEncode for SensitivePdu {}

    pub struct Worker {
        child: Child,
        socket: UnixDatagram,
        pending: BTreeSet<u32>,
        last_seen: Instant,
    }

    impl Worker {
        pub async fn start(reader: Option<&str>) -> Result<Option<Self>, String> {
            let Some(reader) = reader else {
                return Ok(None);
            };
            let mut command = Command::new("/proc/self/exe");
            command.arg("--pcsc-worker");
            Self::open(reader, command, |_| {}).await.map(Some)
        }

        #[cfg(test)]
        #[allow(dead_code)]
        pub async fn fixture(reader: &str, command: Command) -> Result<Self, String> {
            // Only a test harness can supply the fixture path and controls.
            let fixture_env: Vec<_> = command
                .get_envs()
                .filter_map(|(name, value)| {
                    let name_text = name.to_string_lossy();
                    (name_text == "TERMCRAFT_TEST_PCSC_LIBRARY"
                        || name_text.starts_with("TERMCRAFT_PCSC_FIXTURE_"))
                    .then(|| (name.to_owned(), value.map(std::ffi::OsStr::to_owned)))
                })
                .collect();
            Self::open(reader, command, |command| {
                for (name, value) in fixture_env {
                    if let Some(value) = value {
                        command.env(name, value);
                    }
                }
            })
            .await
        }

        #[cfg(test)]
        #[allow(dead_code)]
        pub fn pid(&self) -> u32 {
            self.child.id()
        }

        async fn open(
            reader: &str,
            command: Command,
            setup: impl FnOnce(&mut Command),
        ) -> Result<Self, String> {
            let mut worker = Self::spawn(command, setup)?;
            worker
                .socket
                .send(reader.as_bytes())
                .map_err(|_| "Cannot configure the PC/SC worker")?;
            let ready = tokio::time::timeout(WORKER_TIMEOUT, async {
                let mut bytes = [0; 16];
                loop {
                    match worker.socket.recv(&mut bytes) {
                        Ok(len) if &bytes[..len] == READY => return Ok(()),
                        Ok(_) => return Err("The selected PC/SC reader or the local PC/SC service is unavailable"),
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            tokio::time::sleep(Duration::from_millis(10)).await;
                        }
                        Err(_) => return Err("The local PC/SC worker stopped before becoming ready"),
                    }
                }
            }).await.map_err(|_| "The local PC/SC readiness check timed out")?;
            ready?;
            worker.last_seen = Instant::now();
            Ok(worker)
        }

        fn spawn(mut command: Command, setup: impl FnOnce(&mut Command)) -> Result<Self, String> {
            let (socket, child_socket) =
                pair().map_err(|_| "Cannot create the private PC/SC worker socket")?;
            socket
                .set_nonblocking(true)
                .map_err(|_| "Cannot configure the private PC/SC worker socket")?;
            let parent = std::process::id();
            loader_env::sanitize(&mut command);
            setup(&mut command);
            command
                .stdin(Stdio::from(OwnedFd::from(child_socket)))
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            // SAFETY: Only async-signal-safe syscalls run between fork and exec.
            unsafe {
                command.pre_exec(move || {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL, 0, 0, 0) == -1 {
                        return Err(io::Error::last_os_error());
                    }
                    if libc::getppid() as u32 != parent {
                        return Err(io::Error::from_raw_os_error(libc::ESRCH));
                    }
                    Ok(())
                });
            }
            let child = command
                .spawn()
                .map_err(|_| "Cannot start the owned PC/SC worker")?;
            Ok(Self {
                child,
                socket,
                pending: BTreeSet::new(),
                last_seen: Instant::now(),
            })
        }

        pub fn enqueue(&mut self, packet: &[u8]) -> Result<Option<SvcMessage>, String> {
            // Invalid controls generate no reply and never occupy an in-flight slot.
            if ndr::unassigned_control(packet) {
                return Ok(None);
            }
            let reject = |status| Ok(Some(SensitivePdu(ndr::fail(packet, status)).into()));
            if packet.len() > MAX_PACKET {
                return reject(INVALID);
            }
            let completion = word_at(packet, 12).map_err(|_| "Invalid smartcard completion ID")?;
            if self.pending.contains(&completion) {
                return Err("Duplicate in-flight smartcard completion ID".into());
            }
            if self.pending.len() >= MAX_PENDING {
                return reject(BUSY);
            }
            match self.socket.send(packet) {
                Ok(len) if len == packet.len() => {
                    if self.pending.is_empty() {
                        self.last_seen = Instant::now();
                    }
                    self.pending.insert(completion);
                    Ok(None)
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => reject(BUSY),
                _ => Err("The owned PC/SC worker stopped".into()),
            }
        }

        pub fn poll(&mut self) -> Result<Vec<SvcMessage>, String> {
            let mut out = Vec::new();
            // Allocate and read only when the local worker reports data ready.
            for _ in 0..=MAX_PENDING {
                if !readable(&self.socket)? {
                    break;
                }
                let mut bytes = Zeroizing::new(vec![0; MAX_PACKET + 1]);
                let len = match self.socket.recv(&mut bytes) {
                    Ok(0) => return Err("The owned PC/SC worker stopped".into()),
                    Ok(len) => len,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                    Err(_) => return Err("The owned PC/SC worker stopped".into()),
                };
                bytes.truncate(len);
                self.last_seen = Instant::now();
                if bytes.as_slice() == HEARTBEAT {
                    continue;
                }
                let completion = word_at(&bytes, 8).map_err(|_| "Invalid PC/SC worker response")?;
                if !bytes.starts_with(b"rDCI")
                    || len > MAX_PACKET
                    || word_at(&bytes, 16).map_err(|_| "Invalid PC/SC worker response")? as usize
                        != len - 20
                    || !self.pending.remove(&completion)
                {
                    return Err("Invalid PC/SC worker response".into());
                }
                out.push(SensitivePdu(bytes).into());
            }
            if !self.pending.is_empty() && self.last_seen.elapsed() > WORKER_TIMEOUT {
                return Err("A local PC/SC operation timed out".into());
            }
            Ok(out)
        }
    }

    impl Drop for Worker {
        fn drop(&mut self) {
            let _ = self.socket.shutdown(std::net::Shutdown::Both);
            // Reap only our owned child. A hung card/driver cannot outlive the connection.
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    fn pair() -> io::Result<(UnixDatagram, UnixDatagram)> {
        let mut sockets = [-1; 2];
        // SAFETY: socketpair writes two descriptors into the caller-owned array.
        if unsafe {
            libc::socketpair(
                libc::AF_UNIX,
                libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC,
                0,
                sockets.as_mut_ptr(),
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: on success both descriptors are independently owned connected sockets.
        Ok(unsafe {
            (
                UnixDatagram::from_raw_fd(sockets[0]),
                UnixDatagram::from_raw_fd(sockets[1]),
            )
        })
    }

    fn readable(socket: &UnixDatagram) -> Result<bool, String> {
        use std::os::fd::AsRawFd;
        let mut fd = libc::pollfd {
            fd: socket.as_fd().as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: Poll inspects a single live descriptor with no timeout.
        let result = unsafe { libc::poll(&mut fd, 1, 0) };
        match result {
            n if n >= 0 => Ok(n != 0),
            _ if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted => Ok(false),
            _ => Err("Cannot monitor the owned PC/SC worker".into()),
        }
    }

    pub fn child() -> Result<(), ()> {
        child_with_api(Api::open)
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub fn fixture_child(path: &std::path::Path) -> Result<(), ()> {
        child_with_api(|| Api::open_file(path))
    }

    fn child_with_api(open_api: impl FnOnce() -> Result<Api, ()>) -> Result<(), ()> {
        use std::os::fd::AsRawFd;
        let input = std::io::stdin()
            .as_fd()
            .try_clone_to_owned()
            .map_err(|_| ())?;
        let mut native_type: libc::c_int = 0;
        let mut native_len = size_of::<libc::c_int>() as libc::socklen_t;
        // SAFETY: Getsockopt writes to the correctly sized native_type buffer.
        let inspect = unsafe {
            libc::getsockopt(
                input.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_TYPE,
                (&mut native_type as *mut libc::c_int).cast(),
                &mut native_len,
            )
        };
        if inspect != 0 || native_type != libc::SOCK_SEQPACKET {
            return Err(());
        }
        let socket = UnixDatagram::from(input);
        socket
            .set_read_timeout(Some(WORKER_TIMEOUT))
            .map_err(|_| ())?;
        socket
            .set_write_timeout(Some(WORKER_TIMEOUT))
            .map_err(|_| ())?;
        let mut reader = [0u8; 128];
        let len = socket.recv(&mut reader).map_err(|_| ())?;
        let name = std::str::from_utf8(&reader[..len]).map_err(|_| ())?;
        if len >= reader.len() || name.trim().is_empty() || name.chars().any(char::is_control) {
            return Err(());
        }
        let mut session =
            Session::open(CString::new(name).map_err(|_| ())?, open_api()?).map_err(|_| ())?;
        socket.send(READY).map_err(|_| ())?;
        socket.set_read_timeout(Some(TICK)).map_err(|_| ())?;
        let mut heartbeat = Instant::now();
        loop {
            let mut request = Zeroizing::new(vec![0; MAX_PACKET + 1]);
            match socket.recv(&mut request) {
                Ok(0) => return Ok(()),
                Ok(len) => send(&socket, session.process(&request[..len]))?,
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock
                            | io::ErrorKind::TimedOut
                            | io::ErrorKind::Interrupted
                    ) => {}
                Err(_) => return Err(()),
            }
            if heartbeat.elapsed() >= TICK {
                if session.has_watches() {
                    send(&socket, session.poll_watches())?;
                    socket.send(HEARTBEAT).map_err(|_| ())?;
                }
                heartbeat = Instant::now();
            }
        }
    }

    fn send(socket: &UnixDatagram, replies: Vec<Zeroizing<Vec<u8>>>) -> Result<(), ()> {
        for reply in replies {
            socket.send(&reply).map_err(|_| ())?;
        }
        Ok(())
    }
}
#[cfg(target_os = "linux")]
pub use linux::{Worker, child};

#[cfg(all(test, target_os = "linux"))]
#[allow(unused_imports)]
pub use linux::fixture_child;

#[cfg(any(target_os = "macos", windows, test))]
#[path = "smartcard/framed.rs"]
mod framed;
#[cfg(any(target_os = "macos", windows))]
#[path = "smartcard/native_worker.rs"]
mod native_worker;
#[cfg(any(target_os = "macos", windows))]
#[path = "smartcard/native_pcsc.rs"]
mod pcsc;
#[cfg(any(target_os = "macos", windows))]
pub use native_worker::{Worker, child};
