//! Owned, bounded RFB 3.8 server. Classic VNC authentication is not encryption.
use super::supervisor::{self, Context};
use crate::credentials::Secret;
use std::{
    io,
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;
mod browser;
mod clipboard;
#[cfg(any(windows, target_os = "linux", target_os = "macos", test))]
mod keyboard;
#[cfg(test)]
mod lifecycle_tests;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "linux")]
mod wayland;
#[cfg(windows)]
mod windows;
#[cfg(target_os = "linux")]
mod x11;

#[derive(Clone, Debug)]
pub enum VncSource {
    #[cfg(test)]
    SyntheticFixture {
        events: std::path::PathBuf,
    },
    #[cfg(test)]
    LifecycleFixture {
        entered: Arc<AtomicBool>,
        release: Arc<AtomicBool>,
        released: Arc<AtomicBool>,
        clipboard: bool,
    },
    /// Explicit X11 display, such as :2. The host DISPLAY is never inferred.
    X11 {
        display: String,
    },
    /// Explicitly selected primary display on the current Windows desktop.
    WindowsPrimaryDesktop,
    /// Opens a fresh user-mediated ScreenCast/RemoteDesktop portal selection.
    WaylandPortal,
    MacOsDisplay {
        display_id: u32,
    },
}
pub struct VncServiceConfig {
    pub bind: SocketAddr,
    /// Optional same-instance HTTP viewer. None disables the listener.
    pub browser_bind: Option<SocketAddr>,
    pub allow_non_loopback: bool,
    pub allow_unencrypted: bool,
    pub source: VncSource,
    pub x11_authority: Option<std::path::PathBuf>,
    pub password: Secret,
    pub allow_control: bool,
    pub allow_clipboard: bool,
    pub lifetime: Option<Duration>,
}
impl VncServiceConfig {
    pub fn local(source: VncSource, password: Secret) -> Self {
        Self {
            bind: ([127, 0, 0, 1], 0).into(),
            browser_bind: None,
            allow_non_loopback: false,
            allow_unencrypted: false,
            source,
            x11_authority: None,
            password,
            allow_control: false,
            allow_clipboard: false,
            lifetime: Some(Duration::from_secs(3600)),
        }
    }
    fn validate(&self) -> Result<(), String> {
        if !self.allow_unencrypted {
            return Err("This server uses unencrypted classic VNC. Explicit transport consent is required; use an SSH tunnel on untrusted networks.".into());
        }
        if !self.bind.ip().is_loopback() && !self.allow_non_loopback {
            return Err("Enable network access before using a non-loopback address.".into());
        }
        if self
            .browser_bind
            .is_some_and(|bind| !bind.ip().is_loopback())
            && !self.allow_non_loopback
        {
            return Err(
                "Enable network access before using a non-loopback browser address.".into(),
            );
        }
        let p = self.password.expose();
        if p.is_empty() || p.len() > 8 || !p.bytes().all(|b| b.is_ascii() && !b.is_ascii_control())
        {
            return Err("Classic VNC requires 1–8 printable ASCII password bytes; passwords are never silently truncated.".into());
        }
        if let VncSource::X11 { display } = &self.source {
            let number = display.strip_prefix(':').and_then(|s| {
                // Match x11rb's display/screen u16 parsing without opening a display.
                let (number, screen) = s.split_once('.').unwrap_or((s, "0"));
                screen.parse::<u16>().ok()?;
                number.parse::<u16>().ok()
            });
            // x11rb also constructs a TCP fallback at 6000 + display, even for :N.
            if number.is_none_or(|number| number.checked_add(6000).is_none())
                || display.len() > 32
                || display
                    .bytes()
                    .any(|b| !b.is_ascii_digit() && b != b':' && b != b'.')
            {
                return Err("Choose an explicit local X11 display such as :2 or :2.0 (display 0–59535, screen 0–65535). Remote X11 displays are not accepted.".into());
            }
        }
        Ok(())
    }
}
const MAX_PIXELS: usize = 8_388_608;
const IO_LIMIT: Duration = Duration::from_secs(5);
const IDLE_LIMIT: Duration = Duration::from_secs(300);
#[derive(Clone)]
struct Frame {
    width: u16,
    height: u16,
    rgba: Vec<u8>,
}
impl Frame {
    fn validate(&self) -> io::Result<()> {
        let pixels = usize::from(self.width) * usize::from(self.height);
        if pixels == 0 || pixels > MAX_PIXELS || self.rgba.len() != pixels * 4 {
            return Err(io::Error::other(
                "Display exceeds the bounded VNC framebuffer",
            ));
        }
        Ok(())
    }
}
#[derive(Clone)]
enum Input {
    Key { down: bool, keysym: u32 },
    Pointer { buttons: u8, x: u16, y: u16 },
    Release,
    Clipboard(Zeroizing<String>),
}
trait Source: Send {
    fn enable_clipboard(&mut self) -> io::Result<()> {
        Err(io::Error::other(
            "Clipboard unavailable for selected source",
        ))
    }
    fn clipboard(&mut self) -> io::Result<Option<Zeroizing<String>>> {
        Ok(None)
    }
    fn set_clipboard(&mut self, _text: Zeroizing<String>) -> io::Result<()> {
        Err(io::Error::other("Clipboard disabled"))
    }
    fn capture(&mut self) -> io::Result<Frame>;
    fn input(&mut self, event: Input) -> io::Result<()>;
}
fn open_source(
    source: &VncSource,
    control: bool,
    authority: Option<&std::path::Path>,
    cancel: Arc<dyn Fn() -> bool + Send + Sync>,
) -> Result<Box<dyn Source>, String> {
    #[cfg(not(target_os = "linux"))]
    let _ = cancel;
    match source {
        #[cfg(test)]
        VncSource::SyntheticFixture { events } => Ok(Box::new(browser::tests::Synthetic {
            events: events.clone(),
        })),
        #[cfg(test)]
        VncSource::LifecycleFixture {
            entered,
            release,
            released,
            clipboard,
        } => Ok(Box::new(lifecycle_tests::Controlled::new(
            entered.clone(),
            release.clone(),
            released.clone(),
            *clipboard,
        ))),
        #[cfg(target_os = "linux")]
        VncSource::X11 { display } => {
            x11::X11::open(display, control, authority).map(|s| Box::new(s) as Box<dyn Source>)
        }
        #[cfg(windows)]
        VncSource::WindowsPrimaryDesktop => {
            windows::Desktop::open(control).map(|s| Box::new(s) as Box<dyn Source>)
        }
        #[cfg(target_os = "linux")]
        VncSource::WaylandPortal => {
            wayland::Wayland::open(control, cancel).map(|s| Box::new(s) as Box<dyn Source>)
        }
        #[cfg(target_os = "macos")]
        VncSource::MacOsDisplay { display_id } => {
            macos::Desktop::open(*display_id, control, cancel.clone())
                .map(|s| Box::new(s) as Box<dyn Source>)
        }
        _ => Err("The selected VNC display backend is unavailable on this platform.".into()),
    }
}
struct Shared {
    frame: Mutex<Arc<Frame>>,
    clipboard: Mutex<(u64, Option<Zeroizing<String>>)>,
    input: std::sync::mpsc::SyncSender<Input>,
    failure: Mutex<Option<String>>,
    control_active: AtomicBool,
    release: AtomicBool,
}
// Stop wins a Source-operation status race only when cancellation is actually
// observed. ErrorKind::Interrupted without that signal is still a real failure.
fn source_failed(state: &Shared, context: &Context, message: &'static str) {
    if context.cancelled() {
        return;
    }
    *state.failure.lock().unwrap() = Some(message.into());
    context.request_stop();
}
pub(super) struct Listener {
    browser: Option<browser::Gateway>,
    listener: TcpListener,
    config: VncServiceConfig,
    source: Box<dyn Source>,
    frame: Frame,
}
impl Listener {
    pub(super) fn bind_cancel(
        config: VncServiceConfig,
        cancel: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<Self, String> {
        config.validate()?;
        if matches!(config.source, VncSource::WaylandPortal) && config.allow_clipboard {
            return Err("Wayland portal clipboard sharing is unavailable. Disable clipboard sharing for this source.".into());
        }
        let mut source = open_source(
            &config.source,
            config.allow_control,
            config.x11_authority.as_deref(),
            cancel.clone(),
        )?;
        if config.allow_clipboard {
            source
                .enable_clipboard()
                .map_err(|_| "Clipboard unavailable for selected source".to_string())?;
        }
        let frame = source.capture().map_err(|error| {
            if matches!(config.source, VncSource::WaylandPortal) {
                format!("Wayland sharing failed: {error}")
            } else {
                "Could not capture the explicitly selected display.".into()
            }
        })?;
        frame.validate().map_err(|e| e.to_string())?;
        if cancel() {
            return Err("Service startup cancelled.".into());
        }
        let listener = TcpListener::bind(config.bind)
            .map_err(|e| format!("Could not bind VNC listener: {e}"))?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let browser = config
            .browser_bind
            .map(|bind| browser::Gateway::bind(bind, listener.local_addr()?))
            .transpose()
            .map_err(|e| format!("Could not bind browser viewer: {e}"))?;
        Ok(Self {
            browser,
            listener,
            config,
            source,
            frame,
        })
    }
    pub(super) fn address(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }
    pub(super) fn browser_address(&self) -> io::Result<Option<SocketAddr>> {
        self.browser
            .as_ref()
            .map(browser::Gateway::address)
            .transpose()
    }
    pub(super) fn serve(mut self, context: &Arc<Context>) -> io::Result<()> {
        let (tx, rx) = std::sync::mpsc::sync_channel(64);
        let shared = Arc::new(Shared {
            frame: Mutex::new(Arc::new(self.frame)),
            clipboard: Mutex::new((0, None)),
            input: tx,
            failure: Mutex::new(None),
            control_active: AtomicBool::new(false),
            release: AtomicBool::new(false),
        });
        let state = shared.clone();
        let cancelled = context.clone();
        let capture = thread::Builder::new()
            .name("vnc-selected-display".into())
            .spawn(move || {
                let size = {
                    let f = state.frame.lock().unwrap();
                    (f.width, f.height)
                };
                let mut next = Instant::now();
                'source: while !cancelled.cancelled() {
                    for _ in 0..64 {
                        if cancelled.cancelled() {
                            break;
                        }
                        match rx.try_recv() {
                            Ok(Input::Clipboard(text)) => {
                                {
                                    let mut v = state.clipboard.lock().unwrap();
                                    v.0 = v.0.wrapping_add(1);
                                    v.1 = None;
                                }
                                if self.source.set_clipboard(text).is_err() {
                                    source_failed(
                                        &state,
                                        &cancelled,
                                        "Selected display clipboard failed.",
                                    );
                                    break 'source;
                                }
                            }
                            Ok(event) => {
                                if self.source.input(event).is_err() {
                                    source_failed(
                                        &state,
                                        &cancelled,
                                        "Selected display input failed.",
                                    );
                                    break 'source;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    if cancelled.cancelled() {
                        break;
                    }
                    if state.release.swap(false, Ordering::AcqRel) {
                        if self.source.input(Input::Release).is_err() {
                            source_failed(
                                &state,
                                &cancelled,
                                "Selected display input release failed.",
                            );
                            break 'source;
                        }
                        state.control_active.store(false, Ordering::Release);
                    }
                    if self.config.allow_clipboard {
                        match self.source.clipboard() {
                            Ok(Some(text)) => {
                                let mut v = state.clipboard.lock().unwrap();
                                v.0 = v.0.wrapping_add(1);
                                v.1 = Some(text);
                            }
                            Ok(None) => {}
                            Err(_) => {
                                source_failed(
                                    &state,
                                    &cancelled,
                                    "Selected display clipboard stopped.",
                                );
                                break 'source;
                            }
                        }
                    }
                    if Instant::now() >= next {
                        match self.source.capture().and_then(|f| {
                            f.validate()?;
                            if (f.width, f.height) != size {
                                return Err(io::Error::other(
                                    "Selected display changed size; restart sharing.",
                                ));
                            }
                            Ok(f)
                        }) {
                            Ok(f) => *state.frame.lock().unwrap() = Arc::new(f),
                            Err(_) => {
                                source_failed(
                                    &state,
                                    &cancelled,
                                    "Selected display capture stopped; restart sharing.",
                                );
                                break 'source;
                            }
                        }
                        next = Instant::now() + Duration::from_millis(100);
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                let _ = self.source.input(Input::Release);
            })?;
        let browser_worker = if let Some(browser) = self.browser.take() {
            let ctx = context.clone();
            let failure = shared.clone();
            let worker = thread::Builder::new()
                .name("vnc-browser-gateway".into())
                .spawn(move || {
                    let result = browser.serve(&ctx);
                    if result.is_err() {
                        *failure.failure.lock().unwrap() =
                            Some("Browser viewer listener stopped.".into());
                        ctx.log("Browser viewer stopped with an error.");
                        ctx.request_stop();
                    }
                });
            match worker {
                Ok(worker) => Some(worker),
                Err(error) => {
                    context.request_stop();
                    let _ = capture.join();
                    return Err(error);
                }
            }
        } else {
            None
        };
        let password = self.config.password;
        let control = self.config.allow_control;
        let clipboard = self.config.allow_clipboard;
        let state = shared.clone();
        let result = supervisor::serve_tcp(self.listener, context, move |mut stream, ctx| {
            serve_client(&mut stream, ctx, &state, &password, control, clipboard)
        });
        // Capture failures request cancellation; join both network and display ownership on stop.
        context.request_stop();
        let _ = capture.join();
        if let Some(worker) = browser_worker {
            let _ = worker.join();
        }
        if let Some(error) = shared.failure.lock().unwrap().take() {
            Err(io::Error::other(error))
        } else {
            result
        }
    }
}
fn exact(
    stream: &mut TcpStream,
    mut b: &mut [u8],
    ctx: &Context,
    deadline: Instant,
) -> io::Result<()> {
    while !b.is_empty() {
        let n = supervisor::read(stream, b, ctx, deadline)?;
        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        b = &mut b[n..];
    }
    Ok(())
}
fn send(stream: &mut TcpStream, b: &[u8], ctx: &Context) -> io::Result<()> {
    supervisor::write(stream, b, ctx, Instant::now() + IO_LIMIT)
}
pub(super) fn auth_response(password: &Secret, challenge: &[u8; 16]) -> Zeroizing<[u8; 16]> {
    use des::cipher::{BlockCipherEncrypt, KeyInit};
    let mut key = Zeroizing::new([0u8; 8]);
    for (d, b) in key.iter_mut().zip(password.expose().bytes()) {
        *d = b.reverse_bits()
    }
    let cipher = des::Des::new_from_slice(&*key).unwrap();
    let mut response = Zeroizing::new(*challenge);
    for block in response.chunks_exact_mut(8) {
        cipher.encrypt_block(block.try_into().unwrap())
    }
    response
}
#[derive(Clone, Copy)]
struct Pixel {
    bpp: u8,
    big: bool,
    max: [u16; 3],
    shift: [u8; 3],
}
impl Pixel {
    const DEFAULT: Self = Self {
        bpp: 32,
        big: false,
        max: [255; 3],
        shift: [16, 8, 0],
    };
    fn wire(self) -> [u8; 16] {
        let mut b = [0; 16];
        b[0] = self.bpp;
        b[1] = 24;
        b[2] = u8::from(self.big);
        b[3] = 1;
        for i in 0..3 {
            b[4 + i * 2..6 + i * 2].copy_from_slice(&self.max[i].to_be_bytes());
            b[10 + i] = self.shift[i]
        }
        b
    }
    fn parse(b: &[u8]) -> io::Result<Self> {
        let p = Self {
            bpp: b[0],
            big: b[2] != 0,
            max: [
                u16::from_be_bytes([b[4], b[5]]),
                u16::from_be_bytes([b[6], b[7]]),
                u16::from_be_bytes([b[8], b[9]]),
            ],
            shift: [b[10], b[11], b[12]],
        };
        let mut used = 0u32;
        if !matches!(p.bpp, 8 | 16 | 32) || b[1] > p.bpp || b[2] > 1 || b[3] != 1 {
            return Err(io::Error::other("Unsupported VNC pixel format"));
        }
        for i in 0..3 {
            let m = u32::from(p.max[i]);
            let bits = 32 - m.leading_zeros();
            if m == 0
                || !m.wrapping_add(1).is_power_of_two()
                || u32::from(p.shift[i]) + bits > u32::from(p.bpp)
            {
                return Err(io::Error::other("Invalid VNC pixel channels"));
            }
            let mask = m << p.shift[i];
            if mask & used != 0 {
                return Err(io::Error::other("Overlapping VNC channels"));
            }
            used |= mask;
        }
        Ok(p)
    }
    fn encode(self, rgba: &[u8], out: &mut Vec<u8>) {
        let mut v = 0u32;
        for i in 0..3 {
            v |= (u32::from(rgba[i]) * u32::from(self.max[i]) / 255) << self.shift[i]
        }
        let bytes = if self.big {
            v.to_be_bytes()
        } else {
            v.to_le_bytes()
        };
        let n = usize::from(self.bpp / 8);
        out.extend_from_slice(if self.big {
            &bytes[4 - n..]
        } else {
            &bytes[..n]
        });
    }
}
fn serve_client(
    s: &mut TcpStream,
    c: &Context,
    state: &Shared,
    password: &Secret,
    control: bool,
    share_clipboard: bool,
) -> io::Result<()> {
    let mut control_lease = false;
    let result = (|| {
        send(s, b"RFB 003.008\n", c)?;
        let mut banner = [0; 12];
        exact(s, &mut banner, c, Instant::now() + IO_LIMIT)?;
        let minor = match &banner {
            b"RFB 003.003\n" => 3,
            b"RFB 003.007\n" => 7,
            b"RFB 003.008\n" => 8,
            _ => return Err(io::Error::other("Unsupported RFB version")),
        };
        if minor == 3 {
            send(s, &2u32.to_be_bytes(), c)?
        } else {
            send(s, &[1, 2], c)?;
            let mut choice = [0];
            exact(s, &mut choice, c, Instant::now() + IO_LIMIT)?;
            if choice != [2] {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
        }
        let mut challenge = [0; 16];
        getrandom::fill(&mut challenge)
            .map_err(|_| io::Error::other("VNC challenge generation failed"))?;
        send(s, &challenge, c)?;
        let mut response = Zeroizing::new([0; 16]);
        exact(s, &mut *response, c, Instant::now() + IO_LIMIT)?;
        if !bool::from(response.ct_eq(&*auth_response(password, &challenge))) {
            send(s, &1u32.to_be_bytes(), c)?;
            if minor == 8 {
                let reason = b"Authentication failed";
                send(s, &(reason.len() as u32).to_be_bytes(), c)?;
                send(s, reason, c)?
            }
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        if control {
            if state.control_active.swap(true, Ordering::AcqRel) {
                return Err(io::Error::other("Another controlling VNC client is active"));
            }
            control_lease = true;
        }
        send(s, &0u32.to_be_bytes(), c)?;
        let mut init = [0];
        exact(s, &mut init, c, Instant::now() + IO_LIMIT)?;
        // Always shared: never disconnect another client on a remote exclusive request.
        let frame = state.frame.lock().unwrap().clone();
        let mut init = Vec::new();
        init.extend_from_slice(&frame.width.to_be_bytes());
        init.extend_from_slice(&frame.height.to_be_bytes());
        init.extend_from_slice(&Pixel::DEFAULT.wire());
        let name = b"TermCraft selected display (classic VNC)";
        init.extend_from_slice(&(name.len() as u32).to_be_bytes());
        init.extend_from_slice(name);
        send(s, &init, c)?;
        drop(frame);
        let mut clipboard = clipboard::Clipboard::default();
        let mut clipboard_revision = 0;
        let mut extended_clipboard = false;
        let mut idle = Instant::now() + IDLE_LIMIT;
        let mut pixel = Pixel::DEFAULT;
        let mut next = Instant::now();
        loop {
            if state.failure.lock().unwrap().is_some() {
                return Err(io::Error::other("Selected display unavailable"));
            }
            let mut ty = [0];
            if share_clipboard {
                let local = {
                    let v = state.clipboard.lock().unwrap();
                    if v.0 != clipboard_revision {
                        clipboard_revision = v.0;
                        v.1.clone()
                    } else {
                        None
                    }
                };
                if let Some(text) = local {
                    if let Ok(packet) = clipboard.upload(&text) {
                        send(s, &packet, c)?;
                    }
                }
            }
            use std::io::Read;
            c.check(idle)?;
            match s.read(&mut ty) {
                Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
                Ok(_) => {
                    idle = Instant::now() + IDLE_LIMIT;
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock
                            | io::ErrorKind::TimedOut
                            | io::ErrorKind::Interrupted
                    ) =>
                {
                    continue;
                }
                Err(e) => return Err(e),
            }
            match ty[0] {
                0 => {
                    let mut b = [0; 19];
                    exact(s, &mut b, c, Instant::now() + IO_LIMIT)?;
                    pixel = Pixel::parse(&b[3..])?;
                }
                2 => {
                    let mut h = [0; 3];
                    exact(s, &mut h, c, Instant::now() + IO_LIMIT)?;
                    let n = u16::from_be_bytes([h[1], h[2]]) as usize;
                    if n > 256 {
                        return Err(io::Error::other("Too many VNC encodings"));
                    }
                    let mut b = vec![0; n * 4];
                    exact(s, &mut b, c, Instant::now() + IO_LIMIT)?;
                    if share_clipboard
                        && !extended_clipboard
                        && b.chunks_exact(4).any(|v| {
                            i32::from_be_bytes(v.try_into().unwrap()) == clipboard::ENCODING
                        })
                    {
                        extended_clipboard = true;
                        send(s, &clipboard::Clipboard::capabilities(), c)?;
                    }
                    /* Raw is mandatory even when absent from list, RFC6143 7.5.2. */
                }
                3 => {
                    let mut b = [0; 9];
                    exact(s, &mut b, c, Instant::now() + IO_LIMIT)?;
                    while Instant::now() < next {
                        c.check(Instant::now() + IO_LIMIT)?;
                        thread::sleep(Duration::from_millis(10));
                    }
                    next = Instant::now() + Duration::from_millis(100);
                    let f = state.frame.lock().unwrap().clone();
                    let x = u16::from_be_bytes([b[1], b[2]]);
                    let y = u16::from_be_bytes([b[3], b[4]]);
                    let w = u16::from_be_bytes([b[5], b[6]]).min(f.width.saturating_sub(x));
                    let h = u16::from_be_bytes([b[7], b[8]]).min(f.height.saturating_sub(y));
                    if x >= f.width || y >= f.height || w == 0 || h == 0 {
                        send(s, &[0, 0, 0, 0], c)?;
                        continue;
                    }
                    let mut header = vec![0, 0, 0, 1];
                    for v in [x, y, w, h] {
                        header.extend_from_slice(&v.to_be_bytes())
                    }
                    header.extend_from_slice(&0i32.to_be_bytes());
                    send(s, &header, c)?;
                    let deadline = Instant::now() + IO_LIMIT;
                    let mut row = Vec::with_capacity(usize::from(w) * 4);
                    for yy in y..y + h {
                        row.clear();
                        for xx in x..x + w {
                            let offset =
                                (usize::from(yy) * usize::from(f.width) + usize::from(xx)) * 4;
                            pixel.encode(&f.rgba[offset..offset + 4], &mut row)
                        }
                        supervisor::write(s, &row, c, deadline)?;
                    }
                }
                4 => {
                    let mut b = [0; 7];
                    exact(s, &mut b, c, Instant::now() + IO_LIMIT)?;
                    if control {
                        state
                            .input
                            .try_send(Input::Key {
                                down: b[0] != 0,
                                keysym: u32::from_be_bytes(b[3..7].try_into().unwrap()),
                            })
                            .map_err(|_| io::Error::other("VNC input queue is full"))?;
                    }
                }
                5 => {
                    let mut b = [0; 5];
                    exact(s, &mut b, c, Instant::now() + IO_LIMIT)?;
                    if control {
                        let f = state.frame.lock().unwrap();
                        let x = u16::from_be_bytes([b[1], b[2]]).min(f.width - 1);
                        let y = u16::from_be_bytes([b[3], b[4]]).min(f.height - 1);
                        drop(f);
                        state
                            .input
                            .try_send(Input::Pointer {
                                buttons: b[0] & 0x1f,
                                x,
                                y,
                            })
                            .map_err(|_| io::Error::other("VNC input queue is full"))?;
                    }
                }
                6 => {
                    let mut h = [0; 7];
                    exact(s, &mut h, c, Instant::now() + IO_LIMIT)?;
                    let length = i32::from_be_bytes(h[3..7].try_into().unwrap());
                    let n = length.unsigned_abs() as usize;
                    if n > clipboard::WIRE_LIMIT {
                        return Err(io::Error::other("VNC clipboard message exceeds limit"));
                    }
                    let mut data = Zeroizing::new(vec![0; n]);
                    exact(s, &mut data, c, Instant::now() + IO_LIMIT)?;
                    if !share_clipboard {
                        continue;
                    }
                    let text = if length < 0 {
                        if !extended_clipboard {
                            return Err(io::Error::other("Clipboard extension not negotiated"));
                        }
                        let reply = clipboard
                            .incoming(
                                &data,
                                clipboard::ClipboardPolicy { receive: control },
                                false,
                            )
                            .map_err(io::Error::other)?;
                        if data.len() >= 4
                            && u32::from_be_bytes(data[..4].try_into().unwrap()) & (1 << 24) != 0
                        {
                            clipboard_revision = 0;
                        }
                        if data.len() >= 4
                            && u32::from_be_bytes(data[..4].try_into().unwrap()) & (1 << 24) == 0
                        {
                            if let Some(packet) = reply.packet {
                                send(s, &packet, c)?;
                            }
                        }
                        reply.text.map(Zeroizing::new)
                    } else {
                        if n > 8192 {
                            return Err(io::Error::other("Clipboard text exceeds limit"));
                        }
                        clipboard.classic_remote();
                        Some(Zeroizing::new(
                            data.iter().map(|b| char::from(*b)).collect::<String>(),
                        ))
                    };
                    if control {
                        if let Some(text) = text {
                            if text.len() > 8192 || text.contains('\0') {
                                return Err(io::Error::other("Invalid clipboard text"));
                            }
                            state
                                .input
                                .try_send(Input::Clipboard(text))
                                .map_err(|_| io::Error::other("VNC clipboard queue full"))?;
                        }
                    }
                }
                _ => return Err(io::Error::other("Unsupported VNC client message")),
            }
        }
    })();
    if control_lease {
        state.release.store(true, Ordering::Release);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_x11_display_validation_matches_u16_parser_and_tcp_boundaries() {
        for display in [":0", ":1.0", ":59535", ":59535.65535"] {
            let mut config = VncServiceConfig::local(
                VncSource::X11 {
                    display: display.into(),
                },
                Secret::new("fixture"),
            );
            config.allow_unencrypted = true;
            assert!(config.validate().is_ok(), "valid local display {display}");
        }
        for display in [
            ":59536", ":59536.0", ":65535", ":0.65536", ":0.", ":0.1.2", ":0:1", "remote:0",
        ] {
            let mut config = VncServiceConfig::local(
                VncSource::X11 {
                    display: display.into(),
                },
                Secret::new("fixture"),
            );
            config.allow_unencrypted = true;
            assert!(
                config.validate().is_err(),
                "invalid local display {display}"
            );
        }
    }
    #[test]
    fn overflowing_x11_display_start_records_failed_before_source_open() {
        use crate::services::{ServiceState, ServiceSupervisor};
        for display in [":59536", ":65535"] {
            let supervisor = ServiceSupervisor::default();
            let mut config = VncServiceConfig::local(
                VncSource::X11 {
                    display: display.into(),
                },
                Secret::new("fixture"),
            );
            config.allow_unencrypted = true;
            let error = supervisor.start_vnc(config).unwrap_err();
            assert!(error.contains("local X11 display"), "{error}");
            let statuses = supervisor.statuses();
            assert_eq!(statuses.len(), 1);
            assert_eq!(statuses[0].state, ServiceState::Failed);
            assert!(statuses[0].address.is_none());
            assert_eq!(statuses[0].error.as_deref(), Some(error.as_str()));
        }
    }
    #[test]
    fn overflowing_x11_display_async_start_reaches_failed() {
        use crate::services::{ServiceState, ServiceSupervisor};
        let supervisor = ServiceSupervisor::default();
        let mut config = VncServiceConfig::local(
            VncSource::X11 {
                display: ":65535".into(),
            },
            Secret::new("fixture"),
        );
        config.allow_unencrypted = true;
        let id = supervisor.begin_vnc(config).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let status = supervisor.status(id).unwrap();
            if status.state == ServiceState::Failed {
                assert!(status.error.unwrap().contains("local X11 display"));
                assert!(status.address.is_none());
                break;
            }
            assert_eq!(status.state, ServiceState::Starting);
            assert!(
                Instant::now() < deadline,
                "asynchronous startup must not remain Starting"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn vnc_clipboard_optin_validates_without_opening_display() {
        let mut c = VncServiceConfig::local(
            VncSource::X11 {
                display: ":991".into(),
            },
            Secret::new("fixture"),
        );
        c.allow_unencrypted = true;
        c.allow_clipboard = true;
        assert!(c.validate().is_ok());
    }
    #[test]
    fn vnc_server_truecolor_formats_encode_channels_and_reject_overlap() {
        let mut out = Vec::new();
        Pixel::DEFAULT.encode(&[255, 0, 0, 255], &mut out);
        assert_eq!(out, [0, 0, 255, 0]);
        let mut format = Pixel::DEFAULT.wire();
        format[0] = 16;
        format[1] = 16;
        format[2] = 1;
        format[4..10].copy_from_slice(&[0, 31, 0, 63, 0, 31]);
        format[10..13].copy_from_slice(&[11, 5, 0]);
        let pixel = Pixel::parse(&format).unwrap();
        out.clear();
        pixel.encode(&[255, 0, 0, 255], &mut out);
        assert_eq!(out, [0xf8, 0]);
        format[11] = 11;
        assert!(Pixel::parse(&format).is_err());
        format[10] = 255;
        assert!(Pixel::parse(&format).is_err());
    }
    #[test]
    fn vnc_server_framebuffer_bounds_are_enforced() {
        assert!(
            Frame {
                width: 0,
                height: 2,
                rgba: vec![]
            }
            .validate()
            .is_err()
        );
        assert!(
            Frame {
                width: 4,
                height: 4,
                rgba: vec![0; 63]
            }
            .validate()
            .is_err()
        );
        assert!(
            Frame {
                width: u16::MAX,
                height: u16::MAX,
                rgba: vec![]
            }
            .validate()
            .is_err()
        );
    }
}
