use super::Frame;
use std::io;
type Cancel = std::sync::Arc<dyn Fn() -> bool + Send + Sync>;
#[path = "wayland/helper.rs"]
mod helper;
#[path = "wayland/portal.rs"]
mod portal;
use super::{Input, Source};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
pub(super) struct Wayland {
    // Stream is dropped/joined before portal session closure.
    capture: Option<helper::Capture>,
    portal: portal::Portal,
    selected: portal::Selected,
    held: BTreeMap<u32, u32>,
    buttons: u8,
    control: bool,
    size: (u16, u16),
}
impl Wayland {
    pub(super) fn open(control: bool, cancel: Cancel) -> Result<Self, String> {
        Self::start(control, cancel, None).map_err(|e| e.to_string())
    }
    fn start(
        control: bool,
        cancel: Cancel,
        helper_root: Option<&std::path::Path>,
    ) -> io::Result<Self> {
        // Resolve and seal the optional helper before asking the user for permission.
        let image = helper::Capture::prepare(helper_root)?;
        let (portal, selected, fd) = portal::Portal::open(control, cancel.clone())?;
        let capture = helper::Capture::open(fd, selected.node, selected.serial, cancel, image)?;
        Ok(Self {
            capture: Some(capture),
            portal,
            selected,
            held: BTreeMap::new(),
            buttons: 0,
            control,
            size: (0, 0),
        })
    }
    fn key(&mut self, down: bool, keysym: u32) -> io::Result<()> {
        if keysym > i32::MAX as u32 {
            return Err(io::Error::other("Invalid portal keysym"));
        }
        // Retain the original keysym for matching release, including Unicode aliases.
        let identity = super::keyboard::canonical_keysym(keysym);
        if down && !self.held.contains_key(&identity) && self.held.len() >= 64 {
            return Err(io::Error::other("Too many held portal keys"));
        }
        let value = self.held.get(&identity).copied().unwrap_or(keysym);
        if !down && !self.held.contains_key(&identity) {
            return Ok(());
        }
        self.portal.notify(
            "NotifyKeyboardKeysym",
            &(
                self.portal.session(),
                portal::Dict::new(),
                value as i32,
                u32::from(down),
            ),
        )?;
        if down {
            self.held.insert(identity, value);
        } else {
            self.held.remove(&identity);
        }
        Ok(())
    }
    fn release(&mut self) -> io::Result<()> {
        for sym in self.held.values().copied().collect::<Vec<_>>() {
            self.key(false, sym)?;
        }
        for (mask, code) in [(1, 272), (2, 274), (4, 273)] {
            if self.buttons & mask != 0 {
                self.portal.notify(
                    "NotifyPointerButton",
                    &(self.portal.session(), portal::Dict::new(), code, 0u32),
                )?;
            }
        }
        self.buttons = 0;
        Ok(())
    }
}
impl Source for Wayland {
    fn capture(&mut self) -> io::Result<Frame> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            self.portal.ensure_active()?;
            if let Some(frame) = self.capture.as_mut().unwrap().frame()? {
                self.size = (frame.width, frame.height);
                return Ok(frame);
            }
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "No frame arrived from selected portal stream",
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn input(&mut self, event: Input) -> io::Result<()> {
        if matches!(event, Input::Release) {
            return self.release();
        }
        if !self.control {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Portal source is view-only",
            ));
        }
        self.portal.ensure_active()?;
        match event {
            Input::Key { down, keysym } => self.key(down, keysym),
            Input::Pointer { buttons, x, y } => {
                let (w, h) = self.size;
                if w == 0 || h == 0 || x >= w || y >= h || buttons & 128 != 0 {
                    return Err(io::Error::other("Invalid portal pointer coordinates"));
                }
                let (lw, lh) = self
                    .selected
                    .logical
                    .unwrap_or((i32::from(w), i32::from(h)));
                let px = f64::from(x) * f64::from(lw) / f64::from(w);
                let py = f64::from(y) * f64::from(lh) / f64::from(h);
                self.portal.notify(
                    "NotifyPointerMotionAbsolute",
                    &(
                        self.portal.session(),
                        portal::Dict::new(),
                        self.selected.node,
                        px,
                        py,
                    ),
                )?;
                for (mask, code) in [(1, 272), (2, 274), (4, 273)] {
                    if (buttons ^ self.buttons) & mask != 0 {
                        self.portal.notify(
                            "NotifyPointerButton",
                            &(
                                self.portal.session(),
                                portal::Dict::new(),
                                code,
                                u32::from(buttons & mask != 0),
                            ),
                        )?;
                    }
                }
                for (mask, axis, steps) in [(8, 0u32, -1i32), (16, 0, 1), (32, 1, -1), (64, 1, 1)] {
                    if buttons & mask != 0 && self.buttons & mask == 0 {
                        self.portal.notify(
                            "NotifyPointerAxisDiscrete",
                            &(self.portal.session(), portal::Dict::new(), axis, steps),
                        )?;
                    }
                }
                self.buttons = buttons;
                Ok(())
            }
            Input::Clipboard(_) => Err(io::Error::other("Portal clipboard is unavailable")),
            Input::Release => Ok(()),
        }
    }
}
impl Drop for Wayland {
    fn drop(&mut self) {
        let _ = self.release();
        drop(self.capture.take());
    }
}
#[cfg(test)]
#[path = "wayland/fixture.rs"]
mod fixture;
