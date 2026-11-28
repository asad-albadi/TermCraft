use super::*;
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::ui::terminal_review_tests::{captured, frame, owned_workspace, raw_terminal};
    #[test]
    fn painted_keyboard_and_paste_use_only_the_selected_real_transports() {
        let (ctx, _root, mut w) = owned_workspace();
        let a = raw_terminal(&mut w, "A");
        let _b = raw_terminal(&mut w, "B");
        let c = raw_terminal(&mut w, "C");
        w.enable_broadcast_all();
        w.broadcast_group.select(w.tabs[1].id, false);
        w.set_layout_preset(harbor_workspace::workspace::layout::LayoutPreset::Four);
        w.active = 0;
        frame(&ctx, &mut w, vec![]);
        ctx.memory_mut(|m| m.request_focus(Id::new(("terminal-input", a))));
        frame(&ctx, &mut w, vec![Event::Text("Z".into())]);
        let deadline = Instant::now() + Duration::from_secs(2);
        while (captured(&w.tabs[0]).is_empty() || captured(&w.tabs[2]).is_empty())
            && Instant::now() < deadline
        {
            for t in &mut w.tabs {
                t.poll();
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(captured(&w.tabs[0]), b"Z");
        assert!(captured(&w.tabs[1]).is_empty());
        assert_eq!(captured(&w.tabs[2]), b"Z");
        assert_eq!(w.paste_targets(a), vec![a, c]);
        w.queue_paste(a, "P".into());
        w.resolve_paste(true);
        let deadline = Instant::now() + Duration::from_secs(2);
        while (captured(&w.tabs[0]).len() < 14 || captured(&w.tabs[2]).len() < 14)
            && Instant::now() < deadline
        {
            for t in &mut w.tabs {
                t.tick_paste(Instant::now());
                t.poll();
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(captured(&w.tabs[0]), b"Z\x1b[200~P\x1b[201~");
        assert_eq!(captured(&w.tabs[2]), b"Z\x1b[200~P\x1b[201~");
        assert!(captured(&w.tabs[1]).is_empty());
        // A new tab must not join an already confirmed group.
        let d = raw_terminal(&mut w, "D");
        assert!(!w.paste_targets(a).contains(&d));
        w.tabs[0].send_generated_command(b"FILE");
        let deadline = Instant::now() + Duration::from_secs(2);
        while captured(&w.tabs[0]).len() < 18 && Instant::now() < deadline {
            for t in &mut w.tabs {
                t.poll();
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(captured(&w.tabs[0]), b"Z\x1b[200~P\x1b[201~FILE");
        assert_eq!(captured(&w.tabs[2]), b"Z\x1b[200~P\x1b[201~");
        // A target that fails after painting is reported; accepted source input is not retried.
        w.tabs[2].process.take();
        w.dispatch_keyboard(a, b"Q");
        assert_eq!(w.broadcast_report.failed[0].0, c);
        assert_eq!(captured(&w.tabs[0]), b"Z\x1b[200~P\x1b[201~FILE");
        w.force_close_tab(2);
        assert!(!w.broadcast_group.selected().contains(&c));
    }
}
use harbor_workspace::workspace::{
    broadcast::BroadcastGroup,
    layout::{LayoutPreset, LayoutTree},
};
impl Workspace {
    pub(super) fn set_broadcast_enabled(&mut self, enabled: bool) {
        self.broadcast = enabled;
    }
    pub(super) fn acknowledge_broadcast_failures(&mut self) {
        self.broadcast_report.failed.clear();
    }
    fn retain_broadcast_failure(&mut self, id: u64, error: String) {
        if let Some(old) = self
            .broadcast_report
            .failed
            .iter_mut()
            .find(|old| old.0 == id)
        {
            old.1 = error;
        } else {
            self.broadcast_report.failed.push((id, error));
        }
    }
    pub(super) fn enable_broadcast_all(&mut self) {
        self.broadcast = true;
        self.broadcast_group = BroadcastGroup::default();
        for tab in &self.tabs {
            if tab.ready_for_input() && tab.broadcast_allowed() {
                self.broadcast_group.select(tab.id, true);
            }
        }
    }
    pub(super) fn broadcast_toggle(&mut self, ui: &mut Ui) {
        let mut enabled = self.broadcast;
        if ui
            .checkbox(&mut enabled, "MultiExec · selected targets")
            .changed()
        {
            self.set_broadcast_enabled(enabled);
        }
    }
    pub(super) fn reconcile_broadcast(&mut self) {
        let connected: Vec<_> = self
            .tabs
            .iter()
            .filter(|t| t.ready_for_input() && t.broadcast_allowed())
            .map(|t| t.id)
            .collect();
        let removed = self.broadcast_group.retain_connected(&connected);
        for id in removed {
            self.retain_broadcast_failure(
                id,
                "Target closed, disconnected or awaiting authentication; removed from MultiExec."
                    .into(),
            );
        }
    }
    pub(super) fn dispatch_keyboard(&mut self, source: u64, input: &[u8]) {
        if !self.broadcast || input.is_empty() || !self.broadcast_group.selected().contains(&source)
        {
            return;
        }
        if !self.tabs.iter().any(|t| {
            t.id == source && t.ready_for_input() && t.broadcast_allowed() && t.auth.is_none()
        }) {
            return;
        }
        let tabs = &mut self.tabs;
        let report = self.broadcast_group.dispatch(input, |id, bytes| {
            // The source was already accepted exactly once by terminal_view.
            if id == source {
                return Ok(());
            }
            let tab = tabs
                .iter_mut()
                .find(|t| t.id == id)
                .ok_or("Target closed")?;
            if !tab.ready_for_input() || !tab.broadcast_allowed() || tab.input_busy() {
                return Err("Target is disconnected, busy or awaiting authentication".into());
            }
            match tab.send_user_bytes(bytes, true) {
                Ok(()) => Ok(()),
                Err(error) => {
                    tab.error = error.clone();
                    Err(error)
                }
            }
        });
        self.broadcast_report.sent = report.sent;
        for (id, error) in report.failed {
            self.retain_broadcast_failure(id, error);
        }
    }
    pub(super) fn broadcast_targets_ui(&mut self, ui: &mut Ui) {
        self.broadcast_toggle(ui);
        if !self.broadcast {
            return;
        }
        ui.small("Compatibility network clients use private input; their sign-in prompts cannot be observed by MultiExec.");
        let selected = self.broadcast_group.selected();
        ui.label(format!(
            "{} selected targets · newly opened tabs stay excluded",
            selected.len()
        ));
        ui.horizontal_wrapped(|ui| {
            if ui.button("Select connected").clicked() {
                self.enable_broadcast_all();
            }
            if ui.button("Clear targets").clicked() {
                self.broadcast_group = BroadcastGroup::default();
            }
            for tab in &self.tabs {
                let mut checked = selected.contains(&tab.id);
                if ui
                    .add_enabled(
                        tab.ready_for_input() && tab.broadcast_allowed(),
                        Checkbox::new(&mut checked, self.terminal_title(tab.id)),
                    )
                    .changed()
                {
                    self.broadcast_group.select(tab.id, checked);
                }
            }
        });
        let mut show = false;
        let targets: Vec<_> = self
            .broadcast_group
            .selected()
            .into_iter()
            .filter(|id| self.windows.window(*id) == 0)
            .collect();
        let pages = targets.len().div_ceil(4).max(1);
        self.broadcast_page = self.broadcast_page.min(pages - 1);
        ui.horizontal(|ui| {
            show |= ui
                .add_enabled(!targets.is_empty(), Button::new("View selected targets"))
                .clicked();
            if pages > 1 {
                if ui
                    .add_enabled(self.broadcast_page > 0, Button::new("Previous targets"))
                    .clicked()
                {
                    self.broadcast_page -= 1;
                    show = true;
                }
                ui.label(format!("Page {} / {}", self.broadcast_page + 1, pages));
                if ui
                    .add_enabled(self.broadcast_page + 1 < pages, Button::new("Next targets"))
                    .clicked()
                {
                    self.broadcast_page += 1;
                    show = true;
                }
            }
            ui.small("Detached targets remain in their windows.");
        });
        if show {
            let ids: Vec<_> = targets
                .into_iter()
                .skip(self.broadcast_page * 4)
                .take(4)
                .collect();
            let prefs = &mut self.store.preferences.layout;
            prefs.preset = LayoutPreset::Four;
            self.layout_state.root = LayoutTree::preset(&ids, prefs);
            self.layout_state.active = ids.first().copied();
            if let Some(index) = self
                .tabs
                .iter()
                .position(|t| Some(t.id) == ids.first().copied())
            {
                self.active = index;
            }
            self.persist();
        }
        if !self.broadcast_report.failed.is_empty() {
            if ui.button("Acknowledge MultiExec failures").clicked() {
                self.acknowledge_broadcast_failures();
            }
            ui.collapsing(
                format!(
                    "MultiExec: {} failed/removed targets",
                    self.broadcast_report.failed.len()
                ),
                |ui| {
                    for (id, error) in &self.broadcast_report.failed {
                        crate::ui::errors::show(
                            ui,
                            crate::ui::errors::Domain::Session,
                            format!("Terminal {id}: {error}"),
                        );
                    }
                },
            );
        }
    }
}
