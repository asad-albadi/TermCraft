//! macOS 14+ ScreenCaptureKit one-shot frames; no stream delegates or raw Rust contexts.
//! Every callback owns its bounded sender. Timeouts/cancel drop receivers; late replies
//! cannot touch the Source. At most one capture is outstanding per live Source.
use super::{Frame, Input, MAX_PIXELS, Source};
#[path = "macos_frame.rs"]
mod frame;
use block2::RcBlock;
use objc2::{
    AllocAnyThread,
    rc::{Retained, autoreleasepool},
};
use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
use objc2_core_foundation::{CGPoint, CGRect};
use objc2_core_graphics::*;
use objc2_foundation::{NSArray, NSError, NSProcessInfo, NSString};
use objc2_screen_capture_kit::{
    SCContentFilter, SCScreenshotManager, SCShareableContent, SCStreamConfiguration,
};
use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use zeroize::Zeroizing;
#[path = "macos_lifecycle.rs"]
mod lifecycle;
use lifecycle::{Cancel, Permit, wait};
use std::sync::Arc;
#[path = "macos_keyboard.rs"]
mod keys;
use keys::keycode;
#[path = "macos_pointer.rs"]
mod pointer;
fn error(s: &str) -> io::Error {
    io::Error::other(s)
}
static IN_FLIGHT: AtomicBool = AtomicBool::new(false);
/// The framework can dispose its copied completion block on any worker queue.
/// Keep owned references across callback pools; release them inside their own pool.
struct CaptureObjects {
    filter: Option<Retained<SCContentFilter>>,
    config: Option<Retained<SCStreamConfiguration>>,
}
impl Drop for CaptureObjects {
    fn drop(&mut self) {
        autoreleasepool(|_| {
            drop(self.filter.take());
            drop(self.config.take());
        });
    }
}
fn load_framework() -> Result<(), String> {
    static FRAMEWORK: std::sync::OnceLock<Result<libloading::Library, String>> =
        std::sync::OnceLock::new();
    // Load only a fixed Apple system framework, never a user path/search-path fallback.
    FRAMEWORK
        .get_or_init(|| {
            unsafe {
                libloading::Library::new(
                    "/System/Library/Frameworks/ScreenCaptureKit.framework/ScreenCaptureKit",
                )
            }
            .map_err(|_| "ScreenCaptureKit is unavailable on this Mac.".to_string())
        })
        .as_ref()
        .map_err(Clone::clone)?;
    if [
        c"SCShareableContent",
        c"SCScreenshotManager",
        c"SCContentFilter",
        c"SCStreamConfiguration",
    ]
    .iter()
    .any(|name| objc2::runtime::AnyClass::get(name).is_none())
    {
        return Err("This Mac does not provide the required ScreenCaptureKit APIs.".into());
    }
    Ok(())
}
fn screenshot(id: u32, cancel: &Cancel) -> io::Result<Frame> {
    autoreleasepool(|_| {
        // A lost native completion permanently retains this permit. Repeated Start
        // cannot create unbounded work; relaunch is then required.
        let permit = Permit::acquire(&IN_FLIGHT)?;
        let (tx, rx) = mpsc::sync_channel(1);
        let alive = Arc::new(AtomicBool::new(true));
        let receiver_alive = alive.clone();
        let callback = RcBlock::new(
            move |content: *mut SCShareableContent, err: *mut NSError| unsafe {
                autoreleasepool(|_| {
                    if !alive.load(Ordering::Acquire) {
                        return;
                    }
                    if !err.is_null() || content.is_null() {
                        let _ = tx.try_send(Err(error(
                    "ScreenCaptureKit display access failed; check Screen Recording permission",
                )));
                        return;
                    }
                    let permit = permit.clone();
                    let displays = (&*content).displays();
                    let Some(display) = displays.iter().find(|d| d.displayID() == id) else {
                        let _ = tx
                            .try_send(Err(error("Selected macOS display is no longer available")));
                        return;
                    };
                    let width = display.width();
                    let height = display.height();
                    if width <= 0
                        || height <= 0
                        || (width as usize)
                            .checked_mul(height as usize)
                            .is_none_or(|n| n > super::MAX_PIXELS)
                    {
                        let _ = tx.try_send(Err(error("Selected macOS display exceeds VNC limit")));
                        return;
                    }
                    let filter = SCContentFilter::initWithDisplay_excludingWindows(
                        SCContentFilter::alloc(),
                        &display,
                        &NSArray::new(),
                    );
                    let config = SCStreamConfiguration::new();
                    config.setWidth(width as usize);
                    config.setHeight(height as usize);
                    config.setPixelFormat(u32::from_be_bytes(*b"BGRA"));
                    config.setCapturesAudio(false);
                    let output = tx.clone();
                    let keep_objects = CaptureObjects {
                        filter: Some(filter.clone()),
                        config: Some(config.clone()),
                    };
                    let alive = alive.clone();
                    let image_callback = RcBlock::new(
                        move |image: *mut CGImage, err: *mut NSError| {
                            autoreleasepool(|_| {
                                let _owned = (&permit, &keep_objects);
                                if !alive.load(Ordering::Acquire) {
                                    return;
                                }
                                let result = (|| {
                                    if !err.is_null() || image.is_null() {
                                        return Err(error("ScreenCaptureKit frame failed"));
                                    }
                                    let image = &*image;
                                    if CGImageGetBitsPerComponent(Some(image)) != 8
                                        || CGImageGetBitsPerPixel(Some(image)) != 32
                                        || CGImageGetBitmapInfo(Some(image)).0 & 0x7000 != 0x2000
                                        || !matches!(CGImageGetAlphaInfo(Some(image)).0, 2 | 6)
                                    {
                                        return Err(error(
                                            "ScreenCaptureKit returned unsupported SDR pixel format",
                                        ));
                                    }
                                    let width = CGImageGetWidth(Some(image));
                                    let height = CGImageGetHeight(Some(image));
                                    let stride = CGImageGetBytesPerRow(Some(image));
                                    let expected = stride
                                        .checked_mul(height)
                                        .ok_or_else(|| error("Screen frame layout overflow"))?;
                                    if width == 0
                                        || height == 0
                                        || width > u16::MAX as usize
                                        || height > u16::MAX as usize
                                        || width
                                            .checked_mul(height)
                                            .is_none_or(|n| n > super::MAX_PIXELS)
                                        || stride < width * 4
                                        || expected > super::MAX_PIXELS * 4 + 65536
                                    {
                                        return Err(error("Screen frame exceeds memory limit"));
                                    }
                                    let provider = CGImageGetDataProvider(Some(image))
                                        .ok_or_else(|| error("Missing screen frame data"))?;
                                    let data = CGDataProvider::data(Some(&provider))
                                        .ok_or_else(|| error("Missing screen frame bytes"))?;
                                    let size = usize::try_from(data.length())
                                        .map_err(|_| error("Invalid screen frame size"))?;
                                    if size != expected || data.byte_ptr().is_null() {
                                        return Err(error("Screen frame exceeds memory limit"));
                                    }
                                    frame::bgra(
                                        CGImageGetWidth(Some(image)),
                                        CGImageGetHeight(Some(image)),
                                        CGImageGetBytesPerRow(Some(image)),
                                        std::slice::from_raw_parts(data.byte_ptr(), size),
                                    )
                                })();
                                let _ = output.try_send(result);
                            })
                        },
                    );
                    SCScreenshotManager::captureImageWithFilter_configuration_completionHandler(
                        &filter,
                        &config,
                        Some(&image_callback),
                    );
                })
            },
        );
        unsafe {
            SCShareableContent::getShareableContentWithCompletionHandler(&callback);
        }
        let result = wait(rx, cancel);
        receiver_alive.store(false, Ordering::Release);
        result
    })
}
pub(super) struct Desktop {
    id: u32,
    width: u16,
    height: u16,
    control: bool,
    cancel: Cancel,
    held: Vec<(u32, u16, Option<char>)>,
    buttons: u8,
    point: CGPoint,
    last_capture: Instant,
    clipboard: bool,
    clipboard_revision: isize,
    last: Option<Zeroizing<String>>,
}
impl Desktop {
    pub(super) fn open(id: u32, control: bool, cancel: Cancel) -> Result<Self, String> {
        autoreleasepool(|_| {
            if cancel() {
                return Err("macOS sharing cancelled.".into());
            }
            if NSProcessInfo::processInfo()
                .operatingSystemVersion()
                .majorVersion
                < 14
            {
                return Err("macOS display sharing requires macOS 14 or later.".into());
            }
            load_framework()?;
            if !CGPreflightScreenCaptureAccess() && !CGRequestScreenCaptureAccess() {
                return Err("Allow Screen Recording in macOS System Settings, then Start again (a relaunch may be required).".into());
            }
            if control && !CGPreflightPostEventAccess() && !CGRequestPostEventAccess() {
                return Err("Remote control requires macOS Accessibility permission. Enable it in System Settings, then Start again.".into());
            }
            let id = if id == 0 { CGMainDisplayID() } else { id };
            let initial = screenshot(id, &cancel).map_err(|e| e.to_string())?;
            Ok(Self {
                id,
                width: initial.width,
                height: initial.height,
                control,
                cancel,
                held: Vec::new(),
                buttons: 0,
                point: CGPoint::new(0., 0.),
                last_capture: Instant::now(),
                clipboard: false,
                clipboard_revision: -1,
                last: None,
            })
        })
    }
    fn geometry(&self) -> io::Result<CGRect> {
        autoreleasepool(|_| {
            if !CGDisplayIsActive(self.id) {
                return Err(error("Selected macOS display was disconnected"));
            }
            Ok(CGDisplayBounds(self.id))
        })
    }
    fn key(&self, code: u16, down: bool, text: Option<char>) -> io::Result<()> {
        autoreleasepool(|_| {
            let event = CGEvent::new_keyboard_event(None, code, down)
                .ok_or_else(|| error("macOS keyboard event failed"))?;
            let sibling = keys::modifier_group(code).is_some_and(|(_, other)| {
                CGEventSourceKeyState(CGEventSourceStateID::HIDSystemState, other)
            });
            let flags = keys::event_flags(
                CGEventSourceFlagsState(CGEventSourceStateID::HIDSystemState).0,
                code,
                down,
                text.is_some(),
                sibling,
            );
            CGEvent::set_flags(Some(&event), CGEventFlags(flags));
            if let Some(ch) = text {
                let mut units = [0u16; 2];
                let units = ch.encode_utf16(&mut units);
                unsafe {
                    CGEvent::keyboard_set_unicode_string(
                        Some(&event),
                        units.len() as _,
                        units.as_ptr(),
                    );
                }
            }
            CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
            Ok(())
        })
    }
    fn mouse(&self, plan: pointer::Event) -> io::Result<()> {
        autoreleasepool(|_| {
            let e = CGEvent::new_mouse_event(
                None,
                CGEventType(plan.kind),
                self.point,
                CGMouseButton(plan.button),
            )
            .ok_or_else(|| error("macOS pointer event failed"))?;
            CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&e));
            Ok(())
        })
    }
    fn release(&mut self) -> io::Result<()> {
        autoreleasepool(|_| {
            let mut failed = false;
            for (_, code, text) in self.held.drain(..).collect::<Vec<_>>() {
                failed |= self.key(code, false, text).is_err();
            }
            for bit in [1, 2, 4] {
                if self.buttons & bit != 0 {
                    failed |= self.mouse(pointer::button(bit, false)).is_err();
                }
            }
            self.buttons = 0;
            if failed {
                Err(error("Could not release remote macOS input"))
            } else {
                Ok(())
            }
        })
    }
}
impl Source for Desktop {
    fn capture(&mut self) -> io::Result<Frame> {
        autoreleasepool(|_| {
            self.geometry()?;
            if !CGPreflightScreenCaptureAccess() {
                return Err(error("Screen Recording permission was revoked"));
            }
            while self.last_capture.elapsed() < Duration::from_millis(200) {
                if (self.cancel)() {
                    return Err(error("macOS capture cancelled"));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            self.last_capture = Instant::now();
            let f = screenshot(self.id, &self.cancel)?;
            if (f.width, f.height) != (self.width, self.height) {
                return Err(error("Selected display geometry changed; restart sharing"));
            }
            Ok(f)
        })
    }
    fn input(&mut self, event: Input) -> io::Result<()> {
        autoreleasepool(|_| {
            if matches!(event, Input::Release) {
                return self.release();
            }
            if !self.control || !CGPreflightPostEventAccess() {
                return Err(error("Remote macOS control is not permitted"));
            }
            let bounds = self.geometry()?;
            match event {
                Input::Key { down, keysym } => {
                    let keysym = super::keyboard::canonical_keysym(keysym);
                    let shortcut = self
                        .held
                        .iter()
                        .any(|(_, c, _)| matches!(c, 54 | 55 | 58 | 59 | 61 | 62));
                    let text = if shortcut {
                        None
                    } else {
                        super::keyboard::keysym_character(keysym)
                    };
                    let code = if text.is_some() {
                        keycode(keysym).unwrap_or(0)
                    } else {
                        keycode(keysym).ok_or_else(|| {
                            error("Unsupported macOS shortcut key (US physical key mapping)")
                        })?
                    };
                    if down {
                        if self.held.iter().any(|(s, _, _)| *s == keysym) {
                            return self.key(code, true, text);
                        }
                        if self.held.len() >= 128 {
                            return Err(error("Remote key limit exceeded"));
                        }
                        // Never claim or release keys already held by a local user.
                        if CGEventSourceKeyState(CGEventSourceStateID::HIDSystemState, code) {
                            return Ok(());
                        }
                        self.key(code, true, text)?;
                        self.held.push((keysym, code, text));
                    } else if let Some(index) = self.held.iter().position(|(s, _, _)| *s == keysym)
                    {
                        let (_, code, text) = self.held[index];
                        self.key(code, false, text)?;
                        self.held.remove(index);
                    }
                }
                Input::Pointer { buttons, x, y } => {
                    let (px, py) = frame::position(
                        x,
                        y,
                        self.width,
                        self.height,
                        [
                            bounds.origin.x,
                            bounds.origin.y,
                            bounds.size.width,
                            bounds.size.height,
                        ],
                    )?;
                    self.point = CGPoint::new(px, py);
                    self.mouse(pointer::movement(self.buttons))?;
                    for bit in [1, 2, 4] {
                        if buttons & bit != 0 && self.buttons & bit == 0 {
                            let down = pointer::button(bit, true);
                            if !CGEventSourceButtonState(
                                CGEventSourceStateID::HIDSystemState,
                                CGMouseButton(down.button),
                            ) {
                                self.mouse(down)?;
                                self.buttons |= bit;
                            }
                        } else if buttons & bit == 0 && self.buttons & bit != 0 {
                            self.mouse(pointer::button(bit, false))?;
                            self.buttons &= !bit;
                        }
                    }
                    let scroll = if buttons & 8 != 0 {
                        1
                    } else if buttons & 16 != 0 {
                        -1
                    } else {
                        0
                    };
                    if scroll != 0 {
                        let e = CGEvent::new_scroll_wheel_event2(
                            None,
                            CGScrollEventUnit::Line,
                            1,
                            scroll,
                            0,
                            0,
                        )
                        .ok_or_else(|| error("Scroll event failed"))?;
                        CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&e));
                    }
                }
                _ => {}
            }
            Ok(())
        })
    }
    fn enable_clipboard(&mut self) -> io::Result<()> {
        // This is the macOS user's global pasteboard, shared across displays.
        self.clipboard = true;
        self.clipboard_revision = -1;
        Ok(())
    }
    fn clipboard(&mut self) -> io::Result<Option<Zeroizing<String>>> {
        autoreleasepool(|_| {
            if !self.clipboard {
                return Ok(None);
            }
            let c = NSPasteboard::generalPasteboard();
            let revision = c.changeCount();
            if revision == self.clipboard_revision {
                return Ok(None);
            }
            self.clipboard_revision = revision;
            let Some(value) = c.stringForType(unsafe { NSPasteboardTypeString }) else {
                self.last = None;
                return Ok(None);
            };
            // Inspect native string size BEFORE allocating a Rust copy.
            if value.len_utf16() > super::clipboard::TEXT_LIMIT
                || value.len() > super::clipboard::TEXT_LIMIT
            {
                return Err(error("macOS clipboard text exceeds limit"));
            }
            let text = Zeroizing::new(value.to_string());
            if self.last.as_deref() == Some(&text) {
                return Ok(None);
            }
            self.last = Some(text.clone());
            Ok(Some(text))
        })
    }
    fn set_clipboard(&mut self, text: Zeroizing<String>) -> io::Result<()> {
        autoreleasepool(|_| {
            if !self.control || !self.clipboard || text.len() > super::clipboard::TEXT_LIMIT {
                return Err(error("Remote clipboard update is not permitted"));
            }
            let c = NSPasteboard::generalPasteboard();
            c.clearContents();
            if !c.setString_forType(&NSString::from_str(&text), unsafe {
                NSPasteboardTypeString
            }) {
                return Err(error("macOS clipboard write failed"));
            }
            self.clipboard_revision = c.changeCount();
            self.last = Some(text);
            Ok(())
        })
    }
}
impl Drop for Desktop {
    fn drop(&mut self) {
        let _ = self.release();
    }
}
