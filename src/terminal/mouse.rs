//! Cell-based xterm mouse reporting, independent of GUI coordinates and transports.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MouseTracking {
    #[default]
    Off,
    Press,
    ButtonMotion,
    AnyMotion,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MouseMode {
    pub tracking: MouseTracking,
    pub sgr: bool,
}
impl MouseMode {
    pub fn enabled(self) -> bool {
        self.tracking != MouseTracking::Off
    }
    pub(crate) fn set(&mut self, parameter: usize, on: bool) {
        let tracking = match parameter {
            1000 => MouseTracking::Press,
            1002 => MouseTracking::ButtonMotion,
            1003 => MouseTracking::AnyMotion,
            1006 => {
                self.sgr = on;
                return;
            }
            _ => return,
        };
        if on {
            self.tracking = tracking;
        } else if self.tracking == tracking {
            self.tracking = MouseTracking::Off;
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}
impl MouseButton {
    fn code(self) -> u8 {
        match self {
            Self::Left => 0,
            Self::Middle => 1,
            Self::Right => 2,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseAction {
    Press(MouseButton),
    Release(MouseButton),
    Move,
    WheelUp,
    WheelDown,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MouseModifiers {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
}
impl MouseModifiers {
    fn bits(self) -> u8 {
        (if self.shift { 4 } else { 0 })
            | (if self.alt { 8 } else { 0 })
            | (if self.ctrl { 16 } else { 0 })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalMouseEvent {
    pub action: MouseAction,
    pub col: usize,
    pub row: usize,
    pub modifiers: MouseModifiers,
}
#[derive(Clone, Default)]
pub struct MouseEncoder {
    mode: MouseMode,
    pressed: u8,
    position: Option<(usize, usize)>,
    motion: Option<(usize, usize, u8)>,
}
impl MouseEncoder {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn position(&self) -> Option<(usize, usize)> {
        self.position
    }
    pub fn encode(&mut self, mode: MouseMode, mut event: TerminalMouseEvent) -> Option<Vec<u8>> {
        if mode != self.mode {
            self.reset();
            self.mode = mode;
        }
        if !mode.enabled() {
            return None;
        }
        if event.col > 65534
            || event.row > 65534
            || !mode.sgr && (event.col > 222 || event.row > 222)
        {
            if matches!(event.action, MouseAction::Release(_)) {
                let (col, row) = self.position?;
                event.col = col;
                event.row = row;
            } else {
                return None;
            }
        }
        let release = matches!(event.action, MouseAction::Release(_));
        let code = match event.action {
            MouseAction::Press(button) => {
                self.pressed |= 1 << button.code();
                self.motion = Some((
                    event.col,
                    event.row,
                    32 + self.pressed.trailing_zeros() as u8 | event.modifiers.bits(),
                ));
                button.code()
            }
            MouseAction::Release(button) => {
                if self.pressed & (1 << button.code()) == 0 {
                    return None;
                }
                self.pressed &= !(1 << button.code());
                self.motion = None;
                if mode.sgr { button.code() } else { 3 }
            }
            MouseAction::Move => {
                if mode.tracking == MouseTracking::Press
                    || (mode.tracking == MouseTracking::ButtonMotion && self.pressed == 0)
                {
                    return None;
                }
                let button = if self.pressed == 0 {
                    3
                } else {
                    self.pressed.trailing_zeros() as u8
                };
                let code = 32 + button;
                let signature = (event.col, event.row, code | event.modifiers.bits());
                if self.motion == Some(signature) {
                    return None;
                }
                self.motion = Some(signature);
                code
            }
            MouseAction::WheelUp => 64,
            MouseAction::WheelDown => 65,
        } | event.modifiers.bits();
        self.position = Some((event.col, event.row));
        Some(if mode.sgr {
            format!(
                "\x1b[<{code};{};{}{}",
                event.col + 1,
                event.row + 1,
                if release { 'm' } else { 'M' }
            )
            .into_bytes()
        } else {
            vec![
                27,
                91,
                77,
                code + 32,
                event.col as u8 + 33,
                event.row as u8 + 33,
            ]
        })
    }
    /// Balance an interrupted drag before yielding to selection, another pane or lost focus.
    pub fn release_all(&mut self, mode: MouseMode) -> Option<Vec<u8>> {
        if mode != self.mode {
            self.reset();
            return None;
        }
        let (col, row) = self.position?;
        let mut bytes = Vec::new();
        for button in [MouseButton::Left, MouseButton::Middle, MouseButton::Right] {
            if let Some(report) = self.encode(
                mode,
                TerminalMouseEvent {
                    action: MouseAction::Release(button),
                    col,
                    row,
                    modifiers: MouseModifiers::default(),
                },
            ) {
                bytes.extend(report);
            }
        }
        self.reset();
        (!bytes.is_empty()).then_some(bytes)
    }
}
