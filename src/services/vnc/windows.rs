//! Windows primary display provider. Native runtime qualification is required.
//! GDI objects are created/released on the calling thread; no handles cross workers.
#[path = "windows_clipboard.rs"]
mod clipboard;
use super::{Frame, Input, MAX_PIXELS, Source};
use std::{
    io,
    mem::{size_of, zeroed},
    ptr,
};
use windows_sys::Win32::{
    Graphics::Gdi::*,
    System::StationsAndDesktops::*,
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};
use zeroize::Zeroizing;
pub(super) struct Desktop {
    width: u16,
    height: u16,
    control: bool,
    buttons: u8,
    held: Vec<Held>,
    temporary: Vec<u16>,
    clipboard_enabled: bool,
    clipboard: Option<clipboard::Clipboard>,
}
struct Held {
    sym: u32,
    code: u16,
    flags: u32,
    owned: bool,
    required: u8,
}
fn fail() -> io::Error {
    io::Error::last_os_error()
}
fn metrics() -> io::Result<(u16, u16)> {
    unsafe {
        let w = GetSystemMetrics(SM_CXSCREEN);
        let h = GetSystemMetrics(SM_CYSCREEN);
        if w <= 0
            || h <= 0
            || w > u16::MAX as i32
            || h > u16::MAX as i32
            || w as usize * h as usize > MAX_PIXELS
        {
            return Err(io::Error::other(
                "Primary display exceeds VNC framebuffer limit",
            ));
        }
        Ok((w as u16, h as u16))
    }
}
// Refuse input/capture while a secure or otherwise different desktop is active.
fn active_desktop() -> io::Result<()> {
    unsafe {
        let input = OpenInputDesktop(0, 0, DESKTOP_READOBJECTS);
        if input.is_null() {
            return Err(fail());
        }
        let thread = GetThreadDesktop(windows_sys::Win32::System::Threading::GetCurrentThreadId());
        let name = |handle| -> io::Result<Vec<u16>> {
            let mut needed = 0;
            let mut b = vec![0u16; 256];
            if GetUserObjectInformationW(
                handle,
                UOI_NAME,
                b.as_mut_ptr().cast(),
                (b.len() * 2) as u32,
                &mut needed,
            ) == 0
            {
                return Err(fail());
            }
            b.truncate((needed as usize / 2).saturating_sub(1));
            Ok(b)
        };
        let a = name(input);
        let b = name(thread);
        CloseDesktop(input);
        if a? != b? {
            return Err(io::Error::other(
                "Selected Windows desktop is no longer active",
            ));
        }
        Ok(())
    }
}
impl Desktop {
    pub(super) fn open(control: bool) -> Result<Self, String> {
        active_desktop()
            .map_err(|_| "The selected Windows desktop is not accessible.".to_string())?;
        let (width, height) = metrics().map_err(|e| e.to_string())?;
        Ok(Self {
            width,
            height,
            control,
            buttons: 0,
            held: Vec::new(),
            temporary: Vec::new(),
            clipboard_enabled: false,
            clipboard: None,
        })
    }
    fn key(code: u16, flags: u32) -> io::Result<()> {
        unsafe {
            let event = INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: if flags & KEYEVENTF_UNICODE != 0 {
                            0
                        } else {
                            code
                        },
                        wScan: if flags & KEYEVENTF_UNICODE != 0 {
                            code
                        } else {
                            0
                        },
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            };
            if SendInput(1, &event, size_of::<INPUT>() as i32) != 1 {
                return Err(fail());
            }
            Ok(())
        }
    }
    fn mouse(x: i32, y: i32, flags: u32, data: u32) -> io::Result<()> {
        unsafe {
            let event = INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: x,
                        dy: y,
                        mouseData: data,
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            };
            if SendInput(1, &event, size_of::<INPUT>() as i32) != 1 {
                return Err(fail());
            }
            Ok(())
        }
    }
    fn release(&mut self) -> io::Result<()> {
        for code in std::mem::take(&mut self.temporary) {
            Self::key(code, KEYEVENTF_KEYUP)?;
        }
        let mut released = Vec::new();
        for held in std::mem::take(&mut self.held) {
            if held.owned && !released.contains(&(held.code, held.flags)) {
                Self::key(held.code, held.flags | KEYEVENTF_KEYUP)?;
                released.push((held.code, held.flags));
            }
        }
        for (i, flag) in [
            MOUSEEVENTF_LEFTUP,
            MOUSEEVENTF_MIDDLEUP,
            MOUSEEVENTF_RIGHTUP,
        ]
        .into_iter()
        .enumerate()
        {
            if self.buttons & (1 << i) != 0 {
                Self::mouse(0, 0, flag, 0)?;
            }
        }
        self.buttons = 0;
        Ok(())
    }
}
fn keycode(sym: u32) -> Option<(u16, u32)> {
    Some(match sym {
        0xff08 => (VK_BACK, 0),
        0xff09 => (VK_TAB, 0),
        0xff0d => (VK_RETURN, 0),
        0xff1b => (VK_ESCAPE, 0),
        0xffff => (VK_DELETE, 0),
        0xff50 => (VK_HOME, 0),
        0xff51 => (VK_LEFT, 0),
        0xff52 => (VK_UP, 0),
        0xff53 => (VK_RIGHT, 0),
        0xff54 => (VK_DOWN, 0),
        0xff55 => (VK_PRIOR, 0),
        0xff56 => (VK_NEXT, 0),
        0xff57 => (VK_END, 0),
        0xffe1 => (VK_LSHIFT, 0),
        0xffe2 => (VK_RSHIFT, 0),
        0xffe3 => (VK_LCONTROL, 0),
        0xffe4 => (VK_RCONTROL, 0),
        0xffe9 => (VK_LMENU, 0),
        0xffea => (VK_RMENU, 0),
        0xffeb => (VK_LWIN, 0),
        0xffec => (VK_RWIN, 0),
        0xffbe..=0xffd5 => (VK_F1 + (sym - 0xffbe) as u16, 0),
        _ => return None,
    })
}
impl Source for Desktop {
    fn enable_clipboard(&mut self) -> io::Result<()> {
        self.clipboard_enabled = true;
        Ok(())
    }
    fn clipboard(&mut self) -> io::Result<Option<Zeroizing<String>>> {
        if !self.clipboard_enabled {
            return Ok(None);
        }
        active_desktop()?;
        if self.clipboard.is_none() {
            self.clipboard = Some(clipboard::Clipboard::new()?);
        }
        self.clipboard.as_mut().unwrap().poll()
    }
    fn set_clipboard(&mut self, text: Zeroizing<String>) -> io::Result<()> {
        if !self.clipboard_enabled {
            return Err(io::Error::other("Clipboard disabled"));
        }
        active_desktop()?;
        if self.clipboard.is_none() {
            self.clipboard = Some(clipboard::Clipboard::new()?);
        }
        self.clipboard.as_mut().unwrap().set(text)
    }

