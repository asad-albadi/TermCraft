use eframe::egui::{Key, Modifiers};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyChord {
    pub key: String,
    pub command: bool,
    #[serde(default)]
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}
impl KeyChord {
    pub fn new(key: &str, command: bool, shift: bool, alt: bool) -> Self {
        Self {
            key: Key::from_name(key).map(|k| k.name()).unwrap_or(key).into(),
            command,
            ctrl: false,
            shift,
            alt,
        }
    }
    pub fn from_key(key: Key, m: Modifiers) -> Self {
        let command = m.mac_cmd || m.command || m.ctrl && !cfg!(target_os = "macos");
        Self {
            key: key.name().into(),
            command,
            ctrl: m.ctrl && cfg!(target_os = "macos"),
            shift: m.shift,
            alt: m.alt,
        }
    }
    pub fn label(&self) -> String {
        format!(
            "{}{}{}{}{}",
            if self.command {
                if cfg!(target_os = "macos") {
                    "Cmd+"
                } else {
                    "Ctrl+"
                }
            } else {
                ""
            },
            if self.ctrl { "Ctrl+" } else { "" },
            if self.alt { "Alt+" } else { "" },
            if self.shift { "Shift+" } else { "" },
            self.key
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    NewSession,
    NewTerminal,
    ToggleSidebar,
    NextTab,
    PreviousTab,
    CloseTab,
    Find,
    Copy,
    Paste,
    LineUp,
    LineDown,
    PageUp,
    PageDown,
    HistoryStart,
    HistoryEnd,
    Fullscreen,
}
impl Action {
    pub const ALL: [Self; 16] = [
        Self::NewSession,
        Self::NewTerminal,
        Self::ToggleSidebar,
        Self::NextTab,
        Self::PreviousTab,
        Self::CloseTab,
        Self::Find,
        Self::Copy,
        Self::Paste,
        Self::LineUp,
        Self::LineDown,
        Self::PageUp,
        Self::PageDown,
        Self::HistoryStart,
        Self::HistoryEnd,
        Self::Fullscreen,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::NewSession => "New saved-session form",
            Self::NewTerminal => "New local terminal",
            Self::ToggleSidebar => "Toggle sidebar",
            Self::NextTab => "Next terminal tab",
            Self::PreviousTab => "Previous terminal tab",
            Self::CloseTab => "Close terminal tab",
            Self::Find => "Find terminal output",
            Self::Copy => "Copy selection",
            Self::Paste => "Paste",
            Self::LineUp => "History: line up",
            Self::LineDown => "History: line down",
            Self::PageUp => "History: page up",
            Self::PageDown => "History: page down",
            Self::HistoryStart => "History: start",
            Self::HistoryEnd => "History: end",
            Self::Fullscreen => "Toggle fullscreen",
        }
    }
    pub fn terminal_view(self) -> bool {
        matches!(
            self,
            Self::Copy
                | Self::Paste
                | Self::Find
                | Self::LineUp
                | Self::LineDown
                | Self::PageUp
                | Self::PageDown
                | Self::HistoryStart
                | Self::HistoryEnd
        )
    }
    fn terminal_only(self) -> bool {
        self.terminal_view() || matches!(self, Self::NextTab | Self::PreviousTab | Self::CloseTab)
    }
    pub fn scroll_offset(self, current: f32, line: f32, page: f32, max: f32) -> Option<f32> {
        let amount = match self {
            Self::LineUp => current - line,
            Self::LineDown => current + line,
            Self::PageUp => current - (page - line).max(line),
            Self::PageDown => current + (page - line).max(line),
            Self::HistoryStart => 0.,
            Self::HistoryEnd => max,
            _ => return None,
        };
        Some(amount.clamp(0., max.max(0.)))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusContext {
    Workspace,
    Terminal,
    TextEditor,
    Authentication,
    RemoteDesktop,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    pub action: Action,
    pub chord: KeyChord,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShortcutMap {
    pub bindings: Vec<Binding>,
}
#[derive(Clone, Debug)]
pub struct ShortcutConflict(pub String);
impl ShortcutMap {
    pub fn bind(&mut self, action: Action, mut chord: KeyChord) -> Result<(), ShortcutConflict> {
        let key = Key::from_name(&chord.key)
            .ok_or_else(|| ShortcutConflict("Choose a recognised key".into()))?;
        chord.key = key.name().into();
        let function = chord
            .key
            .strip_prefix('F')
            .is_some_and(|n| n.parse::<u8>().is_ok());
        let navigation = matches!(
            key,
            Key::Escape
                | Key::Tab
                | Key::Enter
                | Key::Backspace
                | Key::Insert
                | Key::Delete
                | Key::Home
                | Key::End
                | Key::PageUp
                | Key::PageDown
                | Key::ArrowUp
                | Key::ArrowDown
                | Key::ArrowLeft
                | Key::ArrowRight
        );
        if !chord.command && !chord.ctrl && !chord.alt && !function && !navigation {
            return Err(ShortcutConflict("Typing keys need a modifier; choose a function/navigation key or add Command, Ctrl or Alt".into()));
        }
        if let Some(binding) = self
            .bindings
            .iter()
            .find(|b| b.action != action && b.chord == chord)
        {
            return Err(ShortcutConflict(format!(
                "{} is already assigned to {}",
                chord.label(),
                binding.action.label()
            )));
        }
        self.unbind(action);
        self.bindings.push(Binding { action, chord });
        Ok(())
    }
    pub fn unbind(&mut self, action: Action) {
        self.bindings.retain(|b| b.action != action);
    }
    pub fn validate(&self) -> Result<(), ShortcutConflict> {
        if self.bindings.len() > Action::ALL.len() {
            return Err(ShortcutConflict("Too many shortcut bindings".into()));
        }
        let mut map = Self {
            bindings: Vec::new(),
        };
        for b in &self.bindings {
            if map.bindings.iter().any(|old| old.action == b.action) {
                return Err(ShortcutConflict("Duplicate action in shortcuts".into()));
            }
            map.bind(b.action, b.chord.clone())?;
        }
        Ok(())
    }
    pub fn resolve(&self, chord: &KeyChord, focus: FocusContext) -> Option<Action> {
        if matches!(
            focus,
            FocusContext::TextEditor | FocusContext::Authentication | FocusContext::RemoteDesktop
        ) || self.validate().is_err()
        {
            return None;
        }
        self.bindings
            .iter()
            .find(|b| {
                &b.chord == chord && (!b.action.terminal_only() || focus == FocusContext::Terminal)
            })
            .map(|b| b.action)
    }
}
impl Default for ShortcutMap {
    fn default() -> Self {
        let mut map = Self {
            bindings: Vec::new(),
        };
        let clipboard_shift = !cfg!(target_os = "macos");
        for (action, key, command, shift, alt) in [
            (Action::NewSession, "N", true, false, false),
            (Action::NewTerminal, "T", true, true, false),
            (Action::ToggleSidebar, "B", true, true, false),
            (Action::NextTab, "PageDown", true, false, false),
            (Action::PreviousTab, "PageUp", true, false, false),
            (Action::CloseTab, "W", true, true, false),
            (Action::Find, "F", true, true, false),
            (Action::Copy, "C", true, clipboard_shift, false),
            (Action::Paste, "V", true, clipboard_shift, false),
            (Action::LineUp, "ArrowUp", true, true, false),
            (Action::LineDown, "ArrowDown", true, true, false),
            (Action::PageUp, "PageUp", false, true, false),
            (Action::PageDown, "PageDown", false, true, false),
            (Action::HistoryStart, "Home", false, true, false),
            (Action::HistoryEnd, "End", false, true, false),
            (Action::Fullscreen, "F11", false, false, false),
        ] {
            map.bind(action, KeyChord::new(key, command, shift, alt))
                .expect("distinct defaults");
        }
        map
    }
}
