//! RFB 3.3/3.7/3.8 transport. Raw, CopyRect, Hextile, desktop resize, cursor and clipboard.
//! Classic security is an explicit opt-in, never selected as a TLS fallback.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Security {
    #[default]
    TlsPassword,
    TlsNone,
    Password,
    None,
}
impl Security {
    pub fn encrypted(self) -> bool {
        matches!(self, Self::TlsPassword | Self::TlsNone)
    }
    fn password(self) -> bool {
        matches!(self, Self::TlsPassword | Self::Password)
    }
}
trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}
type BoxStream = Box<dyn Stream>;
#[path = "vnc_clipboard.rs"]
mod clipboard;
#[path = "vnc_encoding.rs"]
mod encoding;
#[path = "vnc_tls.rs"]
mod tls;
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VncOptions {
    pub embedded: bool,
    pub allow_unencrypted: bool,
    pub certificate_authority: Option<PathBuf>,
    pub security: Security,
    pub clipboard: rdp_options::ClipboardPolicy,
    pub keyboard: rdp_options::KeyboardRouting,
}
impl VncOptions {
    pub fn validate(&self) -> Result<(), String> {
        if self.embedded && !self.security.encrypted() && !self.allow_unencrypted {
            return Err("Classic VNC requires explicit unencrypted transport consent; choose verified TLS for an encrypted connection.".into());
        }
        Ok(())
    }
}

pub(super) fn connect(config: DesktopConfig) -> Result<DesktopSession, String> {
    if !config.session.vnc_options.embedded {
        return Err("This VNC profile selects an installed client".into());
    }
    config.session.vnc_options.validate()?;
    if !config.session.jump.is_empty() && config.stream.is_none() {
        return Err("VNC requires an SSH gateway stream for this profile.".into());
    }
    let options = config.session.vnc_options.clone();
    let view_only = config.session.view_only;
    let (input, rx) = input_channel();
    let (tx, events) = mpsc::sync_channel(32);
    let state = Arc::new(Mutex::new(State {
        ready: true,
        cursor: None,
        frame_revision: 0,
        frame: None,
        rdp_frames: Default::default(),
        challenge: None,
        connected: false,
        dimensions: (1, 1),
        final_message: None,
    }));
    let cancel = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));
    let shared = state.clone();
    let cancelled = cancel.clone();
    let done = finished.clone();
    std::thread::Builder::new()
        .name("vnc-transport".into())
        .spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|_| "Could not initialize VNC transport".to_string())
                .and_then(|runtime| {
                    runtime.block_on(async {
                        let operation = async {
                            let stream = if let Some(stream) = config.stream {
                                stream
                                    .set_nonblocking(true)
                                    .map_err(|_| "Could not configure the VNC gateway stream")?;
                                tokio::net::TcpStream::from_std(stream)
                                    .map_err(|_| "Could not open the VNC gateway stream")?
                            } else {
                                let host = config.session.host.trim_matches(['[', ']']);
                                tokio::time::timeout(
                                    Duration::from_secs(config.timeout_secs as u64),
                                    tokio::net::TcpStream::connect((host, config.session.port)),
                                )
                                .await
                                .map_err(|_| "VNC connection timed out")?
                                .map_err(|_| "Could not connect to the VNC endpoint")?
                            };
                            stream
                                .set_nodelay(true)
                                .map_err(|_| "Could not configure VNC TCP latency")?;
                            run(
                                stream,
                                &config.session,
                                rx,
                                tx,
                                shared.clone(),
                                config.timeout_secs,
                            )
                            .await
                        };
                        tokio::select! {
                            result = operation => result,
                            _ = async { while !cancelled.load(Ordering::Acquire) {
                                tokio::time::sleep(Duration::from_millis(10)).await;
                            }} => Ok(()),
                        }
                    })
                });
            let mut state = shared.lock().unwrap_or_else(|p| p.into_inner());
            state.connected = false;
            state.challenge = None;
            state.final_message = Some(result.err().unwrap_or_else(|| "VNC disconnected".into()));
            done.store(true, Ordering::Release);
        })
        .map_err(|_| "Could not start VNC transport")?;
    Ok(DesktopSession {
        input,
        events,
        state,
        cancel,
        finished,
        closed: false,
        notified: false,
        partial_frames: false,
        vnc: true,
        clipboard: options.clipboard,
        keyboard: options.keyboard,
        view_only,
    })
}