    fn capture(&mut self) -> io::Result<Frame> {
        active_desktop()?;
        if metrics()? != (self.width, self.height) {
            return Err(io::Error::other(
                "Primary display resized; restart VNC sharing",
            ));
        }
        unsafe {
            let screen = GetDC(ptr::null_mut());
            if screen.is_null() {
                return Err(fail());
            }
            let memory = CreateCompatibleDC(screen);
            if memory.is_null() {
                ReleaseDC(ptr::null_mut(), screen);
                return Err(fail());
            }
            let bitmap =
                CreateCompatibleBitmap(screen, i32::from(self.width), i32::from(self.height));
            if bitmap.is_null() {
                DeleteDC(memory);
                ReleaseDC(ptr::null_mut(), screen);
                return Err(fail());
            }
            let old = SelectObject(memory, bitmap);
            if old.is_null() || old as isize == -1 {
                DeleteObject(bitmap);
                DeleteDC(memory);
                ReleaseDC(ptr::null_mut(), screen);
                return Err(io::Error::other(
                    "Could not select the Windows capture bitmap",
                ));
            }
            let mut rgba = vec![0u8; usize::from(self.width) * usize::from(self.height) * 4];
            let mut info: BITMAPINFO = zeroed();
            info.bmiHeader = BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: i32::from(self.width),
                biHeight: -i32::from(self.height),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..zeroed()
            };
            let copied = BitBlt(
                memory,
                0,
                0,
                i32::from(self.width),
                i32::from(self.height),
                screen,
                0,
                0,
                SRCCOPY | CAPTUREBLT,
            );
            // A bitmap must be deselected before GetDIBits.
            SelectObject(memory, old);
            let read = if copied != 0 {
                GetDIBits(
                    screen,
                    bitmap,
                    0,
                    u32::from(self.height),
                    rgba.as_mut_ptr().cast(),
                    &mut info,
                    DIB_RGB_COLORS,
                )
            } else {
                0
            };
            DeleteObject(bitmap);
            DeleteDC(memory);
            ReleaseDC(ptr::null_mut(), screen);
            if read != i32::from(self.height) {
                return Err(fail());
            }
            for pixel in rgba.chunks_exact_mut(4) {
                pixel.swap(0, 2);
                pixel[3] = 255;
            }
            Ok(Frame {
                width: self.width,
                height: self.height,
                rgba,
            })
        }
    }
    fn input(&mut self, event: Input) -> io::Result<()> {
        if !self.control {
            return Ok(());
        }
        active_desktop()?;
        match event {
            Input::Clipboard(_) => {}
            Input::Key { down, keysym } => self.keyboard_event(keysym, down)?,
            Input::Pointer { buttons, x, y } => {
                Self::mouse(
                    i32::from(x) * 65535 / i32::from(self.width.saturating_sub(1).max(1)),
                    i32::from(y) * 65535 / i32::from(self.height.saturating_sub(1).max(1)),
                    MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE,
                    0,
                )?;
                for (i, (down, up)) in [
                    (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
                    (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
                    (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
                ]
                .into_iter()
                .enumerate()
                {
                    if (buttons ^ self.buttons) & (1 << i) != 0 {
                        Self::mouse(0, 0, if buttons & (1 << i) != 0 { down } else { up }, 0)?;
                    }
                }
                if buttons & 8 != 0 && self.buttons & 8 == 0 {
                    Self::mouse(0, 0, MOUSEEVENTF_WHEEL, 120)?;
                }
                if buttons & 16 != 0 && self.buttons & 16 == 0 {
                    Self::mouse(0, 0, MOUSEEVENTF_WHEEL, (-120i32) as u32)?;
                }
                self.buttons = buttons;
            }
            Input::Release => self.release()?,
        }
        Ok(())
    }
}
impl Drop for Desktop {
    fn drop(&mut self) {
        if self.control && active_desktop().is_ok() {
            let _ = self.release();
        }
    }
}

impl Desktop {
    fn keyboard_event(&mut self, sym: u32, down: bool) -> io::Result<()> {
        let sym = super::keyboard::canonical_keysym(sym);
        let old = self.held.iter().position(|h| h.sym == sym);
        if !down && old.is_none() {
            return Ok(());
        }
        let pressed = |code: u16| unsafe { GetAsyncKeyState(i32::from(code)) < 0 };
        let shortcut = [VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN]
            .iter()
            .any(|k| pressed(*k));
        let character = super::keyboard::windows_character(sym, shortcut, |character| unsafe {
            let foreground = GetForegroundWindow();
            let thread = GetWindowThreadProcessId(foreground, ptr::null_mut());
            VkKeyScanExW(character, GetKeyboardLayout(thread))
        });
        let (code, flags, required) = if let Some(index) = old {
            let h = &self.held[index];
            (h.code, h.flags, h.required)
        } else if let Some(plan) = character {
            (
                plan.code,
                if plan.unicode { KEYEVENTF_UNICODE } else { 0 },
                plan.required,
            )
        } else {
            match keycode(sym) {
                Some((code, flags)) => (code, flags, 0),
                None => return Ok(()),
            }
        };
        let owned = old
            .map(|i| self.held[i].owned)
            .unwrap_or_else(|| flags & KEYEVENTF_UNICODE != 0 || !pressed(code));
        if down && old.is_none() {
            if self.held.len() >= 64 {
                return Err(io::Error::other("Too many pressed VNC keys"));
            }
            self.held.push(Held {
                sym,
                code,
                flags,
                owned,
                required,
            });
        }
        let start = self.temporary.len();
        for (bit, key) in [(1, VK_LSHIFT), (2, VK_LCONTROL), (4, VK_LMENU)] {
            let generic = match bit {
                1 => VK_SHIFT,
                2 => VK_CONTROL,
                _ => VK_MENU,
            };
            if required & bit != 0 && !pressed(generic) {
                self.temporary.push(key);
                Self::key(key, 0)?;
            }
        }
        let result = if owned
            && (down
                || !self
                    .held
                    .iter()
                    .any(|h| h.sym != sym && h.code == code && h.flags == flags && h.owned))
        {
            Self::key(code, flags | if down { 0 } else { KEYEVENTF_KEYUP })
        } else {
            Ok(())
        };
        while self.temporary.len() > start {
            let key = *self.temporary.last().unwrap();
            Self::key(key, KEYEVENTF_KEYUP)?;
            self.temporary.pop();
        }
        if !down {
            self.held.retain(|h| h.sym != sym);
        }
        result
    }
}
