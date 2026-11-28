//! In-app desktop sessions with an owned headless protocol worker.
mod engine_path;
mod loader_env;
pub mod rdp_options;
pub mod vnc;
pub(crate) mod wire;
use crate::model::Session;
use std::{
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};
pub use wire::TrustDecision;
/// Desktop sign-in values have no Debug or ordinary serialization implementation.
///
/// ```compile_fail
/// use harbor_workspace::desktop::DesktopInput;
/// use harbor_workspace::credentials::Secret;
/// let answer=DesktopInput::Authenticate{id:1,user:"alice".into(),password:Secret::new("private"),domain:String::new()};
/// serde_json::to_string(&answer).unwrap();
/// ```
/// ```compile_fail
/// use harbor_workspace::desktop::DesktopInput;
/// use harbor_workspace::credentials::Secret;
/// let answer=DesktopInput::Authenticate{id:1,user:"alice".into(),password:Secret::new("private"),domain:String::new()};
/// println!("{answer:?}");
/// ```
pub enum DesktopInput {
    Trust {
        id: u64,
        decision: TrustDecision,
    },
    Authenticate {
        id: u64,
        user: String,
        password: crate::credentials::Secret,
        domain: String,
    },
    Pointer {
        x: u16,
        y: u16,
        button: Option<u8>,
        pressed: bool,
    },
    Scroll {
        x: u16,
        y: u16,
        delta: i16,
        horizontal: bool,
    },
    Key {
        code: u8,
        extended: bool,
        pressed: bool,
    },
    Text {
        value: String,
    },
    Resize {
        width: u16,
        height: u16,
    },
    Clipboard {
        text: String,
    },
    ReleaseAll,
    Close,
}
impl Drop for DesktopInput {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        match self {
            Self::Text { value } => value.zeroize(),
            Self::Clipboard { text } => text.zeroize(),
            _ => {}
        }
    }
}
fn write_input(writer: &mut impl std::io::Write, input: &DesktopInput) -> std::io::Result<()> {
    // This private encoding is used only for the already-owned helper pipe.
    let message = match input {
        DesktopInput::Clipboard { text } => wire::Input::Clipboard { text: text.clone() },
        DesktopInput::Trust { id, decision } => wire::Input::Trust {
            id: *id,
            decision: *decision,
        },
        DesktopInput::Authenticate {
            id,
            user,
            password,
            domain,
        } => wire::Input::Authenticate {
            id: *id,
            user: user.clone(),
            password: password.expose().to_owned(),
            domain: domain.clone(),
        },
        DesktopInput::Pointer {
            x,
            y,
            button,
            pressed,
        } => wire::Input::Pointer {
            x: *x,
            y: *y,
            button: *button,
            pressed: *pressed,
        },
        DesktopInput::Scroll {
            x,
            y,
            delta,
            horizontal,
        } => wire::Input::Scroll {
            x: *x,
            y: *y,
            delta: *delta,
            horizontal: *horizontal,
        },
        DesktopInput::Key {
            code,
            extended,
            pressed,
        } => wire::Input::Key {
            code: *code,
            extended: *extended,
            pressed: *pressed,
        },
        DesktopInput::Text { value } => wire::Input::Text {
            value: value.clone(),
        },
        DesktopInput::Resize { width, height } => wire::Input::Resize {
            width: *width,
            height: *height,
        },
        DesktopInput::ReleaseAll => wire::Input::ReleaseAll,
        DesktopInput::Close => wire::Input::Close,
    };
    wire::write_json(writer, &message)
}
pub struct DesktopConfig {
    /// Optional already-connected carrier; TLS and credentials still use session endpoint identity.
    pub stream: Option<std::net::TcpStream>,
    pub gateway_retry: Option<crate::ssh::desktop_gateway::GatewayRetry>,
    pub id: u64,
    pub session: Session,
    pub engine_path: PathBuf,
    pub certificate_authority: Option<PathBuf>,
    pub remembered_certificate: Option<String>,
    pub timeout_secs: u32,
}
impl DesktopConfig {
    pub fn new(id: u64, session: Session, engine_path: PathBuf) -> Self {
        Self {
            stream: None,
            gateway_retry: None,
            id,
            session,
            engine_path,
            certificate_authority: None,
            remembered_certificate: None,
            timeout_secs: 10,
        }
    }
    pub fn default_engine_path() -> PathBuf {
        engine_path::resolve(
            std::env::current_exe().ok().as_deref(),
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")),
            cfg!(debug_assertions),
        )
    }
}
pub mod carrier;
pub mod framebuffer;
pub use framebuffer::Frame;
pub enum DesktopEvent {
    CursorShape {
        width: u16,
        height: u16,
        hotspot: (u16, u16),
        pixels: Vec<u8>,
    },
    ClipboardReady,
    Clipboard {
        text: String,
    },
    Connected {
        width: u16,
        height: u16,
    },
    Frame(Arc<Frame>),
    Certificate {
        id: u64,
        host: String,
        port: u16,
        fingerprint: String,
        previous: Option<String>,
        reason: String,
        changed: bool,
    },
    TrustConfirmed {
        fingerprint: String,
        remember: bool,
    },
    Authentication {
        id: u64,
        host: String,
        port: u16,
        fingerprint: String,
        user: String,
    },
    Cursor {
        x: u16,
        y: u16,
        visible: bool,
    },
    Status {
        message: String,
    },
    Disconnected {
        message: String,
    },
}
struct State {
    ready: bool,
    cursor: Option<(u16, u16, bool)>,
    frame_revision: u64,
    frame: Option<Arc<Frame>>,
    challenge: Option<(u64, bool)>,
    connected: bool,
    dimensions: (u16, u16),
    final_message: Option<String>,
}
#[derive(Default)]
struct InputControl {
    epoch: AtomicU64,
    release: AtomicBool,
}
struct QueuedInput {
    epoch: u64,
    input: DesktopInput,
}
struct InputQueue {
    tx: mpsc::SyncSender<QueuedInput>,
    control: Arc<InputControl>,
}
struct InputReceiver {
    rx: mpsc::Receiver<QueuedInput>,
    control: Arc<InputControl>,
}
fn input_channel() -> (InputQueue, InputReceiver) {
    let (tx, rx) = mpsc::sync_channel(32);
    let control = Arc::new(InputControl::default());
    (
        InputQueue {
            tx,
            control: control.clone(),
        },
        InputReceiver { rx, control },
    )
}
pub const INPUT_QUEUE_BUSY: &str = "Desktop input queue is busy or disconnected";

