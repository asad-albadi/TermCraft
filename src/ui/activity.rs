use super::*;
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::ui::terminal_review_tests::{owned_workspace, raw_terminal};
    fn paint(ctx: &Context, w: &mut Workspace) {
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1400., 950.))),
                ..Default::default()
            },
            |ui| w.render(ui),
        );
        output.textures_delta.clear();
    }
    #[test]
    fn real_background_output_is_unread_until_painted_and_detached_output_is_seen() {
        let (ctx, _root, mut w) = owned_workspace();
        let a = raw_terminal(&mut w, "A");
        let b = raw_terminal(&mut w, "B");
        w.active = 0;
        w.set_layout_preset(harbor_workspace::workspace::layout::LayoutPreset::Single);
        paint(&ctx, &mut w);
        w.activity.mark_seen(b);
        w.tabs[1].send(b"background");
        let deadline = Instant::now() + Duration::from_secs(3);
        while !w.activity.unread(b) && Instant::now() < deadline {
            paint(&ctx, &mut w);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(w.activity.unread(b));
        assert!(!w.activity.unread(a));
        assert!(w.terminal_tab_label(b).contains("new output"));
        w.active = 1;
        w.layout_state.focus(b);
        paint(&ctx, &mut w);
        assert!(!w.activity.unread(b));
        w.detach_terminal(b);
        paint(&ctx, &mut w);
        w.tabs[1].send(b"detached");
        let deadline = Instant::now() + Duration::from_secs(2);
        while !w.tabs[1].screen.plain().contains("6465746163686564") && Instant::now() < deadline {
            paint(&ctx, &mut w);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(w.tabs[1].screen.plain().contains("6465746163686564"));
        assert!(!w.activity.unread(b));
        // Minimized viewports are not seen by a logic-only tick.
        let mut raw = RawInput::default();
        raw.viewports.get_mut(&ViewportId::ROOT).unwrap().minimized = Some(true);
        let mut output = ctx.run_ui(raw, |ui| w.update_activity_visibility(ui.ctx()));
        output.textures_delta.clear();
        assert!(!w.visible_terminals.contains(&b));
        w.activity.on_output(b, false);
        assert!(w.activity.unread(b));
        w.tabs[1].error = "target failed".into();
        w.pump_terminals();
        assert!(w.terminal_tab_label(b).contains("error"));
        w.force_close_tab(1);
        assert!(!w.activity.unread(b));
    }
}
impl TerminalTab {
    pub(super) fn feed_terminal_output(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let before = self.screen.activity_signature();
        self.screen.feed(bytes);
        self.output_changed |= before != self.screen.activity_signature();
    }
}
impl Workspace {
    pub(super) fn terminal_tab_label(&self, id: u64) -> String {
        format!(
            "{}{}{}",
            self.terminal_title(id),
            if self.activity.unread(id) && !self.visible_terminals.contains(&id) {
                " · new output"
            } else {
                ""
            },
            if self.activity.error(id) {
                " · error"
            } else {
                ""
            }
        )
    }
    pub(super) fn mark_painted_terminal(&mut self, ctx: &Context, id: u64, window: u64) {
        if let Some(desktop) = self
            .tabs
            .iter_mut()
            .find(|t| t.id == id)
            .and_then(|t| t.desktop.as_mut())
        {
            let geometry = ctx.input(|i| {
                let info = i.viewport();
                Some(harbor_workspace::workspace::windows::Geometry {
                    outer_position: info.outer_rect?.min,
                    inner_size: info.inner_rect?.size(),
                })
            });
            if ctx.input(|i| i.viewport().visible()) != Some(false)
                && self.windows.session_fullscreen(
                    window,
                    ctx.viewport_id(),
                    &mut desktop.fullscreen_pending,
                    desktop.connected,
                    geometry,
                )
            {
                ctx.send_viewport_cmd(ViewportCommand::Fullscreen(true));
            }
        }
        self.painted_terminals
            .insert(id, (window, ctx.viewport_id()));
        if ctx.input(|i| i.viewport().visible()) != Some(false) {
            self.visible_terminals.insert(id);
            self.activity.mark_seen(id);
        }
    }
    pub(super) fn update_activity_visibility(&mut self, ctx: &Context) {
        self.visible_terminals = self
            .painted_terminals
            .iter()
            .filter_map(|(&id, &(window, viewport))| {
                if self.windows.window(id) != window || window == 0 && self.page != Page::Terminal {
                    return None;
                }
                let visible = ctx.input(|i| {
                    i.raw
                        .viewports
                        .get(&viewport)
                        .and_then(|info| info.visible())
                });
                (visible != Some(false)).then_some(id)
            })
            .collect();
    }
}
