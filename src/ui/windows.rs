use super::*;
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::ui::terminal_review_tests::{captured, owned_workspace, raw_terminal};
    #[test]
    fn detached_history_has_a_full_themed_surface_behind_it() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        for theme in [Theme::Light, Theme::Dark] {
            let (ctx, _root, mut workspace) = owned_workspace();
            ctx.set_theme(theme);
            let tab = raw_terminal(&mut workspace, "detached-history");
            workspace.detach_terminal(tab);
            let child = viewport_id(workspace.windows.window(tab));
            let inspected = Arc::new(AtomicBool::new(false));
            let rendered = inspected.clone();
            ctx.set_embed_viewports(false);
            Context::set_immediate_viewport_renderer(move |ctx, mut viewport| {
                assert_eq!(viewport.ids.this, child);
                let bounds = Rect::from_min_size(Pos2::ZERO, vec2(360., 260.));
                let mut raw = RawInput {
                    viewport_id: child,
                    screen_rect: Some(bounds),
                    ..Default::default()
                };
                raw.viewports.insert(
                    child,
                    egui::ViewportInfo {
                        parent: Some(viewport.ids.parent),
                        ..Default::default()
                    },
                );
                let mut expected = Color32::TRANSPARENT;
                let mut output = ctx.run_ui(raw, |ui| {
                    expected = fluent::palette(ui).surface;
                    (viewport.viewport_ui_cb)(ui);
                });
                output.textures_delta.clear();
                let command = output
                    .shapes
                    .iter()
                    .position(|shape| {
                        matches!(&shape.shape,
                    egui::Shape::Text(text) if text.galley.text() == "Reattach")
                    })
                    .expect("native child painted its own commands");
                assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(t) if t.galley.text().starts_with("History: "))));
                assert!(output.shapes[..command].iter().any(|shape| matches!(&shape.shape,
                    egui::Shape::Rect(rect) if rect.fill == expected && rect.rect.contains_rect(bounds) && shape.clip_rect.contains_rect(bounds))),
                    "detached client needs an opaque {theme:?} surface behind History");
                rendered.store(true, Ordering::SeqCst);
            });
            let mut raw = RawInput::default();
            raw.viewports.insert(child, egui::ViewportInfo::default());
            ctx.run_ui(raw, |ui| workspace.detached_windows(ui.ctx()))
                .textures_delta
                .clear();
            assert!(inspected.load(Ordering::SeqCst));
        }
    }

    #[test]
    fn detached_minimum_keeps_rename_and_window_actions_reachable() {
        use crate::ui::gui_tests::{shell_frame, shell_pointer, shell_text_position};
        let (ctx, _root, mut workspace) = owned_workspace();
        let id = raw_terminal(&mut workspace, "narrow-detached");
        workspace.detach_terminal(id);
        let window = workspace.windows.window(id);
        let size = vec2(360., 260.);
        let mut actions = Vec::new();
        let mut draw = |ui: &mut Ui| {
            workspace.detached_commands(ui, ViewportClass::Immediate, window, id, &mut actions);
        };
        shell_frame(&ctx, size, 2., vec![], &mut draw);
        let output = shell_frame(&ctx, size, 2., vec![], &mut draw);
        let screen = Rect::from_min_size(Pos2::ZERO, size);
        let more = shell_text_position(&output, "More");
        assert!(screen.contains(shell_text_position(&output, "Reattach")));
        shell_frame(
            &ctx,
            size,
            2.,
            vec![Event::PointerMoved(more), shell_pointer(more, true)],
            &mut draw,
        );
        shell_frame(&ctx, size, 2., vec![shell_pointer(more, false)], &mut draw);
        let output = shell_frame(&ctx, size, 2., vec![], &mut draw);
        for label in ["Reattach", "Close window…", "Fullscreen", "Rename…"] {
            assert!(screen.contains(shell_text_position(&output, label)));
        }
        let rename = shell_text_position(&output, "Rename…");
        shell_frame(
            &ctx,
            size,
            2.,
            vec![Event::PointerMoved(rename), shell_pointer(rename, true)],
            &mut draw,
        );
        shell_frame(
            &ctx,
            size,
            2.,
            vec![shell_pointer(rename, false)],
            &mut draw,
        );
        assert_eq!(workspace.title_editor, Some((id, String::new())));
        assert_eq!(workspace.windows.window(id), window);
        assert!(actions.is_empty());
    }

    #[test]
    fn real_transport_survives_detach_paint_rename_and_reattach_once() {
        let (ctx, _root, mut w) = owned_workspace();
        let id = raw_terminal(&mut w, "original");
        w.windows.rename(id, "My title".into());
        let pointer = w.tabs[0].process.as_ref().unwrap() as *const Process;
        w.detach_terminal(id);
        assert!(w.layout_state.root.is_none());
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1400., 900.))),
                ..Default::default()
            },
            |ui| w.detached_windows(ui.ctx()),
        );
        output.textures_delta.clear();
        drop(output);
        assert_eq!(w.tabs.len(), 1);
        assert_eq!(
            w.tabs[0].process.as_ref().unwrap() as *const Process,
            pointer
        );
        w.tabs[0].screen.feed(b"\x1b]0;server title\x07");
        assert_eq!(w.terminal_title(id), "My title");
        w.tabs[0].send(b"Q");
        let deadline = Instant::now() + Duration::from_secs(2);
        while captured(&w.tabs[0]).is_empty() && Instant::now() < deadline {
            w.tabs[0].poll();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(captured(&w.tabs[0]), b"Q");
        w.reattach_terminal(id);
        assert!(w.windows.detached().is_empty());
        assert_eq!(w.layout_state.root.as_ref().unwrap().ids(), vec![id]);
        assert_eq!(
            w.tabs[0].process.as_ref().unwrap() as *const Process,
            pointer
        );
        assert_eq!(w.terminal_title(id), "My title");
        // Closing a detached terminal still passes through the existing active-session guard.
        w.detach_terminal(id);
        w.close_tab(0);
        assert!(w.pending_session_close.is_some());
        w.resolve_session_close(false, &ctx);
        assert_eq!(w.tabs.len(), 1);
        assert_ne!(w.windows.window(id), 0);
        w.force_close_tab(0);
        assert!(w.windows.detached().is_empty());
    }
}

