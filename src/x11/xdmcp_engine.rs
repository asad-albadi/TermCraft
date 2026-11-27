//! Owned XDMCP display sessions. Never starts or stops a display manager.
use super::{
    XServerManager,
    xdmcp::{XdmcpConfig, check_cancelled},
};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::Duration,
};

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub use super::xdmcp_macos::Session as XdmcpSession;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use super::xdmcp_macos::start;
#[cfg(not(any(
    all(windows, target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64")
)))]
use super::{engine::OwnedProcess, xdmcp::engine_arguments};
#[cfg(not(any(
    all(windows, target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64")
)))]
use std::path::PathBuf;
#[cfg(target_os = "linux")]
use std::time::Instant;

fn validate(config: &XdmcpConfig, width: u32, height: u32) -> Result<(), String> {
    #[cfg(all(windows, target_arch = "x86_64"))]
    {
        super::xdmcp::windows_options(config, width, height).map(|_| ())
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        let _ = (width, height); // Rootless XQuartz uses host geometry.
        super::xdmcp_macos::preflight(config).map(|_| ())
    }
    #[cfg(not(any(
        all(windows, target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64")
    )))]
    engine_arguments(config, config.display.unwrap_or(100), width, height).map(|_| ())
}
pub const LINUX_XEPHYR_ARTIFACT: &str = "xephyr-21.1.26-arch-x86_64";
/// Per-session ownership. The child stops before its private credentials are removed.
#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
pub struct XdmcpSession {
    ready_message: String,
    backend: SessionBackend,
}

#[cfg(all(windows, target_arch = "x86_64"))]
type SessionBackend = super::XServerLease;

#[cfg(not(any(
    all(windows, target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64")
)))]
struct SessionBackend {
    #[cfg(target_os = "linux")]
    clipboard: Option<super::xdmcp_clipboard::Bridge>,
    process: OwnedProcess,
    display: String,
    authority: PathBuf,
    log: std::process::ChildStderr,
    diagnostics: Vec<u8>,
    _runtime: tempfile::TempDir,
}
#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
impl XdmcpSession {
    pub fn display(&self) -> &str {
        self.backend.display()
    }
    pub fn authority_path(&self) -> &Path {
        self.backend.authority_path()
    }
    pub fn process_id(&self) -> Option<u32> {
        self.backend.process_id()
    }
    pub fn is_running(&mut self) -> Result<bool, String> {
        self.backend.is_running()
    }
    pub fn stop_owned(self) -> Result<(), String> {
        self.backend.stop_owned()
    }
}

