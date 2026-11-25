use super::*;
use harbor_workspace::workspace::layout::{LayoutPreset, LayoutTree, Orientation};
impl Workspace {
    pub(super) fn set_layout_preset(&mut self, preset: LayoutPreset) {
        self.store.preferences.layout.preset = preset;
        let ids: Vec<_> = (0..self.tabs.len())
            .map(|offset| self.tabs[(self.active + offset) % self.tabs.len()].id)
            .filter(|id| self.windows.window(*id) == 0)
            .collect();
        self.layout_state.root = LayoutTree::preset(&ids, &self.store.preferences.layout);
        self.layout_state.active = ids.first().copied();
        self.persist();
    }
    pub(super) fn toggle_layout(&mut self) {
        self.set_layout_preset(
            if self.store.preferences.layout.preset == LayoutPreset::Single {
                LayoutPreset::Columns
            } else {
                LayoutPreset::Single
            },
        );
    }
    pub(super) fn layout_picker(&mut self, ui: &mut Ui) {
        let mut preset = self.store.preferences.layout.preset;
        ComboBox::from_id_salt("terminal-layout")
            .selected_text(match preset {
                LayoutPreset::Single => "Single terminal",
                LayoutPreset::Columns => "Two columns",
                LayoutPreset::Rows => "Two rows",
                LayoutPreset::Four => "Four terminals",
            })
            .show_ui(ui, |ui| {
                for (value, label) in [
                    (LayoutPreset::Single, "Single terminal"),
                    (LayoutPreset::Columns, "Two columns"),
                    (LayoutPreset::Rows, "Two rows"),
                    (LayoutPreset::Four, "Four terminals"),
                ] {
                    ui.selectable_value(&mut preset, value, label);
                }
            });
        if preset != self.store.preferences.layout.preset {
            self.set_layout_preset(preset);
        }
    }
    pub(super) fn add_layout_terminal(&mut self, id: u64) {
        let prefs = &mut self.store.preferences.layout;
        if let Some(root) = &self.layout_state.root {
            prefs.capture(root);
        }
        if prefs.preset == LayoutPreset::Single {
            self.layout_state.root = Some(LayoutTree::Leaf { id });
            self.layout_state.active = Some(id);
            return;
        }
        let mut ids = self
            .layout_state
            .root
            .as_ref()
            .map(LayoutTree::ids)
            .unwrap_or_default();
        ids.retain(|id| self.tabs.iter().any(|t| t.id == *id));
        if ids.contains(&id) {
            self.layout_state.active = Some(id);
            return;
        }
        if ids.len() < prefs.preset.capacity() {
            ids.push(id);
            self.layout_state.root = LayoutTree::preset(&ids, prefs);
        } else {
            self.layout_state.focus(id);
        }
        self.layout_state.active = Some(id);
    }
    pub(super) fn render_terminal_layout(
        &mut self,
        ui: &mut Ui,
        p: &Preferences,
    ) -> Vec<(u64, Vec<u8>)> {
        let Some(active) = self
            .tabs
            .get(self.active)
            .filter(|t| self.windows.window(t.id) == 0)
            .or_else(|| self.tabs.iter().find(|t| self.windows.window(t.id) == 0))
            .map(|t| t.id)
        else {
            self.pane_bounds.clear();
            ui.label(
                "All terminals are in detached windows. Use a tab’s menu to reattach it here.",
            );
            return Vec::new();
        };
        self.layout_state.focus(active);
        let bounds = ui.available_rect_before_wrap();
        let requested = self
            .layout_state
            .root
            .clone()
            .unwrap_or(LayoutTree::Leaf { id: active });
        let fits = requested.fits(bounds);
        if !fits && requested.ids().len() > 1 {
            ui.small("This window is too small for the requested layout. Showing the active terminal; enlarge the window to restore all panes.");
        }
        let bounds = ui.available_rect_before_wrap();
        let tree = if requested.fits(bounds) {
            requested.clone()
        } else {
            LayoutTree::Leaf { id: active }
        };
        let panes = tree.layout(bounds);
        self.pane_bounds = panes.iter().map(|pane| (pane.id, pane.rect)).collect();
        let target = ui
            .input(|i| i.events.clone())
            .iter()
            .filter_map(|event| match event {
                Event::PointerButton {
                    pos, pressed: true, ..
                } if ui.ctx().layer_id_at(*pos) == Some(ui.layer_id()) => panes
                    .iter()
                    .find(|pane| pane.rect.contains(*pos))
                    .map(|pane| pane.id),
                _ => None,
            })
            .last();
        if let Some(id) = target {
            let scope = if self
                .tabs
                .iter()
                .any(|tab| tab.id == id && tab.desktop.is_some())
            {
                "rdp-canvas"
            } else {
                "terminal-input"
            };
            ui.memory_mut(|m| {
                let focus = Id::new((scope, id));
                if !m.has_focus(focus) {
                    m.request_focus(focus);
                }
            });
            self.layout_state.active = Some(id);
            if let Some(index) = self.tabs.iter().position(|t| t.id == id) {
                self.active = index;
            }
        }
        let mut emitted = Vec::new();
        let mut close = None;
        for pane in &panes {
            let Some(index) = self.tabs.iter().position(|t| t.id == pane.id) else {
                continue;
            };
            let selected = self.layout_state.active == Some(pane.id);
            let mut child = ui.new_child(
                UiBuilder::new()
                    .id_salt(("terminal-pane", pane.id))
                    .max_rect(pane.rect)
                    .layout(Layout::top_down(Align::Min)),
            );
            child.set_clip_rect(pane.rect.intersect(ui.clip_rect()));
            child.spacing_mut().item_spacing = vec2(4., 4.);
            child.horizontal(|ui| {
                ui.label(
                    RichText::new(self.terminal_title(pane.id))
                        .strong()
                        .color(if selected { accent(ui) } else { ink(ui) }),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .small_button("×")
                        .on_hover_text("Close this terminal")
                        .clicked()
                    {
                        close = Some(pane.id);
                    }
                });
            });
            let bytes = super::pages::terminal_view(&mut child, &mut self.tabs[index], p);
            if pane.rect.intersect(ui.clip_rect()).is_positive() {
                self.mark_painted_terminal(ui.ctx(), pane.id, 0);
            }
            if !bytes.is_empty() {
                emitted.push((pane.id, bytes));
            }
            ui.painter().rect_stroke(
                pane.rect,
                0.,
                Stroke::new(
                    if selected { 1.5 } else { 1. },
                    if selected { accent(ui) } else { line(ui) },
                ),
                StrokeKind::Inside,
            );
        }
        let mut save = false;
        for divider in tree.dividers(bounds) {
            let response = ui.interact(
                divider.rect,
                Id::new(("terminal-divider", &divider.path)),
                Sense::drag(),
            );
            response
                .clone()
                .on_hover_cursor(if divider.axis == Orientation::Horizontal {
                    CursorIcon::ResizeHorizontal
                } else {
                    CursorIcon::ResizeVertical
                });
            ui.painter().rect_filled(
                divider.rect,
                0.,
                if response.hovered() || response.dragged() {
                    accent(ui)
                } else {
                    line(ui)
                },
            );
            if response.dragged() {
                if let Some(pos) = response.interact_pointer_pos() {
                    let extent = if divider.axis == Orientation::Horizontal {
                        divider.parent.width()
                    } else {
                        divider.parent.height()
                    };
                    let offset = if divider.axis == Orientation::Horizontal {
                        pos.x - divider.parent.min.x
                    } else {
                        pos.y - divider.parent.min.y
                    };
                    let ratio = (offset - harbor_workspace::workspace::layout::DIVIDER / 2.)
                        / (extent - harbor_workspace::workspace::layout::DIVIDER).max(1.);
                    if let Some(root) = &mut self.layout_state.root {
                        let _ = root.set_ratio(&divider.path, ratio);
                        self.store.preferences.layout.capture(root);
                    }
                    ui.ctx().request_repaint();
                }
            }
            save |= response.drag_stopped();
        }
        ui.advance_cursor_after_rect(bounds);
        if save {
            self.persist();
        }
        if let Some(id) = close {
            if let Some(index) = self.tabs.iter().position(|t| t.id == id) {
                self.close_tab(index);
            }
        }
        emitted
    }
}
#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::ui::terminal_review_tests::{captured, owned_workspace, raw_terminal};
    use harbor_workspace::workspace::layout::LayoutPreset;
    #[cfg(unix)]
    #[test]
    fn four_painted_panes_route_input_once_to_the_clicked_owner() {
        let (ctx, _root, mut w) = owned_workspace();
        for name in [
            "pane-owned-A",
            "pane-owned-B",
            "pane-owned-C",
            "pane-owned-D",
        ] {
            raw_terminal(&mut w, name);
        }
        w.set_layout_preset(LayoutPreset::Four);
        let render = |ctx: &Context, w: &mut Workspace, events| {
            let mut out = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1500., 1200.))),
                    events,
                    ..Default::default()
                },
                |ui| w.terminals(ui),
            );
            out.textures_delta.clear();
            out
        };
        let out = render(&ctx, &mut w, vec![]);
        assert_eq!(w.pane_bounds.len(), 4);
        fn text(shape: &egui::epaint::Shape, result: &mut String) {
            match shape {
                egui::epaint::Shape::Text(s) => result.push_str(s.galley.text()),
                egui::epaint::Shape::Vec(s) => {
                    for shape in s {
                        text(shape, result);
                    }
                }
                _ => {}
            }
        }
        let mut painted = String::new();
        for shape in out.shapes {
            text(&shape.shape, &mut painted);
        }
        for name in [
            "pane-owned-A",
            "pane-owned-B",
            "pane-owned-C",
            "pane-owned-D",
        ] {
            assert!(painted.contains(name), "missing actual pane header {name}");
        }
        let (id, rect) = w.pane_bounds[2];
        let pos = rect.center();
        let events = vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::default(),
            },
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::default(),
            },
            Event::Text("Z".into()),
        ];
        render(&ctx, &mut w, events);
        let deadline = Instant::now() + Duration::from_secs(2);
        while w.tabs.iter().all(|t| captured(t).is_empty()) && Instant::now() < deadline {
            for tab in &mut w.tabs {
                tab.poll();
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        for tab in &w.tabs {
            if tab.id == id {
                assert_eq!(captured(tab), b"Z");
            } else {
                assert!(captured(tab).is_empty());
            }
        }
        assert_eq!(w.tabs[w.active].id, id);
        let active = id;
        let other = w.tabs.iter().position(|t| t.id != active).unwrap();
        let removed = w.tabs[other].id;
        w.force_close_tab(other);
        assert_eq!(w.tabs[w.active].id, active);
        assert_eq!(w.layout_state.root.as_ref().unwrap().ids().len(), 3);
        assert!(
            !w.layout_state
                .root
                .as_ref()
                .unwrap()
                .ids()
                .contains(&removed)
        );
    }
    #[cfg(unix)]
    #[test]
    fn dragging_real_divider_persists_the_ratio_and_small_views_fall_back() {
        let (ctx, root, mut w) = owned_workspace();
        for _ in 0..2 {
            w.start(Session {
                protocol: "Unavailable".into(),
                ..Default::default()
            });
        }
        w.set_layout_preset(LayoutPreset::Columns);
        let render = |ctx: &Context, w: &mut Workspace, size, events| {
            let mut out = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                    events,
                    ..Default::default()
                },
                |ui| w.terminals(ui),
            );
            out.textures_delta.clear();
        };
        render(&ctx, &mut w, vec2(1500., 1000.), vec![]);
        let left = w.pane_bounds[0].1;
        let right = w.pane_bounds[1].1;
        let start = pos2((left.max.x + right.min.x) / 2., left.center().y);
        let end = start + vec2(120., 0.);
        render(
            &ctx,
            &mut w,
            vec2(1500., 1000.),
            vec![
                Event::PointerMoved(start),
                Event::PointerButton {
                    pos: start,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::default(),
                },
            ],
        );
        render(
            &ctx,
            &mut w,
            vec2(1500., 1000.),
            vec![Event::PointerMoved(end)],
        );
        render(
            &ctx,
            &mut w,
            vec2(1500., 1000.),
            vec![Event::PointerButton {
                pos: end,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::default(),
            }],
        );
        let reloaded = Store::load(&root.path().join("workspace.json")).unwrap();
        assert!(reloaded.preferences.layout.ratios[0] > 0.55);
        assert_eq!(reloaded.preferences.layout.preset, LayoutPreset::Columns);
        w.set_layout_preset(LayoutPreset::Four);
        render(&ctx, &mut w, vec2(400., 650.), vec![]);
        assert_eq!(
            w.pane_bounds.len(),
            1,
            "an undersized multi-pane view must preserve a usable single pane"
        );
        assert_eq!(
            w.store.preferences.layout.preset,
            LayoutPreset::Four,
            "small-window fallback must not overwrite the requested preference"
        );
    }
}