impl InputQueue {
    fn try_send(&self, input: DesktopInput) -> Result<(), String> {
        if matches!(&input, DesktopInput::ReleaseAll) {
            #[allow(deprecated)]
            self.control
                .epoch
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |epoch| {
                    epoch.checked_add(1)
                })
                .map_err(|_| "Desktop input generations exhausted")?;
            self.control.release.store(true, Ordering::Release);
            return Ok(());
        }
        self.send_ordered(input)
    }

    fn send_ordered(&self, input: DesktopInput) -> Result<(), String> {
        let epoch = self.control.epoch.load(Ordering::Acquire);
        self.tx
            .try_send(QueuedInput { epoch, input })
            .map_err(|_| INPUT_QUEUE_BUSY.into())
    }
}
fn flush_release(writer: &mut impl std::io::Write, input: &InputReceiver) -> std::io::Result<()> {
    if input.control.release.swap(false, Ordering::AcqRel) {
        write_input(writer, &DesktopInput::ReleaseAll)?;
    }
    Ok(())
}
fn pump_input(
    writer: &mut impl std::io::Write,
    input: &InputReceiver,
) -> std::io::Result<Option<bool>> {
    flush_release(writer, input)?;
    match input.rx.recv_timeout(Duration::from_millis(10)) {
        Ok(value) => {
            flush_release(writer, input)?;
            if value.epoch == input.control.epoch.load(Ordering::Acquire) {
                write_input(writer, &value.input)?;
            }
            Ok(Some(true))
        }
        Err(mpsc::RecvTimeoutError::Timeout) => Ok(Some(false)),
        Err(_) => Ok(None),
    }
}
pub struct DesktopSession {
    input: InputQueue,
    events: mpsc::Receiver<DesktopEvent>,
    state: Arc<Mutex<State>>,
    cancel: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
    closed: bool,
    notified: bool,
    clipboard: rdp_options::ClipboardPolicy,
    vnc: bool,
    view_only: bool,
    keyboard: rdp_options::KeyboardRouting,
}
impl DesktopSession {
    pub fn connect(config: DesktopConfig) -> Result<Self, String> {
        if !matches!(config.session.protocol.as_str(), "RDP" | "VNC") {
            return Err("Choose an RDP or VNC desktop profile".into());
        }
        crate::model::validate_host(&config.session.host)?;
        crate::model::validate_user(&config.session.user)?;
        if config.session.port == 0 || config.timeout_secs == 0 || config.timeout_secs > 3600 {
            return Err("Choose a desktop port and a timeout between 1 and 3600 seconds".into());
        }
        if config.session.protocol == "VNC" {
            return vnc::connect(config);
        }
        if !config.session.jump.trim().is_empty() && config.stream.is_none() {
            return Err("An RDP gateway profile requires an authenticated SSH carrier".into());
        }
        if config.session.view_only && config.session.rdp_options.printer_mapping.is_some() {
            return Err("View-only sessions cannot expose a local printer".into());
        }
        if config.session.view_only && !config.session.rdp_options.serial_mappings.is_empty() {
            return Err("View-only sessions cannot expose local serial devices".into());
        }
        if config.session.view_only && config.session.rdp_options.smartcard_reader.is_some() {
            return Err("View-only sessions cannot expose a local smart card reader".into());
        }
        config
            .session
            .rdp_options
            .negotiate(&rdp_options::RdpCapabilities::embedded())?;
        let clipboard = config.session.rdp_options.clipboard;
        let view_only = config.session.view_only;
        let keyboard = config.session.rdp_options.keyboard;
        let width = u16::try_from(config.session.width).map_err(|_| "Invalid desktop width")?;
        let height = u16::try_from(config.session.height).map_err(|_| "Invalid desktop height")?;
        wire::dimensions(width, height)?;
        if config
            .remembered_certificate
            .as_ref()
            .is_some_and(|s| !fingerprint(s))
        {
            return Err("Invalid remembered desktop certificate".into());
        }
        let init = wire::Config {
            version: wire::VERSION,
            connect_to: None,
            carrier_token: None,
            options: config.session.rdp_options.clone(),
            host: config.session.host,
            port: config.session.port,
            user: config.session.user,
            width,
            height,
            timeout_secs: config.timeout_secs,
            certificate_authority: config
                .certificate_authority
                .or(config.session.rdp_options.certificate_authority),
            remembered_certificate: config.remembered_certificate,
        };
        let (input, rx) = input_channel();
        let (tx, events) = mpsc::sync_channel(32);
        let state = Arc::new(Mutex::new(State {
            ready: false,
            cursor: None,
            frame_revision: 0,
            frame: None,
            challenge: None,
            connected: false,
            dimensions: (width, height),
            final_message: None,
        }));
        let cancel = Arc::new(AtomicBool::new(false));
        let finished = Arc::new(AtomicBool::new(false));
        let shared = state.clone();
        let cancelled = cancel.clone();
        let done = finished.clone();
        std::thread::Builder::new()
            .name("rdp-supervisor".into())
            .spawn(move || {
                let result = supervise(
                    config.engine_path,
                    init,
                    config.stream,
                    config.gateway_retry,
                    rx,
                    tx,
                    shared.clone(),
                    cancelled,
                );
                let mut state = shared.lock().unwrap_or_else(|p| p.into_inner());
                state.challenge = None;
                state.connected = false;
                if state.final_message.is_none() {
                    state.final_message = Some(
                        result
                            .err()
                            .unwrap_or_else(|| "Remote desktop disconnected".into()),
                    );
                }
                drop(state);
                done.store(true, Ordering::Release);
            })
            .map_err(|_| "Could not start the desktop supervisor")?;
        Ok(Self {
            input,
            events,
            state,
            cancel,
            finished,
            closed: false,
            notified: false,
            clipboard,
            vnc: false,
            view_only,
            keyboard,
        })
    }
    pub fn send(&self, mut input: DesktopInput) -> Result<(), String> {
        if self.closed || self.finished.load(Ordering::Acquire) {
            return Err("Desktop connection is closed".into());
        }
        check_input_policy(self.clipboard, self.keyboard, self.view_only, &input)?;
        if self.vnc {
            match &input {
                DesktopInput::Key { code, extended, .. }
                    if vnc::keysym(*code, *extended).is_none() =>
                {
                    return Err("Unsupported VNC key".into());
                }
                DesktopInput::Resize { .. } => return Err(
                    "This VNC transport supports server-driven resize; the canvas scales locally"
                        .into(),
                ),
                _ => {}
            }
        }
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if matches!(&input, DesktopInput::ReleaseAll) && !state.connected {
            return Ok(());
        }
        match &mut input {
            DesktopInput::Pointer {
                x,
                y,
                button,
                pressed,
            } => {
                if (*x >= state.dimensions.0 || *y >= state.dimensions.1)
                    && button.is_some()
                    && *pressed
                {
                    return Ok(());
                }
                *x = (*x).min(state.dimensions.0 - 1);
                *y = (*y).min(state.dimensions.1 - 1);
            }
            DesktopInput::Scroll { x, y, .. } => {
                *x = (*x).min(state.dimensions.0 - 1);
                *y = (*y).min(state.dimensions.1 - 1);
            }
            _ => {}
        }
        let answer = match &input {
            DesktopInput::Trust { id, .. } => Some((*id, true)),
            DesktopInput::Authenticate {
                id,
                user,
                password,
                domain,
            } => {
                if user.is_empty()
                    || user.len() > 4096
                    || password.expose().len() > 16384
                    || domain.len() > 4096
                {
                    return Err("Desktop sign-in fields exceed their supported limits".into());
                }
                Some((*id, false))
            }
            DesktopInput::Pointer { x, y, button, .. } => {
                if *x >= state.dimensions.0
                    || *y >= state.dimensions.1
                    || button.is_some_and(|b| !(1..=3).contains(&b))
                {
                    return Err("Pointer is outside the remote desktop".into());
                }
                None
            }
            DesktopInput::Scroll { x, y, delta, .. } => {
                if *x >= state.dimensions.0
                    || *y >= state.dimensions.1
                    || !(-256..=255).contains(delta)
                {
                    return Err("Invalid desktop scroll event".into());
                }
                None
            }
            DesktopInput::Text { value } => {
                if value.len() > 4096 {
                    return Err("Desktop text batch exceeds 4 KiB".into());
                }
                None
            }
            DesktopInput::Resize { width, height } => {
                wire::dimensions(*width, *height)?;
                None
            }
            _ => None,
        };
        if let Some(answer) = answer {
            if state.challenge != Some(answer) {
                return Err("This desktop sign-in prompt is stale or already answered".into());
            }
        } else if !matches!(input, DesktopInput::Close | DesktopInput::ReleaseAll)
            && (!state.connected || state.challenge.is_some())
        {
            return Err("Complete desktop sign-in before sending input".into());
        }
        self.input
            .try_send(input)
            .map_err(|_| "Desktop input queue is busy or disconnected")?;
        if answer.is_some() {
            state.challenge = None;
        }
        Ok(())
    }
    /// Release after preceding input. Cancellation uses the priority ReleaseAll
    /// path in send(), which also invalidates queued input.
    pub fn release_after_input(&self) -> Result<(), String> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if self.closed || self.is_finished() || !state.connected || state.challenge.is_some() {
            return Err("The desktop is not ready for macro input.".into());
        }
        self.input.send_ordered(DesktopInput::ReleaseAll)
    }

    pub fn poll(&mut self) -> Vec<DesktopEvent> {
        if self.closed {
            return Vec::new();
        }
        let mut events = self.events.try_iter().take(32).collect::<Vec<_>>();
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((x, y, visible)) = state.cursor.take() {
            events.push(DesktopEvent::Cursor { x, y, visible });
        }
        if let Some(frame) = state.frame.take() {
            events.push(DesktopEvent::Frame(frame));
        }
        if self.finished.load(Ordering::Acquire) && !self.notified {
            self.notified = true;
            events.push(DesktopEvent::Disconnected {
                message: state
                    .final_message
                    .take()
                    .unwrap_or_else(|| "Remote desktop disconnected".into()),
            });
        }
        events
    }
    pub fn close(&mut self) {
        self.closed = true;
        self.cancel.store(true, Ordering::Release);
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        state.frame = None;
        state.challenge = None;
        state.connected = false;
    }
    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }
}
impl Drop for DesktopSession {
    fn drop(&mut self) {
        self.close();
    }
}
fn fingerprint(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn supervise(
    path: PathBuf,
    mut config: wire::Config,
    stream: Option<std::net::TcpStream>,
    retry: Option<crate::ssh::desktop_gateway::GatewayRetry>,
    input: InputReceiver,
    events: mpsc::SyncSender<DesktopEvent>,
    state: Arc<Mutex<State>>,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    let _relay = if let Some(stream) = stream {
        let timeout = Duration::from_secs(config.timeout_secs as u64);
        let relay = carrier::SocketRelay::for_rdp(stream, retry, timeout)?;
        config.connect_to = Some(relay.address());
        config.carrier_token = Some(relay.token());
        Some(relay)
    } else {
        None
    };
    if let Err(error) = std::fs::metadata(&path) {
        if error.kind() == std::io::ErrorKind::NotFound {
            return Err(engine_path::launch_error(&path, &error));
        }
    }
    // Development fixtures name their own helper. Installed releases accept only
    // the declared engine next to the application.
    let verified = if cfg!(debug_assertions) {
        None
    } else {
        let catalog = crate::runtime::RuntimeCatalog::for_current_executable()?;
        let helper = catalog.rdp()?;
        if helper.path != path {
            return Err("RDP helper must be the verified engine next to TermCraft".into());
        }
        Some(helper)
    };
    #[cfg(target_os = "linux")]
    let (mut command, _image) = match verified {
        Some(helper) => {
            let (command, image) = helper.sealed_command()?;
            (command, Some(image))
        }
        None => (Command::new(&path), None),
    };
    #[cfg(target_os = "macos")]
    let (mut command, _snapshot) = match verified {
        Some(helper) => {
            let (command, snapshot) = helper.snapshot_command()?;
            (command, Some(snapshot))
        }
        None => (Command::new(&path), None),
    };
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let mut command = Command::new(verified.as_ref().map_or(&path, |helper| &helper.path));
    configure_rdp_command(&mut command);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    }
    let mut child = command
        .spawn()
        .map_err(|error| engine_path::launch_error(&path, &error))?;
    #[cfg(target_os = "linux")]
    drop(_image);
    #[cfg(windows)]
    drop(verified);
    let mut stdin = child
        .stdin
        .take()
        .ok_or("RDP helper has no private input pipe")?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or("RDP helper has no private output pipe")?;
    let write_cancel = cancel.clone();
    let write_config = config.clone();
    let writer = std::thread::spawn(move || {
        let result = (|| {
            wire::write_json(&mut stdin, &wire::Input::Configure(write_config))?;
            while !write_cancel.load(Ordering::Acquire) {
                if pump_input(&mut stdin, &input)?.is_none() {
                    break;
                }
            }
            Ok::<(), std::io::Error>(())
        })();
        if result.is_err() {
            write_cancel.store(true, Ordering::Release);
        }
    });
    let startup_deadline =
        std::time::Instant::now() + Duration::from_secs(config.timeout_secs.max(5) as u64);
    let read_cancel = cancel.clone();
    let shared = state.clone();
    let reader = std::thread::spawn(move || {
        let result = read_events(&mut stdout, &config, &events, &shared, &read_cancel);
        if let Err(message) = result {
            let mut state = shared.lock().unwrap_or_else(|p| p.into_inner());
            if state.final_message.is_none() {
                state.final_message = Some(message);
            }
        }
        read_cancel.store(true, Ordering::Release);
    });
    loop {
        if std::time::Instant::now() > startup_deadline
            && !state.lock().unwrap_or_else(|p| p.into_inner()).ready
        {
            state
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .final_message = Some("RDP helper startup timed out".into());
            cancel.store(true, Ordering::Release);
        }
        if cancel.load(Ordering::Acquire) {
            let _ = child.kill();
            break;
        }
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => {
                let _ = child.kill();
                break;
            }
        }
    }
    let _ = child.wait();
    cancel.store(true, Ordering::Release);
    let _ = writer.join();
    let _ = reader.join();
    Ok(())
}
fn read_events(
    reader: &mut impl std::io::Read,
    config: &wire::Config,
    events: &mpsc::SyncSender<DesktopEvent>,
    state: &Arc<Mutex<State>>,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let mut ready = false;
    while !cancel.load(Ordering::Acquire) {
        let (kind, data) = wire::read_packet(reader)
            .map_err(|_| "RDP helper transport closed or sent an invalid packet")?;
        if kind == 2 {
            if !ready || data.len() < 12 {
                return Err("Invalid desktop framebuffer packet".into());
            }
            let width = u16::from_be_bytes(data[..2].try_into().unwrap());
            let height = u16::from_be_bytes(data[2..4].try_into().unwrap());
            let length = wire::dimensions(width, height)?;
            if data.len() != length + 12 {
                return Err("Desktop framebuffer length does not match its dimensions".into());
            }
            let mut state = state.lock().unwrap_or_else(|p| p.into_inner());
            if !state.connected {
                return Err("Desktop frame arrived before connection authentication".into());
            }
            let revision = u64::from_be_bytes(data[4..12].try_into().unwrap());
            if revision <= state.frame_revision {
                return Err("Desktop frame revision did not advance".into());
            }
            state.frame_revision = revision;
            state.dimensions = (width, height);
            state.frame = Some(Arc::new(Frame {
                width,
                height,
                revision: u64::from_be_bytes(data[4..12].try_into().unwrap()),
                pixels: data[12..].to_vec(),
            }));
            continue;
        }
        let message: wire::Event = serde_json::from_slice(&data)
            .map_err(|_| "RDP helper sent an invalid control message")?;
        if !ready {
            if matches!(
                message,
                wire::Event::Ready {
                    version: wire::VERSION
                }
            ) {
                ready = true;
                state.lock().unwrap_or_else(|p| p.into_inner()).ready = true;
                continue;
            }
            return Err("RDP helper protocol version does not match TermCraft".into());
        }
        let event = match message {
            wire::Event::ClipboardReady => DesktopEvent::ClipboardReady,
            wire::Event::Clipboard { text } => {
                if !config.options.clipboard.from_remote() || text.len() > 8192 {
                    return Err("Unexpected or oversized RDP clipboard text".into());
                }
                DesktopEvent::Clipboard { text }
            }
            wire::Event::Ready { .. } => return Err("Unexpected RDP helper handshake".into()),
            wire::Event::Connected { width, height } => {
                wire::dimensions(width, height)?;
                let mut state = state.lock().unwrap_or_else(|p| p.into_inner());
                state.connected = true;
                state.challenge = None;
                state.dimensions = (width, height);
                DesktopEvent::Connected { width, height }
            }
            wire::Event::Certificate {
                id,
                host,
                port,
                fingerprint: fp,
                previous,
                reason,
                changed,
            } => {
                check_endpoint(config, id, &host, port, &fp)?;
                state.lock().unwrap_or_else(|p| p.into_inner()).challenge = Some((id, true));
                DesktopEvent::Certificate {
                    id,
                    host,
                    port,
                    fingerprint: fp,
                    previous,
                    reason,
                    changed,
                }
            }
            wire::Event::Authentication {
                id,
                host,
                port,
                fingerprint: fp,
                user,
            } => {
                check_endpoint(config, id, &host, port, &fp)?;
                state.lock().unwrap_or_else(|p| p.into_inner()).challenge = Some((id, false));
                DesktopEvent::Authentication {
                    id,
                    host,
                    port,
                    fingerprint: fp,
                    user,
                }
            }
            wire::Event::TrustConfirmed {
                fingerprint: fp,
                remember,
            } => {
                if !fingerprint(&fp) {
                    return Err("Invalid RDP certificate fingerprint".into());
                }
                DesktopEvent::TrustConfirmed {
                    fingerprint: fp,
                    remember,
                }
            }
            wire::Event::Cursor { x, y, visible } => {
                state.lock().unwrap_or_else(|p| p.into_inner()).cursor = Some((x, y, visible));
                continue;
            }
            wire::Event::Status { message } => DesktopEvent::Status { message },
            wire::Event::Disconnected { message } => {
                state
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .final_message = Some(message);
                return Ok(());
            }
        };
        events
            .try_send(event)
            .map_err(|_| "Desktop event queue is full; connection stopped")?;
    }
    Ok(())
}
fn check_endpoint(
    config: &wire::Config,
    id: u64,
    host: &str,
    port: u16,
    fp: &str,
) -> Result<(), String> {
    if id == 0 || host != config.host || port != config.port || !fingerprint(fp) {
        Err("RDP helper challenge does not match this endpoint".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod queue_tests {
    use super::*;
    #[test]
    fn local_keyboard_and_view_only_are_enforced_before_queueing_input() {
        use rdp_options::{ClipboardPolicy, KeyboardRouting};
        let key = DesktopInput::Key {
            code: 30,
            extended: false,
            pressed: true,
        };
        assert!(
            check_input_policy(
                ClipboardPolicy::Disabled,
                KeyboardRouting::Local,
                false,
                &key
            )
            .is_err()
        );
        assert!(
            check_input_policy(
                ClipboardPolicy::Disabled,
                KeyboardRouting::Remote,
                true,
                &key
            )
            .is_err()
        );
        assert!(
            check_input_policy(
                ClipboardPolicy::Disabled,
                KeyboardRouting::Remote,
                false,
                &key
            )
            .is_ok()
        );
        assert!(
            check_input_policy(
                ClipboardPolicy::Disabled,
                KeyboardRouting::Local,
                true,
                &DesktopInput::ReleaseAll
            )
            .is_ok()
        );
        let pointer = DesktopInput::Pointer {
            x: 1,
            y: 1,
            button: None,
            pressed: false,
        };
        assert!(
            check_input_policy(
                ClipboardPolicy::Disabled,
                KeyboardRouting::Local,
                false,
                &pointer
            )
            .is_ok()
        );
        for policy in [
            ClipboardPolicy::Disabled,
            ClipboardPolicy::ToRemote,
            ClipboardPolicy::FromRemote,
            ClipboardPolicy::Bidirectional,
        ] {
            let input = DesktopInput::Clipboard {
                text: "hello 🐧".into(),
            };
            assert_eq!(
                check_input_policy(policy, KeyboardRouting::Remote, false, &input).is_ok(),
                policy.to_remote()
            );
            assert!(check_input_policy(policy, KeyboardRouting::Remote, true, &input).is_err());
        }
        let input = DesktopInput::Clipboard {
            text: "x".repeat(8193),
        };
        assert!(
            check_input_policy(
                ClipboardPolicy::Bidirectional,
                KeyboardRouting::Remote,
                false,
                &input
            )
            .is_err()
        );
    }
    #[test]
    fn focus_release_is_encoded_even_with_full_input_queue() {
        let (tx, rx) = input_channel();
        for code in 1..=32 {
            tx.try_send(DesktopInput::Key {
                code,
                extended: false,
                pressed: true,
            })
            .unwrap();
        }
        let _ = tx.try_send(DesktopInput::ReleaseAll);
        let mut output = Vec::new();
        while pump_input(&mut output, &rx).unwrap() == Some(true) {}
        let mut bytes = std::io::Cursor::new(&output);
        let mut messages = Vec::new();
        while bytes.position() < (output.len() as u64) {
            let (kind, payload) = wire::read_packet(&mut bytes).unwrap();
            assert_eq!(kind, 1);
            messages.push(serde_json::from_slice::<wire::Input>(&payload).unwrap());
        }
        assert!(
            matches!(messages.last(), Some(wire::Input::ReleaseAll)),
            "A focus-loss release was lost behind full input"
        );
        tx.try_send(DesktopInput::Key {
            code: 99,
            extended: false,
            pressed: true,
        })
        .unwrap();
        let mut next = Vec::new();
        assert_eq!(pump_input(&mut next, &rx).unwrap(), Some(true));
        let (_, bytes) = wire::read_packet(&mut std::io::Cursor::new(next)).unwrap();
        assert!(matches!(
            serde_json::from_slice::<wire::Input>(&bytes).unwrap(),
            wire::Input::Key {
                code: 99,
                pressed: true,
                ..
            }
        ));
    }
}

fn check_input_policy(
    clipboard: rdp_options::ClipboardPolicy,
    keyboard: rdp_options::KeyboardRouting,
    view_only: bool,
    input: &DesktopInput,
) -> Result<(), String> {
    if let DesktopInput::Clipboard { text } = input {
        if !clipboard.to_remote() || text.len() > 8192 {
            return Err("Clipboard upload is disabled or exceeds 8192 UTF-8 bytes".into());
        }
    }
    if view_only
        && matches!(
            input,
            DesktopInput::Pointer { .. }
                | DesktopInput::Scroll { .. }
                | DesktopInput::Key { .. }
                | DesktopInput::Text { .. }
                | DesktopInput::Clipboard { .. }
        )
    {
        return Err("Remote desktop is view-only".into());
    }
    if keyboard == rdp_options::KeyboardRouting::Local
        && matches!(input, DesktopInput::Key { .. } | DesktopInput::Text { .. })
    {
        return Err("Keyboard input is routed locally for this session".into());
    }
    Ok(())
}

fn configure_rdp_command(command: &mut std::process::Command) {
    loader_env::sanitize(command);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
}

#[cfg(all(test, target_os = "linux"))]
#[test]
fn rdp_helper_launch_removes_loader_overrides() {
    use sha2::{Digest, Sha256};
    let directory = tempfile::tempdir().unwrap();
    let helper = "termcraft-rdp-engine";
    let bytes = std::fs::read("/bin/sh").unwrap();
    std::fs::copy("/bin/sh", directory.path().join(helper)).unwrap();
    let manifest = serde_json::json!({
        "schema_version": 1,
        "platform": "linux",
        "architecture": std::env::consts::ARCH,
        "rdp_helper_version": "fixture",
        "files": {helper: Sha256::digest(&bytes).iter().map(|byte| format!("{byte:02x}")).collect::<String>()}
    });
    std::fs::write(
        directory.path().join("PACKAGE-MANIFEST.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let artifact = crate::runtime::RuntimeCatalog::new(directory.path())
        .rdp()
        .unwrap();
    let (mut command, _sealed_image) = artifact.sealed_command().unwrap();
    command.arg("-c").arg(r#"
        test -z "${LD_LIBRARY_PATH+x}${LD_PRELOAD+x}${DYLD_FALLBACK_LIBRARY_PATH+x}${LIBPCSCLITE_DELEGATE+x}${TERMCRAFT_TEST_PCSC_LIBRARY+x}" &&
        test "${TERMCRAFT_OTHER_SETTING+x}" = x
    "#);
    command
        .env("LD_LIBRARY_PATH", "")
        .env("LD_PRELOAD", "")
        .env("DYLD_FALLBACK_LIBRARY_PATH", "")
        .env("LIBPCSCLITE_DELEGATE", "")
        .env("TERMCRAFT_TEST_PCSC_LIBRARY", "")
        .env("TERMCRAFT_OTHER_SETTING", "present");
    configure_rdp_command(&mut command);
    assert!(command.status().unwrap().success());
}
