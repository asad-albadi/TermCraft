use super::*;

impl Workspace {
    pub(super) fn sync_remote_sidebar(&mut self) {
        let connection = self
            .tabs
            .get(self.active)
            .filter(|t| {
                self.page == Page::Terminal
                    && self.windows.window(t.id) == 0
                    && t.browser.as_ref().is_some_and(|b| b.visible)
            })
            .map(|t| t.id);
        if connection != self.sidebar_connection {
            self.sidebar_connection = connection;
            self.sidebar_remote_selected = connection.is_some();
            if connection.is_some() {
                self.store.preferences.sidebar_visible = true;
            }
        }
    }
    pub(super) fn top(&mut self, root: &mut Ui) {
        Panel::top("menus")
            .frame(
                Frame::new()
                    .fill(mist(root))
                    .inner_margin(Margin::symmetric(8, 0)),
            )
            .show(root, |ui| {
                egui::MenuBar::new().ui(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.menu_button("Sessions", |ui| {
                            if ui.button("New session").clicked() {
                                self.new_session("SSH");
                                ui.close();
                            }
                            if ui.button("Start local terminal").clicked() {
                                self.local();
                                ui.close();
                            }
                            ui.separator();
                            if ui.button("SSH keys and agents…").clicked() {
                                self.ssh_keys.open = true;
                                ui.close();
                            }
                            if ui.button("Credential vault…").clicked() {
                                self.credentials.open = true;
                                ui.close();
                            }
                            if ui.button("Shared session sources…").clicked() {
                                self.shared_sources.manager = true;
                                ui.close();
                            }
                            if ui.button("Import MobaXterm sessions…").clicked() {
                                self.choose_session_import(session_import::Format::Moba, ui.ctx());
                                ui.close();
                            }
                            if ui.button("Import PuTTY sessions…").clicked() {
                                self.choose_session_import(session_import::Format::Putty, ui.ctx());
                                ui.close();
                            }
                            if ui.button("Saved sessions").clicked() {
                                self.page = Page::Home;
                                ui.close();
                            }
                            if ui.button("Local files").clicked() {
                                self.page = Page::Files;
                                ui.close();
                            }
                        });
                        ui.menu_button("View", |ui| {
                            if ui.button("Home tab").clicked() {
                                self.page = Page::Home;
                                ui.close();
                            }
                            if ui
                                .checkbox(
                                    &mut self.store.preferences.sidebar_visible,
                                    "Show sidebar",
                                )
                                .changed()
                            {
                                self.persist();
                            }
                            if ui
                                .button(if self.windows.fullscreen(0) {
                                    "Restore main window"
                                } else {
                                    "Fullscreen main window"
                                })
                                .clicked()
                            {
                                self.toggle_window_fullscreen(ui.ctx(), 0);
                                ui.close();
                            }
                            if ui.button("Keyboard shortcuts…").clicked() {
                                self.shortcut_editor_open = true;
                                ui.close();
                            }
                            self.layout_picker(ui);
                            self.broadcast_toggle(ui);
                        });
                        ui.menu_button("X server", |ui| {
                            ui.label("Configure the managed X display or an installed X server.");
                            if ui.button("X server settings").clicked() {
                                self.page = Page::Servers;
                                self.servers_category = servers::Section::Xserver;
                                ui.close();
                            }
                        });
                        ui.menu_button("Tools", |ui| {
                            for (label, page) in [
                                ("Tools and plugins", Page::Tools),
                                ("Servers", Page::Servers),
                                ("SSH tunneling", Page::Tunnels),
                                ("Local files", Page::Files),
                            ] {
                                if ui.button(label).clicked() {
                                    self.page = page;
                                    ui.close();
                                }
                            }
                        });
                        ui.menu_button("Games", |ui| {
                            self.games_menu(ui);
                            ui.separator();
                            self.screensaver_settings(ui);
                        });
                        ui.menu_button("Settings", |ui| {
                            if ui.button("Configuration").clicked() {
                                self.page = Page::Settings;
                                ui.close();
                            }
                        });
                        let macro_menu = if self.macro_recording() {
                            "Macros · Recording"
                        } else {
                            "Macros"
                        };
                        ui.menu_button(macro_menu, |ui| {
                            self.macro_controls(ui);
                            if ui.button("Manage macros").clicked() {
                                self.page = Page::Macros;
                                ui.close();
                            }
                        });
                        ui.menu_button("Help", |ui| {
                            ui.label(crate::ui::fluent::emphasized(
                                ui.ctx(),
                                RichText::new("TermCraft personal workspace"),
                            ));
                            if ui.button("Feature parity and known differences").clicked() {
                                self.parity_open = true;
                                ui.close();
                            }
                            ui.hyperlink_to(
                                "Original documentation",
                                "https://mobaxterm.mobatek.net/documentation.html",
                            );
                            ui.label("Create a Session to connect to a remote host.");
                            ui.label("Double-click a saved session or focus it and press Enter.");
                            ui.label(self.shortcut_hint(Action::NewSession, "New session"));
                            ui.label(self.shortcut_hint(Action::NewTerminal, "New local terminal"));
                            ui.separator();
                            ui.label("System clients provide remote protocols and X11.");
                        });
                        ui.separator();
                        self.appearance_picker(ui);
                        if self.clipboard_read.is_some()
                            && icons::button(ui, "disconnect", "Cancel clipboard read", false)
                                .clicked()
                        {
                            self.clipboard_read = None;
                        }
                    });
                });
            });
        Panel::top("commands")
            .frame(
                Frame::new()
                    .fill(mist(root))
                    .stroke(Stroke::new(1., line(root)))
                    .inner_margin(Margin::symmetric(8, 5)),
            )
            .show(root, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(4., 4.);
                    if icons::tile(
                        ui,
                        "desktop",
                        "Session",
                        &self.shortcut_hint(Action::NewSession, "New session"),
                        None,
                    )
                    .clicked()
                    {
                        self.new_session("SSH");
                    }
                    if icons::tile(ui, "desktop", "Servers", "Servers", None).clicked() {
                        self.page = Page::Servers;
                    }
                    if icons::tile(ui, "tools", "Tools", "Tools", None).clicked() {
                        self.page = Page::Tools;
                    }
                    if icons::tile(ui, "games", "Games", "Games", None).clicked() {
                        self.games.play.open();
                    }
                    let sidebar = self.store.preferences.sidebar_visible;
                    if icons::tile(
                        ui,
                        "folder",
                        "Sessions",
                        &self.shortcut_hint(
                            Action::ToggleSidebar,
                            if sidebar {
                                "Hide sessions sidebar"
                            } else {
                                "Show sessions sidebar"
                            },
                        ),
                        Some(sidebar),
                    )
                    .clicked()
                    {
                        self.store.preferences.sidebar_visible = !sidebar;
                        self.persist();
                    }
                    if icons::tile(ui, "terminal", "Split", "Toggle split terminals", None)
                        .clicked()
                    {
                        self.toggle_layout();
                        if !self.tabs.is_empty() {
                            self.page = Page::Terminal;
                        }
                    }
                    if icons::tile(
                        ui,
                        "terminal",
                        "MultiExec",
                        if self.broadcast {
                            "Turn MultiExec off"
                        } else {
                            "Choose MultiExec targets"
                        },
                        Some(self.broadcast),
                    )
                    .clicked()
                    {
                        self.set_broadcast_enabled(!self.broadcast);
                    }
                    if icons::tile(ui, "link", "Tunneling", "SSH tunneling", None).clicked() {
                        self.page = Page::Tunnels;
                    }
                    if icons::tile(
                        ui,
                        "folder",
                        "Packages",
                        "Manage TermCraft packages and inspect legacy packages",
                        None,
                    )
                    .clicked()
                    {
                        self.page = Page::Tools;
                        self.tools_category = tools::Category::Packages;
                    }
                    if icons::tile(ui, "settings", "Settings", "Settings", None).clicked() {
                        self.page = Page::Settings;
                    }
                    let help = icons::tile(ui, "help", "Help", "Help and keyboard shortcuts", None);
                    egui::Popup::menu(&help).show(|ui| {
                        ui.set_max_width((ui.ctx().content_rect().width() - 32.).clamp(1., 320.));
                        ui.label(crate::ui::fluent::emphasized(
                            ui.ctx(),
                            RichText::new("Keyboard shortcuts"),
                        ));
                        ui.label(self.shortcut_hint(Action::NewSession, "New session"));
                        ui.label(self.shortcut_hint(Action::NewTerminal, "New local terminal"));
                        ui.label("Double-click a saved session or focus it and press Enter.");
                    });
                    if icons::tile(
                        ui,
                        "desktop",
                        "X server",
                        "Configure the system X server",
                        None,
                    )
                    .clicked()
                    {
                        self.page = Page::Servers;
                        self.servers_category = servers::Section::Xserver;
                    }
                    if icons::tile(ui, "exit", "Exit", "Exit TermCraft", None).clicked() {
                        ui.ctx().send_viewport_cmd(ViewportCommand::Close);
                    }
                });
            });
        Panel::bottom("status")
            .frame(
                Frame::new()
                    .fill(mist(root))
                    .inner_margin(Margin::symmetric(16, 7)),
            )
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    let palette = fluent::palette(ui);
                    let error = self.status_error;
                    let text = if !error && self.status_time.elapsed() > Duration::from_secs(12) {
                        "Ready"
                    } else {
                        &self.status
                    };
                    dot(
                        ui,
                        if error {
                            palette.error
                        } else {
                            palette.secondary_text
                        },
                    );
                    let summary = format!(
                        "{} connections    •    {} saved sessions",
                        self.tabs.iter().filter(|t| t.running()).count(),
                        self.store.sessions.len()
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(summary)
                                .size(12.)
                                .color(palette.secondary_text),
                        );
                        if let Some(diagnostic) = &self.settings_load_error {
                            ui.colored_label(palette.error, "Settings damaged")
                                .fluent_hint(diagnostic);
                        }
                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            ui.add(
                                Label::new(RichText::new(text).size(12.).color(if error {
                                    palette.error
                                } else {
                                    palette.secondary_text
                                }))
                                .truncate(),
                            )
                            .fluent_hint(text);
                        });
                    });
                });
            });
    }
    pub(super) fn connection_commands(&mut self, ui: &mut Ui) {
        if self.tabs.is_empty() || self.page != Page::Terminal {
            return;
        }
        self.active = self.active.min(self.tabs.len() - 1);
        let mut layout_toggle = false;
        let mut duplicate = false;
        let mut reconnect = false;
        let mut cancel_startup = false;
        let mut save_output = false;
        let mut log = false;
        let mut copy = false;
        let mut clear = false;
        let compact_commands = ui.available_width() < 360.;
        ui.horizontal(|ui| {
            let index=self.active;
            let t=&self.tabs[index];

            let status = if t.terminal_starting(){ "Starting" } else if t.running(){ "Connected" } else { "Disconnected" };
            ui.label(RichText::new("●").color(if t.running(){accent(ui)}else{muted(ui)})).fluent_hint(format!("{status} · {}",t.connection_status));
            let browser = t.web_browser.is_some();
            let title_width = (ui.available_width() - if compact_commands {45.} else {120.}).max(1.);
            if !browser {
                ui.allocate_ui_with_layout(vec2(title_width,32.),Layout::left_to_right(Align::Center),|ui| {
                    ui.add(Label::new(RichText::new(&t.session.name).strong()).truncate())
                        .fluent_hint(format!("{} · {status} · {}",t.session.name,t.connection_status));
                });
            } else {
                ui.allocate_ui_with_layout(vec2(title_width,32.),Layout::left_to_right(Align::Center),|ui| {
                    super::web_browser::toolbar(ui,&mut self.tabs[index]);
                });
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                icons::menu(ui,"menu","Connection actions", |ui| {
                    if self.tabs[index].desktop.is_some() {ui.menu_button("Remote desktop controls",|ui|super::desktop::toolbar(ui,&mut self.tabs[index]));}
                    if self.tabs[index].session.protocol=="SFTP" {ui.menu_button("SFTP commands",|ui|self.sftp_compatibility_commands(ui,index));}
                    if self.tabs[index].session.protocol!="Browser" && icons::action(ui,"Find").clicked() { self.tabs[index].search_open=!self.tabs[index].search_open; ui.close(); }
                    if self.tabs[index].terminal_starting() {
                        if icons::action(ui,"Cancel startup").clicked() { cancel_startup=true; ui.close(); }
                    } else if icons::action(ui,"Reconnect").clicked() { reconnect=true; ui.close(); }
                    self.layout_picker(ui);
                    layout_toggle=ui.button("Toggle split").clicked();
                    let t=&mut self.tabs[index];
                    if t.session.protocol=="SSH" {
                        if let Some(browser)=&mut t.browser {ui.checkbox(&mut browser.visible,"Remote files");}
                        else {ui.small("OpenSSH compatibility uses the saved session’s companion SFTP action for transfers.");}
                    }
                    if t.session.protocol != "Browser" {
                        ui.menu_button("Terminal profile",|ui|{
                            super::terminal_settings::overrides(ui,&mut t.session.terminal,&self.store.preferences.terminal_settings());
                        });
                        ui.menu_button("Keyword highlighting",|ui|{
                            ui.selectable_value(&mut t.session.highlight_enabled,None,"Global");
                            ui.selectable_value(&mut t.session.highlight_enabled,Some(true),"On");
                            ui.selectable_value(&mut t.session.highlight_enabled,Some(false),"Off");
                        });
                        ui.small(format!("Open for {} seconds",t.started.elapsed().as_secs()));
                        super::pages::history_status(ui,t);
                        copy = ui.button("Copy terminal output").clicked();
                        save_output = ui.button("Save output to file").clicked();
                        clear = ui.button("Clear screen").clicked();
                        log = ui
                            .button(if t.log.is_some() {
                                "Stop live logging"
                            } else {
                                "Start live logging"
                            })
                            .clicked();
                    }
                    duplicate = ui.button("Duplicate session").clicked();
                });
                if !compact_commands && !browser {
                    let t=&mut self.tabs[index];
                    if t.terminal_starting() {
                        cancel_startup = icons::button(ui,"disconnect","Cancel startup",false).clicked();
                    } else {
                        reconnect = icons::button(ui,"refresh","Reconnect",false).clicked();
                    }
                    if t.session.protocol != "Browser" && icons::button(ui,"search","Find",false).clicked() {
                        t.search_open = !t.search_open;
                    }
                }

            });
        });
        if layout_toggle {
            self.toggle_layout();
        }
        if cancel_startup {
            let tab = &mut self.tabs[self.active];
            if tab.pending_portable.take().is_some() {
                tab.connection_status = "Portable Unix startup cancelled".into();
            } else {
                tab.pending_mosh = None;
                tab.connection_status = "Mosh startup cancelled".into();
            }
        }
        if reconnect {
            self.reconnect_active();
        }
        if duplicate {
            let s = self.tabs[self.active].session.clone();
            let launch = self.tabs[self.active].launch.clone();
            self.start_with_launch(s, launch);
        }
        if clear {
            self.tabs[self.active].screen.clear();
        }
        if copy {
            ui.ctx().copy_text(self.tabs[self.active].screen.plain());
        }
        if save_output {
            if let Some(path) = rfd::FileDialog::new()
                .set_file_name("terminal-output.txt")
                .save_file()
            {
                match fs::write(path, self.tabs[self.active].screen.plain()) {
                    Ok(()) => self.message("Terminal output saved"),
                    Err(e) => self.message(e.to_string()),
                }
            }
        }
        if log {
            let t = &mut self.tabs[self.active];
            if t.log.is_some() {
                t.log = None;
            } else if let Some(path) = rfd::FileDialog::new()
                .set_file_name("session.log")
                .save_file()
            {
                match fs::OpenOptions::new().create(true).append(true).open(path) {
                    Ok(f) => t.log = Some(f),
                    Err(e) => self.message(e.to_string()),
                }
            }
        }
    }
}
