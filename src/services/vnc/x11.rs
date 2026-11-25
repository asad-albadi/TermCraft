//! Explicit local X11 source. No implicit DISPLAY, Xauthority mutation, shell or host daemon.
use super::{Frame, Input, MAX_PIXELS, Source};
use std::{
    io,
    io::IoSlice,
    os::fd::AsRawFd,
    sync::Mutex,
    time::{Duration, Instant},
};
use x11rb::{
    connection::Connection,
    protocol::{xproto::*, xtest::ConnectionExt as _},
    rust_connection::{DefaultStream, PollMode, RustConnection, Stream},
    utils::RawFdContainer,
};
use zeroize::Zeroizing;
mod clipboard;
#[path = "x11/keyboard.rs"]
mod keyboard;
struct Timed {
    stream: DefaultStream,
    deadline: Mutex<Instant>,
}
impl Stream for Timed {
    fn poll(&self, mode: PollMode) -> io::Result<()> {
        loop {
            let remain = self
                .deadline
                .lock()
                .unwrap()
                .saturating_duration_since(Instant::now());
            if remain.is_zero() {
                return Err(io::ErrorKind::TimedOut.into());
            }
            let mut fd = libc::pollfd {
                fd: self.stream.as_raw_fd(),
                events: (if mode.readable() { libc::POLLIN } else { 0 })
                    | (if mode.writable() { libc::POLLOUT } else { 0 }),
                revents: 0,
            };
            let result = unsafe { libc::poll(&mut fd, 1, remain.as_millis().min(50) as i32) };
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
        self.stream.read(b, fds)
    }
    fn write(&self, b: &[u8], fds: &mut Vec<RawFdContainer>) -> io::Result<usize> {
        self.stream.write(b, fds)
    }
    fn write_vectored(
        &self,
        b: &[IoSlice<'_>],
        fds: &mut Vec<RawFdContainer>,
    ) -> io::Result<usize> {
        self.stream.write_vectored(b, fds)
    }
}
pub(super) struct X11 {
    conn: RustConnection<Timed>,
    root: u32,
    width: u16,
    height: u16,
    bpp: u8,
    pad: u8,
    big: bool,
    masks: [u32; 3],
    keyboard: Option<keyboard::Keyboard>,
    held: Vec<keyboard::Held>,
    temporary: Vec<u8>,
    buttons: u8,
    control: bool,
    clipboard: Option<clipboard::Clipboard>,
}
fn error(e: impl std::fmt::Display) -> io::Error {
    io::Error::other(e.to_string())
}
impl X11 {
    pub(super) fn open(
        display: &str,
        control: bool,
        authority: Option<&std::path::Path>,
    ) -> Result<Self, String> {
        Self::connect(display,control,authority).map_err(|_|"Could not open selected local X11 display; check display name and Xauthority permission.".into())
    }
    fn connect(
        display: &str,
        control: bool,
        authority: Option<&std::path::Path>,
    ) -> io::Result<Self> {
        let parsed = x11rb::reexports::x11rb_protocol::parse_display::parse_display(Some(display))
            .map_err(error)?;
        let addr = parsed
            .connect_instruction()
            .find(|a| {
                matches!(
                    a,
                    x11rb::reexports::x11rb_protocol::parse_display::ConnectAddress::Socket(_)
                )
            })
            .ok_or_else(|| error("Local X11 display required"))?;
        let path = match &addr {
            x11rb::reexports::x11rb_protocol::parse_display::ConnectAddress::Socket(path) => path,
            _ => return Err(error("Local X11 display required")),
        };
        let (stream, (_family, address)) =
            DefaultStream::from_unix_stream(local_socket(std::path::Path::new(path))?)?;
        let selected_authority = authority
            .map(std::path::Path::to_path_buf)
            .or_else(|| std::env::var_os("XAUTHORITY").map(std::path::PathBuf::from))
            .or_else(|| {
                std::env::var_os("HOME").map(|p| std::path::PathBuf::from(p).join(".Xauthority"))
            });
        let (auth_name, auth_data) = match selected_authority {
            Some(path) if authority.is_some() || path.exists() => {
                authority_cookie(&path, parsed.display, &address)?
            }
            _ => Default::default(),
        };
        let conn = RustConnection::connect_to_stream_with_auth_info(
            Timed {
                stream,
                deadline: Mutex::new(Instant::now() + Duration::from_secs(2)),
            },
            usize::from(parsed.screen),
            auth_name,
            auth_data,
        )
        .map_err(error)?;
        let setup = conn.setup();
        let screen = setup
            .roots
            .get(usize::from(parsed.screen))
            .ok_or_else(|| error("Invalid X11 screen"))?;
        let root = screen.root;
        let width = screen.width_in_pixels;
        let height = screen.height_in_pixels;
        if width > i16::MAX as u16
            || height > i16::MAX as u16
            || usize::from(width) * usize::from(height) > MAX_PIXELS
        {
            return Err(error("Display exceeds VNC framebuffer limit"));
        }
        let format = setup
            .pixmap_formats
            .iter()
            .find(|f| f.depth == screen.root_depth)
            .ok_or_else(|| error("Unsupported X11 pixel format"))?;
        if !matches!(format.bits_per_pixel, 16 | 24 | 32)
            || !matches!(format.scanline_pad, 8 | 16 | 32)
        {
            return Err(error("Unsupported X11 pixel depth"));
        }
        let visual = screen
            .allowed_depths
            .iter()
            .flat_map(|d| &d.visuals)
            .find(|v| v.visual_id == screen.root_visual)
            .ok_or_else(|| error("Missing X11 visual"))?;
        if visual.class != VisualClass::TRUE_COLOR {
            return Err(error("VNC source needs a TrueColor X11 display"));
        }
        let (bpp, pad, big, masks) = (
            format.bits_per_pixel,
            format.scanline_pad,
            setup.image_byte_order == ImageOrder::MSB_FIRST,
            [visual.red_mask, visual.green_mask, visual.blue_mask],
        );
        let mut used = 0u32;
        for mask in masks {
            if mask == 0 || used & mask != 0 || (bpp < 32 && mask >> bpp != 0) {
                return Err(error("Invalid X11 visual masks"));
            }
            let maximum = mask >> mask.trailing_zeros();
            if !maximum.wrapping_add(1).is_power_of_two() {
                return Err(error("Non-contiguous X11 visual mask"));
            }
            used |= mask;
        }
        let keyboard = if control {
            Some(keyboard::Keyboard::load(&conn)?)
        } else {
            None
        };
        Ok(Self {
            conn,
            root,
            width,
            height,
            bpp,
            pad,
            big,
            masks,
            keyboard,
            held: Vec::new(),
            temporary: Vec::new(),
            buttons: 0,
            control,
            clipboard: None,
        })
    }
    fn deadline(&self) {
        *self.conn.stream().deadline.lock().unwrap() = Instant::now() + Duration::from_secs(2)
    }
    fn fake(&self, ty: u8, detail: u8, x: i16, y: i16) -> io::Result<()> {
        self.conn
            .xtest_fake_input(ty, detail, 0, self.root, x, y, 0)
            .map_err(error)?
            .check()
            .map_err(error)
    }
    fn release(&mut self) -> io::Result<()> {
        for key in std::mem::take(&mut self.temporary) {
            self.fake(KEY_RELEASE_EVENT, key, 0, 0)?;
        }
        let mut released = Vec::new();
        for key in std::mem::take(&mut self.held) {
            if key.owned && !released.contains(&key.code) {
                self.fake(KEY_RELEASE_EVENT, key.code, 0, 0)?;
                released.push(key.code);
            }
        }
        for b in 0..5 {
            if self.buttons & (1 << b) != 0 {
                self.fake(BUTTON_RELEASE_EVENT, b + 1, 0, 0)?;
            }
        }
        self.buttons = 0;
        self.conn.flush().map_err(error)
    }
}
impl Source for X11 {
    fn enable_clipboard(&mut self) -> io::Result<()> {
        self.deadline();
        self.clipboard = Some(clipboard::Clipboard::open(&self.conn, self.root)?);
        Ok(())
    }
    fn clipboard(&mut self) -> io::Result<Option<Zeroizing<String>>> {
        self.deadline();
        match self.clipboard.as_mut() {
            Some(c) => c.poll(&self.conn),
            None => Ok(None),
        }
    }
    fn set_clipboard(&mut self, text: Zeroizing<String>) -> io::Result<()> {
        self.deadline();
        self.clipboard
            .as_mut()
            .ok_or_else(|| error("Clipboard disabled"))?
            .set(&self.conn, text)
    }

    fn capture(&mut self) -> io::Result<Frame> {
        self.deadline();
        let geometry = self
            .conn
            .get_geometry(self.root)
            .map_err(error)?
            .reply()
            .map_err(error)?;
        if geometry.width != self.width || geometry.height != self.height {
            return Err(error("Selected display resized"));
        }
        let image = self
            .conn
            .get_image(
                ImageFormat::Z_PIXMAP,
                self.root,
                0,
                0,
                self.width,
                self.height,
                u32::MAX,
            )
            .map_err(error)?
            .reply()
            .map_err(error)?;
        let bytes = usize::from(self.bpp / 8);
        let bits = usize::from(self.width) * usize::from(self.bpp);
        let pad = usize::from(self.pad);
        let stride = bits.div_ceil(pad) * pad / 8;
        if image.data.len() != stride * usize::from(self.height) {
            return Err(error("Malformed X11 framebuffer"));
        }
        let mut rgba = vec![255; usize::from(self.width) * usize::from(self.height) * 4];
        for y in 0..usize::from(self.height) {
            for x in 0..usize::from(self.width) {
                let src = &image.data[y * stride + x * bytes..][..bytes];
                let mut value = 0u32;
                if self.big {
                    for b in src {
                        value = (value << 8) | u32::from(*b)
                    }
                } else {
                    for (i, b) in src.iter().enumerate() {
                        value |= u32::from(*b) << (i * 8)
                    }
                }
                let dest = (y * usize::from(self.width) + x) * 4;
                for (i, mask) in self.masks.iter().enumerate() {
                    let shift = mask.trailing_zeros();
                    let max = mask >> shift;
                    rgba[dest + i] =
                        (u64::from((value & mask) >> shift) * 255 / u64::from(max)) as u8;
                }
            }
        }
        Ok(Frame {
            width: self.width,
            height: self.height,
            rgba,
        })
    }
    fn input(&mut self, event: Input) -> io::Result<()> {
        if !self.control {
            return Ok(());
        }
        self.deadline();
        match event {
            Input::Clipboard(_) => {}
            Input::Key { down, keysym } => self.keyboard_event(keysym, down)?,
            Input::Pointer { buttons, x, y } => {
                self.fake(
                    MOTION_NOTIFY_EVENT,
                    0,
                    x.min(self.width - 1) as i16,
                    y.min(self.height - 1) as i16,
                )?;
                for b in 0..5 {
                    if (buttons ^ self.buttons) & (1 << b) != 0 {
                        self.fake(
                            if buttons & (1 << b) != 0 {
                                BUTTON_PRESS_EVENT
                            } else {
                                BUTTON_RELEASE_EVENT
                            },
                            b + 1,
                            0,
                            0,
                        )?;
                    }
                }
                self.buttons = buttons;
            }
            Input::Release => self.release()?,
        }
        self.conn.flush().map_err(error)
    }
}
impl Drop for X11 {
    fn drop(&mut self) {
        if self.control {
            self.deadline();
            let _ = self.release();
        }
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

fn local_socket(path: &std::path::Path) -> io::Result<std::os::unix::net::UnixStream> {
    use std::os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::ffi::OsStrExt,
    };
    let bytes = path.as_os_str().as_bytes();
    let mut addr: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    if bytes.len() >= addr.sun_path.len() {
        return Err(error("X11 socket path too long"));
    }
    addr.sun_family = libc::AF_UNIX as _;
    for (d, b) in addr.sun_path.iter_mut().zip(bytes) {
        *d = *b as _;
    }
    let raw = unsafe {
        libc::socket(
            libc::AF_UNIX,
            libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
            0,
        )
    };
    if raw < 0 {
        return Err(io::Error::last_os_error());
    }
    let owned = unsafe { OwnedFd::from_raw_fd(raw) };
    let result = unsafe {
        libc::connect(
            owned.as_raw_fd(),
            (&addr as *const libc::sockaddr_un).cast(),
            (std::mem::size_of::<libc::sa_family_t>() + bytes.len() + 1) as _,
        )
    };
    if result != 0 {
        let e = io::Error::last_os_error();
        if e.raw_os_error() != Some(libc::EINPROGRESS) {
            return Err(e);
        }
        let mut fd = libc::pollfd {
            fd: owned.as_raw_fd(),
            events: libc::POLLOUT,
            revents: 0,
        };
        if unsafe { libc::poll(&mut fd, 1, 2000) } <= 0 {
            return Err(io::ErrorKind::TimedOut.into());
        }
        let mut socket_error = 0i32;
        let mut length = std::mem::size_of::<i32>() as libc::socklen_t;
        if unsafe {
            libc::getsockopt(
                owned.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_ERROR,
                (&mut socket_error as *mut i32).cast(),
                &mut length,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        if socket_error != 0 {
            return Err(io::Error::from_raw_os_error(socket_error));
        }
    }
    Ok(owned.into())
}
