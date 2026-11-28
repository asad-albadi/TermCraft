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
        Panel::top("workspace-bar")
            .frame(
                Frame::new()
                    .fill(mist(root))
                    .inner_margin(Margin::symmetric(6, 4)),
            )
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(3., 0.);
                    icons::menu(ui, "menu", "App menu", |ui| {
                        self.app_menu(ui);
                        ui.separator();
                        ui.menu_button("Commands", |ui| {
                            ScrollArea::vertical()
                                .id_salt("all-commands")
                                .max_height((ui.ctx().content_rect().height() - 48.).max(120.))
                                .show(ui, |ui| {
                                    for (command, _, label) in Self::CHROME_COMMANDS {
                                        if icons::action(ui, label).clicked() {
                                            self.chrome_command(command, ui.ctx());
                                            ui.close();
                                        }
                                    }
                                });
                        });
                    });
                    let connection = self.page == Page::Terminal
                        && self
                            .tabs
                            .get(self.active)
                            .is_some_and(|t| self.windows.window(t.id) == 0);
                    let browser = connection
                        && self.tabs[self.active].session.protocol == "Browser"
                        && self.tabs[self.active].web_browser.is_some();
                    let browser_width = if browser {
                        (ui.available_width() * 0.35).clamp(220., 600.)
                    } else {
                        0.
                    };
                    let clipboard = self.clipboard_read.is_some();
                    let reserve = if connection { 150. } else { 0. }
                        + if clipboard { 35. } else { 0. }
                        + browser_width;
                    let count = ((ui.available_width() - 300. - reserve) / 35.)
                        .floor()
                        .clamp(
                            if browser { 0. } else { 2. },
                            Self::CHROME_COMMANDS.len() as f32,
                        ) as usize;
                    for &(command, kind, label) in Self::CHROME_COMMANDS.iter().take(count) {
                        let selected = match command {
                            "sidebar" => self.store.preferences.sidebar_visible,
                            "broadcast" => self.broadcast,
                            "servers" => self.page == Page::Servers,
                            "tools" => {
                                self.page == Page::Tools
                                    && self.tools_category != tools::Category::Packages
                            }
                            "packages" => {
                                self.page == Page::Tools
                                    && self.tools_category == tools::Category::Packages
                            }
                            "settings" => self.page == Page::Settings,
                            "tunnel" => self.page == Page::Tunnels,
                            _ => false,
                        };
                        let response = icons::button(ui, kind, label, selected);
                        if matches!(command, "sidebar" | "broadcast") {
                            response.widget_info(|| {
                                WidgetInfo::selected(
                                    WidgetType::Checkbox,
                                    response.enabled(),
                                    selected,
                                    label,
                                )
                            });
                        }
                        if response.clicked() {
                            self.chrome_command(command, ui.ctx());
                        }
                    }
                    let tabs_width = (ui.available_width() - reserve - 40.).max(110.);
                    ui.allocate_ui_with_layout(
                        vec2(tabs_width, 36.),
                        Layout::left_to_right(Align::Center),
                        |ui| self.tab_bar(ui),
                    );
                    if browser
                        && self
                            .tabs
                            .get(self.active)
                            .is_some_and(|t| t.web_browser.is_some())
                    {
                        ui.allocate_ui_with_layout(
                            vec2(browser_width, 32.),
                            Layout::left_to_right(Align::Center),
                            |ui| super::web_browser::toolbar(ui, &mut self.tabs[self.active]),
                        );
                    }
                    icons::menu(ui, "theme", "Theme", |ui| {
                        let mut appearance = self.store.preferences.appearance;
                        for (value, label) in [
                            (Appearance::System, "System"),
                            (Appearance::Light, "Light"),
                            (Appearance::Dark, "Dark"),
                        ] {
                            if ui.selectable_value(&mut appearance, value, label).clicked() {
                                ui.close();
                            }
                        }
                        if appearance != self.store.preferences.appearance {
                            self.store.preferences.appearance = appearance;
                            self.apply_appearance(ui.ctx());
                            self.persist();
                        }
                    });
                    if clipboard
                        && icons::button(ui, "disconnect", "Cancel clipboard read", false).clicked()
                    {
                        self.clipboard_read = None;
                    }
                    if connection {
                        ui.allocate_ui_with_layout(
                            vec2(140., 32.),
                            Layout::left_to_right(Align::Center),
                            |ui| self.connection_commands(ui),
                        );
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

    fn app_menu(&mut self, ui: &mut Ui) {
        ui.menu_button("Sessions", |ui| {
            if icons::action(ui, "New session").clicked() {
                self.new_session("SSH");
                ui.close();
            }
            if icons::action(ui, "Start local terminal").clicked() {
                self.local();
                ui.close();
            }
            ui.separator();
            if icons::action(ui, "SSH keys and agents…").clicked() {
                self.ssh_keys.open = true;
                ui.close();
            }
            if icons::action(ui, "Credential vault…").clicked() {
                self.credentials.open = true;
                ui.close();
            }
            if icons::action(ui, "Shared session sources…").clicked() {
                self.shared_sources.manager = true;
                ui.close();
            }
            if icons::action(ui, "Import MobaXterm sessions…").clicked() {
                self.choose_session_import(session_import::Format::Moba, ui.ctx());
                ui.close();
            }
            if icons::action(ui, "Import PuTTY sessions…").clicked() {
                self.choose_session_import(session_import::Format::Putty, ui.ctx());
                ui.close();
            }
            if icons::action(ui, "Saved sessions").clicked() {
                self.page = Page::Home;
                ui.close();
            }
            if icons::action(ui, "Local files").clicked() {
                self.page = Page::Files;
                ui.close();
            }
        });
        ui.menu_button("View", |ui| {
            if icons::action(ui, "Home tab").clicked() {
                self.page = Page::Home;
                ui.close();
            }
            if ui
                .checkbox(&mut self.store.preferences.sidebar_visible, "Show sidebar")
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
            if icons::action(ui, "Keyboard shortcuts…").clicked() {
                self.shortcut_editor_open = true;
                ui.close();
            }
            self.layout_picker(ui);
            self.broadcast_toggle(ui);
        });
        ui.menu_button("X server", |ui| {
            ui.label("Configure the managed X display or an installed X server.");
            if icons::action(ui, "X server settings").clicked() {
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
            if icons::action(ui, "Configuration").clicked() {
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
            if icons::action(ui, "Manage macros").clicked() {
                self.page = Page::Macros;
                ui.close();
            }
        });
        ui.menu_button("Help", |ui| {
            ui.label(crate::ui::fluent::emphasized(
                ui.ctx(),
                RichText::new("TermCraft personal workspace"),
            ));
            if icons::action(ui, "Feature parity and known differences").clicked() {
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
    }
    const CHROME_COMMANDS: &'static [(&'static str, &'static str, &'static str)] = &[
        ("new", "session", "New session"),
        ("servers", "servers", "Servers"),
        ("tools", "tools", "Tools"),
        ("games", "games", "Games"),
        ("sidebar", "sessions", "Sessions"),
        ("split", "split", "Toggle split"),
        ("broadcast", "broadcast", "MultiExec"),
        ("tunnel", "tunnel", "SSH tunneling"),
        ("packages", "packages", "Packages"),
        ("settings", "settings", "Settings"),
        ("help", "help", "Help"),
        ("xserver", "xserver", "X server"),
        ("exit", "exit", "Exit TermCraft"),
    ];
    fn chrome_command(&mut self, command: &str, ctx: &Context) {
        match command {
            "new" => self.new_session("SSH"),
            "servers" => self.page = Page::Servers,
            "tools" => self.page = Page::Tools,
            "games" => self.games.play.open(),
            "sidebar" => {
                self.store.preferences.sidebar_visible = !self.store.preferences.sidebar_visible;
                self.persist();
            }
            "split" => self.toggle_layout(),
            "broadcast" => self.set_broadcast_enabled(!self.broadcast),
            "tunnel" => self.page = Page::Tunnels,
            "packages" => {
                self.page = Page::Tools;
                self.tools_category = tools::Category::Packages;
            }
            "settings" => self.page = Page::Settings,
            "help" => self.parity_open = true,
            "xserver" => {
                self.page = Page::Servers;
                self.servers_category = servers::Section::Xserver;
            }
            "exit" => ctx.send_viewport_cmd(ViewportCommand::Close),
            _ => {}
        }
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
        let compact_commands = false;
        ui.horizontal(|ui| {
            let index=self.active;
            let t=&self.tabs[index];

            let status = if t.terminal_starting(){ "Starting" } else if t.running(){ "Connected" } else { "Disconnected" };
            ui.label(RichText::new("●").color(if t.running(){accent(ui)}else{muted(ui)})).fluent_hint(format!("{status} · {}",t.connection_status));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                icons::menu(ui,"menu","Connection actions", |ui| {
                    if compact_commands {
                        let t=&mut self.tabs[index];
                        if t.session.protocol != "Browser" && icons::button(ui,"search","Find",false).clicked() {
                            t.search_open = !t.search_open;
                            ui.close();
                        }
                        if t.terminal_starting() {
                            if ui.button("Cancel startup").clicked() { cancel_startup=true; ui.close(); }
                        } else if ui.button("Reconnect").clicked() { reconnect=true; ui.close(); }
                        ui.separator();
                    }
                    if self.tabs[index].desktop.is_some() {ui.menu_button("Remote desktop controls",|ui|super::desktop::toolbar(ui,&mut self.tabs[index]));}
                    if self.tabs[index].session.protocol=="SFTP" {ui.menu_button("SFTP commands",|ui|self.sftp_compatibility_commands(ui,index));}
                    if self.tabs[index].session.protocol!="Browser" && icons::action(ui,"Find").clicked() { self.tabs[index].search_open=!self.tabs[index].search_open; ui.close(); }
                    if icons::action(ui,"Reconnect").clicked() { reconnect=true; ui.close(); }
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
                if !compact_commands {
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