impl Workspace {
    pub(super) fn collect_terminal_requests(&mut self, ctx: &Context) {
        let requests: Vec<_> = self
            .tabs
            .iter_mut()
            .filter_map(|t| t.paste_request.take().map(|text| (t.id, text)))
            .collect();
        for (id, text) in requests {
            if text.after.is_empty() {
                self.queue_paste(id, text.text);
            } else {
                self.queue_paste_to(self.paste_targets(id), text.text, text.after);
            }
        }
        let reads: Vec<_> = self
            .tabs
            .iter_mut()
            .filter_map(|t| std::mem::take(&mut t.clipboard_request).then_some(t.id))
            .collect();
        for id in reads {
            self.request_clipboard(id, ctx);
        }
    }
}
use harbor_workspace::workspace::windows::{Geometry, ROOT};
pub(super) fn viewport_id(window: u64) -> ViewportId {
    harbor_workspace::workspace::windows::viewport_id(window)
}
impl Workspace {
    pub(super) fn terminal_title(&self, id: u64) -> String {
        let Some(tab) = self.tabs.iter().find(|t| t.id == id) else {
            return String::new();
        };
        let browser_title = tab
            .web_browser
            .as_ref()
            .map(|browser| browser.handle.state().title);
        self.windows
            .title(
                id,
                browser_title.as_deref().unwrap_or(&tab.screen.title),
                &tab.session.title(),
            )
            .to_owned()
    }
    pub(super) fn sync_root_selection(&mut self) {
        let id = self
            .layout_state
            .active
            .filter(|id| {
                self.tabs
                    .iter()
                    .any(|t| t.id == *id && self.windows.window(*id) == ROOT)
            })
            .or_else(|| {
                self.tabs
                    .get(self.active)
                    .filter(|t| self.windows.window(t.id) == ROOT)
                    .map(|t| t.id)
            })
            .or_else(|| {
                self.tabs
                    .iter()
                    .find(|t| self.windows.window(t.id) == ROOT)
                    .map(|t| t.id)
            });
        if let Some(id) = id {
            self.active = self.tabs.iter().position(|t| t.id == id).unwrap();
            self.layout_state.focus(id);
        }
    }
    pub(super) fn detach_terminal(&mut self, id: u64) {
        if !self.tabs.iter().any(|t| t.id == id) {
            return;
        }
        let window = self.windows.detach(id);
        self.window_active.insert(window, id);
        self.layout_state.remove(id);
        self.sync_root_selection();
    }
    pub(super) fn reattach_terminal(&mut self, id: u64) {
        if self.windows.reattach(id, ROOT).is_err() {
            return;
        }
        if let Some(index) = self.tabs.iter().position(|t| t.id == id) {
            self.active = index;
            self.page = Page::Terminal;
            self.add_layout_terminal(id);
        }
    }
    pub(super) fn toggle_window_fullscreen(&mut self, ctx: &Context, window: u64) {
        if self.windows.fullscreen(window) {
            self.windows.leave_fullscreen(window);
            ctx.send_viewport_cmd(ViewportCommand::Fullscreen(false));
        } else {
            let geometry = ctx.input(|i| {
                let info = i.viewport();
                Some(Geometry {
                    outer_position: info.outer_rect?.min,
                    inner_size: info.inner_rect?.size(),
                })
            });
            self.windows.enter_fullscreen(window, geometry);
            ctx.send_viewport_cmd(ViewportCommand::Fullscreen(true));
        }
    }
    pub(super) fn restore_window_geometry(&mut self, ctx: &Context, window: u64) {
        let fullscreen = ctx.input(|i| i.viewport().fullscreen);
        if fullscreen == Some(false) {
            if let Some(g) = self.windows.restore_after_ack(window, false) {
                ctx.send_viewport_cmd(ViewportCommand::OuterPosition(g.outer_position));
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(g.inner_size));
            }
        }
    }
    pub(super) fn window_title_editor(&mut self, ctx: &Context, window: u64) {
        if self
            .title_editor
            .as_ref()
            .is_some_and(|(tab, _)| self.windows.window(*tab) != window)
        {
            return;
        }
        let Some((id, mut text)) = self.title_editor.take() else {
            return;
        };
        if !self.tabs.iter().any(|t| t.id == id) {
            return;
        }
        let mut open = true;
        let mut done = false;
        let size = fluent::dialog_size(ctx, 420., 124.);
        let modal = fluent::modal(
            ctx,
            Id::new("terminal-title-editor"),
            "Rename terminal tab",
            size,
            |ui| {
                ui.label("A custom title takes priority over the server title. Leave empty to use the server/session title.");
                let name = ui.add(TextEdit::singleline(&mut text).char_limit(128));
                fluent::select_name_on_entry(ui, &name, Id::new(("terminal-title", id)), &text);
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        self.windows.rename(id, text.clone());
                        done = true;
                    }
                    if ui.button("Cancel").clicked() {
                        done = true;
                    }
                });
            },
        );
        open &= !modal.should_close();
        if open && !done {
            self.title_editor = Some((id, text));
        }
    }
    fn detached_commands(
        &mut self,
        ui: &mut Ui,
        class: ViewportClass,
        window: u64,
        active: u64,
        actions: &mut Vec<(u64, bool)>,
    ) {
        ui.horizontal(|ui| {
            let ids = self.windows.terminals(window);
            let width = ui.available_width().max(80.);
            ui.allocate_ui_with_layout(
                vec2(width, 38.),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ScrollArea::horizontal()
                        .id_salt(("detached-tabs", window))
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                for id in &ids {
                                    let running = self
                                        .tabs
                                        .iter()
                                        .find(|t| t.id == *id)
                                        .map(TerminalTab::running);
                                    let (response, _) = workspace_tab(
                                        ui,
                                        &self.terminal_tab_label(*id),
                                        active == *id,
                                        running,
                                        false,
                                        *id,
                                    );
                                    if response.clicked() {
                                        self.window_active.insert(window, *id);
                                    }
                                }
                            })
                        });
                },
            );
        });
        ui.horizontal(|ui| {
            if ui.button("Reattach").clicked() {
                actions.push((active, false));
            }
            ui.menu_button("More", |ui| {
                if let Some(index) = self.tabs.iter().position(|t| t.id == active) {
                    if self.tabs[index].web_browser.is_some() {
                        ui.menu_button("Browser navigation", |ui| {
                            ui.set_min_width(300.);
                            super::web_browser::toolbar(ui, &mut self.tabs[index]);
                        });
                    }
                    if self.tabs[index].desktop.is_some() {
                        ui.menu_button("Remote desktop controls", |ui| {
                            super::desktop::toolbar(ui, &mut self.tabs[index])
                        });
                    }
                    if self.tabs[index].session.protocol == "SFTP" {
                        ui.menu_button("SFTP commands", |ui| {
                            self.sftp_compatibility_commands(ui, index)
                        });
                    }
                }
                if let Some(tab) = self.tabs.iter().find(|t| t.id == active) {
                    super::pages::history_status(ui, tab);
                }
                if ui.button("Close window…").clicked() {
                    self.window_close_prompt.insert(window);
                }
                if class != ViewportClass::EmbeddedWindow {
                    let label = if self.windows.fullscreen(window) {
                        "Restore window"
                    } else {
                        "Fullscreen"
                    };
                    if ui
                        .button(label)
                        .fluent_hint(self.shortcut_hint(Action::Fullscreen, label))
                        .clicked()
                    {
                        self.toggle_window_fullscreen(ui.ctx(), window);
                    }
                }
                if ui.button("Rename…").clicked() {
                    self.title_editor = Some((
                        active,
                        self.windows.override_title(active).unwrap_or("").to_owned(),
                    ));
                }
            });
        });
    }

    pub(super) fn detached_windows(&mut self, ctx: &Context) {
        self.restore_window_geometry(ctx, ROOT);
        let prefs = self.store.preferences.clone();
        let mut actions = Vec::new();
        for window in self.windows.detached() {
            let ids = self.windows.terminals(window);
            let active = self
                .window_active
                .get(&window)
                .copied()
                .filter(|id| ids.contains(id))
                .unwrap_or(ids[0]);
            let title = self.terminal_title(active);
            ctx.show_viewport_immediate(viewport_id(window), ViewportBuilder::default().with_title(title).with_inner_size([960.,640.]).with_min_inner_size([360.,260.]), |ui, class| {
                ui.painter().rect_filled(ui.max_rect(), 0., content(ui));
                let child_ctx = ui.ctx().clone();
                if class != ViewportClass::EmbeddedWindow && child_ctx.input(|i| i.viewport().close_requested()) {
                    child_ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                    self.window_close_prompt.insert(window);
                }
                // Lifecycle requests are always intercepted. The existing
                // confirmation appears after wake without requiring Close again.
                if self.screensaver_frame(ui) { return; }
                if self.window_close_prompt.contains(&window) {
                    if child_ctx.input(|i| i.viewport().close_requested()) { self.games_frame(ui); }
                    let size = super::fluent::dialog_size(&child_ctx, 420., 110.);
                    let response = super::fluent::modal(&child_ctx, Id::new(("detached-close", window)), "Close terminal window?", size, |ui| {
                        ui.label("Reattach these terminals, or close the selected session? Active sessions and remote edits require confirmation in the main window.");
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("Reattach all").clicked() { actions.extend(ids.iter().map(|id| (*id, false))); self.window_close_prompt.remove(&window); }
                            if ui.button("Close selected session").clicked() { actions.push((active, true)); self.window_close_prompt.remove(&window); }
                            let safe = ui.button("Cancel");
                            super::fluent::focus_on_entry(ui, &safe, ui.id());
                            if safe.clicked() { self.window_close_prompt.remove(&window); }
                        });
                    });
                    if response.should_close() { self.window_close_prompt.remove(&window); }
                    return;
                }
                if self.games_frame(ui) { return; }
                if self.import_applying(){ui.disable();}
                self.handle_shortcuts(&child_ctx, window);
                self.restore_window_geometry(&child_ctx, window);
                self.detached_commands(ui, class, window, active, &mut actions);
                if self.window_close_prompt.contains(&window) { child_ctx.request_repaint(); return; }
                if self.title_editor.as_ref().is_some_and(|(tab, _)| self.windows.window(*tab) == window) {
                    self.window_title_editor(&child_ctx, window);
                    return;
                }
                if let Some(index) = self.tabs.iter().position(|t| t.id == active) {
                    super::remote_files::authentication_ui(ui,&mut self.tabs[index]);
                    self.tabs[index].search_ui(ui);
                    if !self.tabs[index].error.is_empty() { crate::ui::errors::show(ui, crate::ui::errors::protocol(&self.tabs[index].session.protocol), &self.tabs[index].error); }
                    let bytes = super::pages::terminal_view(ui, &mut self.tabs[index], &prefs);
                    self.mark_painted_terminal(&child_ctx,active,window);
                    self.dispatch_keyboard(active, &bytes);
                }
            });
        }
        // Only one close request may occupy the existing confirmation controller.
        let mut close_started = false;
        for (id, close) in actions {
            if !close {
                self.reattach_terminal(id);
            } else if !close_started {
                if let Some(index) = self.tabs.iter().position(|t| t.id == id) {
                    self.close_tab(index);
                    close_started = true;
                    ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Focus);
                }
            }
        }
        let live = self.windows.detached();
        self.window_active.retain(|id, _| live.contains(id));
        self.window_close_prompt.retain(|id| live.contains(id));
    }
}
