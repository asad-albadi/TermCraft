//! Route pointer events through the actually painted, clipped live terminal rows.
use eframe::egui::{Context, Event, LayerId, Modifiers, MouseWheelUnit, PointerButton, Pos2, Rect};
use harbor_workspace::terminal::mouse::{
    MouseAction, MouseButton, MouseEncoder, MouseMode, MouseModifiers, TerminalMouseEvent,
};
#[derive(Clone)]
pub(super) struct Viewport {
    pub clip: Rect,
    pub rows: Vec<(usize, Rect)>,
    pub cell_width: f32,
    pub cols: usize,
    pub layer: LayerId,
}
impl Viewport {
    pub fn cell(&self, ctx: &Context, pos: Pos2) -> Option<(usize, usize)> {
        if !self.clip.contains(pos) || ctx.layer_id_at(pos) != Some(self.layer) {
            return None;
        }
        self.rows.iter().find_map(|(row, rect)| {
            if !rect.contains(pos) {
                return None;
            }
            let col = ((pos.x - rect.min.x) / self.cell_width).floor() as usize;
            (col < self.cols).then_some((col, *row))
        })
    }
}
#[derive(Clone, Default)]
pub(super) struct Router {
    encoder: MouseEncoder,
    wheel: f32,
    pointer: Option<Pos2>,
    mode: MouseMode,
}
fn modifiers(m: Modifiers) -> MouseModifiers {
    MouseModifiers {
        shift: m.shift,
        alt: m.alt,
        ctrl: m.ctrl,
    }
}
fn button(b: PointerButton) -> Option<MouseButton> {
    match b {
        PointerButton::Primary => Some(MouseButton::Left),
        PointerButton::Middle => Some(MouseButton::Middle),
        PointerButton::Secondary => Some(MouseButton::Right),
        _ => None,
    }
}
impl Router {
    pub fn suspend(&mut self, mode: MouseMode) -> Vec<u8> {
        self.wheel = 0.;
        self.encoder.release_all(mode).unwrap_or_default()
    }

