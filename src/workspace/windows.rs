//! View placement and titles. Transports remain owned by the workspace's terminal registry.
use eframe::egui::{Pos2, Vec2, ViewportId};
use std::collections::{BTreeMap, BTreeSet};
pub type WindowId = u64;
pub const ROOT: WindowId = 0;
pub fn viewport_id(window: WindowId) -> ViewportId {
    if window == ROOT {
        ViewportId::ROOT
    } else {
        ViewportId::from_hash_of(("termcraft-window", window))
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    pub outer_position: Pos2,
    pub inner_size: Vec2,
}
#[derive(Default)]
struct WindowState {
    fullscreen: bool,
    saved: Option<Geometry>,
    restore: Option<Geometry>,
}
#[derive(Default)]
pub struct WorkspaceWindows {
    next: WindowId,
    placements: BTreeMap<u64, WindowId>,
    titles: BTreeMap<u64, String>,
    states: BTreeMap<WindowId, WindowState>,
}
impl WorkspaceWindows {
    pub fn window(&self, terminal: u64) -> WindowId {
        self.placements.get(&terminal).copied().unwrap_or(ROOT)
    }
    pub fn detach(&mut self, terminal: u64) -> WindowId {
        let current = self.window(terminal);
        if current != ROOT {
            return current;
        }
        self.next += 1;
        let window = self.next;
        self.placements.insert(terminal, window);
        self.states.entry(window).or_default();
        window
    }
    pub fn reattach(&mut self, terminal: u64, destination: WindowId) -> Result<(), &'static str> {
        if destination != ROOT && !self.detached().contains(&destination) {
            return Err("Destination window no longer exists");
        }
        let old = self.window(terminal);
        if destination == ROOT {
            self.placements.remove(&terminal);
        } else {
            self.placements.insert(terminal, destination);
        }
        if old != ROOT && self.terminals(old).is_empty() {
            self.states.remove(&old);
        }
        Ok(())
    }
    pub fn terminals(&self, window: WindowId) -> Vec<u64> {
        self.placements
            .iter()
            .filter_map(|(&id, &w)| (w == window).then_some(id))
            .collect()
    }
    pub fn detached(&self) -> Vec<WindowId> {
        self.placements
            .values()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    pub fn replace_terminal(&mut self, old: u64, new: u64) {
        if let Some(window) = self.placements.remove(&old) {
            self.placements.insert(new, window);
        }
        if let Some(title) = self.titles.remove(&old) {
            self.titles.insert(new, title);
        }
    }
    pub fn remove(&mut self, terminal: u64) {
        let _ = self.reattach(terminal, ROOT);
        self.titles.remove(&terminal);
    }
    pub fn rename(&mut self, terminal: u64, label: String) {
        let label: String = label
            .trim()
            .chars()
            .filter(|c| !c.is_control())
            .take(128)
            .collect();
        if label.is_empty() {
            self.titles.remove(&terminal);
        } else {
            self.titles.insert(terminal, label);
        }
    }
    pub fn override_title(&self, terminal: u64) -> Option<&str> {
        self.titles.get(&terminal).map(String::as_str)
    }
    pub fn title<'a>(&'a self, terminal: u64, server: &'a str, fallback: &'a str) -> &'a str {
        self.override_title(terminal)
            .unwrap_or(if server.is_empty() { fallback } else { server })
    }
    pub fn fullscreen(&self, window: WindowId) -> bool {
        self.states.get(&window).is_some_and(|s| s.fullscreen)
    }
    /// Consume a session's initial request only after authentication and first display.
    /// Returns whether the caller should send a native fullscreen command.
    pub fn session_fullscreen(
        &mut self,
        window: WindowId,
        rendered_viewport: ViewportId,
        requested: &mut bool,
        connected: bool,
        geometry: Option<Geometry>,
    ) -> bool {
        if rendered_viewport != viewport_id(window) || !connected || !*requested {
            return false;
        }
        *requested = false;
        if self.fullscreen(window) {
            return false;
        }
        self.enter_fullscreen(window, geometry);
        true
    }
    pub fn enter_fullscreen(&mut self, window: WindowId, geometry: Option<Geometry>) {
        let state = self.states.entry(window).or_default();
        if !state.fullscreen {
            state.saved = geometry.filter(|g| {
                g.outer_position.is_finite()
                    && g.inner_size.is_finite()
                    && g.inner_size.x > 0.
                    && g.inner_size.y > 0.
            });
            state.restore = None;
            state.fullscreen = true;
        }
    }
    pub fn leave_fullscreen(&mut self, window: WindowId) {
        let state = self.states.entry(window).or_default();
        state.fullscreen = false;
        state.restore = state.saved.take();
    }
    pub fn restore_after_ack(
        &mut self,
        window: WindowId,
        native_fullscreen: bool,
    ) -> Option<Geometry> {
        if native_fullscreen {
            return None;
        }
        self.states.get_mut(&window)?.restore.take()
    }
}
