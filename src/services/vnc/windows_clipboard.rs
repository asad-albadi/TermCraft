//! CF_UNICODETEXT on the explicitly selected current input desktop.
use std::{io, ptr};
use windows_sys::Win32::{
    Foundation::{GlobalFree, HWND},
    System::{DataExchange::*, Memory::*},
    UI::WindowsAndMessaging::*,
};
use zeroize::Zeroizing;
pub(super) struct Clipboard {
    window: HWND,
    sequence: u32,
}
// Constructed and destroyed only on the source worker. The source stores this lazily.
unsafe impl Send for Clipboard {}
struct Open;
impl Drop for Open {
    fn drop(&mut self) {
        unsafe {
            CloseClipboard();
        }
    }
}
impl Clipboard {
    pub(super) fn new() -> io::Result<Self> {
        unsafe {
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            let window = CreateWindowExW(
                0,
                class.as_ptr(),
                ptr::null(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
            );
            if window.is_null() {
                return Err(io::Error::last_os_error());
            }
            Ok(Self {
                window,
                sequence: 0,
            })
        }
    }
    fn open(&self) -> Option<Open> {
        if unsafe { OpenClipboard(self.window) } == 0 {
            None
        } else {
            Some(Open)
        }
    }
    pub(super) fn poll(&mut self) -> io::Result<Option<Zeroizing<String>>> {
        unsafe {
            let sequence = GetClipboardSequenceNumber();
            if sequence == self.sequence {
                return Ok(None);
            }
            let Some(_guard) = self.open() else {
                return Ok(None);
            };
            self.sequence = sequence;
            let data = GetClipboardData(13);
            if data.is_null() {
                return Ok(Some(Zeroizing::new(String::new())));
            }
            let size = GlobalSize(data);
            if size == 0 || size > 32770 || size % 2 != 0 {
                return Ok(None);
            }
            let p = GlobalLock(data);
            if p.is_null() {
                return Err(io::Error::last_os_error());
            }
            let wide = std::slice::from_raw_parts(p.cast::<u16>(), size / 2);
            let text = wide.iter().position(|c| *c == 0).and_then(|n| {
                String::from_utf16(&wide[..n])
                    .ok()
                    .map(|t| t.replace("\r\n", "\n"))
            });
            GlobalUnlock(data);
            Ok(text.filter(|t| t.len() <= 8192).map(Zeroizing::new))
        }
    }
    pub(super) fn set(&mut self, text: Zeroizing<String>) -> io::Result<()> {
        unsafe {
            if text.len() > 8192 || text.contains('\0') {
                return Err(io::Error::other("Invalid clipboard text"));
            }
            let text = Zeroizing::new(text.replace("\r\n", "\n").replace('\n', "\r\n"));
            let wide = Zeroizing::new(text.encode_utf16().chain(Some(0)).collect::<Vec<_>>());
            let Some(_guard) = self.open() else {
                return Err(io::Error::other("Selected clipboard busy"));
            };
            let memory = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2);
            if memory.is_null() {
                return Err(io::Error::last_os_error());
            }
            let data = GlobalLock(memory);
            if data.is_null() {
                GlobalFree(memory);
                return Err(io::Error::last_os_error());
            }
            ptr::copy_nonoverlapping(wide.as_ptr(), data.cast::<u16>(), wide.len());
            GlobalUnlock(memory);
            if EmptyClipboard() == 0 || SetClipboardData(13, memory).is_null() {
                GlobalFree(memory);
                return Err(io::Error::last_os_error());
            }
            self.sequence = GetClipboardSequenceNumber();
            Ok(())
        }
    }
}
impl Drop for Clipboard {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.window);
        }
    }
}