    pub fn route(
        &mut self,
        ctx: &Context,
        mode: MouseMode,
        viewport: &Viewport,
        active: bool,
        events: &[Event],
        current_modifiers: Modifiers,
    ) -> Vec<u8> {
        let mut output = Vec::new();
        if mode != self.mode {
            self.encoder.reset();
            self.wheel = 0.;
            self.mode = mode;
        }
        if !active || !mode.enabled() || current_modifiers.shift {
            if mode.enabled() {
                if let Some(bytes) = self.encoder.release_all(mode) {
                    output.extend(bytes);
                }
            } else {
                self.encoder.reset();
            }
            self.wheel = 0.;
            return output;
        }
        if self.pointer.is_none() {
            self.pointer = ctx.input(|i| i.pointer.latest_pos());
        }
        let mut wheel_budget = 32u32;
        for event in events {
            let mut emit = |encoder: &mut MouseEncoder, action, col, row, m| {
                if let Some(bytes) = encoder.encode(
                    mode,
                    TerminalMouseEvent {
                        action,
                        col,
                        row,
                        modifiers: modifiers(m),
                    },
                ) {
                    output.extend(bytes);
                }
            };
            match *event {
                Event::PointerMoved(pos) => {
                    self.pointer = Some(pos);
                    if let Some((col, row)) = viewport.cell(ctx, pos) {
                        emit(
                            &mut self.encoder,
                            MouseAction::Move,
                            col,
                            row,
                            current_modifiers,
                        );
                    }
                }
                Event::PointerButton {
                    pos,
                    button: b,
                    pressed,
                    modifiers: m,
                } => {
                    self.pointer = Some(pos);
                    let Some(b) = button(b) else {
                        continue;
                    };
                    if m.shift {
                        if let Some(bytes) = self.encoder.release_all(mode) {
                            output.extend(bytes);
                        }
                        continue;
                    }
                    let cell = viewport
                        .cell(ctx, pos)
                        .or_else(|| (!pressed).then(|| self.encoder.position()).flatten());
                    if let Some((col, row)) = cell {
                        emit(
                            &mut self.encoder,
                            if pressed {
                                MouseAction::Press(b)
                            } else {
                                MouseAction::Release(b)
                            },
                            col,
                            row,
                            m,
                        );
                    }
                }
                Event::MouseWheel {
                    unit,
                    delta,
                    modifiers: m,
                    ..
                } if !m.shift => {
                    let Some((col, row)) = self.pointer.and_then(|pos| viewport.cell(ctx, pos))
                    else {
                        self.wheel = 0.;
                        continue;
                    };
                    let lines = match unit {
                        MouseWheelUnit::Point => {
                            delta.y
                                / viewport
                                    .rows
                                    .first()
                                    .map(|(_, r)| r.height())
                                    .unwrap_or(18.)
                                    .max(1.)
                        }
                        MouseWheelUnit::Line => delta.y,
                        MouseWheelUnit::Page => delta.y * viewport.rows.len() as f32,
                    };
                    if !lines.is_finite() {
                        continue;
                    }
                    self.wheel = (self.wheel + lines).clamp(-32., 32.);
                    let steps = (self.wheel.trunc() as i32)
                        .clamp(-(wheel_budget as i32), wheel_budget as i32);
                    wheel_budget -= steps.unsigned_abs();
                    self.wheel = if wheel_budget == 0 {
                        0.
                    } else {
                        self.wheel - steps as f32
                    };
                    for _ in 0..steps.unsigned_abs() {
                        emit(
                            &mut self.encoder,
                            if steps > 0 {
                                MouseAction::WheelUp
                            } else {
                                MouseAction::WheelDown
                            },
                            col,
                            row,
                            m,
                        );
                    }
                }
                Event::PointerGone => {
                    self.pointer = None;
                    self.wheel = 0.;
                    if let Some(bytes) = self.encoder.release_all(mode) {
                        output.extend(bytes);
                    }
                }
                _ => {}
            }
            if output.len() >= 16384 {
                if let Some(bytes) = self.encoder.release_all(mode) {
                    output.extend(bytes);
                }
                break;
            }
        }
        output
    }
}
pub(super) struct Pending {
    bytes: Vec<u8>,
    after: Router,
    mode: MouseMode,
}
impl super::TerminalTab {
    pub(super) fn retry_mouse(&mut self) {
        let Some(pending) = self.pending_mouse.take() else {
            return;
        };
        if !self.ready_for_input() || pending.mode != self.screen.mouse_mode() {
            self.mouse = Router::default();
            return;
        }
        match self.transmit(&pending.bytes, false) {
            Ok(()) => self.mouse = pending.after,
            Err(error) if harbor_workspace::terminal::paste::retryable_input_error(&error) => {
                self.pending_mouse = Some(pending);
                self.error = error;
            }
            Err(error) => {
                self.error = error;
                self.mouse = Router::default();
            }
        }
    }
    pub(super) fn route_mouse(
        &mut self,
        ctx: &Context,
        viewport: &Viewport,
        active: bool,
        events: &[Event],
        modifiers: Modifiers,
    ) {
        self.retry_mouse();
        if self.pending_mouse.is_some() || self.paste.is_some() || self.paste_end.is_some() {
            return;
        }
        if !self.ready_for_input() {
            self.mouse = Router::default();
            return;
        }
        let mut after = self.mouse.clone();
        let mode = self.screen.mouse_mode();
        let bytes = after.route(ctx, mode, viewport, active, events, modifiers);
        if bytes.is_empty() {
            self.mouse = after;
            return;
        }
        match self.transmit(&bytes, false) {
            Ok(()) => self.mouse = after,
            Err(error) if harbor_workspace::terminal::paste::retryable_input_error(&error) => {
                self.pending_mouse = Some(Pending { bytes, after, mode });
                self.error = error;
            }
            Err(error) => {
                self.error = error;
                self.mouse = Router::default();
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui;
    use harbor_workspace::terminal::mouse::MouseTracking;
    fn wheel() -> Event {
        Event::MouseWheel {
            unit: MouseWheelUnit::Line,
            delta: egui::vec2(0., 1.),
            phase: egui::TouchPhase::Move,
            modifiers: Modifiers::default(),
        }
    }
    #[test]
    fn wheel_work_is_bounded_per_frame_and_small_deltas_accumulate() {
        let ctx = Context::default();
        let mut router = Router::default();
        let mode = MouseMode {
            tracking: MouseTracking::Press,
            sgr: true,
        };
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_max(Pos2::ZERO, egui::pos2(800., 600.))),
                ..Default::default()
            },
            |ui| {
                ui.allocate_space(egui::vec2(800., 600.));
                let viewport = Viewport {
                    clip: Rect::from_min_max(egui::pos2(200., 100.), egui::pos2(500., 400.)),
                    rows: vec![(
                        0,
                        Rect::from_min_max(egui::pos2(200., 120.), egui::pos2(500., 140.)),
                    )],
                    cell_width: 10.,
                    cols: 30,
                    layer: ui.layer_id(),
                };
                let mut events = vec![Event::PointerMoved(egui::pos2(205., 130.))];
                events.extend((0..100).map(|_| wheel()));
                let bytes =
                    router.route(&ctx, mode, &viewport, true, &events, Modifiers::default());
                assert_eq!(bytes.windows(3).filter(|b| *b == b"\x1b[<").count(), 32);
                for _ in 0..3 {
                    assert!(
                        router
                            .route(
                                &ctx,
                                mode,
                                &viewport,
                                true,
                                &[Event::MouseWheel {
                                    unit: MouseWheelUnit::Point,
                                    delta: egui::vec2(0., 5.),
                                    phase: egui::TouchPhase::Move,
                                    modifiers: Modifiers::default()
                                }],
                                Modifiers::default()
                            )
                            .is_empty()
                    );
                }
                assert_eq!(
                    router.route(
                        &ctx,
                        mode,
                        &viewport,
                        true,
                        &[Event::MouseWheel {
                            unit: MouseWheelUnit::Point,
                            delta: egui::vec2(0., 5.),
                            phase: egui::TouchPhase::Move,
                            modifiers: Modifiers::default()
                        }],
                        Modifiers::default()
                    ),
                    b"\x1b[<64;1;1M"
                );
            },
        );
        out.textures_delta.clear();
    }
    #[test]
    fn sidebar_history_clip_scroll_offsets_focus_and_shift_gate_mouse() {
        let ctx = Context::default();
        let mut router = Router::default();
        let mode = MouseMode {
            tracking: MouseTracking::Press,
            sgr: true,
        };
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_max(Pos2::ZERO, egui::pos2(800., 600.))),
                ..Default::default()
            },
            |ui| {
                ui.allocate_space(egui::vec2(800., 600.));
                let viewport = Viewport {
                    clip: Rect::from_min_max(egui::pos2(200., 100.), egui::pos2(500., 400.)),
                    rows: vec![(
                        7,
                        Rect::from_min_max(egui::pos2(180., 120.), egui::pos2(580., 140.)),
                    )],
                    cell_width: 10.,
                    cols: 40,
                    layer: ui.layer_id(),
                };
                let m = Modifiers::default();
                for pos in [
                    egui::pos2(50., 130.),
                    egui::pos2(210., 110.),
                    egui::pos2(510., 130.),
                ] {
                    assert!(
                        router
                            .route(
                                &ctx,
                                mode,
                                &viewport,
                                true,
                                &[Event::PointerMoved(pos), wheel()],
                                m
                            )
                            .is_empty()
                    );
                }
                let pos = egui::pos2(205., 130.);
                assert_eq!(
                    router.route(
                        &ctx,
                        mode,
                        &viewport,
                        true,
                        &[Event::PointerMoved(pos), wheel()],
                        m
                    ),
                    b"\x1b[<64;3;8M"
                );
                assert!(
                    router
                        .route(&ctx, mode, &viewport, false, &[wheel()], m)
                        .is_empty()
                );
                assert!(
                    router
                        .route(
                            &ctx,
                            mode,
                            &viewport,
                            true,
                            &[wheel()],
                            Modifiers { shift: true, ..m }
                        )
                        .is_empty()
                );
            },
        );
        out.textures_delta.clear();
    }
}
