use super::*;
mod play;
mod save;

pub(super) struct State {
    pub play: play::PlayState,
    pub idle: harbor_workspace::games::screensaver::ScreensaverState,
    last: Instant,
    seconds: u64,
    wake_owner: Option<ViewportId>,
    blurred_callbacks: std::collections::BTreeSet<ViewportId>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            play: Default::default(),
            idle: Default::default(),
            last: Instant::now(),
            seconds: 300,
            wake_owner: None,
            blurred_callbacks: Default::default(),
        }
    }
}
impl Workspace {
    pub(super) fn screensaver_settings(&mut self, ui: &mut Ui) {
        let mut enabled = self.games.idle.timeout().is_some();
        let changed = ui
            .checkbox(&mut enabled, "Idle screen for this app session")
            .changed();
        let timeout_changed = ui
            .add_enabled(
                enabled,
                egui::Slider::new(&mut self.games.seconds, 30..=3600).text("Idle seconds"),
            )
            .changed();
        if changed || timeout_changed {
            self.games
                .idle
                .configure(enabled.then(|| Duration::from_secs(self.games.seconds)));
            self.games.last = Instant::now();
            self.games.wake_owner = None;
        }
    }

    /// A blocked frame is exclusively an idle-screen frame, including the wakeup
    /// frame. Neither game keys nor wakeup events reach hidden terminal controls.
    pub(super) fn screensaver_frame(&mut self, ui: &mut Ui) -> bool {
        if self.games.play.is_open() && !self.games.idle.active() && self.games.wake_owner.is_none()
        {
            self.games.idle.on_input();
            self.games.last = Instant::now();
            return false;
        }
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.games.last);
        self.games.last = now;
        let input = ui.ctx().input(|input| {
            input.viewport().close_requested()
                || input.events.iter().any(|event| {
                    matches!(
                        event,
                        Event::Key { .. }
                            | Event::ModifiersChanged(_)
                            | Event::Text(_)
                            | Event::Ime(_)
                            | Event::MouseMoved(_)
                            | Event::Paste(_)
                            | Event::Copy
                            | Event::Cut
                            | Event::PointerMoved(_)
                            | Event::PointerButton { .. }
                            | Event::MouseWheel { .. }
                            | Event::Touch { .. }
                            | Event::Zoom(_)
                            | Event::Rotate(_)
                    )
                })
        });
        let live_viewports = ui.ctx().input(|state| {
            state
                .raw
                .viewports
                .iter()
                .map(|(&id, info)| (id, info.focused))
                .collect::<Vec<_>>()
        });
        let current = ui.ctx().viewport_id();
        self.games
            .blurred_callbacks
            .retain(|id| live_viewports.iter().any(|(live, _)| live == id));
        let (callback_blurred, callback_focused) = ui.ctx().input(|state| {
            let loss = state
                .events
                .iter()
                .any(|event| matches!(event, Event::WindowFocused(false)));
            (
                !state.raw.focused || loss,
                state.raw.focused
                    && !loss
                    && state
                        .raw
                        .viewports
                        .get(&current)
                        .and_then(|info| info.focused)
                        != Some(false),
            )
        });
        let mut first_blur_callback = false;
        if !self.games.idle.active() {
            if callback_blurred {
                // The owner's actual callback must pass through existing native
                // handlers before its stale blurred pointer may be ignored.
                first_blur_callback = self.games.blurred_callbacks.insert(current);
            } else if callback_focused {
                self.games.blurred_callbacks.remove(&current);
            }
        }
        let held_before_idle = live_viewports.iter().any(|&(id, _)| {
            !self.games.blurred_callbacks.contains(&id)
                && ui.ctx().input_for(id, |state| {
                    !state.keys_down.is_empty() || state.modifiers.any() || state.pointer.any_down()
                })
        });
        let lifecycle_input = ui.ctx().input(|state| {
            state.events.iter().any(|event| {
                matches!(
                    event,
                    Event::WindowFocused(false)
                        | Event::PointerGone
                        | Event::Touch {
                            phase: TouchPhase::Cancel,
                            ..
                        }
                )
            })
        });
        if !self.games.idle.active()
            && (input || held_before_idle || lifecycle_input || first_blur_callback)
        {
            // Ordinary input and gestures already owned by terminals/native
            // desktops must finish there. Never activate idle midway through a
            // held key/button, even without repeat events in another viewport.
            self.games.idle.on_input();
        } else {
            self.games.idle.tick(elapsed);
        }
        let waking = self.games.wake_owner.is_some();
        let mut owner_missing = false;
        let mut owner_unfocused = false;
        let mut pointer_canceled = false;
        if let Some(owner) = self.games.wake_owner {
            ui.ctx().input(|state| {
                // eframe supplies the complete live viewport map to each pass.
                owner_missing = !state.raw.viewports.contains_key(&owner);
                owner_unfocused = state
                    .raw
                    .viewports
                    .get(&owner)
                    .is_some_and(|info| info.focused == Some(false));
                if owner == current {
                    owner_unfocused |= !state.raw.focused
                        || state
                            .events
                            .iter()
                            .any(|event| matches!(event, Event::WindowFocused(false)));
                    pointer_canceled = state.events.iter().any(|event| {
                        matches!(
                            event,
                            Event::PointerGone
                                | Event::Touch {
                                    phase: TouchPhase::Cancel,
                                    ..
                                }
                        )
                    });
                }
            });
            if owner_missing || owner_unfocused {
                self.games.wake_owner = None;
            }
            if owner == current && (owner_unfocused || pointer_canceled) {
                ui.ctx().input_mut(|state| {
                    // PointerGone intentionally retains egui's pressed buttons;
                    // cancellation of a swallowed wake gesture must not do so.
                    state.pointer = Default::default();
                    if owner_unfocused {
                        state.keys_down.clear();
                        state.modifiers = Modifiers::default();
                    }
                });
            }
        }
        let recovered = owner_missing || owner_unfocused;
        let dismissed = input && self.games.idle.on_input();
        if dismissed || waking {
            // Consume repeats and release too, so a held wakeup Enter cannot
            // execute terminal commands after its initial press was swallowed.
            let owner = ui.ctx().viewport_id();
            if !recovered && (dismissed || self.games.wake_owner == Some(owner)) {
                let held = ui.ctx().input(|input| {
                    !input.keys_down.is_empty() || input.modifiers.any() || input.pointer.any_down()
                });
                self.games.wake_owner = held.then_some(owner);
            }
            ui.ctx().input_mut(|input| input.events.clear());
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        if !self.games.idle.active() && !dismissed && !waking {
            if let Some(timeout) = self.games.idle.timeout() {
                ui.ctx()
                    .request_repaint_after(timeout.min(Duration::from_secs(1)));
            }
            return false;
        }
        let rect = ui.available_rect_before_wrap();
        ui.painter().rect_filled(rect, 0., content(ui));
        let center = rect.center();
        let logo = Rect::from_center_size(center - vec2(0., 42.), vec2(96., 96.));
        ui.painter().image(
            self.logo.id(),
            logo,
            Rect::from_min_max(Pos2::ZERO, pos2(1., 1.)),
            Color32::WHITE,
        );
        ui.painter().text(
            center + vec2(0., 42.),
            Align2::CENTER_CENTER,
            "Press a key or click to return",
            FontId::proportional(18.),
            ink(ui),
        );
        ui.ctx().request_repaint_after(Duration::from_millis(100));
        true
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    use crate::ui::terminal_review_tests::{captured, owned_workspace, raw_terminal};

    #[test]
    fn disabled_screensaver_preserves_ordinary_input_frame() {
        let (ctx, _dir, mut w) = owned_workspace();
        let mut blocked = true;
        let mut output = ctx.run_ui(
            RawInput {
                events: vec![Event::Text("normal".into())],
                ..Default::default()
            },
            |ui| {
                blocked = w.screensaver_frame(ui);
            },
        );
        output.textures_delta.clear();
        assert!(!blocked);
        assert!(ctx.input(|input| {
            input
                .events
                .iter()
                .any(|event| matches!(event, Event::Text(text) if text == "normal"))
        }));
    }

    #[test]
    fn ime_and_raw_mouse_input_dismiss_the_idle_screen() {
        let (ctx, _dir, mut w) = owned_workspace();
        for event in [
            Event::Ime(egui::ImeEvent::Commit("wake".into())),
            Event::MouseMoved(vec2(1., 0.)),
        ] {
            w.games.idle.configure(Some(Duration::from_secs(1)));
            w.games.idle.tick(Duration::from_secs(1));
            let mut blocked = false;
            let mut output = ctx.run_ui(
                RawInput {
                    events: vec![event],
                    ..Default::default()
                },
                |ui| {
                    blocked = w.screensaver_frame(ui);
                },
            );
            output.textures_delta.clear();
            assert!(blocked);
            assert!(!w.games.idle.active());
            assert!(ctx.input(|input| input.events.is_empty()));
        }
    }

    #[test]
    fn wakeup_key_repeats_are_consumed_until_release() {
        let (ctx, _dir, mut w) = owned_workspace();
        w.games.idle.configure(Some(Duration::from_secs(1)));
        w.games.idle.tick(Duration::from_secs(1));
        for (pressed, repeat) in [(true, false), (true, true), (false, false)] {
            let mut blocked = false;
            let mut output = ctx.run_ui(
                RawInput {
                    events: vec![Event::Key {
                        key: Key::Enter,
                        physical_key: None,
                        pressed,
                        repeat,
                        modifiers: Modifiers::default(),
                    }],
                    ..Default::default()
                },
                |ui| {
                    blocked = w.screensaver_frame(ui);
                },
            );
            output.textures_delta.clear();
            assert!(
                blocked,
                "the full wakeup press/repeat/release must be consumed"
            );
            assert!(ctx.input(|input| input.events.is_empty()));
        }
        let mut blocked = true;
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            blocked = w.screensaver_frame(ui);
        });
        output.textures_delta.clear();
        assert!(
            !blocked,
            "normal input resumes after the wakeup key is released"
        );
    }

    #[test]
    fn screensaver_dismisses_without_losing_terminal_input() {
        assert_wake_preserves_session(false);
    }

    #[test]
    fn screensaver_dismissal_in_detached_viewport_preserves_terminal() {
        assert_wake_preserves_session(true);
    }

    fn assert_wake_preserves_session(detached: bool) {
        let (ctx, _dir, mut w) = owned_workspace();
        let id = raw_terminal(&mut w, "owned-saver");
        let process = w.tabs[0].process.as_ref().unwrap() as *const Process;
        w.tabs[0].send(b"before-idle");
        w.games.idle.configure(Some(Duration::from_secs(1)));
        w.games.idle.tick(Duration::from_secs(1));
        if detached {
            w.detach_terminal(id);
        }
        ctx.memory_mut(|memory| memory.request_focus(Id::new(("terminal-input", id))));
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1320., 900.))),
                events: vec![Event::Text("do-not-broadcast".into())],
                ..Default::default()
            },
            |ui| {
                if detached {
                    w.detached_windows(ui.ctx());
                } else {
                    w.render(ui);
                }
            },
        );
        output.textures_delta.clear();
        assert!(!w.games.idle.active());
        assert_eq!(ctx.input(|input| input.events.len()), 0);
        assert_eq!(w.tabs[0].id, id);
        assert_eq!(
            w.tabs[0].process.as_ref().unwrap() as *const Process,
            process
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        while captured(&w.tabs[0]).len() < b"before-idle".len() && Instant::now() < deadline {
            w.tabs[0].poll();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(captured(&w.tabs[0]), b"before-idle");
        w.tabs[0].send(b"after-wake");
        let deadline = Instant::now() + Duration::from_secs(2);
        while captured(&w.tabs[0]).len() < b"before-idleafter-wake".len()
            && Instant::now() < deadline
        {
            w.tabs[0].poll();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(captured(&w.tabs[0]), b"before-idleafter-wake");
    }
}

#[cfg(all(test, unix))]
mod lifecycle_tests {
    use super::*;

    use crate::ui::terminal_review_tests::{captured, owned_workspace, raw_terminal};

    fn touch(phase: TouchPhase) -> Event {
        Event::Touch {
            device_id: TouchDeviceId(1),
            id: TouchId(1),
            phase,
            pos: pos2(30., 30.),
            force: None,
        }
    }
    fn press() -> Vec<Event> {
        vec![
            touch(TouchPhase::Start),
            Event::PointerMoved(pos2(30., 30.)),
            Event::PointerButton {
                pos: pos2(30., 30.),
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::default(),
            },
        ]
    }
    fn frame(ctx: &Context, w: &mut Workspace, raw: RawInput) -> (bool, egui::FullOutput) {
        let mut blocked = false;
        let live_children = raw
            .viewports
            .keys()
            .copied()
            .filter(|id| *id != ViewportId::ROOT)
            .collect::<Vec<_>>();
        ctx.set_embed_viewports(false);
        let mut output = ctx.run_ui(raw, |ui| {
            // Keep declared child input states alive across root passes, as the
            // native backend does when Workspace renders detached viewports.
            if ui.ctx().viewport_id() == ViewportId::ROOT {
                for &child in &live_children {
                    ui.ctx()
                        .show_viewport_deferred(child, ViewportBuilder::default(), |_, _| {});
                }
            }
            blocked = w.screensaver_frame(ui);
        });
        output.textures_delta.clear();
        (blocked, output)
    }
    fn discard(mut out: egui::FullOutput) {
        out.textures_delta.clear();
    }
    fn arm(w: &mut Workspace) {
        w.games.idle.configure(Some(Duration::from_secs(1)));
        w.games.idle.tick(Duration::from_secs(1));
    }

    #[test]
    fn preidle_root_and_detached_held_inputs_without_repeat_prevent_activation() {
        for owner in [ViewportId::ROOT, ViewportId::from_hash_of("preidle-child")] {
            for pointer in [false, true] {
                let (ctx, _dir, mut w) = owned_workspace();
                w.games.idle.configure(Some(Duration::from_secs(1)));
                let mut raw = RawInput {
                    viewport_id: owner,
                    events: if pointer {
                        press()
                    } else {
                        vec![Event::Key {
                            key: Key::Enter,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: Modifiers::default(),
                        }]
                    },
                    ..Default::default()
                };
                raw.viewports.insert(
                    owner,
                    egui::ViewportInfo {
                        focused: Some(true),
                        ..Default::default()
                    },
                );
                assert!(!frame(&ctx, &mut w, raw.clone()).0);
                assert!(
                    w.games.wake_owner.is_none(),
                    "pre-idle input belongs to the terminal, not the wake guard"
                );
                raw.viewport_id = ViewportId::ROOT;
                raw.events.clear();
                w.games.last = Instant::now() - Duration::from_secs(2);
                assert!(
                    !frame(&ctx, &mut w, raw.clone()).0,
                    "held root/detached input must prevent idle activation without repeats"
                );
                assert!(!w.games.idle.active());
                assert!(ctx.input_for(owner, |i| if pointer {
                    i.pointer.any_down()
                } else {
                    i.keys_down.contains(&Key::Enter)
                }));
                raw.viewport_id = owner;
                raw.events = if pointer {
                    vec![Event::PointerButton {
                        pos: pos2(30., 30.),
                        button: PointerButton::Primary,
                        pressed: false,
                        modifiers: Modifiers::default(),
                    }]
                } else {
                    vec![Event::Key {
                        key: Key::Enter,
                        physical_key: None,
                        pressed: false,
                        repeat: false,
                        modifiers: Modifiers::default(),
                    }]
                };
                assert!(
                    !frame(&ctx, &mut w, raw.clone()).0,
                    "ordinary pre-idle release must reach existing handlers"
                );
                assert!(ctx.input_for(owner, |i| !i.events.is_empty()));
                raw.viewport_id = ViewportId::ROOT;
                raw.events.clear();
                w.games.last = Instant::now() - Duration::from_secs(2);
                assert!(frame(&ctx, &mut w, raw).0);
                assert!(
                    w.games.idle.active(),
                    "idle can begin once all live gestures are released"
                );
            }
        }
    }

    #[test]
    fn preidle_held_modifier_without_repeat_prevents_activation() {
        let (ctx, _dir, mut w) = owned_workspace();
        w.games.idle.configure(Some(Duration::from_secs(1)));
        let mut raw = RawInput {
            events: vec![Event::ModifiersChanged(Modifiers {
                ctrl: true,
                ..Default::default()
            })],
            ..Default::default()
        };
        assert!(!frame(&ctx, &mut w, raw.clone()).0);
        raw.events.clear();
        w.games.last = Instant::now() - Duration::from_secs(2);
        assert!(!frame(&ctx, &mut w, raw.clone()).0);
        assert!(!w.games.idle.active());
        raw.events = vec![Event::ModifiersChanged(Modifiers::default())];
        assert!(!frame(&ctx, &mut w, raw.clone()).0);
        raw.events.clear();
        w.games.last = Instant::now() - Duration::from_secs(2);
        assert!(frame(&ctx, &mut w, raw).0);
    }

    #[test]
    fn ordinary_input_at_idle_threshold_is_preserved_before_activation() {
        let (ctx, _dir, mut w) = owned_workspace();
        w.games.idle.configure(Some(Duration::from_secs(1)));
        w.games.last = Instant::now() - Duration::from_secs(2);
        assert!(
            !frame(
                &ctx,
                &mut w,
                RawInput {
                    events: vec![Event::Text("terminal-owned".into())],
                    ..Default::default()
                }
            )
            .0
        );
        assert!(!w.games.idle.active());
        assert!(ctx.input(|i| {
            i.events
                .iter()
                .any(|e| matches!(e, Event::Text(t) if t == "terminal-owned"))
        }));
    }

    #[test]
    fn preidle_blur_frame_is_preserved_and_stale_removed_or_blurred_pointer_does_not_inhibit() {
        for removed in [false, true] {
            let (ctx, _dir, mut w) = owned_workspace();
            let child = ViewportId::from_hash_of("blurred-preidle-child");
            w.games.idle.configure(Some(Duration::from_secs(1)));
            let mut raw = RawInput {
                viewport_id: child,
                events: press(),
                ..Default::default()
            };
            raw.viewports.insert(
                child,
                egui::ViewportInfo {
                    focused: Some(true),
                    ..Default::default()
                },
            );
            assert!(!frame(&ctx, &mut w, raw.clone()).0);
            // Backend inventory may report blur before the child's queued
            // focus-loss callback has reached its native input handlers.
            let mut parent = raw.clone();
            parent.viewport_id = ViewportId::ROOT;
            parent.events.clear();
            parent.viewports.get_mut(&child).unwrap().focused = Some(false);
            w.games.last = Instant::now() - Duration::from_secs(2);
            assert!(
                !frame(&ctx, &mut w, parent).0,
                "held child input must inhibit idle until its actual blur callback is allowed through"
            );
            assert!(!w.games.idle.active());
            raw.events = vec![Event::WindowFocused(false)];
            raw.focused = false;
            raw.viewports.get_mut(&child).unwrap().focused = Some(false);
            w.games.last = Instant::now() - Duration::from_secs(2);
            assert!(
                !frame(&ctx, &mut w, raw.clone()).0,
                "blur must reach the existing native input handler before idle can start"
            );
            assert!(ctx.input_for(child, |i| {
                i.events
                    .iter()
                    .any(|e| matches!(e, Event::WindowFocused(false)))
            }));
            assert!(
                ctx.input_for(child, |i| i.pointer.any_down()),
                "pre-idle pointer state must not be cleared by the idle screen"
            );
            raw.viewport_id = ViewportId::ROOT;
            raw.focused = true;
            raw.events.clear();
            if removed {
                raw.viewports.remove(&child);
            }
            w.games.last = Instant::now() - Duration::from_secs(2);
            assert!(frame(&ctx, &mut w, raw).0);
            assert!(
                w.games.idle.active(),
                "no permanent stale-pointer idle inhibitor after blur/removal"
            );
        }
    }

    #[test]
    fn canceled_touch_and_focus_loss_recover_stale_pointer_without_replaying_input() {
        for cancel in [
            vec![touch(TouchPhase::Cancel), Event::PointerGone],
            vec![Event::WindowFocused(false)],
        ] {
            let (ctx, _dir, mut w) = owned_workspace();
            arm(&mut w);
            assert!(
                frame(
                    &ctx,
                    &mut w,
                    RawInput {
                        events: press(),
                        ..Default::default()
                    }
                )
                .0
            );
            assert_eq!(w.games.wake_owner, Some(ViewportId::ROOT));
            let mut dependency_keeps_pointer_down = false;
            let mut blocked = false;
            let mut output = ctx.run_ui(
                RawInput {
                    events: cancel,
                    ..Default::default()
                },
                |ui| {
                    dependency_keeps_pointer_down = ui.ctx().input(|i| i.pointer.any_down());
                    blocked = w.screensaver_frame(ui);
                },
            );
            output.textures_delta.clear();
            assert!(
                dependency_keeps_pointer_down,
                "pinned egui retains the canceled pointer button"
            );
            assert!(blocked, "recovery frame must be swallowed");
            assert!(
                w.games.wake_owner.is_none(),
                "cancel/focus loss must release the stale wake guard"
            );
            assert!(ctx.input(|i| i.events.is_empty()));
            assert!(!ctx.input(|i| i.pointer.any_down()));
            assert!(
                !frame(
                    &ctx,
                    &mut w,
                    RawInput {
                        events: vec![Event::Text("next".into())],
                        ..Default::default()
                    }
                )
                .0
            );
            assert!(ctx.input(|i| {
                i.events
                    .iter()
                    .any(|e| matches!(e, Event::Text(t) if t == "next"))
            }));
        }
    }

    #[test]
    fn unrelated_viewport_cannot_release_guard_but_missing_owner_recovers() {
        let (ctx, _dir, mut w) = owned_workspace();
        let child = ViewportId::from_hash_of("owned-wake-child");
        arm(&mut w);
        let mut raw = RawInput {
            viewport_id: child,
            events: press(),
            ..Default::default()
        };
        raw.viewports.insert(
            child,
            egui::ViewportInfo {
                parent: Some(ViewportId::ROOT),
                focused: Some(true),
                ..Default::default()
            },
        );
        assert!(frame(&ctx, &mut w, raw).0);
        assert_eq!(w.games.wake_owner, Some(child));
        let mut raw = RawInput::default();
        raw.viewports.insert(
            child,
            egui::ViewportInfo {
                focused: Some(true),
                ..Default::default()
            },
        );
        assert!(frame(&ctx, &mut w, raw).0);
        assert_eq!(
            w.games.wake_owner,
            Some(child),
            "another viewport's empty pointer state must not release it"
        );
        assert!(
            frame(
                &ctx,
                &mut w,
                RawInput {
                    events: vec![Event::Text("recovery-only".into())],
                    ..Default::default()
                }
            )
            .0
        );
        assert!(
            w.games.wake_owner.is_none(),
            "removed owner must not lock all surviving viewports"
        );
        assert!(ctx.input(|i| i.events.is_empty()));
        assert!(!frame(&ctx, &mut w, RawInput::default()).0);
    }

    #[test]
    fn held_wakeup_guard_waits_with_bounded_repaint() {
        let (ctx, _dir, mut w) = owned_workspace();
        arm(&mut w);
        frame(
            &ctx,
            &mut w,
            RawInput {
                events: press(),
                ..Default::default()
            },
        );
        let mut delay = Duration::ZERO;
        for _ in 0..5 {
            let (blocked, out) = frame(&ctx, &mut w, RawInput::default());
            assert!(blocked);
            delay = out.viewport_output[&ViewportId::ROOT].repaint_delay;
        }
        assert!(
            delay >= Duration::from_millis(50),
            "waiting must not request an immediate repaint loop: {delay:?}"
        );
    }

    #[test]
    fn immediate_detached_close_is_canceled_and_queued_while_idle_with_owned_pty() {
        assert_immediate_close_preserves_pty(false);
        assert_immediate_close_preserves_pty(true);
    }

    fn assert_immediate_close_preserves_pty(held_wake: bool) {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        let (ctx, _dir, mut w) = owned_workspace();
        let id = raw_terminal(&mut w, "lifecycle-owned");
        let process = w.tabs[0].process.as_ref().unwrap() as *const Process;
        w.tabs[0].send(b"before-close");
        w.detach_terminal(id);
        let window = w.windows.window(id);
        let child = super::super::windows::viewport_id(window);
        arm(&mut w);
        if held_wake {
            let mut raw = RawInput {
                events: press(),
                ..Default::default()
            };
            raw.viewports.insert(child, egui::ViewportInfo::default());
            assert!(frame(&ctx, &mut w, raw).0);
        }
        let cancel_close = Arc::new(AtomicBool::new(false));
        let confirmation_visible = Arc::new(AtomicBool::new(false));
        let close_once = Arc::new(AtomicBool::new(true));
        let visible = confirmation_visible.clone();
        ctx.set_embed_viewports(false);
        Context::set_immediate_viewport_renderer(move |ctx, mut viewport| {
            let mut raw = RawInput {
                viewport_id: viewport.ids.this,
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(960., 640.))),
                ..Default::default()
            };
            raw.viewports.insert(
                viewport.ids.this,
                egui::ViewportInfo {
                    parent: Some(viewport.ids.parent),
                    events: if close_once.swap(false, Ordering::SeqCst) {
                        vec![egui::ViewportEvent::Close]
                    } else {
                        vec![]
                    },
                    ..Default::default()
                },
            );
            let mut out = ctx.run_ui(raw, |ui| (viewport.viewport_ui_cb)(ui));
            out.textures_delta.clear();
            if out.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.job.text.contains("Reattach these terminals"))) {
                visible.store(true, Ordering::SeqCst);
            }
        });
        let mut raw = RawInput::default();
        raw.viewports.insert(child, egui::ViewportInfo::default());
        let mut first = ctx.run_ui(raw.clone(), |ui| w.detached_windows(ui.ctx()));
        // Nested immediate output defers viewport commands to the outermost pass.
        cancel_close.store(
            first.viewport_output[&child]
                .commands
                .iter()
                .any(|command| matches!(command, ViewportCommand::CancelClose)),
            Ordering::SeqCst,
        );
        first.textures_delta.clear();
        assert!(
            cancel_close.load(Ordering::SeqCst),
            "nonembedded Close must always be intercepted before idle gate; pending={}",
            w.window_close_prompt.contains(&window)
        );
        assert!(w.window_close_prompt.contains(&window));
        if held_wake {
            assert_eq!(
                w.games.wake_owner,
                Some(ViewportId::ROOT),
                "child Close must not release another viewport's held wake guard"
            );
            raw.events = vec![Event::PointerButton {
                pos: pos2(30., 30.),
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::default(),
            }];
        } else {
            assert!(
                !w.games.idle.active(),
                "native Close counts as activity; confirmation needs no extra wake input"
            );
        }
        discard(ctx.run_ui(raw.clone(), |ui| w.render(ui)));
        raw.events.clear();
        discard(ctx.run_ui(raw, |ui| w.detached_windows(ui.ctx())));
        assert!(confirmation_visible.load(Ordering::SeqCst));
        assert_eq!(
            w.tabs[0].process.as_ref().unwrap() as *const Process,
            process
        );
        assert_eq!(w.tabs[0].id, id);
        let deadline = Instant::now() + Duration::from_secs(2);
        while captured(&w.tabs[0]).len() < b"before-close".len() && Instant::now() < deadline {
            w.tabs[0].poll();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(captured(&w.tabs[0]), b"before-close");
        w.close_tab(0);
        assert!(
            w.pending_session_close.is_some(),
            "existing active-session confirmation remains required"
        );
        w.resolve_session_close(false, &ctx);
        assert_eq!(
            w.tabs[0].process.as_ref().unwrap() as *const Process,
            process
        );
    }
}