async fn bytes<R: AsyncRead + Unpin>(reader: &mut R, n: usize) -> Result<Vec<u8>, String> {
    if n > wire::MAX_FRAME {
        return Err("VNC message exceeds its memory limit".into());
    }
    let mut data = vec![0; n];
    reader
        .read_exact(&mut data)
        .await
        .map_err(|_| "VNC stream ended during a message")?;
    Ok(data)
}
async fn send<W: AsyncWrite + Unpin>(writer: &mut W, data: &[u8]) -> Result<(), String> {
    writer
        .write_all(data)
        .await
        .map_err(|_| "VNC write failed".into())
}
async fn number<R: AsyncRead + Unpin>(reader: &mut R) -> Result<u32, String> {
    Ok(u32::from_be_bytes(
        bytes(reader, 4).await?.try_into().unwrap(),
    ))
}
async fn event(tx: &mpsc::SyncSender<DesktopEvent>, value: DesktopEvent) -> Result<(), String> {
    let mut value = value;
    loop {
        match tx.try_send(value) {
            Ok(()) => return Ok(()),
            Err(mpsc::TrySendError::Full(v)) => {
                value = v;
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Err(_) => return Err("VNC view closed".into()),
        }
    }
}
fn response(password: &str, challenge: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
    use des::cipher::{BlockCipherEncrypt, KeyInit};
    if password.is_empty() || password.len() > 8 || !password.is_ascii() {
        return Err("Classic VNC passwords must contain 1–8 ASCII bytes; longer passwords are not silently truncated".into());
    }
    let mut key = Zeroizing::new([0u8; 8]);
    for (dst, src) in key.iter_mut().zip(password.bytes()) {
        *dst = src.reverse_bits();
    }
    let cipher = des::Des::new_from_slice(key.as_ref())
        .map_err(|_| "VNC authentication initialization failed")?;
    let mut result = Zeroizing::new(challenge.to_vec());
    for block in result.chunks_exact_mut(8) {
        cipher.encrypt_block(block.try_into().unwrap());
    }
    Ok(result)
}
async fn handshake(
    mut stream: BoxStream,
    session: &Session,
    rx: &InputReceiver,
    tx: &mpsc::SyncSender<DesktopEvent>,
    state: &Arc<Mutex<State>>,
) -> Result<(BoxStream, u16, u16), String> {
    let banner = bytes(&mut stream, 12).await?;
    let version = match banner.as_slice() {
        b"RFB 003.003\n" => 3,
        b"RFB 003.007\n" => 7,
        b"RFB 003.008\n" => 8,
        _ => return Err("Unsupported RFB version; expected 3.3, 3.7 or 3.8".into()),
    };
    send(&mut stream, &banner).await?;
    let wanted = if session.vnc_options.security.encrypted() {
        19
    } else if session.vnc_options.security.password() {
        2
    } else {
        1
    };
    if version == 3 {
        if number(&mut stream).await? != wanted {
            return Err("VNC server does not offer the selected security type".into());
        }
    } else {
        let count = bytes(&mut stream, 1).await?[0] as usize;
        if count == 0 {
            return Err("VNC server refused the connection".into());
        }
        let choices = bytes(&mut stream, count).await?;
        if !choices.contains(&(wanted as u8)) {
            return Err(
                "VNC server does not offer the selected security type; no fallback was attempted"
                    .into(),
            );
        }
        send(&mut stream, &[wanted as u8]).await?;
    }
    let mut fingerprint = String::new();
    if wanted == 19 {
        (stream, fingerprint) = tls::negotiate(stream, session).await?;
    }
    if session.vnc_options.security.password() {
        let challenge = bytes(&mut stream, 16).await?;
        state.lock().unwrap_or_else(|p| p.into_inner()).challenge = Some((1, false));
        event(
            tx,
            DesktopEvent::Authentication {
                id: 1,
                host: session.host.clone(),
                port: session.port,
                fingerprint,
                user: "VNC".into(),
            },
        )
        .await?;
        loop {
            match rx.rx.try_recv() {
                Ok(QueuedInput {
                    input: DesktopInput::Authenticate { ref password, .. },
                    ..
                }) => {
                    let result = response(password.expose(), &challenge)?;
                    send(&mut stream, &result).await?;
                    break;
                }
                Ok(_) => {}
                Err(mpsc::TryRecvError::Disconnected) => return Err("VNC sign-in cancelled".into()),
                Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
            }
        }
    }
    if (session.vnc_options.security.password() || wanted == 19 || version == 8)
        && number(&mut stream).await? != 0
    {
        return Err("VNC authentication was rejected".into());
    }
    send(&mut stream, &[1]).await?; // Shared session; do not evict other clients.
    let init = bytes(&mut stream, 24).await?;
    let w = u16::from_be_bytes(init[0..2].try_into().unwrap());
    let h = u16::from_be_bytes(init[2..4].try_into().unwrap());
    wire::dimensions(w, h)?;
    let name = u32::from_be_bytes(init[20..24].try_into().unwrap()) as usize;
    if name > 8192 {
        return Err("VNC desktop name exceeds 8 KiB".into());
    }
    bytes(&mut stream, name).await?;
    // 32-bit little-endian true-colour RGB, 24 significant bits.
    send(
        &mut stream,
        &[
            0, 0, 0, 0, 32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0,
        ],
    )
    .await?;
    let mut encodings = vec![1, encoding::HEXTILE, 0, -223, EXTENDED_DESKTOP_SIZE];
    if !session.view_only {
        encodings.push(encoding::CURSOR);
    }
    if session.vnc_options.clipboard != rdp_options::ClipboardPolicy::Disabled {
        encodings.push(clipboard::ENCODING);
    }
    let mut enc = vec![2, 0, 0, encodings.len() as u8];
    for encoding in encodings {
        enc.extend(encoding.to_be_bytes());
    }
    send(&mut stream, &enc).await?;
    Ok((stream, w, h))
}
fn update_request(w: u16, h: u16, incremental: bool) -> Vec<u8> {
    let mut p = vec![3, u8::from(incremental), 0, 0, 0, 0];
    p.extend(w.to_be_bytes());
    p.extend(h.to_be_bytes());
    p
}
const EXTENDED_DESKTOP_SIZE: i32 = -308;
const SCREEN_BYTES: usize = 16;

#[derive(Default)]
struct ResizeState {
    screen: Option<[u8; SCREEN_BYTES]>,
    desired: Option<(u16, u16)>,
    pending: bool,
    prohibited: bool,
}

impl ResizeState {
    fn update(&mut self, reason: u16, status: u16, size: (u16, u16), screens: &[u8]) {
        if reason == 1 {
            self.pending = false;
            if status != 0 {
                // The remaining fields are undefined on failure. Preserve the current layout.
                self.prohibited |= status == 1;
                return;
            }
        }
        // Resizing a single full-frame screen preserves its identity and flags.
        // Leave multihead and partial-screen layouts alone.
        self.screen = <[u8; SCREEN_BYTES]>::try_from(screens)
            .ok()
            .filter(|screen| {
                screen[4..8] == [0; 4]
                    && screen[8..10] == size.0.to_be_bytes()
                    && screen[10..12] == size.1.to_be_bytes()
            });
    }

    fn take_packet(&mut self, current: (u16, u16)) -> Option<[u8; 8 + SCREEN_BYTES]> {
        if self.pending || self.prohibited {
            return None;
        }
        let mut screen = self.screen?;
        let (width, height) = self.desired.take()?;
        if (width, height) == current {
            return None;
        }
        let mut packet = [0; 8 + SCREEN_BYTES];
        packet[0] = 251;
        packet[2..4].copy_from_slice(&width.to_be_bytes());
        packet[4..6].copy_from_slice(&height.to_be_bytes());
        packet[6] = 1;
        screen[8..10].copy_from_slice(&width.to_be_bytes());
        screen[10..12].copy_from_slice(&height.to_be_bytes());
        packet[8..].copy_from_slice(&screen);
        self.pending = true;
        Some(packet)
    }
}

async fn run<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
    stream: S,
    session: &Session,
    rx: InputReceiver,
    tx: mpsc::SyncSender<DesktopEvent>,
    state: Arc<Mutex<State>>,
    timeout: u32,
) -> Result<(), String> {
    let (mut stream, width, height) = {
        let operation = handshake(Box::new(stream), session, &rx, &tx, &state);
        tokio::pin!(operation);
        let mut remaining = Duration::from_secs(timeout as u64);
        let mut last = std::time::Instant::now();
        loop {
            tokio::select! {
                result = &mut operation => break result?,
                _ = tokio::time::sleep(Duration::from_millis(10)) => {
                    let now = std::time::Instant::now();
                    if state.lock().unwrap_or_else(|p|p.into_inner()).challenge.is_none() {
                        remaining = remaining.saturating_sub(now.duration_since(last));
                        if remaining.is_zero() { return Err("VNC handshake timed out".into()); }
                    }
                    last = now;
                }
            }
        }
    };
    {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.connected = true;
        s.challenge = None;
        s.dimensions = (width, height);
    }
    event(&tx, DesktopEvent::Connected { width, height }).await?;
    if session.vnc_options.clipboard != rdp_options::ClipboardPolicy::Disabled {
        event(&tx, DesktopEvent::ClipboardReady).await?;
    }
    send(&mut stream, &update_request(width, height, false)).await?;
    let (mut reader, writer) = tokio::io::split(stream);
    let writer = tokio::sync::Mutex::new(writer);
    // Serialize peer clipboard actions and local clipboard uploads with the writer.
    let clipboard = Mutex::new(clipboard::Clipboard::default());
    let resize = Mutex::new(ResizeState::default());
    let read = async {
        let mut frame = Frame {
            width,
            height,
            revision: 0,
            pixels: vec![0; wire::dimensions(width, height)?],
        };
        loop {
            match bytes(&mut reader, 1).await?[0] {
                0 => {
                    let head = bytes(&mut reader, 3).await?;
                    let count = u16::from_be_bytes(head[1..3].try_into().unwrap());
                    if count > 4096 {
                        return Err("VNC update has too many rectangles".into());
                    }
                    let mut resized = false;
                    let mut resize_advertised = false;
                    let mut changed = false;
                    let mut cursor = None;
                    for _ in 0..count {
                        let r = bytes(&mut reader, 12).await?;
                        let val = |i| u16::from_be_bytes([r[i], r[i + 1]]);
                        let (x, y, w, h) = (val(0), val(2), val(4), val(6));
                        let enc = i32::from_be_bytes(r[8..12].try_into().unwrap());
                        if enc == EXTENDED_DESKTOP_SIZE {
                            // RFB invalidates the old pixels on a size change. The server
                            // repaints on the next incremental request; a full request here
                            // would solicit another ExtendedDesktopSize rectangle.
                            let header = bytes(&mut reader, 4).await?;
                            let screens =
                                bytes(&mut reader, usize::from(header[0]) * SCREEN_BYTES).await?;
                            if x != 1 || y == 0 {
                                let length = wire::dimensions(w, h)?;
                                if (w, h) != (frame.width, frame.height) {
                                    frame.pixels = vec![0; length];
                                    frame.width = w;
                                    frame.height = h;
                                    changed = true;
                                }
                            }
                            resize.lock().unwrap_or_else(|p| p.into_inner()).update(
                                x,
                                y,
                                (w, h),
                                &screens,
                            );
                            resize_advertised = true;
                            continue;
                        }
                        if enc == -223 {
                            if x != 0 || y != 0 {
                                return Err("Invalid VNC DesktopSize origin".into());
                            }
                            frame.pixels = vec![0; wire::dimensions(w, h)?];
                            frame.width = w;
                            frame.height = h;
                            resized = true;
                            changed = true;
                            continue;
                        }
                        if enc == encoding::CURSOR && !session.view_only {
                            cursor = Some(encoding::cursor(&mut reader, (x, y), (w, h)).await?);
                            continue;
                        }
                        if w == 0
                            || h == 0
                            || x as u32 + w as u32 > frame.width as u32
                            || y as u32 + h as u32 > frame.height as u32
                        {
                            return Err("VNC rectangle is outside framebuffer".into());
                        }
                        let rect = encoding::Rect::new(x, y, w, h);
                        match enc {
                            0 => encoding::raw(&mut reader, &mut frame, rect).await?,
                            1 => encoding::copyrect(&mut reader, &mut frame, rect).await?,
                            encoding::HEXTILE => {
                                encoding::hextile(&mut reader, &mut frame, rect).await?
                            }
                            _ => return Err(format!("VNC server sent unrequested encoding {enc}")),
                        }
                        changed = true;
                    }
                    if changed {
                        frame.revision = frame
                            .revision
                            .checked_add(1)
                            .ok_or("VNC frame revision exhausted")?;
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.dimensions = (frame.width, frame.height);
                        s.frame = Some(Arc::new(frame.clone()));
                    }
                    if let Some(shape) = cursor {
                        event(&tx, shape).await?;
                    }
                    send(
                        &mut *writer.lock().await,
                        &update_request(frame.width, frame.height, !resized),
                    )
                    .await?;
                    if resize_advertised && !session.view_only {
                        rx.control.ready.notify_one();
                    }
                }
                2 => {} // Bell carries no payload.
                3 => {
                    let head = bytes(&mut reader, 7).await?;
                    let length = i32::from_be_bytes(head[3..].try_into().unwrap());
                    let size = length.unsigned_abs() as usize;
                    let policy = session.vnc_options.clipboard;
                    if length >= 0 {
                        if size > clipboard::TEXT_LIMIT {
                            return Err("VNC classic clipboard exceeds 8 KiB".into());
                        }
                        let data = Zeroizing::new(bytes(&mut reader, size).await?);
                        if policy.from_remote() {
                            clipboard
                                .lock()
                                .unwrap_or_else(|p| p.into_inner())
                                .classic_remote();
                            let text = data.iter().map(|b| char::from(*b)).collect();
                            event(&tx, DesktopEvent::Clipboard { text }).await?;
                        }
                    } else {
                        if size > clipboard::WIRE_LIMIT {
                            return Err("VNC extended clipboard exceeds its message limit".into());
                        }
                        let data = Zeroizing::new(bytes(&mut reader, size).await?);
                        if policy == rdp_options::ClipboardPolicy::Disabled {
                            continue;
                        }
                        let reply = {
                            let mut writer = writer.lock().await;
                            let reply = clipboard
                                .lock()
                                .unwrap_or_else(|p| p.into_inner())
                                .incoming(&data, policy, session.view_only)?;
                            if let Some(packet) = &reply.packet {
                                send(&mut *writer, packet).await?;
                            }
                            reply
                        };
                        if let Some(text) = reply.text {
                            event(&tx, DesktopEvent::Clipboard { text }).await?;
                        }
                    }
                }
                _ => return Err("Unsupported VNC server message".into()),
            }
        }
    };
    let write = async {
        let mut held = BTreeSet::new();
        let mut buttons = 0u8;
        let mut point = (0u16, 0u16);
        loop {
            let input = rx.rx.try_recv();
            if rx.control.release.swap(false, Ordering::AcqRel) {
                let mut writer = writer.lock().await;
                for key in std::mem::take(&mut held) {
                    send(&mut *writer, &key_packet(key, false)).await?;
                }
                if buttons != 0 {
                    buttons = 0;
                    send(&mut *writer, &pointer_packet(point, 0)).await?;
                }
            }
            let current = state.lock().unwrap_or_else(|p| p.into_inner()).dimensions;
            let packet = resize
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .take_packet(current);
            if let Some(packet) = packet {
                send(&mut *writer.lock().await, &packet).await?;
            }
            match input {
                Ok(value) => {
                    if value.epoch != rx.control.epoch.load(Ordering::Acquire) {
                        continue;
                    }
                    if check_input_policy(
                        session.vnc_options.clipboard,
                        session.vnc_options.keyboard,
                        session.view_only,
                        &value.input,
                    )
                    .is_err()
                    {
                        continue;
                    }
                    let mut out = Zeroizing::new(Vec::new());
                    match &value.input {
                        DesktopInput::Close => return Ok(()),
                        DesktopInput::Pointer {
                            x,
                            y,
                            button,
                            pressed,
                        } => {
                            let dims = state.lock().unwrap_or_else(|p| p.into_inner()).dimensions;
                            if (*x >= dims.0 || *y >= dims.1) && button.is_some() && *pressed {
                                continue;
                            }
                            point = ((*x).min(dims.0 - 1), (*y).min(dims.1 - 1));
                            if let Some(button) = button {
                                let mask = match button {
                                    1 => 1,
                                    2 => 4,
                                    3 => 2,
                                    _ => return Err("Unsupported VNC pointer button".into()),
                                };
                                if *pressed {
                                    buttons |= mask
                                } else {
                                    buttons &= !mask
                                }
                            }
                            out.extend(pointer_packet(point, buttons));
                        }
                        DesktopInput::Key {
                            code,
                            extended,
                            pressed,
                        } => {
                            let key = keysym(*code, *extended).ok_or("Unsupported VNC key")?;
                            if *pressed {
                                held.insert(key);
                            } else {
                                held.remove(&key);
                            }
                            out.extend(key_packet(key, *pressed));
                        }
                        DesktopInput::Text { value } => {
                            for c in value.chars() {
                                let key = if c as u32 <= 255 {
                                    c as u32
                                } else {
                                    0x01000000 | c as u32
                                };
                                out.extend(key_packet(key, true));
                                out.extend(key_packet(key, false));
                            }
                        }
                        DesktopInput::Clipboard { text } => {
                            let upload = {
                                let mut writer = writer.lock().await;
                                let result = clipboard
                                    .lock()
                                    .unwrap_or_else(|p| p.into_inner())
                                    .upload(text);
                                match result {
                                    Ok(packet) => {
                                        send(&mut *writer, &packet).await?;
                                        Ok(())
                                    }
                                    Err(message) => Err(message),
                                }
                            };
                            if let Err(message) = upload {
                                event(&tx, DesktopEvent::Status { message }).await?;
                            }
                        }
                        DesktopInput::Scroll {
                            x,
                            y,
                            delta,
                            horizontal,
                        } => {
                            let dims = state.lock().unwrap_or_else(|p| p.into_inner()).dimensions;
                            point = ((*x).min(dims.0 - 1), (*y).min(dims.1 - 1));
                            let mask = if *horizontal {
                                if *delta > 0 { 1 << 5 } else { 1 << 6 }
                            } else if *delta > 0 {
                                1 << 3
                            } else {
                                1 << 4
                            };
                            for _ in 0..((delta.unsigned_abs() as usize + 119) / 120).min(8) {
                                out.extend(pointer_packet(point, buttons | mask));
                                out.extend(pointer_packet(point, buttons));
                            }
                        }
                        DesktopInput::Resize { width, height } => {
                            if !session.view_only && wire::dimensions(*width, *height).is_ok() {
                                resize.lock().unwrap_or_else(|p| p.into_inner()).desired =
                                    Some((*width, *height));
                            }
                        }
                        DesktopInput::ReleaseAll => {
                            for key in std::mem::take(&mut held) {
                                out.extend(key_packet(key, false));
                            }
                            if buttons != 0 {
                                buttons = 0;
                                out.extend(pointer_packet(point, 0));
                            }
                        }
                        _ => return Err("Unexpected VNC authentication/control answer".into()),
                    }
                    if !out.is_empty() {
                        send(&mut *writer.lock().await, &out).await?;
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => return Ok(()),
                // A notification sent before this await is retained as a permit.
                Err(mpsc::TryRecvError::Empty) => rx.control.ready.notified().await,
            }
        }
    };
    tokio::select! {result=read=>result,result=write=>result}
}
fn key_packet(key: u32, pressed: bool) -> Vec<u8> {
    let mut p = vec![4, u8::from(pressed), 0, 0];
    p.extend(key.to_be_bytes());
    p
}
fn pointer_packet((x, y): (u16, u16), buttons: u8) -> Vec<u8> {
    let mut p = vec![5, buttons];
    p.extend(x.to_be_bytes());
    p.extend(y.to_be_bytes());
    p
}
pub(super) fn keysym(code: u8, ext: bool) -> Option<u32> {
    Some(match (code, ext) {
        (29, false) => 0xffe3,
        (29, true) => 0xffe4,
        (42, false) => 0xffe1,
        (54, false) => 0xffe2,
        (56, false) => 0xffe9,
        (56, true) => 0xffea,
        (91, true) => 0xffeb,
        (92, true) => 0xffec,
        (75, true) => 0xff51,
        (72, true) => 0xff52,
        (77, true) => 0xff53,
        (80, true) => 0xff54,
        (71, true) => 0xff50,
        (79, true) => 0xff57,
        (82, true) => 0xff63,
        (83, true) => 0xffff,
        (73, true) => 0xff55,
        (81, true) => 0xff56,
        (28, _) => 0xff0d,
        (15, _) => 0xff09,
        (1, _) => 0xff1b,
        (14, _) => 0xff08,
        (59..=68, false) => 0xffbe + (code - 59) as u32,
        (87, false) => 0xffc8,
        (88, false) => 0xffc9,
        (_, false) => {
            let pairs = [
                (30, b'a'),
                (48, b'b'),
                (46, b'c'),
                (32, b'd'),
                (18, b'e'),
                (33, b'f'),
                (34, b'g'),
                (35, b'h'),
                (23, b'i'),
                (36, b'j'),
                (37, b'k'),
                (38, b'l'),
                (50, b'm'),
                (49, b'n'),
                (24, b'o'),
                (25, b'p'),
                (16, b'q'),
                (19, b'r'),
                (31, b's'),
                (20, b't'),
                (22, b'u'),
                (47, b'v'),
                (17, b'w'),
                (45, b'x'),
                (21, b'y'),
                (44, b'z'),
                (57, b' '),
                (12, b'-'),
                (13, b'='),
                (26, b'['),
                (27, b']'),
                (39, b';'),
                (40, b'\''),
                (41, b'`'),
                (43, b'\\'),
                (51, b','),
                (52, b'.'),
                (53, b'/'),
            ];
            if (2..=11).contains(&code) {
                b"1234567890"[(code - 2) as usize] as u32
            } else {
                pairs.iter().find(|(c, _)| *c == code)?.1 as u32
            }
        }
        _ => return None,
    })
}

#[cfg(test)]
#[path = "vnc_tests.rs"]
mod tests;
