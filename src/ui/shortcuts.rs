use super::*;
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::ui::terminal_review_tests::{captured, frame, owned_workspace, raw_terminal};
    #[test]
    fn real_history_pageup_changes_view_without_transport_bytes() {
        let (ctx, _root, mut w) = owned_workspace();
        let id = raw_terminal(&mut w, "history");
        for i in 0..200 {
            w.tabs[0].screen.feed(format!("row {i}\r\n").as_bytes());
        }
        frame(&ctx, &mut w, vec![]);
        frame(&ctx, &mut w, vec![]);
        let before = w.tabs[0].history_offset;
        assert!(before > 100.);
        ctx.memory_mut(|m| m.request_focus(Id::new(("terminal-input", id))));
        frame(
            &ctx,
            &mut w,
            vec![Event::Key {
                key: Key::PageUp,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers {
                    shift: true,
                    ..Default::default()
                },
            }],
        );
        assert!(
            w.tabs[0].history_offset < before,
            "PageUp must change the actual painted scroll state"
        );
        std::thread::sleep(Duration::from_millis(40));
        w.tabs[0].poll();
        assert!(
            captured(&w.tabs[0]).is_empty(),
            "local history key reached the shell"
        );
    }
    #[test]
    fn native_paste_payload_and_shortcut_do_not_request_another_clipboard_read() {
        let (ctx, _root, mut w) = owned_workspace();
        let id = raw_terminal(&mut w, "paste");
        frame(&ctx, &mut w, vec![]);
        ctx.memory_mut(|m| m.request_focus(Id::new(("terminal-input", id))));
        let prefs = w.store.preferences.clone();
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1100., 750.))),
                events: vec![
                    Event::Key {
                        key: Key::V,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Modifiers {
                            ctrl: true,
                            shift: true,
                            ..Default::default()
                        },
                    },
                    Event::Paste("owned payload".into()),
                ],
                ..Default::default()
            },
            |ui| {
                super::super::pages::terminal_view(ui, &mut w.tabs[0], &prefs);
            },
        );
        output.textures_delta.clear();
        assert!(
            !w.tabs[0].clipboard_request,
            "native paste already carries the bytes; do not read the clipboard again"
        );
        assert_eq!(
            w.tabs[0].paste_request.as_ref().unwrap().text,
            "owned payload"
        );
    }
    #[test]
    fn modifier_shortcut_text_event_is_consumed_before_terminal_encoding() {
        use harbor_workspace::workspace::shortcuts::{Action, KeyChord};
        let (ctx, _root, mut w) = owned_workspace();
        let id = raw_terminal(&mut w, "chord");
        w.store
            .preferences
            .shortcuts
            .bind(
                Action::ToggleSidebar,
                KeyChord::new("E", false, false, true),
            )
            .unwrap();
        frame(&ctx, &mut w, vec![]);
        ctx.memory_mut(|m| m.request_focus(Id::new(("terminal-input", id))));
        let before = w.store.preferences.sidebar_visible;
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1320., 900.))),
                events: vec![
                    Event::Key {
                        key: Key::E,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Modifiers {
                            alt: true,
                            ..Default::default()
                        },
                    },
                    Event::Text("€".into()),
                ],
                ..Default::default()
            },
            |ui| {
                w.terminals(ui);
                w.handle_shortcuts(ui.ctx(), 0);
            },
        );
        output.textures_delta.clear();
        assert_ne!(w.store.preferences.sidebar_visible, before);
        std::thread::sleep(Duration::from_millis(40));
        w.tabs[0].poll();
        assert!(
            captured(&w.tabs[0]).is_empty(),
            "text paired with an application chord leaked into the shell"
        );
    }
    #[test]
    fn shortcut_capture_keeps_typed_keys_out_of_the_focused_shell() {
        let (ctx, _root, mut w) = owned_workspace();
        let id = raw_terminal(&mut w, "capture");
        frame(&ctx, &mut w, vec![]);
        ctx.memory_mut(|m| m.request_focus(Id::new(("terminal-input", id))));
        w.shortcut_editor_open = true;
        w.shortcut_capture = Some(harbor_workspace::workspace::shortcuts::Action::NewTerminal);
        w.set_shortcut_context(&ctx);
        frame(&ctx, &mut w, vec![Event::Text("K".into())]);
        std::thread::sleep(Duration::from_millis(40));
        w.tabs[0].poll();
        assert!(
            captured(&w.tabs[0]).is_empty(),
            "binding capture typed into the terminal behind the dialog"
        );
    }
    #[test]
    fn closing_shortcut_targets_focused_pane_instead_of_stale_tab_index() {
        let (ctx, _root, mut w) = owned_workspace();
        let a = raw_terminal(&mut w, "A");
        let b = raw_terminal(&mut w, "B");
        ctx.memory_mut(|m| m.request_focus(Id::new(("terminal-input", a))));
        let mut output = ctx.run_ui(
            RawInput {
                events: vec![Event::Key {
                    key: Key::W,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers {
                        ctrl: true,
                        shift: true,
                        ..Default::default()
                    },
                }],
                ..Default::default()
            },
            |ui| w.handle_shortcuts(ui.ctx(), 0),
        );
        output.textures_delta.clear();
        w.resolve_session_close(true, &ctx);
        assert_eq!(w.tabs.iter().map(|t| t.id).collect::<Vec<_>>(), vec![b]);
    }
    #[test]
    fn edited_global_binding_respects_real_textedit_focus_and_store_reload() {
        use harbor_workspace::workspace::shortcuts::{Action, KeyChord};
        let (ctx, root, mut w) = owned_workspace();
        let id = raw_terminal(&mut w, "focus");
        w.store
            .preferences
            .shortcuts
            .bind(
                Action::ToggleSidebar,
                KeyChord::new("F12", false, false, false),
            )
            .unwrap();
        w.persist();
        let loaded = Store::load(&w.store_path).unwrap();
        assert_eq!(loaded.preferences.shortcuts, w.store.preferences.shortcuts);
        let event = Event::Key {
            key: Key::F12,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        let editor = Id::new("owned-editor");
        let mut text = String::new();
        let before = w.store.preferences.sidebar_visible;
        let mut output = ctx.run_ui(
            RawInput {
                events: vec![event.clone()],
                ..Default::default()
            },
            |ui| {
                ui.add(TextEdit::singleline(&mut text).id(editor));
                ui.memory_mut(|m| m.request_focus(editor));
                w.handle_shortcuts(ui.ctx(), 0);
            },
        );
        output.textures_delta.clear();
        assert_eq!(w.store.preferences.sidebar_visible, before);
        ctx.memory_mut(|m| m.request_focus(Id::new(("terminal-input", id))));
        let mut output = ctx.run_ui(
            RawInput {
                events: vec![
                    Event::Key {
                        key: Key::F12,
                        physical_key: None,
                        pressed: false,
                        repeat: false,
                        modifiers: Modifiers::NONE,
                    },
                    event,
                ],
                ..Default::default()
            },
            |ui| w.handle_shortcuts(ui.ctx(), 0),
        );
        output.textures_delta.clear();
        assert_ne!(w.store.preferences.sidebar_visible, before);
        drop(root);
    }
}
use harbor_workspace::workspace::shortcuts::{Action, FocusContext, KeyChord};
const BLOCKED: &str = "termcraft-shortcuts-blocked";
impl Workspace {
    pub(super) fn shortcuts_blocked(&self) -> bool {
        self.shared_modal()
            || self.vault_modal()
            || self.ssh_keys.open
            || self.session_import.is_some()
            || self.shortcut_editor_open
            || self.shortcut_capture.is_some()
            || self.session_form.is_some()
            || self.title_editor.is_some()
            || self.highlight_editor.is_some()
            || self.terminal_profile_editor.is_some()
            || self.pending_session_close.is_some()
            || self.pending_remote_tab_close.is_some()
            || self.pending_paste.is_some()
            || self.pending_remote_exit
    }
    pub(super) fn set_shortcut_context(&self, ctx: &Context) {
        ctx.data_mut(|d| d.insert_temp(Id::new(BLOCKED), self.shortcuts_blocked()));
    }
    fn shortcut_focus(&self, ctx: &Context) -> FocusContext {
        if self.shortcuts_blocked() {
            return FocusContext::Authentication;
        }
        if ctx
            .data(|d| d.get_temp::<bool>(Id::new("remote-desktop-keyboard-capture")))
            .unwrap_or(false)
        {
            return FocusContext::RemoteDesktop;
        }
        match ctx.memory(|m| m.focused()) {
            None => FocusContext::Workspace,
            Some(id)
                if self
                    .tabs
                    .iter()
                    .any(|t| Id::new(("terminal-input", t.id)) == id) =>
            {
                FocusContext::Terminal
            }
            _ => FocusContext::TextEditor,
        }
    }
    pub(super) fn handle_shortcuts(&mut self, ctx: &Context, window: u64) {
        let owner = ctx
            .memory(|m| m.focused())
            .and_then(|focused| {
                self.tabs
                    .iter()
                    .find(|t| Id::new(("terminal-input", t.id)) == focused)
            })
            .map(|t| self.windows.window(t.id));
        if owner.is_some_and(|owner| owner != window)
            || owner.is_none() && ctx.viewport_id() != super::windows::viewport_id(window)
        {
            return;
        }

        if let Some(focused) = ctx.memory(|m| m.focused()) {
            if let Some(index) = self.tabs.iter().position(|t| {
                Id::new(("terminal-input", t.id)) == focused && self.windows.window(t.id) == window
            }) {
                if window == 0 {
                    self.active = index;
                } else {
                    self.window_active.insert(window, self.tabs[index].id);
                }
            }
        }
        let focus = self.shortcut_focus(ctx);
        let mut actions = Vec::new();
        let map = &self.store.preferences.shortcuts;
        ctx.input_mut(|i| {
            let mut paired_text = false;
            i.events.retain(|event| {
                if matches!(event, Event::Text(_)) && paired_text {
                    paired_text = false;
                    return false;
                }
                if matches!(event, Event::Key { pressed: true, .. }) {
                    paired_text = false;
                }
                if let Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    repeat,
                    ..
                } = event
                {
                    if let Some(action) = map.resolve(&KeyChord::from_key(*key, *modifiers), focus)
                    {
                        if !action.terminal_view() {
                            if !repeat {
                                actions.push(action);
                            }
                            paired_text = true;
                            return false;
                        }
                    }
                }
                true
            })
        });
        for action in actions {
            match action {
                Action::NewSession => {
                    self.new_session("SSH");
                    ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Focus);
                }
                Action::NewTerminal => {
                    self.local();
                    if window != 0 {
                        let id = self.tabs[self.active].id;
                        self.layout_state.remove(id);
                        if self.windows.reattach(id, window).is_ok() {
                            self.window_active.insert(window, id);
                        } else {
                            self.add_layout_terminal(id);
                        }
                    }
                }
                Action::ToggleSidebar => {
                    self.store.preferences.sidebar_visible =
                        !self.store.preferences.sidebar_visible;
                    self.persist();
                }
                Action::Fullscreen => self.toggle_window_fullscreen(ctx, window),
                Action::NextTab | Action::PreviousTab => {
                    let ids: Vec<_> = self
                        .tabs
                        .iter()
                        .filter(|t| self.windows.window(t.id) == window)
                        .map(|t| t.id)
                        .collect();
                    if ids.is_empty() {
                        continue;
                    }
                    let current = if window == 0 {
                        self.tabs.get(self.active).map(|t| t.id)
                    } else {
                        self.window_active.get(&window).copied()
                    };
                    let index = ids.iter().position(|id| Some(*id) == current).unwrap_or(0);
                    let next = if action == Action::NextTab {
                        (index + 1) % ids.len()
                    } else {
                        (index + ids.len() - 1) % ids.len()
                    };
                    let id = ids[next];
                    if window == 0 {
                        self.active = self.tabs.iter().position(|t| t.id == id).unwrap();
                        self.layout_state.focus(id);
                        self.page = Page::Terminal;
                    } else {
                        self.window_active.insert(window, id);
                    }
                    ctx.memory_mut(|m| m.request_focus(Id::new(("terminal-input", id))));
                }
                Action::CloseTab => {
                    let id = if window == 0 {
                        self.tabs.get(self.active).map(|t| t.id)
                    } else {
                        self.window_active.get(&window).copied()
                    };
                    if let Some(index) = self
                        .tabs
                        .iter()
                        .position(|t| Some(t.id) == id && self.windows.window(t.id) == window)
                    {
                        self.close_tab(index);
                        ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Focus);
                    }
                }
                _ => {}
            }
        }
    }
    pub(super) fn shortcut_editor(&mut self, ctx: &Context) {
        if !self.shortcut_editor_open {
            return;
        }
        if let Some(action) = self.shortcut_capture {
            let event = ctx.input(|i| {
                i.events.iter().find_map(|e| {
                    if let Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        repeat: false,
                        ..
                    } = e
                    {
                        Some((*key, *modifiers))
                    } else {
                        None
                    }
                })
            });
            if let Some((key, modifiers)) = event {
                ctx.input_mut(|i| {
                    i.events
                        .retain(|e| !matches!(e, Event::Key { pressed: true, .. }))
                });
                match self
                    .store
                    .preferences
                    .shortcuts
                    .bind(action, KeyChord::from_key(key, modifiers))
                {
                    Ok(()) => {
                        self.shortcut_capture = None;
                        self.shortcut_error.clear();
                        self.persist();
                    }
                    Err(error) => self.shortcut_error = error.0,
                }
            }
        }
        let mut open = true;
        Window::new("Keyboard shortcuts").open(&mut open).default_width(600.).vscroll(true).show(ctx,|ui| {
            ui.label("Changes save immediately. Terminal history shortcuts scroll locally. Text fields, authentication and remote desktop capture suppress application actions.");
            ui.small("Native clipboard events keep their platform meaning. Command means Ctrl on Windows/Linux and Cmd on macOS.");
            if let Some(action)=self.shortcut_capture {ui.label(format!("Press a new shortcut for {}",action.label()));if ui.button("Cancel key capture").clicked(){self.shortcut_capture=None;}}
            if let Err(error)=self.store.preferences.shortcuts.validate(){ui.colored_label(ui.visuals().error_fg_color,error.0);}
            if !self.shortcut_error.is_empty(){ui.colored_label(ui.visuals().error_fg_color,&self.shortcut_error);}
            Grid::new("shortcut-bindings").num_columns(4).show(ui,|ui| {
                for action in Action::ALL {
                    ui.label(action.label());
                    let label=self.store.preferences.shortcuts.bindings.iter().find(|b|b.action==action).map(|b|b.chord.label()).unwrap_or_else(||"Unbound".into());
                    ui.label(label);
                    if ui.button("Change").clicked(){self.shortcut_capture=Some(action);self.shortcut_error.clear();}
                    if ui.button("Unbind").clicked(){self.store.preferences.shortcuts.unbind(action);self.persist();}
                    ui.end_row();
                }
            });
            if ui.button("Restore default shortcuts").clicked(){self.store.preferences.shortcuts=Default::default();self.shortcut_capture=None;self.shortcut_error.clear();self.persist();}
        });
        if !open {
            self.shortcut_editor_open = false;
            self.shortcut_capture = None;
        }
    }
}
pub(super) fn terminal_actions(
    ui: &mut Ui,
    t: &mut TerminalTab,
    p: &Preferences,
) -> Option<Action> {
    if !ui.memory(|m|m.has_focus(Id::new(("terminal-input",t.id))))
        || ui.ctx().data(|d|d.get_temp::<bool>(Id::new(BLOCKED))).unwrap_or(false)
        || ui.ctx().data(|d|d.get_temp::<bool>(Id::new("remote-desktop-keyboard-capture"))).unwrap_or(false)
        || ui.input(|i|i.events.iter().any(|e|matches!(e,Event::PointerButton{pos,pressed:true,..} if !ui.clip_rect().contains(*pos)))) {return None;}
    let actions: Vec<_> = ui.input(|i| {
        i.events
            .iter()
            .filter_map(|event| {
                if let Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = event
                {
                    p.shortcuts
                        .resolve(
                            &KeyChord::from_key(*key, *modifiers),
                            FocusContext::Terminal,
                        )
                        .filter(|a| a.terminal_view())
                } else {
                    None
                }
            })
            .collect()
    });
    let mut scroll = None;
    for action in actions {
        match action {
            Action::Copy => match t.selection.selected(&t.screen) {
                Ok(s) if !s.is_empty() => ui.ctx().copy_text(s),
                Err(e) => t.error = e,
                _ => {}
            },
            Action::Paste => {
                if t.ready_for_input()
                    && !ui.input(|i| {
                        i.events
                            .iter()
                            .any(|event| matches!(event, Event::Paste(_)))
                    })
                {
                    t.clipboard_request = true;
                }
            }
            Action::Find => {
                t.search_open = true;
                t.search_focus = true;
                ui.memory_mut(|m| m.request_focus(Id::new(("terminal-search", t.id))));
                ui.ctx().request_repaint();
            }
            action => scroll = Some(action),
        }
    }
    scroll
}
pub(super) fn is_application_key(key: Key, modifiers: Modifiers, p: &Preferences) -> bool {
    p.shortcuts
        .resolve(&KeyChord::from_key(key, modifiers), FocusContext::Terminal)
        .is_some()
}

pub(super) fn input_blocked(ctx: &Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(Id::new(BLOCKED)))
        .unwrap_or(false)
}

impl TerminalTab {
    pub(super) fn search_ui(&mut self, ui: &mut Ui) {
        if !self.search_open {
            return;
        }
        ui.horizontal(|ui| {
            let response = ui.add(
                TextEdit::singleline(&mut self.search)
                    .id(Id::new(("terminal-search", self.id)))
                    .hint_text("Find in terminal output"),
            );
            if std::mem::take(&mut self.search_focus) {
                response.request_focus();
            }
            let count = if self.search.is_empty() {
                0
            } else {
                self.screen.search_count(&self.search)
            };
            ui.label(format!("{count} matches"));
            if ui.small_button("×").on_hover_text("Close search").clicked() {
                self.search_open = false;
                ui.memory_mut(|m| m.request_focus(Id::new(("terminal-input", self.id))));
            }
        });
    }
}