#[cfg(not(any(
    all(windows, target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64")
)))]
impl SessionBackend {
    fn display(&self) -> &str {
        &self.display
    }
    fn authority_path(&self) -> &Path {
        &self.authority
    }
    fn process_id(&self) -> Option<u32> {
        self.process.id()
    }
    fn is_running(&mut self) -> Result<bool, String> {
        #[cfg(target_os = "linux")]
        if let Some(bridge) = self.clipboard.as_mut() {
            bridge.check()?;
        }
        use std::io::Read;
        let mut buffer = [0; 4096];
        for _ in 0..4 {
            match self.log.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if self.diagnostics.len() < 4096 {
                        self.diagnostics
                            .extend_from_slice(&buffer[..n.min(4096 - self.diagnostics.len())]);
                    }
                }
            }
        }
        self.process.is_running().map_err(|e| e.to_string())
    }
    fn stop_owned(mut self) -> Result<(), String> {
        #[cfg(target_os = "linux")]
        drop(self.clipboard.take());
        self.process.stop().map_err(|e| e.to_string())
    }
}
impl XServerManager {
    /// Dedicated XDMCP-capable engine; never passes XDMCP arguments to Xwayland.
    pub fn start_xdmcp_cancellable(
        &self,
        config: XdmcpConfig,
        width: u32,
        height: u32,
        cancelled: &AtomicBool,
    ) -> Result<XdmcpSession, String> {
        let root = match &self.installation {
            Some(path) => path.clone(),
            None => {
                crate::runtime::RuntimeCatalog::for_current_executable()?.service_runtime_root()
            }
        };
        start(&root, config, width, height, cancelled)
    }
}
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
static MAC_WORKERS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
struct MacWorker;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
impl MacWorker {
    fn start() -> Self {
        MAC_WORKERS.fetch_add(1, Ordering::AcqRel);
        Self
    }
}
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
impl Drop for MacWorker {
    fn drop(&mut self) {
        MAC_WORKERS.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Holds the session on its worker, so drop/cancellation never waits on the render thread.
pub struct ManagedXdmcp {
    cancelled: Arc<AtomicBool>,
    status: Receiver<Result<String, String>>,
    finished: Arc<AtomicBool>,
}
impl ManagedXdmcp {
    pub fn start(
        manager: XServerManager,
        config: XdmcpConfig,
        width: u32,
        height: u32,
    ) -> Result<Self, String> {
        Self::start_inner(
            manager,
            config,
            None,
            width,
            height,
            #[cfg(test)]
            None,
        )
    }
    /// Explicit Connect action: fresh resolution (<=2s) precedes bounded engine startup.
    /// Persisted endpoint data is accepted only if present in this fresh exact-host/port answer.
    pub fn start_hostname(
        manager: XServerManager,
        config: XdmcpConfig,
        host: String,
        port: u16,
        width: u32,
        height: u32,
    ) -> Result<Self, String> {
        let request = super::xdmcp_host::HostRequest {
            host,
            port,
            mode: config.mode,
            generation: 0,
        };
        request.validate()?;
        let mut preflight = config.clone();
        // A placeholder is used only for option/consent validation. It never reaches the engine.
        preflight.selected_endpoint = Some(std::net::SocketAddr::from(([127, 0, 0, 1], port)));
        validate(&preflight, width, height)?;
        Self::start_inner(
            manager,
            config,
            Some(request),
            width,
            height,
            #[cfg(test)]
            None,
        )
    }
    fn start_inner(
        manager: XServerManager,
        config: XdmcpConfig,
        host: Option<super::xdmcp_host::HostRequest>,
        width: u32,
        height: u32,
        #[cfg(test)] before_publish: Option<Box<dyn FnOnce(&XdmcpSession, &AtomicBool) + Send>>,
    ) -> Result<Self, String> {
        // Validate numeric configurations before allocating a worker.
        if host.is_none() {
            #[cfg(all(windows, target_arch = "x86_64"))]
            super::xdmcp::windows_network(&config, width, height)?;
            #[cfg(not(all(windows, target_arch = "x86_64")))]
            validate(&config, width, height)?;
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let stop = cancelled.clone();
        let finished = Arc::new(AtomicBool::new(false));
        let done = finished.clone();
        let (sender, status) = mpsc::sync_channel(2);
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        let worker = MacWorker::start();
        std::thread::Builder::new()
            .name("termcraft-xdmcp-session".into())
            .spawn(move || {
                #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
                let _worker = worker;
                let prepared = match host {
                    Some(request)=>super::xdmcp_host::resolve_for_connection(request,&stop)
                        .and_then(|addresses| {
                            let endpoint=match config.selected_endpoint {
                                Some(selected) if addresses.contains(&selected)=>selected,
                                Some(_)=>return Err("The selected endpoint is absent from fresh hostname resolution; resolve and select again".into()),
                                None if addresses.len()==1=>addresses[0],
                                None=>return Err("Hostname has several IPv4 endpoints; resolve and select one in session settings".into()),
                            };
                            let mut resolved=config;resolved.selected_endpoint=Some(endpoint);Ok(resolved)
                        }),
                    None=>Ok(config),
                };
                let result = prepared.and_then(|config| manager
                    .start_xdmcp_cancellable(config, width, height, &stop))
                    .and_then(|mut session| {
                        #[cfg(test)]
                        if let Some(hook) = before_publish { hook(&session, &stop); }
                        if let Err(cancelled) = check_cancelled(&stop) {
                            session.stop_owned()?;
                            return Err(cancelled);
                        }
                        if sender
                            .send(Ok(session.ready_message.clone()))
                            .is_err()
                        {
                            return session.stop_owned();
                        }
                        while !stop.load(Ordering::Relaxed) {
                            if !session.is_running()? {
                                return Ok(());
                            }
                            std::thread::sleep(Duration::from_millis(30));
                        }
                        session.stop_owned()
                    });
                done.store(true, Ordering::Relaxed);
                let _ = sender.try_send(result.map(|()| "XDMCP session ended".into()));
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            cancelled,
            status,
            finished,
        })
    }
    /// After cancelling or dropping all handles, drain workers at application
    /// exit. Tab close only signals cancellation and never blocks a frame.
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    pub fn wait_for_shutdown(timeout: Duration) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        while MAC_WORKERS.load(Ordering::Acquire) != 0 {
            if std::time::Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        true
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed)
    }
    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Relaxed)
    }
    pub fn poll(&self) -> Option<Result<String, String>> {
        self.status.try_recv().ok()
    }
}
impl Drop for ManagedXdmcp {
    fn drop(&mut self) {
        self.cancel()
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
fn start(
    root: &Path,
    config: XdmcpConfig,
    width: u32,
    height: u32,
    cancelled: &AtomicBool,
) -> Result<XdmcpSession, String> {
    use std::{
        io::{Read, Write},
        os::{
            fd::AsRawFd,
            unix::{
                fs::{OpenOptionsExt, PermissionsExt},
                process::CommandExt,
            },
        },
        process::{Command, Stdio},
    };
    let deadline = Instant::now() + Duration::from_secs(8);
    super::xdmcp_payload::check_budget(cancelled, deadline)?;
    let mut random = [0; 2];
    getrandom::fill(&mut random).map_err(|e| e.to_string())?;
    let display = config
        .display
        .unwrap_or(100 + u16::from_ne_bytes(random) % 900);
    let args = engine_arguments(&config, display, width, height)?;
    // Refuse occupied displays before launching; the engine's exclusive lock and
    // -nopn also reject races and partial binds. Never remove an existing socket/lock.
    let socket = PathBuf::from(format!("/tmp/.X11-unix/X{display}"));
    if socket.symlink_metadata().is_ok()
        || Path::new(&format!("/tmp/.X{display}-lock"))
            .symlink_metadata()
            .is_ok()
    {
        return Err(format!(
            "Display :{display} is already occupied; choose another display"
        ));
    }
    if std::env::var_os("DISPLAY").is_none_or(|v| v.is_empty()) {
        return Err(
            "The owned Xephyr XDMCP candidate requires a host X11 display for its nested window"
                .into(),
        );
    }
    if !config.keyboard.is_empty() {
        let catalog =
            std::fs::read_to_string("/usr/share/X11/xkb/rules/evdev.lst").map_err(|e| {
                format!(
                    "Cannot read the installed XKB keyboard layouts; check xkeyboard-config: {e}"
                )
            })?;
        if !super::xdmcp::registered_layout(&catalog, &config.keyboard) {
            return Err(format!(
                "XKB keyboard layout '{}' is absent from the installed evdev catalogue; choose an installed layout or clear it to use the host keyboard",
                config.keyboard
            ));
        }
    }
    let session_ready = config.clipboard == super::xdmcp::ClipboardPolicy::Bidirectional
        || !config.keyboard.is_empty();
    let engine = if session_ready {
        "Xephyr-session-ready"
    } else {
        "Xephyr"
    };
    let binary = root.join("x11/linux-x86_64").join(engine);
    let executable = if session_ready {
        super::xdmcp_payload::load_ready_engine(&binary, cancelled, deadline).map_err(|e| {
            format!("Install the pinned session-ready Xephyr for the selected keyboard or clipboard: {e}")
        })?
    } else {
        super::xdmcp_payload::load_pinned(&binary, cancelled, deadline)
            .map_err(|e| format!("Install/verify {LINUX_XEPHYR_ARTIFACT}: {e}"))?
    };
    let runtime = tempfile::Builder::new()
        .prefix("termcraft-xdmcp-")
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
        .map_err(|e| e.to_string())?;
    let authority = runtime.path().join("authority");
    let mut cookie = zeroize::Zeroizing::new([0; 16]);
    getrandom::fill(cookie.as_mut()).map_err(|e| e.to_string())?;
    let mut auth = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&authority)
        .map_err(|e| e.to_string())?;
    auth.write_all(&65535u16.to_be_bytes())
        .map_err(|e| e.to_string())?;
    for field in [
        b"".as_slice(),
        display.to_string().as_bytes(),
        b"MIT-MAGIC-COOKIE-1",
        cookie.as_slice(),
    ] {
        auth.write_all(&(field.len() as u16).to_be_bytes())
            .and_then(|()| auth.write_all(field))
            .map_err(|e| e.to_string())?;
    }
    drop(auth);
    let mut command = Command::new(format!("/proc/self/fd/{}", executable.as_raw_fd()));
    if session_ready {
        command.arg("-termcraft-session-ready");
    }
    if config.clipboard == super::xdmcp::ClipboardPolicy::Bidirectional {
        command.arg("-termcraft-internal-clipboard-parent");
    }
    command
        .arg0(&binary)
        .args(args)
        .arg("-auth")
        .arg(&authority)
        .arg("-displayfd")
        .arg("1")
        .env_remove("WAYLAND_SOCKET")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    super::xdmcp_payload::check_budget(cancelled, deadline)?;
    let mut child = command
        .spawn()
        .map_err(|e| format!("Could not start owned Xephyr: {e}"))?;
    let mut ready = child.stdout.take().unwrap();
    let mut log = child.stderr.take().unwrap();
    for fd in [ready.as_raw_fd(), log.as_raw_fd()] {
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Could not monitor the owned XDMCP engine".into());
        }
    }
    let mut process = OwnedProcess::new(child);
    let mut output = Vec::new();
    let mut diagnostics = Vec::new();
    while Instant::now() < deadline {
        super::xdmcp_payload::check_budget(cancelled, deadline)?;
        let mut buffer = [0; 256];
        if let Ok(n) = log.read(&mut buffer) {
            if diagnostics.len() < 4096 {
                diagnostics.extend_from_slice(&buffer[..n.min(4096 - diagnostics.len())]);
            }
        }
        if !process.is_running().map_err(|e| e.to_string())? {
            // Exit may precede the next poll. Drain the available bounded diagnostics.
            let remaining = 4096usize.saturating_sub(diagnostics.len());
            let _ = log
                .by_ref()
                .take(remaining as u64)
                .read_to_end(&mut diagnostics);
            let layout_hint = if config.keyboard.is_empty() {
                String::new()
            } else {
                format!(
                    " (keyboard layout {}; check the installed xorg-xkbcomp and xkeyboard-config data)",
                    config.keyboard
                )
            };
            return Err(format!(
                "Owned Xephyr exited during XDMCP startup{layout_hint}: {}",
                String::from_utf8_lossy(&diagnostics)
            ));
        }
        let eof = match ready.read(&mut buffer) {
            Ok(n) => {
                output.extend_from_slice(&buffer[..n]);
                n == 0
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) =>
            {
                false
            }
            Err(e) => return Err(format!("Cannot read XDMCP readiness channel: {e}")),
        };
        if readiness_complete(&output, display, session_ready, eof)? {
            // Peer credentials correlate the socket with the child, never another display owner.
            let stream = connect_ready_socket(&socket, deadline, cancelled)?;
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
                || unsafe { peer.assume_init() }.pid != process.id().unwrap() as libc::pid_t
            {
                return Err("XDMCP display socket belongs to an unexpected process".into());
            }
            super::xdmcp_listener::verify_tcp_listener(
                6000 + display,
                process
                    .id()
                    .ok_or("Owned XDMCP process exited during listener verification")?,
            )?;
            super::xdmcp_payload::check_budget(cancelled, deadline)?;
            let clipboard = if config.clipboard == super::xdmcp::ClipboardPolicy::Bidirectional {
                check_cancelled(cancelled)?;
                Some(super::xdmcp_clipboard::Bridge::start(
                    config
                        .clipboard_display
                        .as_deref()
                        .ok_or("Select parent clipboard display")?,
                    config.clipboard_authority.as_deref(),
                    stream,
                    cookie.as_slice(),
                    process.id().ok_or("Owned clipboard engine exited")?,
                    &socket,
                    cancelled,
                    deadline,
                )?)
            } else {
                None
            };
            check_cancelled(cancelled)?;
            let ready_message = if clipboard.is_some() {
                format!(
                    "XDMCP display session :{display} established with bidirectional clipboard active"
                )
            } else if session_ready {
                format!("XDMCP display session :{display} established")
            } else {
                format!("Owned X server and listener :{display} ready")
            };
            return Ok(XdmcpSession {
                ready_message,
                backend: SessionBackend {
                    clipboard,
                    process,
                    display: format!(":{display}"),
                    authority,
                    log,
                    diagnostics,
                    _runtime: runtime,
                },
            });
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Err(if session_ready {
        "Timed out waiting for the selected XDMCP manager to establish its display session"
    } else {
        "Timed out waiting for the owned Xephyr XDMCP display"
    }
    .into())
}
#[cfg(target_os = "linux")]
fn readiness_complete(
    bytes: &[u8],
    display: u16,
    bridged: bool,
    eof: bool,
) -> Result<bool, String> {
    let mut expected = format!("{display}\n");
    if bridged {
        expected.push_str("session-ready-v1\n");
    }
    if !expected.as_bytes().starts_with(bytes) {
        return Err("Invalid XDMCP engine readiness response".into());
    }
    if bytes == expected.as_bytes() {
        return Ok(!bridged || eof);
    }
    if eof {
        return Err("XDMCP readiness channel closed before session initialization".into());
    }
    Ok(false)
}

#[cfg(target_os = "linux")]
pub(super) fn connect_ready_socket(
    path: &Path,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> Result<std::os::unix::net::UnixStream, String> {
    use std::os::{
        fd::{AsRawFd, FromRawFd},
        unix::net::UnixStream,
    };
    let path = path.as_os_str().as_encoded_bytes();
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    if path.len() >= address.sun_path.len() || path.contains(&0) {
        return Err("Invalid owned X display socket path".into());
    }
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    for (dst, src) in address.sun_path.iter_mut().zip(path) {
        *dst = *src as libc::c_char;
    }
    loop {
        check_cancelled(cancelled)?;
        if Instant::now() >= deadline {
            return Err("Timed out connecting to owned XDMCP display".into());
        }
        let fd = unsafe {
            libc::socket(
                libc::AF_UNIX,
                libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
                0,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let stream = unsafe { UnixStream::from_raw_fd(fd) };
        if unsafe {
            libc::connect(
                stream.as_raw_fd(),
                (&address as *const libc::sockaddr_un).cast(),
                std::mem::size_of::<libc::sockaddr_un>() as libc::socklen_t,
            )
        } == 0
        {
            return Ok(stream);
        }
        let error = std::io::Error::last_os_error();
        if !matches!(
            error.raw_os_error(),
            Some(
                libc::EAGAIN | libc::EINPROGRESS | libc::EINTR | libc::ENOENT | libc::ECONNREFUSED
            )
        ) {
            return Err(error.to_string());
        }
        std::thread::sleep(
            Duration::from_millis(10).min(deadline.saturating_duration_since(Instant::now())),
        );
    }
}

#[cfg(all(windows, target_arch = "x86_64"))]
fn start(
    root: &Path,
    config: XdmcpConfig,
    width: u32,
    height: u32,
    cancelled: &AtomicBool,
) -> Result<XdmcpSession, String> {
    let backend = super::native_windows::start_xdmcp(root, &config, width, height, cancelled)?;
    let ready_message = format!(
        "Owned Windows X11 listener {} ready. XDMCP manager authentication is pending confirmation.",
        backend.display(),
    );
    Ok(XdmcpSession {
        ready_message,
        backend,
    })
}

#[cfg(not(any(
    all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"),
    all(windows, target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
)))]
fn start(_: &Path, _: XdmcpConfig, _: u32, _: u32, _: &AtomicBool) -> Result<XdmcpSession, String> {
    Err("The owned native XDMCP engine has not passed its platform qualification. Installed-client fallback is not automatic".into())
}

#[cfg(all(test, windows, target_arch = "x86_64"))]
mod windows_tests {
    use super::*;
    use crate::x11::xdmcp::QueryMode;
    use std::time::Instant;

    #[test]
    fn windows_xdmcp_managed_loopback_reaches_native_approval_gate() {
        for mode in [QueryMode::Direct, QueryMode::Indirect] {
            let config = XdmcpConfig {
                mode,
                selected_endpoint: Some("127.0.0.1:1177".parse().unwrap()),
                allow_network_listener: true,
                ..Default::default()
            };
            let direct = ManagedXdmcp::start(XServerManager::default(), config.clone(), 800, 600);
            let resolved = ManagedXdmcp::start_hostname(
                XServerManager::default(),
                config,
                "127.0.0.1".into(),
                1177,
                800,
                600,
            );
            for pending in [direct, resolved] {
                let managed = pending.expect("Loopback is a valid Windows XDMCP endpoint");
                let deadline = Instant::now() + Duration::from_secs(3);
                loop {
                    if let Some(status) = managed.poll() {
                        assert!(status.unwrap_err().contains("no approved native build"));
                        break;
                    }
                    assert!(Instant::now() < deadline, "XDMCP worker did not finish");
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        }
    }
}

#[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
#[path = "xdmcp_chooser_test.rs"]
mod chooser_test;

#[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
pub(super) mod tests {
    #[test]
    fn fresh_hostname_resolution_rejects_stale_persisted_ip_before_engine_start() {
        let _system_resolver = super::super::xdmcp_host::TEST_SYSTEM_RESOLVER
            .lock()
            .unwrap();
        let config = super::super::xdmcp::XdmcpConfig {
            selected_endpoint: Some("192.0.2.222:1177".parse().unwrap()),
            advertise_address: Some("192.0.2.1".parse().unwrap()),
            allow_network_listener: true,
            ..Default::default()
        };
        let pending = super::ManagedXdmcp::start_hostname(
            super::super::XServerManager::default(),
            config,
            "localhost".into(),
            1177,
            800,
            600,
        )
        .unwrap();
        let end = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            if let Some(result) = pending.poll() {
                assert!(result.unwrap_err().contains("fresh hostname resolution"));
                break;
            }
            assert!(std::time::Instant::now() < end);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(pending.is_finished());
    }
    #[test]
    fn hostname_start_still_requires_explicit_listener_consent() {
        let config = super::super::xdmcp::XdmcpConfig {
            advertise_address: Some("192.0.2.1".parse().unwrap()),
            ..Default::default()
        };
        assert!(
            super::ManagedXdmcp::start_hostname(
                super::super::XServerManager::default(),
                config,
                "localhost".into(),
                1177,
                800,
                600
            )
            .err()
            .unwrap()
            .contains("listener")
        );
    }

    use super::*;
    use std::{
        io::Write,
        os::unix::{fs::PermissionsExt, process::CommandExt},
        process::{Child, Command, Stdio},
    };
    pub(in crate::x11) struct DisplayManager(Child);
    impl Drop for DisplayManager {
        fn drop(&mut self) {
            unsafe {
                libc::kill(-(self.0.id() as libc::pid_t), libc::SIGTERM);
            }
            let _ = self.0.wait();
        }
    }
    /// Observe before is_running reaps the child, so failures retain status/signal.
    fn observed_running(session: &mut XdmcpSession) -> (bool, String) {
        let pid = session.process_id();
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        let observed = pid.map(|pid| unsafe {
            libc::waitid(
                libc::P_PID,
                pid,
                &mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        });
        let status = if observed == Some(0) && unsafe { info.si_pid() } != 0 {
            format!(
                "pid={pid:?} si_code={} exit_status_or_signal={}",
                info.si_code,
                unsafe { info.si_status() }
            )
        } else {
            format!("pid={pid:?} waitid={observed:?}")
        };
        let result = session.is_running();
        let details = format!(
            "owned engine {status}; bridge/engine running result={result:?}; Xephyr log={}",
            String::from_utf8_lossy(&session.backend.diagnostics)
        );
        (matches!(result, Ok(true)), details)
    }
    #[test]
    fn saturated_socket_readiness_stays_bounded_and_cancellable() {
        use std::os::{fd::AsRawFd, unix::net::UnixListener};
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("ready");
        let listener = UnixListener::bind(&path).unwrap();
        assert_eq!(unsafe { libc::listen(listener.as_raw_fd(), 1) }, 0);
        let mut clients = Vec::new();
        for _ in 0..256 {
            match connect_ready_socket(
                &path,
                Instant::now() + Duration::from_millis(2),
                &AtomicBool::new(false),
            ) {
                Ok(client) => clients.push(client),
                Err(_) => break,
            }
        }
        assert!(!clients.is_empty());
        let start = Instant::now();
        assert!(
            connect_ready_socket(
                &path,
                start + Duration::from_millis(20),
                &AtomicBool::new(false)
            )
            .unwrap_err()
            .contains("Timed out")
        );
        assert!(start.elapsed() < Duration::from_millis(500));
        assert!(
            connect_ready_socket(
                &path,
                Instant::now() + Duration::from_secs(1),
                &AtomicBool::new(true)
            )
            .unwrap_err()
            .contains("cancel")
        );
    }
    #[test]
    fn reset_socket_recreation_is_waited_for_within_the_owned_deadline() {
        use std::os::unix::net::UnixListener;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("reset");
        let bind = path.clone();
        let worker = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(40));
            UnixListener::bind(bind).unwrap()
        });
        let connection = connect_ready_socket(
            &path,
            Instant::now() + Duration::from_millis(500),
            &AtomicBool::new(false),
        );
        let _listener = worker.join().unwrap();
        assert!(connection.is_ok(), "{connection:?}");
    }
    #[test]
    fn legacy_session_json_preserves_disabled_xdmcp_defaults() {
        let session: crate::model::Session =
            serde_json::from_str(r#"{"protocol":"XDMCP","host":"legacy.example","port":1177}"#)
                .unwrap();
        assert!(session.xdmcp_options.is_default());
        assert!(
            serde_json::to_value(session)
                .unwrap()
                .get("xdmcp_options")
                .is_none()
        );
    }
    #[test]
    #[ignore = "real session-ready Xephyr in a private XDMCP namespace without a manager"]
    fn manager_wait_honors_cancellation_and_timeout_before_parent_access() {
        assert_eq!(
            std::env::var("TERMCRAFT_TEST_XSERVER_ISOLATED").as_deref(),
            Ok("1")
        );
        let root = PathBuf::from(std::env::var_os("TERMCRAFT_TEST_XDMCP_PACKAGE").unwrap());
        for (display, cancel) in [(182u16, true), (183, false)] {
            let config = XdmcpConfig {
                selected_endpoint: Some("127.0.0.1:1178".parse().unwrap()),
                display: Some(display),
                advertise_address: Some("192.0.2.1".parse().unwrap()),
                allow_network_listener: true,
                clipboard: super::super::xdmcp::ClipboardPolicy::Bidirectional,
                clipboard_display: Some(":599".into()),
                ..Default::default()
            };
            let start = Instant::now();
            let pending = ManagedXdmcp::start(
                XServerManager::for_installation(root.clone()),
                config,
                640,
                480,
            )
            .unwrap();
            let signal = pending.cancelled.clone();
            let observer = std::thread::spawn(move || {
                let start = Instant::now();
                let socket = PathBuf::from(format!("/tmp/.X11-unix/X{display}"));
                let stream =
                    connect_ready_socket(&socket, start + Duration::from_secs(3), &signal).unwrap();
                use std::os::fd::AsRawFd;
                let mut peer = std::mem::MaybeUninit::<libc::ucred>::uninit();
                let mut size = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
                assert_eq!(
                    unsafe {
                        libc::getsockopt(
                            stream.as_raw_fd(),
                            libc::SOL_SOCKET,
                            libc::SO_PEERCRED,
                            peer.as_mut_ptr().cast(),
                            &mut size,
                        )
                    },
                    0
                );
                let pid = unsafe { peer.assume_init() }.pid as u32;
                drop(stream);
                if cancel {
                    signal.store(true, Ordering::Relaxed);
                }
                pid
            });
            // Even after the owned socket opens, no ready status may precede
            // the cancellation or manager-timeout error.
            let err = loop {
                if let Some(status) = pending.poll() {
                    break status.unwrap_err();
                }
                assert!(start.elapsed() < Duration::from_secs(10));
                std::thread::sleep(Duration::from_millis(10));
            };
            assert!(pending.is_finished() && pending.poll().is_none());
            let pid = observer.join().unwrap();
            let elapsed = start.elapsed();
            if cancel {
                assert!(
                    err.contains("cancel") && elapsed < Duration::from_secs(3),
                    "{err} {elapsed:?}"
                );
            } else {
                assert!(
                    (err.contains("Timed out") || err.contains("deadline"))
                        && elapsed < Duration::from_secs(9),
                    "{err} {elapsed:?}"
                );
            }
            assert!(!Path::new(&format!("/proc/{pid}")).exists());
            println!("manager wait cancel={cancel}: {err}; elapsed={elapsed:?}; child reaped");
        }
    }

    #[test]
    #[ignore = "owned stock Xephyr inside the private XDMCP namespace"]
    fn cancellation_before_ready_publication_reports_only_the_cleanup_or_cancel_error() {
        assert_eq!(
            std::env::var("TERMCRAFT_TEST_XSERVER_ISOLATED").as_deref(),
            Ok("1")
        );
        let package = PathBuf::from(std::env::var_os("TERMCRAFT_TEST_XDMCP_PACKAGE").unwrap());
        for (display, force_wait_error) in [(184, false), (185, true)] {
            let config = XdmcpConfig {
                selected_endpoint: Some("127.0.0.1:1178".parse().unwrap()),
                display: Some(display),
                advertise_address: Some("192.0.2.1".parse().unwrap()),
                allow_network_listener: true,
                ..Default::default()
            };
            let (observed, owned) = mpsc::channel();
            let before_publish = move |session: &XdmcpSession, stop: &AtomicBool| {
                let pid = session.process_id().unwrap();
                let authority = session.authority_path().to_owned();
                assert!(authority.exists());
                assert!(Path::new(&format!("/tmp/.X11-unix/X{display}")).exists());
                if force_wait_error {
                    // Reap this fixture's known child to force OwnedProcess::stop
                    // to report ECHILD on its following try_wait.
                    let child = pid as libc::pid_t;
                    assert_eq!(unsafe { libc::kill(child, libc::SIGTERM) }, 0);
                    assert_eq!(
                        unsafe { libc::waitpid(child, std::ptr::null_mut(), 0) },
                        child
                    );
                }
                stop.store(true, Ordering::Relaxed);
                observed.send((pid, authority)).unwrap();
            };
            let pending = ManagedXdmcp::start_inner(
                XServerManager::for_installation(package.clone()),
                config,
                None,
                640,
                480,
                Some(Box::new(before_publish)),
            )
            .unwrap();
            let (pid, authority) = owned.recv_timeout(Duration::from_secs(10)).unwrap();
            let expected = if force_wait_error {
                std::io::Error::from_raw_os_error(libc::ECHILD).to_string()
            } else {
                "XDMCP cancelled".into()
            };
            assert_eq!(
                pending.status.recv_timeout(Duration::from_secs(3)).unwrap(),
                Err(expected)
            );
            assert!(matches!(
                pending.status.recv_timeout(Duration::from_secs(1)),
                Err(mpsc::RecvTimeoutError::Disconnected)
            ));
            assert!(!authority.exists() && !Path::new(&format!("/proc/{pid}")).exists());
            assert!(!Path::new(&format!("/tmp/.X11-unix/X{display}")).exists());
        }
    }

    #[test]
    fn cancellation_before_start_never_spawns_engine() {
        let result = XServerManager::for_installation(PathBuf::from("/nonexistent"))
            .start_xdmcp_cancellable(XdmcpConfig::default(), 800, 600, &AtomicBool::new(true));
        assert!(result.err().unwrap().contains("cancel"));
    }
    #[test]
    #[ignore = "must run inside tests/support/run_xserver_isolated.sh with the private xdm package"]
    fn actual_xdm_nondefault_port_opens_selected_display_and_cleanup_preserves_host() {
        actual_xdm(false, false, "");
    }
    #[test]
    #[ignore = "must run in the private XDMCP display namespace"]
    fn actual_xdm_negotiated_clipboard_both_directions() {
        actual_xdm(true, false, "");
    }
    #[test]
    #[ignore = "owned private xdm and isolated X11/network fixture required"]
    fn actual_hostname_connect_resolves_fresh_and_manages_authenticated_window() {
        actual_xdm(false, true, "");
    }
    pub(in crate::x11) fn start_test_manager(dir: &Path, fixture: &Path) -> DisplayManager {
        std::fs::create_dir(dir.join("auth")).unwrap();
        std::fs::write(dir.join("servers"), b"").unwrap();
        std::fs::write(dir.join("access"), b"127.0.0.1\n192.0.2.1\n").unwrap();
        let marker = dir.join("managed");
        let setup = dir.join("setup");
        std::fs::write(
            &setup,
            format!(
                "#!/bin/sh\nexec /usr/bin/python '{}' '{}'\n",
                fixture.join("xdmcp_window.py").display(),
                marker.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&setup, std::fs::Permissions::from_mode(0o700)).unwrap();
        let config = dir.join("config");
        let mut file = std::fs::File::create(&config).unwrap();
        for (key, value) in [
            (
                "DisplayManager.servers",
                dir.join("servers").display().to_string(),
            ),
            (
                "DisplayManager.accessFile",
                dir.join("access").display().to_string(),
            ),
            (
                "DisplayManager.authDir",
                dir.join("auth").display().to_string(),
            ),
            (
                "DisplayManager.pidFile",
                dir.join("xdm.pid").display().to_string(),
            ),
            (
                "DisplayManager.errorLogFile",
                dir.join("xdm.log").display().to_string(),
            ),
            ("DisplayManager.requestPort", "1177".into()),
            ("DisplayManager.daemonMode", "false".into()),
            ("DisplayManager*authorize", "true".into()),
            ("DisplayManager*authName", "MIT-MAGIC-COOKIE-1".into()),
            ("DisplayManager*setup", setup.display().to_string()),
            ("DisplayManager*openDelay", "1".into()),
            ("DisplayManager*openRepeat", "3".into()),
            (
                "DisplayManager*greeterLib",
                fixture.join("libXdmGreet.so").display().to_string(),
            ),
        ] {
            writeln!(file, "{key}: {value}").unwrap();
        }
        drop(file);
        let manager_log = std::fs::File::create(dir.join("manager.log")).unwrap();
        let mut command = Command::new(fixture.join("xdm"));
        command
            .env("LD_LIBRARY_PATH", fixture.join("lib"))
            .args(["-nodaemon", "-debug", "0", "-config"])
            .arg(&config)
            .stdout(Stdio::from(manager_log.try_clone().unwrap()))
            .stderr(Stdio::from(manager_log));
        unsafe {
            command.pre_exec(|| {
                if libc::setpgid(0, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let dm = DisplayManager(command.spawn().unwrap());
        let endpoint = "127.0.0.1:1177".parse().unwrap();
        let start = Instant::now();
        loop {
            let replies = super::super::xdmcp::discover(
                &super::super::xdmcp::DiscoveryRequest::direct(endpoint),
                &AtomicBool::new(false),
            )
            .unwrap();
            if !replies.is_empty() {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "xdm did not answer: {}",
                format!(
                    "{} {}",
                    std::fs::read_to_string(dir.join("manager.log")).unwrap(),
                    std::fs::read_to_string(dir.join("xdm.log")).unwrap_or_default()
                )
            );
        }
        dm
    }
    #[test]
    fn readiness_requires_exact_complete_pinned_protocol() {
        for bytes in [b"".as_slice(), b"177\n", b"177\nsession-ready-v1"] {
            assert_eq!(readiness_complete(bytes, 177, true, false), Ok(false));
            assert!(readiness_complete(bytes, 177, true, true).is_err());
        }
        let ready = b"177\nsession-ready-v1\n";
        assert_eq!(readiness_complete(ready, 177, true, false), Ok(false));
        assert_eq!(readiness_complete(ready, 177, true, true), Ok(true));
        for bytes in [
            b"176\n".as_slice(),
            b"177\ninvalid",
            b"177\nsession-ready-v1\nsession-ready-v1\n",
        ] {
            assert!(readiness_complete(bytes, 177, true, false).is_err());
        }
        assert_eq!(readiness_complete(b"177\n", 177, false, false), Ok(true));
    }
    #[test]
    #[ignore = "private XDMCP display and authentication fixture required"]
    fn actual_us_keyboard_without_clipboard() {
        actual_xdm(false, false, "us");
    }
    #[test]
    #[ignore = "private XDMCP display and authentication fixture required"]
    fn actual_german_keyboard_with_clipboard() {
        actual_xdm(true, false, "de");
    }
    #[test]
    #[ignore = "private XDMCP display and authentication fixture required"]
    fn actual_german_keyboard_via_hostname() {
        actual_xdm(false, true, "de");
    }
    #[test]
    #[ignore = "private XDMCP namespace with the German XKB symbols file masked"]
    fn missing_requested_symbols_fails_and_cleans_owned_display() {
        assert_eq!(
            std::env::var("TERMCRAFT_TEST_XSERVER_ISOLATED").as_deref(),
            Ok("1")
        );
        assert_eq!(
            std::fs::metadata("/usr/share/X11/xkb/symbols/de")
                .unwrap()
                .len(),
            0
        );
        let children = || std::fs::read_to_string("/proc/thread-self/children").unwrap();
        let before = children();
        let manager = XServerManager::for_installation(PathBuf::from(
            std::env::var_os("TERMCRAFT_TEST_XDMCP_PACKAGE").unwrap(),
        ));
        let error = manager
            .start_xdmcp_cancellable(
                XdmcpConfig {
                    selected_endpoint: Some("127.0.0.1:1177".parse().unwrap()),
                    display: Some(184),
                    keyboard: "de".into(),
                    advertise_address: Some("192.0.2.1".parse().unwrap()),
                    allow_network_listener: true,
                    ..Default::default()
                },
                640,
                480,
                &AtomicBool::new(false),
            )
            .err()
            .unwrap();
        assert!(
            error.contains("keyboard layout de") && error.contains("Failed to compile keymap"),
            "{error}"
        );
        assert_eq!(children(), before);
        assert!(!Path::new("/tmp/.X11-unix/X184").exists());
        println!("German symbols masked: {error}; owned child reaped");
    }
    pub(super) fn host_keyboard_map() -> Vec<u8> {
        let result = Command::new("/usr/bin/xkbcomp")
            .args(["-xkb", ":0", "-"])
            .env_remove("XAUTHORITY")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        result.stdout
    }
    pub(super) fn verify_keyboard(marker: &Path, keyboard: &str, parent: &[u8]) {
        if keyboard.is_empty() {
            return;
        }
        let keysyms = std::fs::read_to_string(marker.with_extension("keyboard")).unwrap();
        let expected = if keyboard == "de" {
            "122 121"
        } else {
            "121 122"
        };
        assert_eq!(keysyms, expected);
        assert_eq!(host_keyboard_map(), parent);
        println!("Nested {keyboard} keycodes 29/52 returned {keysyms}; parent XKB map unchanged");
    }
    fn actual_xdm(clipboard: bool, hostname: bool, keyboard: &str) {
        assert_eq!(
            std::env::var("TERMCRAFT_TEST_XSERVER_ISOLATED").as_deref(),
            Ok("1")
        );
        let host = std::env::var("TERMCRAFT_TEST_XSERVER_FIXTURE_PID").unwrap();
        let package = PathBuf::from(std::env::var("TERMCRAFT_TEST_XDMCP_PACKAGE").unwrap());
        let fixture = PathBuf::from(std::env::var("TERMCRAFT_TEST_XDMCP_FIXTURE").unwrap());
        let dir = tempfile::tempdir().unwrap();
        let parent_map = if keyboard.is_empty() {
            Vec::new()
        } else {
            host_keyboard_map()
        };
        let mut dm = start_test_manager(dir.path(), &fixture);
        let marker = dir.path().join("managed");
        let endpoint = "127.0.0.1:1177".parse().unwrap();
        let options = XdmcpConfig {
            selected_endpoint: Some(endpoint),
            keyboard: keyboard.to_owned(),
            clipboard: if clipboard {
                super::super::xdmcp::ClipboardPolicy::Bidirectional
            } else {
                super::super::xdmcp::ClipboardPolicy::Disabled
            },
            clipboard_display: clipboard.then(|| ":0".into()),
            display: Some(177),
            advertise_address: Some("192.0.2.1".parse().unwrap()),
            allow_network_listener: true,
            ..Default::default()
        };
        let manager = XServerManager::for_installation(package);
        if hostname {
            let mut hostname_options = options.clone();
            hostname_options.selected_endpoint = None;
            let managed = ManagedXdmcp::start_hostname(
                manager,
                hostname_options,
                "localhost".into(),
                1177,
                800,
                600,
            )
            .unwrap();
            let deadline = Instant::now() + Duration::from_secs(12);
            let mut ready = None;
            while !marker.exists() || ready.is_none() {
                if let Some(status) = managed.poll() {
                    ready = Some(status.unwrap());
                }
                assert!(
                    !managed.is_finished(),
                    "hostname session finished before authenticated window"
                );
                assert!(
                    Instant::now() < deadline,
                    "hostname xdm window did not appear"
                );
                std::thread::sleep(Duration::from_millis(20));
            }
            let expected = if clipboard {
                "XDMCP display session :177 established with bidirectional clipboard active"
            } else if keyboard.is_empty() {
                "Owned X server and listener :177 ready"
            } else {
                "XDMCP display session :177 established"
            };
            assert_eq!(ready.as_deref(), Some(expected));
            println!("{expected}");
            let observed = std::fs::read_to_string(&marker).unwrap();
            assert!(
                observed.contains(":177")
                    && observed.contains("800x600")
                    && observed.contains("green pixel verified"),
                "{observed}"
            );
            verify_keyboard(&marker, keyboard, &parent_map);
            managed.cancel();
            let deadline = Instant::now() + Duration::from_secs(3);
            while !managed.is_finished() {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(Path::new(&format!("/proc/{host}")).exists());
            println!(
                "Fresh localhost resolution, UDP 1177, authenticated xdm mapped window and green pixel; managed cancellation completed; isolated host display survived"
            );
            return;
        }
        let mut session = manager
            .start_xdmcp_cancellable(options, 800, 600, &AtomicBool::new(false))
            .unwrap_or_else(|e| {
                panic!(
                    "{e}; xdm: {}",
                    format!(
                        "{} {}",
                        std::fs::read_to_string(dir.path().join("manager.log")).unwrap(),
                        std::fs::read_to_string(dir.path().join("xdm.log")).unwrap_or_default()
                    )
                )
            });
        assert_eq!(session.display(), ":177");
        let pid = session.process_id().unwrap();
        let command_line = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap();
        let internal = command_line
            .split(|b| *b == 0)
            .any(|arg| arg == b"-termcraft-internal-clipboard-parent");
        assert_eq!(
            internal, clipboard,
            "only bidirectional clipboard may identify its internal parent client"
        );
        let auth = session.authority_path().to_path_buf();
        let start = Instant::now();
        while !marker.exists() {
            let (running, details) = observed_running(&mut session);
            assert!(
                running,
                "{details}; xdm: {}",
                format!(
                    "{} {}",
                    std::fs::read_to_string(dir.path().join("manager.log")).unwrap(),
                    std::fs::read_to_string(dir.path().join("xdm.log")).unwrap_or_default()
                )
            );
            assert!(dm.0.try_wait().unwrap().is_none());
            assert!(
                start.elapsed() < Duration::from_secs(8),
                "xdm did not manage the display: {}",
                format!(
                    "{} {}",
                    std::fs::read_to_string(dir.path().join("manager.log")).unwrap(),
                    std::fs::read_to_string(dir.path().join("xdm.log")).unwrap_or_default()
                )
            );
            std::thread::sleep(Duration::from_millis(40));
        }
        let observed = std::fs::read_to_string(&marker).unwrap();
        assert!(observed.contains(":177"), "{observed}");
        assert!(
            observed.contains("800x600") && observed.contains("green pixel verified"),
            "{observed}"
        );
        verify_keyboard(&marker, keyboard, &parent_map);
        if clipboard {
            let authority = std::fs::read_to_string(marker.with_extension("authority")).unwrap();
            super::super::xdmcp_clipboard::verify_managed(&session, Path::new(&authority));
            assert!(session.is_running().unwrap());
        }
        // The manager's authenticated client is still live. A wrong cookie must fail.
        {
            use std::{
                io::{Read, Write},
                os::unix::net::UnixStream,
            };
            let mut client = UnixStream::connect("/tmp/.X11-unix/X177").unwrap();
            client
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            let mut setup = [0u8; 48];
            setup[0] = b'l';
            setup[2] = 11;
            setup[6] = 18;
            setup[8] = 16;
            setup[12..30].copy_from_slice(b"MIT-MAGIC-COOKIE-1");
            setup[32..].fill(0x5a);
            client.write_all(&setup).unwrap();
            let mut reply = [0];
            client.read_exact(&mut reply).unwrap();
            assert_eq!(reply[0], 0, "wrong cookie was admitted");
        }
        std::fs::remove_file(&marker).unwrap();
        let output = Command::new("/usr/bin/python")
            .arg(fixture.join("xdmcp_output.py"))
            .env("DISPLAY", ":0")
            .env_remove("XAUTHORITY")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "RandR helper status={} stdout={} stderr={}; owned observation={}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
            observed_running(&mut session).1
        );
        let output = String::from_utf8(output.stdout).unwrap();
        let parts = output.split_whitespace().collect::<Vec<_>>();
        assert_eq!(parts.len(), 3);
        let monitor = parts[0].to_string();
        let host_dimensions = format!("{}x{}", parts[1], parts[2]);
        let mut second = manager
            .start_xdmcp_cancellable(
                XdmcpConfig {
                    selected_endpoint: Some(endpoint),
                    display: Some(178),
                    fullscreen: true,
                    monitor: Some(monitor),
                    advertise_address: Some("192.0.2.1".parse().unwrap()),
                    allow_network_listener: true,
                    ..Default::default()
                },
                640,
                480,
                &AtomicBool::new(false),
            )
            .unwrap();
        let second_pid = second.process_id().unwrap();
        let second_auth = second.authority_path().to_path_buf();
        let start = Instant::now();
        while !marker.exists() {
            assert!(second.is_running().unwrap());
            assert!(start.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(20));
        }
        let observed_second = std::fs::read_to_string(&marker).unwrap();
        assert!(
            observed_second.contains(":178") && observed_second.contains(&host_dimensions),
            "selected monitor geometry was not applied: {observed_second}; expected {host_dimensions}"
        );
        assert!(Path::new(&format!("/proc/{host}")).exists());
        session.stop_owned().unwrap();
        assert!(
            second.is_running().unwrap(),
            "closing the first session stopped the second"
        );
        second.stop_owned().unwrap();
        assert!(!Path::new(&format!("/proc/{second_pid}")).exists());
        assert!(!second_auth.exists());
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
        assert!(!auth.exists());
        assert!(Path::new(&format!("/proc/{host}")).exists());
        println!(
            "Real xdm discovery on UDP 1177, MANAGE and authenticated mapped window on :177; wrong cookie rejected; second session survived first close; both owned PID/auth cleaned; isolated Cage display survived. Clipboard policy tested separately for this run."
        );
    }
}
