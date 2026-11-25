use super::terminal_mouse::Viewport;
use eframe::egui::{Context, Event, Modifiers, PointerButton};
use harbor_workspace::{
    model::ClipboardSettings,
    terminal::{
        Screen,
        selection::{
            ClipboardAction, InputEvent, SelectionMode, SelectionPoint, TerminalSelection,
        },
    },
};
#[derive(Default)]
pub(super) struct Actions {
    pub copy: Option<String>,
    pub paste: bool,
    pub menu: bool,
    pub error: Option<String>,
}
#[derive(Default)]
pub(super) struct Controller {
    pub selection: Option<TerminalSelection>,
    pub mode: SelectionMode,
    anchor: Option<(u64, usize, u64)>,
}
impl Controller {
    pub fn selected(&self, screen: &Screen) -> Result<String, String> {
        self.selection
            .as_ref()
            .map(|s| s.try_extract(screen, self.mode))
            .unwrap_or_else(|| Ok(String::new()))
    }
    fn drag(&mut self, screen: &Screen, point: SelectionPoint) {
        let Some((line, col, epoch)) = self.anchor else {
            return;
        };
        let Some(row) = line
            .checked_sub(screen.first_line_id())
            .and_then(|row| usize::try_from(row).ok())
        else {
            self.anchor = None;
            self.selection = None;
            return;
        };
        if epoch != screen.selection_epoch() {
            self.anchor = None;
            self.selection = None;
            return;
        }
        let mut anchor = SelectionPoint { row, col };
        let mut focus = point;
        if self.mode == SelectionMode::Rectangle {
            if focus.col >= anchor.col {
                focus.col += 1;
            } else {
                anchor.col += 1;
            }
        } else if focus >= anchor {
            focus.col += 1;
        } else {
            anchor.col += 1;
        }
        self.selection = Some(TerminalSelection::new(screen, anchor, focus));
    }
    pub fn handle(
        &mut self,
        ctx: &Context,
        screen: &Screen,
        viewport: &Viewport,
        events: &[Event],
        settings: &ClipboardSettings,
        active: bool,
        selection_allowed: bool,
        current_modifiers: Modifiers,
    ) -> Actions {
        let mut actions = Actions::default();
        if !active {
            self.anchor = None;
            return actions;
        }
        if !selection_allowed {
            self.anchor = None;
        }
        let mut copy = false;
        for event in events {
            match *event {
                Event::Copy => copy = true,
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers,
                } if selection_allowed => {
                    if pressed {
                        if let Some((col, row)) = viewport.cell(ctx, pos) {
                            self.anchor = Some((
                                screen.first_line_id() + row as u64,
                                col,
                                screen.selection_epoch(),
                            ));
                            self.mode = if modifiers.alt {
                                SelectionMode::Rectangle
                            } else {
                                SelectionMode::Linear
                            };
                            self.selection = None;
                        }
                    } else {
                        if self.anchor.is_some() {
                            if let Some((col, row)) = viewport.cell(ctx, pos) {
                                let is_double = ctx.input(|i| {
                                    i.pointer.button_double_clicked(PointerButton::Primary)
                                });
                                if is_double {
                                    self.selection = TerminalSelection::word(
                                        screen,
                                        SelectionPoint { row, col },
                                        &settings.word_delimiters,
                                    );
                                    self.mode = SelectionMode::Linear;
                                } else if self.selection.is_some() {
                                    self.drag(screen, SelectionPoint { row, col });
                                }
                            }
                            copy |= ClipboardAction::from_input(
                                &InputEvent::SelectionFinished,
                                settings,
                            ) == Some(ClipboardAction::Copy)
                                && self.selection.is_some();
                        }
                        self.anchor = None;
                    }
                }
                Event::PointerMoved(pos) if selection_allowed && self.anchor.is_some() => {
                    if let Some((col, row)) = viewport.cell(ctx, pos) {
                        self.drag(screen, SelectionPoint { row, col });
                    }
                }
                Event::PointerButton {
                    pos,
                    button: PointerButton::Secondary,
                    pressed: false,
                    modifiers,
                } if selection_allowed && viewport.cell(ctx, pos).is_some() => {
                    match ClipboardAction::from_input(
                        &InputEvent::RightClick {
                            shift: modifiers.shift || current_modifiers.shift,
                        },
                        settings,
                    ) {
                        Some(ClipboardAction::Paste) => actions.paste = true,
                        Some(ClipboardAction::Menu) => actions.menu = true,
                        _ => {}
                    }
                }
                Event::PointerGone => self.anchor = None,
                _ => {}
            }
        }
        if copy {
            match self.selected(screen) {
                Ok(text) if !text.is_empty() => actions.copy = Some(text),
                Err(error) => actions.error = Some(error),
                _ => {}
            }
        }
        actions
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::terminal_mouse::Viewport;
    use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2, Rect};
    use harbor_workspace::{model::ClipboardSettings, terminal::Screen};
    #[test]
    fn cell_drag_copies_rectangle_and_mouse_mode_requires_shift() {
        let ctx = egui::Context::default();
        let mut controller = Controller::default();
        let mut screen = Screen::new(8, 3, 100);
        screen.feed(b"abcd\r\n1234");
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800., 600.))),
                ..Default::default()
            },
            |ui| {
                ui.allocate_space(egui::vec2(800., 600.));
                let viewport = Viewport {
                    clip: Rect::from_min_max(egui::pos2(200., 100.), egui::pos2(280., 160.)),
                    rows: vec![
                        (
                            0,
                            Rect::from_min_max(egui::pos2(200., 100.), egui::pos2(280., 120.)),
                        ),
                        (
                            1,
                            Rect::from_min_max(egui::pos2(200., 120.), egui::pos2(280., 140.)),
                        ),
                    ],
                    cell_width: 10.,
                    cols: 8,
                    layer: ui.layer_id(),
                };
                let settings = ClipboardSettings {
                    copy_on_select: true,
                    ..Default::default()
                };
                let mods = Modifiers {
                    alt: true,
                    ..Default::default()
                };
                let events = vec![
                    Event::PointerButton {
                        pos: egui::pos2(215., 110.),
                        button: PointerButton::Primary,
                        pressed: true,
                        modifiers: mods,
                    },
                    Event::PointerMoved(egui::pos2(225., 130.)),
                    Event::PointerButton {
                        pos: egui::pos2(225., 130.),
                        button: PointerButton::Primary,
                        pressed: false,
                        modifiers: mods,
                    },
                ];
                assert!(
                    controller
                        .handle(
                            &ctx, &screen, &viewport, &events, &settings, true, false, mods
                        )
                        .copy
                        .is_none()
                );
                let actions = controller.handle(
                    &ctx, &screen, &viewport, &events, &settings, true, true, mods,
                );
                assert_eq!(actions.copy.as_deref(), Some("bc\n23"));
                assert!(actions.error.is_none());
            },
        );
        output.textures_delta.clear();
    }
}
