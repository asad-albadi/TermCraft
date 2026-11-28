use super::*;

type RuntimeRows = std::sync::Arc<std::sync::OnceLock<Vec<(&'static str, String, String)>>>;

fn runtime_providers(ui: &mut Ui) {
    use harbor_workspace::runtime::{RuntimeCatalog, ToolId, ToolProvider};

    let refresh = ui
        .horizontal(|ui| {
            ui.label(crate::ui::fluent::emphasized(
                ui.ctx(),
                RichText::new("Runtime providers").size(18.),
            ));
            ui.button("Refresh").clicked()
        })
        .inner;
    let id = Id::new("termcraft-runtime-providers");
    let previous = ui.ctx().data(|data| data.get_temp::<RuntimeRows>(id));
    let rows = previous
        .filter(|rows| !refresh || rows.get().is_none())
        .unwrap_or_else(|| {
            let rows = RuntimeRows::default();
            let result = rows.clone();
            let ctx = ui.ctx().clone();
            ui.ctx().data_mut(|data| data.insert_temp(id, rows.clone()));
            std::thread::spawn(move || {
                let catalog = RuntimeCatalog::for_current_executable();
                let items = [
                    ("SSH", ToolId::Ssh),
                    ("SFTP", ToolId::Sftp),
                    ("SCP", ToolId::Scp),
                    ("RDP", ToolId::Rdp),
                    ("VNC", ToolId::Vnc),
                    ("FTP / FTPS", ToolId::Ftp),
                    ("Telnet", ToolId::Telnet),
                    ("Rlogin", ToolId::Rlogin),
                    ("Rsh", ToolId::Rsh),
                    ("Serial", ToolId::Serial),
                    ("Packaged Mosh", ToolId::Mosh),
                    ("Installed Mosh", ToolId::SystemMosh),
                    ("Packaged Chromium browser", ToolId::BundledChromium),
                    (
                        "Installed Chromium browser (compatibility)",
                        ToolId::SystemChromium,
                    ),
                    ("Managed X display", ToolId::XServer),
                    ("XDMCP", ToolId::Xdmcp),
                    ("Installed SSH", ToolId::OpenSsh),
                    ("Installed SFTP", ToolId::OpenSftp),
                    ("Installed RDP", ToolId::ExternalRdp),
                    ("Installed VNC", ToolId::ExternalVnc),
                    ("Local shell", ToolId::Shell),
                    ("SSH keys", ToolId::KeyGenerator),
                    ("Live packet capture", ToolId::PcapLibrary),
                ];
                let _ = result.set(
                    items
                        .into_iter()
                        .map(|(name, tool)| {
                            if tool == ToolId::Rdp && cfg!(debug_assertions) {
                                let path =
                                    harbor_workspace::desktop::DesktopConfig::default_engine_path();
                                if path.is_file() {
                                    return (
                                        name,
                                        "Development helper".into(),
                                        path.display().to_string(),
                                    );
                                }
                                return (
                                    name,
                                    "Unavailable".into(),
                                    "Build the development RDP helper with scripts/run-dev.sh."
                                        .into(),
                                );
                            }
                            let provider = catalog
                                .as_ref()
                                .map_err(Clone::clone)
                                .and_then(|catalog| catalog.resolve(tool));
                            let (label, detail) = match provider {
                                Ok(ToolProvider::Embedded) => ("Embedded".into(), String::new()),
                                Ok(ToolProvider::Bundled { path, version }) => {
                                    (format!("Packaged · {version}"), path.display().to_string())
                                }
                                Ok(ToolProvider::System { path, version }) => {
                                    ("Installed".into(), format!("{version}\n{}", path.display()))
                                }
                                Err(error) => ("Unavailable".into(), error),
                            };
                            (name, label, detail)
                        })
                        .collect(),
                );
                ctx.request_repaint();
            });
            rows
        });
    if let Some(rows) = rows.get() {
        Grid::new("clients")
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                for (name, label, detail) in rows {
                    ui.label(*name);
                    ui.label(label).fluent_hint(detail);
                    ui.end_row();
                }
            });
    } else {
        ui.spinner();
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SettingsCategory {
    Appearance,
    Terminal,
    Safety,
    Storage,
    About,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum SessionSection {
    Connection,
    Advanced,
    Security,
    Display,
    Redirection,
    Terminal,
    Network,
    Bookmark,
}

impl SessionSection {
    const ALL: [Self; 8] = [
        Self::Connection,
        Self::Advanced,
        Self::Security,
        Self::Display,
        Self::Redirection,
        Self::Network,
        Self::Terminal,
        Self::Bookmark,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::Connection => "Connection",
            Self::Advanced => "Advanced",
            Self::Security => "Security",
            Self::Display => "Display & input",
            Self::Redirection => "Redirection",
            Self::Network => "Network",
            Self::Terminal => "Terminal",
            Self::Bookmark => "Bookmark",
        }
    }
    fn available(self, protocol: &str) -> bool {
        match self {
            Self::Connection | Self::Bookmark => true,
            Self::Advanced => matches!(
                protocol,
                "SSH" | "Serial" | "Mosh" | "FTP" | "VNC" | "Browser" | "XDMCP" | "Local"
            ),
            Self::Security | Self::Display | Self::Redirection => protocol == "RDP",
            Self::Network => matches!(protocol, "SSH" | "SFTP" | "Mosh" | "Telnet" | "RDP" | "VNC"),
            Self::Terminal => protocol != "Browser",
        }
    }
    fn choices(&mut self, ui: &mut Ui, protocol: &str) {
        for section in Self::ALL
            .into_iter()
            .filter(|section| section.available(protocol))
        {
            ui.selectable_value(self, section, section.label());
        }
    }
}

fn protocol_choices(ui: &mut Ui, current: &str) -> Option<&'static str> {
    let mut chosen = None;
    ui.horizontal_wrapped(|ui| {
        for protocol in PROTOCOLS {
            let kind = match *protocol {
                "SSH" | "SFTP" => "lock",
                "RDP" | "VNC" | "XDMCP" => "desktop",
                "Serial" => "serial",
                "FTP" => "folder",
                "Browser" => "link",
                _ => "terminal",
            };
            if tool_button(ui, kind, protocol, protocol, Some(current == *protocol)).clicked() {
                chosen = Some(*protocol);
            }
        }
    });
    chosen
}

impl Workspace {
    fn session_navigation(&mut self, ui: &mut Ui, s: &mut Session, size: fluent::DialogSize) {
        use SessionSection::*;
        let compact = ui.ctx().content_rect().height() < 550. && size.width < 840.;
        let chosen = if compact {
            let mut chosen = None;
            ui.horizontal(|ui| {
                ui.label("Protocol");
                ui.menu_button(&s.protocol, |ui| {
                    ui.set_width(size.width.min(360.) - 24.);
                    chosen = protocol_choices(ui, &s.protocol);
                    if chosen.is_some() {
                        ui.close();
                    }
                });
                ComboBox::from_id_salt("session-section")
                    .selected_text(self.session_section.label())
                    .show_ui(ui, |ui| {
                        self.session_section.choices(ui, &s.protocol);
                    });
            });
            chosen
        } else {
            protocol_choices(ui, &s.protocol)
        };
        if let Some(protocol) = chosen
            && s.protocol != protocol
        {
            self.session_section = Connection;
            s.protocol = protocol.into();
            s.port = protocol_port(protocol);
            if protocol == "Mosh" && self.editing.is_none() {
                s.mosh_provider = harbor_workspace::model::MoshProvider::Bundled;
            }
            if protocol == "FTP" {
                s.ftp_options = harbor_workspace::protocols::ftp_actor::SessionOptions::embedded();
            }
            if protocol == "Browser" {
                s.browser_options.embedded = true;
                if self.editing.is_none() {
                    s.browser_options.runtime = harbor_workspace::browser::Runtime::BundledChromium;
                }
            }
        }
        if !compact {
            ui.separator();
            ui.horizontal_wrapped(|ui| self.session_section.choices(ui, &s.protocol));
        }
        ui.separator();
    }
    fn session_fields(&mut self, ui: &mut Ui, s: &mut Session, section: SessionSection) {
        use SessionSection::*;
        ui.set_width(ui.available_width());
        if section == Bookmark {
            fluent::form_text(ui, "Session name", &mut s.name, "Optional display name");
            fluent::form_text(ui, "Folder", &mut s.folder, "My sessions");
        }
        if section == Connection {
            match s.protocol.as_str() {
                "Local" => {
                    fluent::form_text(
                        ui,
                        "Shell executable",
                        &mut s.command,
                        "Use the system default",
                    );
                    session_path(
                        ui,
                        "Start folder",
                        &mut s.directory,
                        "",
                        rfd::FileDialog::pick_folder,
                    );
                }
                "Serial" => {
                    fluent::form_text(
                        ui,
                        "Device / COM port",
                        &mut s.serial_device,
                        if cfg!(windows) {
                            "COM3"
                        } else {
                            "/dev/ttyUSB0"
                        },
                    );
                    fluent::form_row(ui, "Baud rate", |ui, label| {
                        ui.add(DragValue::new(&mut s.baud).range(50..=4_000_000))
                            .labelled_by(label)
                    });
                }
                "Browser" => {
                    fluent::form_text(ui, "Website address", &mut s.host, "https://example.com");
                }
                _ => {
                    let host =
                        fluent::form_text(ui, "Remote host", &mut s.host, "Hostname or IP address");
                    fluent::focus_on_entry(ui, &host, Id::new("session-form"));
                    fluent::form_text(ui, "Username", &mut s.user, "Use the client default");
                    fluent::form_row(ui, "Port", |ui, label| {
                        ui.add(DragValue::new(&mut s.port).range(1..=65535))
                            .labelled_by(label)
                    });
                }
            }

            if s.protocol == "Rsh" {
                fluent::form_text(
                    ui,
                    "Remote command",
                    &mut s.command,
                    "Required command for Rsh",
                );
            }
            if s.protocol == "Telnet" {
                ui.small("Direct Telnet connections are unencrypted. To configure an SSH gateway, open Network and select SSH gateway.");
            } else if matches!(s.protocol.as_str(), "Rlogin" | "Rsh") {
                ui.small("Direct connections are unencrypted. This protocol requires a permitted reserved local source port.");
            }
        }
        if section == Advanced && s.protocol == "FTP" {
            super::ftp_settings::session(ui, s);
        }
        if section == Advanced && s.protocol == "Local" {
            super::xserver::session_settings(ui, s);
        }
        if section == Advanced && s.protocol == "Serial" {
            super::serial_settings::session(ui, s);
        }
        if section == Advanced && s.protocol == "Mosh" {
            use harbor_workspace::model::MoshProvider;
            fluent::form_row(ui, "SSH port", |ui, label| {
                ui.add(DragValue::new(&mut s.mosh_ssh_port).range(1..=65535))
                    .labelled_by(label)
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Mosh provider");
                ui.selectable_value(
                    &mut s.mosh_provider,
                    MoshProvider::Bundled,
                    "Packaged client",
                );
                ui.selectable_value(
                    &mut s.mosh_provider,
                    MoshProvider::System,
                    "Installed client",
                );
            });
            if s.mosh_provider == MoshProvider::Bundled {
                ui.small("Requires the Arch Linux x86-64 Mosh payload, system OpenSSH and Perl, and UDP access to the Mosh server.");
            } else {
                ui.small("Uses the system Mosh installation and system OpenSSH.");
            }
        }
        if section == Advanced && s.protocol == "SSH" {
            ui.horizontal_wrapped(|ui| {
                ui.label("SSH engine");
                ui.selectable_value(
                    &mut s.ssh_engine,
                    SshEngine::Embedded,
                    "Embedded + remote files",
                );
                ui.selectable_value(
                    &mut s.ssh_engine,
                    SshEngine::OpenSsh,
                    "OpenSSH compatibility",
                );
            });
            if s.ssh_engine == SshEngine::OpenSsh {
                ui.label("Uses installed OpenSSH configuration. Graphical remote files are unavailable in this mode.");
            } else {
                use harbor_workspace::remote_files::capabilities::RemoteBackendKind;
                ui.horizontal_wrapped(|ui| {
                    ui.label("Remote files");
                    ui.selectable_value(
                        &mut s.remote_backend,
                        RemoteBackendKind::Sftp,
                        "SFTP (default)",
                    );
                    ui.selectable_value(
                        &mut s.remote_backend,
                        RemoteBackendKind::Scp,
                        "SCP compatibility",
                    );
                });
                if s.remote_backend == RemoteBackendKind::Scp {
                    ui.small("SCP requires Python 3 on the remote host.");
                }
            }
        }
        if section == Network && matches!(s.protocol.as_str(), "SSH" | "SFTP" | "Mosh") {
            ui.add_space(5.);
            CollapsingHeader::new("SSH options")
                .default_open(true)
                .show(ui, |ui| {
                    session_path(
                        ui,
                        "Private key",
                        &mut s.key,
                        "Optional key file",
                        rfd::FileDialog::pick_file,
                    );
                    fluent::form_text(ui, "Jump host", &mut s.jump, "user@bastion:22");
                    if s.protocol == "SSH" {
                        fluent::form_row(ui, "SSH agent", |ui, _| {
                            ui.horizontal_wrapped(|ui| {
                                ui.selectable_value(&mut s.agent_mode, AgentMode::System, "System");
                                ui.selectable_value(
                                    &mut s.agent_mode,
                                    AgentMode::Disabled,
                                    "Disabled",
                                );
                                if s.ssh_engine == SshEngine::Embedded {
                                    ui.selectable_value(
                                        &mut s.agent_mode,
                                        AgentMode::Owned,
                                        "TermCraft-owned",
                                    );
                                    if cfg!(windows) {
                                        ui.selectable_value(
                                            &mut s.agent_mode,
                                            AgentMode::Pageant,
                                            "Pageant",
                                        );
                                    }
                                }
                                if ui.button("Manage keys…").clicked() {
                                    self.ssh_keys.open = true;
                                }
                            })
                        });
                        ui.small(
                            "An explicit private-key file takes precedence over agent identities.",
                        );
                        fluent::form_text(
                            ui,
                            "Remote command",
                            &mut s.command,
                            "Optional startup command",
                        );
                        super::xserver::session_settings(ui, s);
                    }
                    fluent::form_row(ui, "Transfer", |ui, label| {
                        ui.checkbox(&mut s.compression, "Enable compression")
                            .labelled_by(label)
                    });
                });
            ui.menu_button("Credential help",|ui|{ui.label("Embedded SSH asks for credentials in Remote files. OpenSSH asks in the terminal. Saving passwords requires opting into the credential vault.");});
        }
        if section == Network && matches!(s.protocol.as_str(), "SSH" | "SFTP" | "Mosh") {
            super::ssh_options::session(ui, s, &self.store.preferences.ssh_defaults);
        }
        if section == Network && matches!(s.protocol.as_str(), "RDP" | "VNC" | "Telnet") {
            CollapsingHeader::new("SSH gateway").show(ui,|ui|{
           fluent::form_text(ui,"Gateway chain",&mut s.jump,"user@bastion:22 (optional; comma-separated hops)");
           if !s.jump.trim().is_empty(){fluent::form_text(ui,"Gateway private key",&mut s.key,"Optional local SSH key file");ui.small("The gateway is verified and authenticated separately. Desktop certificates and credentials remain bound to the target endpoint.");}
       });
            if !s.jump.trim().is_empty() {
                super::ssh_options::session(ui, s, &self.store.preferences.ssh_defaults);
            }
        }
        if section == Security && s.protocol == "RDP" {
            use harbor_workspace::model::DesktopEngine;
            ui.horizontal_wrapped(|ui| {
                ui.label("Desktop engine");
                ui.selectable_value(
                    &mut s.desktop_engine,
                    DesktopEngine::Embedded,
                    if cfg!(debug_assertions) {
                        "Development helper"
                    } else {
                        "Packaged helper"
                    },
                );
                ui.selectable_value(
                    &mut s.desktop_engine,
                    DesktopEngine::External,
                    "Installed client",
                );
            });
        }
        if section == Display && s.protocol == "RDP" {
            ui.horizontal_wrapped(|ui| {
                ui.label("Desktop size");
                ui.add(DragValue::new(&mut s.width).range(200..=8192));
                ui.label("×");
                ui.add(DragValue::new(&mut s.height).range(200..=8192));
            });
        }
        if s.protocol == "RDP" && matches!(section, Security | Display | Redirection) {
            super::rdp_settings::session(ui, s, section);
        }
        if section == Advanced && s.protocol == "VNC" {
            super::vnc_settings::session(ui, s);
        }

        if (section == Display && s.protocol == "RDP")
            || (section == Advanced && s.protocol == "VNC")
        {
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut s.fullscreen, "Full screen")
                    .fluent_hint("Enter fullscreen when the connected desktop is first displayed.");
                ui.checkbox(&mut s.view_only, "View only");
            });
            ui.label(RichText::new(if s.protocol=="VNC"&&s.vnc_options.embedded{"VNC opens inside the workspace. The view fits server dimensions; server-driven resize is supported."}else if s.protocol=="RDP"&&s.desktop_engine==harbor_workspace::model::DesktopEngine::Embedded{"RDP opens inside the workspace using NLA and verified TLS. The view fits the negotiated desktop."}else{"Opens in an installed desktop client: Windows Remote Desktop, FreeRDP, TigerVNC, or macOS Screen Sharing."}).size(12.).color(muted(ui)));
        }
        if section == Advanced && s.protocol == "Browser" {
            super::web_browser::settings(ui, s);
        }
        if s.protocol == "XDMCP" {
            if section == Advanced {
                super::xdmcp::settings(ui, s, &mut self.xdmcp_discovery);
            } else {
                super::xdmcp::sync_editor(s, &mut self.xdmcp_discovery);
            }
        } else {
            self.xdmcp_discovery.end_editor();
        }
        if section == Terminal && super::macro_events::supported_session(s) {
            ui.horizontal_wrapped(|ui| {
                ui.label("Startup macro");
                ComboBox::from_id_salt("startup-macro")
                    .selected_text(s.startup_macro.as_deref().unwrap_or("None"))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut s.startup_macro, None, "None");
                        for value in &self.store.macros {
                            ui.selectable_value(
                                &mut s.startup_macro,
                                Some(value.name.clone()),
                                &value.name,
                            );
                        }
                    });
            });
        }
        if section == Terminal && s.protocol != "Browser" {
            CollapsingHeader::new("Terminal profile overrides").show(ui,|ui|{ui.small("Checked options apply when you open this saved session. Unchecked options inherit the global profile.");super::terminal_settings::overrides(ui,&mut s.terminal,&self.store.preferences.terminal_settings());});
            ui.horizontal_wrapped(|ui| {
                ui.label("Keyword highlighting");
                ui.selectable_value(&mut s.highlight_enabled, None, "Global setting");
                ui.selectable_value(&mut s.highlight_enabled, Some(true), "On");
                ui.selectable_value(&mut s.highlight_enabled, Some(false), "Off");
            });
        }
    }
    pub(super) fn session_dialog(&mut self, ctx: &Context) {
        let Some(mut s) = self.session_form.take() else {
            self.xdmcp_discovery.end_editor();
            return;
        };
        let mut cancel = false;
        let mut connect = false;
        let mut save = false;
        let footer_extra = if ctx.content_rect().width() < 424. {
            40.
        } else {
            0.
        };
        let size = super::fluent::dialog_size(ctx, 940., 182. + footer_extra);
        let response = super::fluent::modal(
            ctx,
            Id::new("session-form"),
            if self.editing.is_some() {
                "Edit session"
            } else {
                "New session"
            },
            size,
            |ui| {
                let navigation_start = ui.cursor().top();
                self.session_navigation(ui, &mut s, size);
                let section = self.session_section;
                let nav_height = ui.cursor().top() - navigation_start;
                ScrollArea::vertical()
                    .id_salt(("session-settings-body", section))
                    .max_height((size.body_height - nav_height).max(1.))
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        self.session_fields(ui, &mut s, section);
                    });
                ui.add_space(8.);
                ui.checkbox(&mut self.save_session, "Save this session");
                if !self.form_error.is_empty() {
                    ui.colored_label(super::fluent::palette(ui).error, &self.form_error);
                }
                ui.separator();
                let [launch, store, safe] =
                    fluent::dialog_actions(ui, ["Connect", "Save only", "Cancel"], Some(0));
                connect = launch.clicked();
                save = store.clicked();
                cancel = safe.clicked();
            },
        );
        if cancel
            || (!self.ssh_keys.open
                && response.is_top_modal
                && !response.any_popup_open
                && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)))
        {
            self.xdmcp_discovery.end_editor();
            return;
        }
        if connect || save {
            let valid = if s.protocol == "Browser" {
                harbor_workspace::browser::validated_address(&s.host, s.browser_options.embedded)
                    .and_then(|_| {
                        if s.browser_options.embedded {
                            harbor_workspace::browser::validated_proxy(&s.browser_options.proxy)?;
                        }
                        Ok(())
                    })
            } else if matches!(s.protocol.as_str(), "Local" | "Serial") {
                Ok(())
            } else {
                validate_host(&s.host).and_then(|_| validate_user(&s.user))
            };
            let valid = valid.and_then(|_| {
                if matches!(s.protocol.as_str(), "SSH" | "SFTP" | "Mosh") {
                    s.ssh_options.validate(&self.store.preferences.ssh_defaults)
                } else {
                    Ok(())
                }
            });
            let valid = valid.and_then(|_| {
                if s.protocol == "RDP" {
                    s.rdp_options.validate()
                } else {
                    Ok(())
                }
            });
            let valid = valid.and_then(|_| {
                if s.protocol == "VNC" {
                    s.vnc_options.validate()
                } else {
                    Ok(())
                }
            });
            let valid = valid.and_then(|_| {
                if s.protocol == "Serial" {
                    s.serial_line.settings(s.baud).validate()
                } else {
                    Ok(())
                }
            });
            if let Err(e) = valid {
                self.form_error = e;
                if s.protocol == "XDMCP" {
                    self.xdmcp_discovery.finish_resolution(&mut s, ctx);
                }
                self.session_form = Some(s);
                return;
            }
            if s.protocol == "Serial" && s.serial_device.trim().is_empty() {
                self.form_error = "Choose a serial device or COM port.".into();
                self.session_form = Some(s);
                return;
            }
            self.xdmcp_discovery.end_editor();
            if self.save_session || save {
                if s.folder.trim().is_empty() {
                    s.folder = "My sessions".into();
                }
                if s.name.trim().is_empty() {
                    s.name = if s.protocol == "Local" {
                        "Local terminal".into()
                    } else if s.protocol == "Serial" {
                        s.serial_device.clone()
                    } else if s.protocol == "Browser" && s.host.trim().is_empty() {
                        "Browser".into()
                    } else {
                        s.host.clone()
                    };
                }
                if let Some(i) = self.editing {
                    s.id = self.store.sessions[i].id.clone();
                    self.store.sessions[i] = s.clone();
                } else {
                    s.id = harbor_workspace::model::new_session_id();
                    self.store.sessions.push(s.clone());
                }
                self.persist();
            }
            if connect {
                self.start(s);
            }
            self.editing = None;
            self.form_error.clear();
        } else {
            if s.protocol == "XDMCP" {
                self.xdmcp_discovery.finish_resolution(&mut s, ctx);
            }
            self.session_form = Some(s);
        }
    }
    pub(super) fn terminals(&mut self, ui: &mut Ui) {
        if self.tabs.is_empty() {
            self.page = Page::Home;
            return;
        }
        self.sync_root_selection();
        if !self.tabs.iter().any(|t| self.windows.window(t.id) == 0) {
            ui.label("All terminals are detached. Reattach a terminal to show its controls here.");
            return;
        }
        self.active = self.active.min(self.tabs.len() - 1);
        if self.tabs[self.active].session.protocol == "XDMCP" {
            let tab = &mut self.tabs[self.active];
            ui.label(&tab.connection_status);
            if !tab.error.is_empty() {
                ui.colored_label(Color32::RED, &tab.error);
            }
            if tab
                .xdmcp
                .as_ref()
                .is_some_and(|session| !session.is_finished())
                && ui.button("Cancel / disconnect XDMCP").clicked()
            {
                tab.xdmcp = None;
                tab.connection_status = "XDMCP cancellation requested".into();
            }
            ui.small("The remote desktop opens in its own owned X display window. Closing this session stops only that display.");
            return;
        }
        self.tabs[self.active].search_ui(ui);
        if !self.tabs[self.active].error.is_empty() {
            Frame::new()
                .fill(ui.visuals().widgets.hovered.bg_fill)
                .corner_radius(super::fluent::CONTROL_RADIUS)
                .inner_margin(12)
                .show(ui, |ui| {
                    ui.label(&self.tabs[self.active].error);
                });
        }
        if !self.tabs[self.active].running() {
            if let Some(p) = &self.tabs[self.active].process {
                if p.exited {
                    ui.label(
                        RichText::new(format!(
                            "{}  ·  Reconnect to start again, or save the output.",
                            p.exit_message
                        ))
                        .color(muted(ui)),
                    );
                }
            }
        }
        if self.broadcast && self.tabs[self.active].session.protocol != "Browser" {
            self.broadcast_targets_ui(ui);
        }
        let preferences = self.store.preferences.clone();
        let emitted = self.render_terminal_layout(ui, &preferences);
        for (source, bytes) in emitted {
            self.dispatch_keyboard(source, &bytes);
        }
        self.collect_terminal_requests(ui.ctx());
    }
    pub(super) fn sftp_compatibility_commands(&mut self, ui: &mut Ui, index: usize) {
        if self.tabs[index].session.protocol == "SFTP" {
            let mut upload = false;
            let mut download = false;
            let mut list = false;
            ui.horizontal(|ui| {
                upload = ui.button("Upload file…").clicked();
                download = ui.button("Download path").clicked();
                ui.add(
                    crate::ui::fluent::singleline(&mut self.tabs[index].remote_path)
                        .hint_text("Remote file or directory")
                        .desired_width(260.),
                );
                list = ui.button("List files").clicked();
            });
            if list {
                self.tabs[index].send_generated_command(b"ls -la\n");
            }
            if upload {
                if let Some(path) = rfd::FileDialog::new().pick_file() {
                    match connection::sftp_quote_path(&path) {
                        Ok(q) => {
                            let mut command = b"put -p ".to_vec();
                            command.extend(q);
                            command.push(b'\n');
                            self.tabs[index].send_generated_command(&command);
                        }
                        Err(e) => self.message(e),
                    }
                }
            }
            if download {
                let remote = self.tabs[index].remote_path.clone();
                if !remote.is_empty() {
                    if let Some(dest) = rfd::FileDialog::new().pick_folder() {
                        match (
                            connection::sftp_quote(&remote),
                            connection::sftp_quote_path(&dest),
                        ) {
                            (Ok(r), Ok(d)) => {
                                let mut command = format!("get -pR {r} ").into_bytes();
                                command.extend(d);
                                command.push(b'\n');
                                self.tabs[index].send_generated_command(&command);
                            }
                            (Err(e), _) | (_, Err(e)) => self.message(e),
                        }
                    }
                }
            }
        }
    }
    pub(super) fn files_page(&mut self, ui: &mut Ui) {
        ui.heading("Local files");
        let mut go = false;
        ui.horizontal_wrapped(|ui| {
            let parent = ui.button("↑").fluent_hint("Parent folder");
            parent.widget_info(|| {
                WidgetInfo::labeled(WidgetType::Button, parent.enabled(), "Parent folder")
            });
            if parent.clicked() {
                if let Some(p) = self.files.path.parent() {
                    self.files.path = p.into();
                    self.files.refresh();
                }
            }
            if ui.button("Home").clicked() {
                self.files.path = home_dir();
                self.files.refresh();
            }
            let r = ui.add_sized(
                [(ui.available_width() - 140.).clamp(96., 640.), 34.],
                crate::ui::fluent::singleline(&mut self.files.path_text),
            );
            r.widget_info(|| WidgetInfo::labeled(WidgetType::TextEdit, r.enabled(), "Folder path"));
            go = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            go |= ui.button("Go").clicked();
            if icons::action(ui, "Browse…").clicked() {
                if let Some(p) = rfd::FileDialog::new()
                    .set_directory(&self.files.path)
                    .pick_folder()
                {
                    self.files.path = p;
                    self.files.refresh();
                }
            }
        });
        if go {
            let path = PathBuf::from(&self.files.path_text);
            if path.is_dir() {
                self.files.path = path;
                self.files.refresh();
            } else {
                self.files.error = "That folder does not exist or cannot be opened.".into();
            }
        }
        ui.horizontal_wrapped(|ui|{if icons::action(ui,"Refresh").clicked(){self.files.refresh();}if icons::action(ui,"New folder").clicked(){self.files.show_new=true;self.files.new_name.clear();}ui.menu_button("More",|ui|{if ui.add_enabled(self.files.selected.is_some(),Button::new("Copy")).clicked(){self.files.clipboard=self.files.selected.clone();}
   if ui.add_enabled(self.files.clipboard.is_some(),Button::new("Paste copy")).clicked(){if let Some(from)=&self.files.clipboard{if from.is_file(){let to=self.files.path.join(from.file_name().unwrap());if to.exists(){self.files.error="A file with that name already exists. Rename it before copying.".into();}else{match copy_new_file(from,&to){Ok(_)=>self.files.refresh(),Err(e)=>self.files.error=e.to_string()}}}else{self.files.error="Folder copying is not supported yet. Open the system file manager for this operation.".into();}}}
   if ui.add_enabled(self.files.selected.is_some(),Button::new("Rename")).clicked(){self.files.error.clear();self.rename_file=self.files.selected.clone();self.rename_text=self.rename_file.as_ref().unwrap().file_name().unwrap().to_string_lossy().into();}
   if ui.add_enabled(self.files.selected.is_some(),Button::new("Delete")).clicked(){self.delete_file=self.files.selected.clone();}
   }); ui.add(crate::ui::fluent::singleline(&mut self.files.query).hint_text("Filter files").desired_width(170.));});
        if self.files.show_new {
            ui.horizontal_wrapped(|ui| {
                ui.add(
                    crate::ui::fluent::singleline(&mut self.files.new_name)
                        .hint_text("Folder name"),
                );
                if ui.button("Create").clicked() {
                    if valid_filename(&self.files.new_name) {
                        match fs::create_dir(self.files.path.join(&self.files.new_name)) {
                            Ok(()) => {
                                self.files.show_new = false;
                                self.files.refresh();
                            }
                            Err(e) => self.files.error = e.to_string(),
                        }
                    } else {
                        self.files.error =
                            "Enter a single folder name, without path separators.".into();
                    }
                }
                if ui.button("Cancel").clicked() {
                    self.files.show_new = false;
                }
            });
        }
        if !self.files.error.is_empty() {
            ui.colored_label(ui.visuals().error_fg_color, &self.files.error);
        }
        ui.separator();
        let mut open = None;
        let mut edit = None;
        let mut system = None;
        ui.horizontal(|ui| {
            ui.add_space(35.);
            ui.label(RichText::new("Name").color(muted(ui)));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new("Size").color(muted(ui)));
            });
        });
        ScrollArea::vertical()
            .id_salt("filelist")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for entry in &self.files.entries {
                    if !entry
                        .name
                        .to_lowercase()
                        .contains(&self.files.query.to_lowercase())
                    {
                        continue;
                    }
                    let row = explorer::ExplorerRow {
                        name: &entry.name,
                        kind: entry.kind,
                        path: entry.path.display().to_string(),
                        bytes: (!entry.dir).then_some(entry.size),
                        detail: if entry.dir {
                            "Folder".into()
                        } else {
                            size_text(entry.size)
                        },
                    };
                    let response = row.show(
                        ui,
                        ui.id().with(&entry.path),
                        self.files.selected.as_ref() == Some(&entry.path),
                        37.,
                        Sense::click(),
                    );
                    if response.clicked() {
                        self.files.selected = Some(entry.path.clone());
                    }
                    if response.double_clicked() {
                        open = Some(entry.clone());
                    }
                    context_actions(&response, |ui, focus_first| {
                        if context_item(ui, "Open", focus_first) {
                            open = Some(entry.clone());
                            ui.close();
                        }
                        if !entry.dir && icons::action(ui, "Edit as text").clicked() {
                            edit = Some(entry.path.clone());
                            ui.close();
                        }
                        if ui.button("Open in system app").clicked() {
                            system = Some(entry.path.clone());
                            ui.close();
                        }
                    });
                }
            });
        if let Some(e) = open {
            if e.dir {
                self.files.path = e.path;
                self.files.refresh();
            } else {
                edit = Some(e.path);
            }
        }
        if edit.is_some() && self.editor.as_ref().is_some_and(|editor| editor.dirty) {
            self.message("Save or close the current document before opening another file.");
            edit = None;
        }
        if let Some(p) = edit {
            match fs::metadata(&p) {
                Ok(m) if m.len() <= 2_000_000 => match fs::read_to_string(&p) {
                    Ok(text) => {
                        self.editor = Some(Editor {
                            closing: false,
                            closing_app: false,
                            error: String::new(),
                            path: p,
                            text,
                            dirty: false,
                        })
                    }
                    Err(_) => self.message("This file is not UTF-8 text. Use Open in system app."),
                },
                _ => self
                    .message("This file is too large for the text editor. Use Open in system app."),
            }
        }
        if let Some(p) = system {
            if let Err(e) = connection::open_path(&p) {
                self.message(e);
            }
        }
    }
    pub(super) fn confirm_local_document_exit(&mut self, ctx: &Context) -> bool {
        let Some(editor) = self.editor.as_mut().filter(|editor| editor.dirty) else {
            return false;
        };
        editor.closing = true;
        editor.closing_app = true;
        self.allow_session_exit = false;
        self.allow_remote_exit = false;
        ctx.send_viewport_cmd(ViewportCommand::CancelClose);
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
        true
    }

    pub(super) fn file_dialogs(&mut self, ctx: &Context) {
        if let Some(p) = self.delete_file.clone() {
            let mut cancel = false;
            let mut delete = false;
            let size = super::fluent::dialog_size(ctx, 410., 102.);
            let modal = super::fluent::modal(
                ctx,
                Id::new("delete-local-item"),
                "Delete this item?",
                size,
                |ui| {
                    super::fluent::dialog_body(ui, size, |ui| {
                        ui.label(p.display().to_string());
                        ui.label("This permanently removes the item. Only empty folders can be deleted here.");
                    });
                    let [remove, safe] =
                        fluent::dialog_actions(ui, ["Delete permanently", "Cancel"], None);
                    delete = remove.clicked();
                    fluent::focus_on_entry(ui, &safe, ui.id());
                    cancel = safe.clicked();
                },
            );
            cancel |= modal.should_close();
            if cancel {
                self.delete_file = None;
            } else if delete {
                let result = if p.is_dir() {
                    fs::remove_dir(p)
                } else {
                    fs::remove_file(p)
                };
                match result {
                    Ok(()) => self.files.refresh(),
                    Err(e) => self.files.error = e.to_string(),
                }
                self.delete_file = None;
            }
        }
        if let Some(p) = self.rename_file.clone() {
            let mut cancel = false;
            let mut rename = false;
            let size = fluent::dialog_size(ctx, 420., 110.);
            let modal = fluent::modal(ctx, Id::new("Rename item"), "Rename item", size, |ui| {
                let field = fluent::field(
                    ui,
                    "New name",
                    &mut self.rename_text,
                    "File name",
                    size.width,
                );
                fluent::select_name_on_entry(ui, &field, ui.id(), &self.rename_text);
                if !self.files.error.is_empty() {
                    ui.colored_label(super::fluent::palette(ui).error, &self.files.error);
                }
                let [commit, safe] = fluent::dialog_actions(ui, ["Rename", "Cancel"], Some(0));
                rename = commit.clicked();
                cancel = safe.clicked();
            });
            if cancel || modal.should_close() {
                self.rename_file = None;
            } else if rename {
                if valid_filename(&self.rename_text) {
                    let dest = p.parent().unwrap().join(&self.rename_text);
                    if dest.exists() {
                        self.files.error = "An item with that name already exists.".into();
                    } else {
                        match fs::rename(p, dest) {
                            Ok(()) => {
                                self.rename_file = None;
                                self.files.refresh();
                            }
                            Err(e) => self.files.error = e.to_string(),
                        }
                    }
                } else {
                    self.files.error = "Enter a single file name.".into();
                }
            }
        }
        if let Some(mut editor) = self.editor.take() {
            let mut open = true;
            let mut save = false;
            let size = fluent::dialog_size(ctx, 850., 130.);
            fluent::modeless(
                ctx,
                Id::new("text-editor"),
                if editor.dirty {
                    "Text editor · Unsaved"
                } else {
                    "Text editor"
                },
                size,
            )
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(editor.path.display().to_string());
                ui.horizontal_wrapped(|ui| {
                    save = primary(ui, "Save file").clicked();
                    ui.label(RichText::new("UTF-8 text").color(muted(ui)));
                });
                if !editor.error.is_empty() {
                    ui.colored_label(ui.visuals().error_fg_color, &editor.error);
                }
                ScrollArea::both().show(ui, |ui| {
                    let response = ui.add(
                        TextEdit::multiline(&mut editor.text)
                            .font(TextStyle::Monospace)
                            .desired_width(f32::INFINITY)
                            .desired_rows(28)
                            .code_editor(),
                    );
                    editor.dirty |= response.changed();
                });
            });
            if !open && editor.dirty {
                editor.closing = true;
                open = true;
            }
            if editor.closing {
                let size = super::fluent::dialog_size(ctx, 420., 104.);
                let modal = super::fluent::modal(
                    ctx,
                    Id::new("local-editor-close"),
                    "Save changes before closing?",
                    size,
                    |ui| {
                        ui.label(editor.path.display().to_string());
                        if !editor.error.is_empty() {
                            ui.colored_label(ui.visuals().error_fg_color, &editor.error);
                        }
                        ui.horizontal_wrapped(|ui| {
                            save = ui.button("Save and close").clicked();
                            if ui.button("Discard changes").clicked() {
                                open = false;
                            }
                            let safe = ui.button("Keep editing");
                            super::fluent::focus_on_entry(ui, &safe, ui.id());
                            if safe.clicked() {
                                editor.closing = false;
                            }
                        });
                    },
                );
                if modal.should_close() {
                    editor.closing = false;
                    open = true;
                    save = false;
                }
                if !editor.closing {
                    editor.closing_app = false;
                }
            }
            if save {
                match fs::write(&editor.path, &editor.text) {
                    Ok(()) => {
                        editor.dirty = false;
                        open = !editor.closing;
                        editor.error.clear();
                        self.message("File saved");
                    }
                    Err(e) => editor.error = e.to_string(),
                }
            }
            if open {
                self.editor = Some(editor);
            } else if editor.closing_app {
                self.continue_window_close(ctx);
            }
        }
    }
    pub(super) fn tunnels_page(&mut self, ui: &mut Ui) {
        fluent::page_scroll(ui, "tunnels_page", |ui| {
            page_heading(
                ui,
                "SSH tunnels",
                "Forward a local port, expose a remote port, or create a SOCKS proxy.",
            );
            ui.horizontal_wrapped(|ui| {
                if primary(ui, "+  New tunnel").clicked() {
                    if self.store.tunnels.len() >= 5 {
                        self.message(
                            "Five saved tunnels reached. Edit or remove an existing tunnel first.",
                        );
                    } else {
                        self.tunnel_form = Tunnel::default();
                        self.tunnel_editing = None;
                        self.tunnel_open = true;
                    }
                }
                ui.label(
                    RichText::new(format!("{} / 5 saved", self.store.tunnels.len()))
                        .color(muted(ui))
                        .size(12.),
                );
            });
            ui.add_space(14.);
            if self.store.tunnels.is_empty() {
                empty_state(
                    ui,
                    "link",
                    "No tunnels yet",
                    "Create a tunnel to reach a database, service, or private network over SSH.",
                );
            }
            let mut run = None;
            let mut edit = None;
            let mut remove = None;
            for (i, t) in self.store.tunnels.iter().enumerate() {
                surface(ui).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        icon(ui, "link", 25., accent(ui));
                        ui.allocate_ui_with_layout(
                            vec2((ui.available_width() - 230.).max(80.), 52.),
                            Layout::top_down(Align::Min),
                            |ui| {
                                ui.add(
                                    Label::new(crate::ui::fluent::emphasized(
                                        ui.ctx(),
                                        RichText::new(&t.name).size(16.),
                                    ))
                                    .truncate(),
                                )
                                .on_hover_text(&t.name);
                                ui.add(
                                    Label::new(
                                        RichText::new(format!(
                                            "{}  {}:{}  →  {}:{}  via {}",
                                            t.mode,
                                            t.bind,
                                            t.local_port,
                                            t.destination,
                                            t.remote_port,
                                            t.host
                                        ))
                                        .color(muted(ui))
                                        .size(12.),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(format!(
                                    "{} {}:{} → {}:{} via {}",
                                    t.mode,
                                    t.bind,
                                    t.local_port,
                                    t.destination,
                                    t.remote_port,
                                    t.host
                                ));
                            },
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.menu_button("More", |ui| {
                                if icons::action(ui, "Remove").clicked() {
                                    remove = Some(i);
                                    ui.close();
                                }
                                if icons::action(ui, "Edit").clicked() {
                                    edit = Some(i);
                                    ui.close();
                                }
                            });
                            if primary(ui, "Start").clicked() {
                                run = Some(t.clone());
                            }
                        });
                    });
                });
                ui.add_space(5.);
            }
            if let Some(i) = edit {
                self.tunnel_form = self.store.tunnels[i].clone();
                self.tunnel_editing = Some(i);
                self.tunnel_open = true;
            }
            if let Some(i) = remove {
                self.store.tunnels.remove(i);
                self.persist();
            }
            if let Some(t) = run {
                match t.args() {
                    Ok(args) => {
                        let s = Session {
                            name: format!("Tunnel: {}", t.name),
                            host: t.host.clone(),
                            protocol: "SSH".into(),
                            ..Default::default()
                        };
                        self.start_with_launch(
                            s,
                            Some(connection::Launch {
                                rdp_profile: None,
                                program: "ssh".into(),
                                args,
                                directory: None,
                            }),
                        );
                        self.tabs[self.active].screen.feed(b"SSH tunnel\r\nAuthenticate below if requested. Keep this tab open to keep forwarding active.\r\n");
                    }
                    Err(e) => self.message(e),
                }
            }
        });
    }
    pub(super) fn tunnel_dialog(&mut self, ctx: &Context) {
        if !self.tunnel_open {
            return;
        }
        let mut open = true;
        let mut save = false;
        let mut cancel = false;
        let size = fluent::dialog_size(ctx, 520., 138.);
        fluent::modeless(ctx, Id::new("Configure SSH tunnel"), "Configure SSH tunnel", size).open(&mut open).collapsible(false).show(ctx,|ui|{fluent::dialog_body(ui,size,|ui|{let t=&mut self.tunnel_form;ui.horizontal(|ui|{for mode in ["Local","Remote","Dynamic"]{ui.selectable_value(&mut t.mode,mode.into(),mode);}});ui.label(RichText::new(match t.mode.as_str(){"Remote"=>"Listen on the SSH server and forward to a destination from your computer.","Dynamic"=>"Create a local SOCKS proxy through the SSH server.",_=>"Listen on your computer and reach a destination from the SSH server."}).color(muted(ui)).size(12.));fluent::settings_grid(ui, "tunnel_fields", |ui|{let name=fluent::field(ui,"Name",&mut t.name,"Tunnel name",390.);fluent::focus_on_entry(ui,&name,ui.id());ui.end_row();field(ui,"SSH server",&mut t.host,"bastion.example.com");field(ui,"SSH username",&mut t.user,"");ui.label("SSH port");ui.add(DragValue::new(&mut t.ssh_port).range(1..=65535));ui.end_row();field(ui,"Private key",&mut t.key,"Optional key file");field(ui,"Listen address",&mut t.bind,"127.0.0.1");ui.label("Listen port");ui.add(DragValue::new(&mut t.local_port).range(1..=65535));ui.end_row();if t.mode!="Dynamic"{field(ui,"Destination",&mut t.destination,"localhost");ui.label("Destination port");ui.add(DragValue::new(&mut t.remote_port).range(1..=65535));ui.end_row();}});});if !self.form_error.is_empty(){ui.colored_label(super::fluent::palette(ui).error,&self.form_error);}ui.horizontal(|ui|{cancel=ui.button("Cancel").clicked();save=primary(ui,"Save tunnel").clicked();});});
        if !open || cancel {
            self.tunnel_open = false;
            self.form_error.clear();
        }
        if save {
            match self.tunnel_form.args() {
                Ok(_) => {
                    if let Some(i) = self.tunnel_editing {
                        self.store.tunnels[i] = self.tunnel_form.clone();
                    } else if self.store.tunnels.len() < 5 {
                        self.store.tunnels.push(self.tunnel_form.clone());
                    }
                    self.persist();
                    self.tunnel_open = false;
                    self.form_error.clear();
                }
                Err(e) => self.form_error = e,
            }
        }
    }
    pub(super) fn macros_page(&mut self, ui: &mut Ui) {
        fluent::page_scroll(ui, "macros_page", |ui| {
            page_heading(
                ui,
                "Macros",
                "Record and replay input in your terminal or embedded desktop.",
            );
            ui.horizontal_wrapped(|ui| {
                ui.menu_button("New macro", |ui| {
                    if ui.button("New text macro").clicked() {
                        self.macro_form = Macro {
                            append_enter: true,
                            ..Macro::default()
                        };
                        self.macro_editing = None;
                        self.macro_open = true;
                    }
                    if ui.button("New event macro").clicked() {
                        self.macro_form = Macro {
                            script: Some(harbor_workspace::macros::Script {
                                version: 1,
                                events: Vec::new(),
                            }),
                            ..Macro::default()
                        };
                        self.macro_editing = None;
                        self.macro_open = true;
                    }
                });
                self.macro_controls(ui);
                ui.menu_button("Library", |ui| {
                    if icons::action(ui, "Import").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Macro JSON", &["json"])
                            .pick_file()
                        {
                            match super::macro_events::read_import(&path).and_then(|imported| {
                                let mut values = self.store.macros.clone();
                                values.extend(imported);
                                super::macro_events::validate_library(&values)?;
                                Ok(values)
                            }) {
                                Ok(values) => {
                                    self.store.macros = values;
                                    self.persist();
                                }
                                Err(error) => self.message(error),
                            }
                        }
                    }
                    if icons::action(ui, "Export").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_file_name("macros.json")
                            .save_file()
                        {
                            let result = serde_json::to_vec_pretty(&self.store.macros)
                                .map_err(|error| error.to_string())
                                .and_then(|bytes| {
                                    fs::write(path, bytes).map_err(|error| error.to_string())
                                });
                            match result {
                                Ok(()) => self.message("Macros exported."),
                                Err(error) => self.message(error),
                            }
                        }
                    }
                });
            });
            ui.add_space(12.);
            if self.store.macros.is_empty() {
                empty_state(
                    ui,
                    "macro",
                    "No saved macros",
                    "Create a macro or record input from a connected session.",
                );
            }
            let mut edit = None;
            let mut remove = None;
            let mut run = None;
            for (index, value) in self.store.macros.iter().enumerate() {
                surface(ui).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        icon(ui, "macro", 24., accent(ui));
                        ui.allocate_ui_with_layout(
                            vec2((ui.available_width() - 230.).max(80.), 52.),
                            Layout::top_down(Align::Min),
                            |ui| {
                                ui.add(
                                    Label::new(
                                        crate::ui::fluent::emphasized(
                                            ui.ctx(),
                                            RichText::new(&value.name),
                                        )
                                        .size(16.),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(&value.name);
                                let description = value.script.as_ref().map_or_else(
                                    || value.text.lines().next().unwrap_or("").to_owned(),
                                    |script| format!("{} events", script.events.len()),
                                );
                                ui.add(
                                    Label::new(
                                        RichText::new(description).size(12.).color(muted(ui)),
                                    )
                                    .truncate(),
                                );
                            },
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.menu_button("More", |ui| {
                                if icons::action(ui, "Remove").clicked() {
                                    remove = Some(index);
                                    ui.close();
                                }
                                if icons::action(ui, "Edit").clicked() {
                                    edit = Some(index);
                                    ui.close();
                                }
                            });
                            if ui
                                .add_enabled(
                                    self.tabs.get(self.active).is_some_and(|tab| {
                                        super::macro_events::supported_session(&tab.session)
                                    }),
                                    Button::new("Run in active tab"),
                                )
                                .clicked()
                            {
                                run = Some(value.clone());
                            }
                        });
                    });
                });
                ui.add_space(5.);
            }
            if let Some(index) = edit {
                self.macro_form = self.store.macros[index].clone();
                self.macro_editing = Some(index);
                self.macro_open = true;
            }
            if let Some(index) = remove {
                let name = &self.store.macros[index].name;
                let attached = self
                    .store
                    .sessions
                    .iter()
                    .any(|session| session.startup_macro.as_ref() == Some(name));
                let mut values = self.store.macros.clone();
                values.remove(index);
                if attached {
                    self.message("Remove this macro from session startup before deleting it.");
                } else if let Err(error) = super::macro_events::validate_library(&values) {
                    self.message(error);
                } else {
                    self.store.macros = values;
                    self.persist();
                }
            }
            if let Some(value) = run {
                match self.queue_macro(self.tabs[self.active].id, &value) {
                    Ok(()) => self.page = Page::Terminal,
                    Err(error) => self.message(error),
                }
            }
        });
    }

    pub(super) fn macro_dialog(&mut self, ctx: &Context) {
        if !self.macro_open {
            return;
        }
        let mut open = true;
        let mut save = false;
        let size = fluent::dialog_size(ctx, 560., 130.);
        let mut cancel = false;
        fluent::modeless(ctx, Id::new("Edit macro"), "Edit macro", size)
            .open(&mut open)
            .collapsible(false)
            .show(ctx, |ui| {
                fluent::dialog_body(ui, size, |ui| {
                    ui.label("Name");
                    ui.add(crate::ui::fluent::singleline(&mut self.macro_form.name));
                    if let Some(script) = &mut self.macro_form.script {
                        super::macro_events::edit_events(ui, script);
                    } else {
                        ui.label("Text to send");
                        ui.add(
                            TextEdit::multiline(&mut self.macro_form.text)
                                .font(TextStyle::Monospace)
                                .desired_width(f32::INFINITY)
                                .desired_rows(8),
                        );
                        ui.checkbox(
                            &mut self.macro_form.append_enter,
                            "Press Enter after sending",
                        );
                    }
                });
                if !self.macro_error.is_empty() {
                    ui.colored_label(ui.visuals().error_fg_color, &self.macro_error);
                }
                ui.horizontal_wrapped(|ui| {
                    save = primary(ui, "Save macro").clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });
        if !open || cancel {
            self.macro_error.clear();
            self.macro_open = false;
        }
        if save {
            self.macro_form.name = self.macro_form.name.trim().to_owned();
            if self.macro_form.name.is_empty() {
                self.macro_form.name = "Untitled macro".into();
            }
            let mut values = self.store.macros.clone();
            if let Some(value) = self.macro_editing.and_then(|index| values.get_mut(index)) {
                let renamed = value.name != self.macro_form.name;
                if renamed
                    && self
                        .store
                        .sessions
                        .iter()
                        .any(|session| session.startup_macro.as_ref() == Some(&value.name))
                {
                    self.macro_error =
                        "Remove the old macro name from session startup before renaming it.".into();
                    return;
                }
                *value = self.macro_form.clone();
            } else {
                values.push(self.macro_form.clone());
            }
            if let Err(error) = super::macro_events::validate_library(&values) {
                self.macro_error = error;
                return;
            }
            self.store.macros = values;
            self.persist();
            self.macro_error.clear();
            self.macro_open = false;
        }
    }
    pub(super) fn tools_page(&mut self, ui: &mut Ui) {
        ui.heading("Tools");
        ui.horizontal_wrapped(|ui| {
            for &(category, label) in tools::Category::ALL {
                ui.selectable_value(&mut self.tools_category, category, label);
            }
        });
        ui.separator();
        self.tools.poll(ui);
        let category = self.tools_category;
        fluent::page_scroll(ui, &format!("tools-{category:?}"), |ui| match category {
            tools::Category::Network => {
                self.network_tools(ui);
                self.tools.network(ui);
            }
            tools::Category::Files => self.tools.files(ui),
            tools::Category::Packages => self.portable_packages_section(ui),
            tools::Category::Plugins => self.plugins_section(ui),
            tools::Category::Runtime => {
                ui.horizontal_wrapped(|ui| {
                    if ui.button("SSH keys and agents…").clicked() {
                        self.ssh_keys.open = true;
                    }
                    if ui.button("X server settings").clicked() {
                        self.page = Page::Servers;
                        self.servers_category = servers::Section::Xserver;
                    }
                });
                runtime_providers(ui);
            }
            #[cfg(windows)]
            tools::Category::Windows => self.tools.windows(ui),
        });
    }
    fn network_tools(&mut self, ui: &mut Ui) {
        surface(ui).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let host_label = ui.label("Host");
                fluent::text_input(ui, host_label.id, &mut self.diagnostic_host, "", 280.);
                if ui.button("DNS lookup").clicked() {
                    let host = self.diagnostic_host.clone();
                    if let Err(e) = validate_host(&host) {
                        self.diagnostic_result = e;
                    } else {
                        let (tx, rx) = mpsc::channel();
                        self.diagnostic_rx = Some(rx);
                        std::thread::spawn(move || {
                            use std::net::ToSocketAddrs;
                            let result = (host.as_str(), 0)
                                .to_socket_addrs()
                                .map(|a| {
                                    let mut ips = a.map(|a| a.ip().to_string()).collect::<Vec<_>>();
                                    ips.sort();
                                    ips.dedup();
                                    ips.join("\n")
                                })
                                .unwrap_or_else(|e| e.to_string());
                            let _ = tx.send(result);
                        });
                    }
                }
                if ui.button("Ping").clicked() {
                    if validate_host(&self.diagnostic_host).is_ok() {
                        let a = vec![
                            if cfg!(windows) { "-n" } else { "-c" }.into(),
                            "4".into(),
                            self.diagnostic_host.clone(),
                        ];
                        self.start_tool("Ping", "ping", a);
                    } else {
                        self.diagnostic_result = "Enter a valid host first.".into();
                    }
                }
                if ui.button("Test SSH port").clicked() {
                    let host = self.diagnostic_host.clone();
                    let (tx, rx) = mpsc::channel();
                    self.diagnostic_rx = Some(rx);
                    std::thread::spawn(move || {
                        use std::net::{TcpStream, ToSocketAddrs};
                        let r = (host.as_str(), 22)
                            .to_socket_addrs()
                            .and_then(|mut a| {
                                a.next()
                                    .ok_or_else(|| std::io::Error::other("No address found"))
                            })
                            .and_then(|a| TcpStream::connect_timeout(&a, Duration::from_secs(4)))
                            .map(|_| format!("{host}:22 is reachable"))
                            .unwrap_or_else(|e| format!("Connection failed: {e}"));
                        let _ = tx.send(r);
                    });
                }
            });
            if self.diagnostic_rx.is_some() {
                ui.spinner();
                ui.label("Checking…");
            }
            if !self.diagnostic_result.is_empty() {
                ui.add(
                    Label::new(RichText::new(&self.diagnostic_result).monospace()).selectable(true),
                );
            }
        });
    }
    pub(super) fn start_tool(&mut self, name: &str, program: &str, args: Vec<String>) {
        let program = connection::find_program(program)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| program.to_string());
        self.start_with_launch(
            Session {
                name: name.into(),
                protocol: "Tool".into(),
                ..Default::default()
            },
            Some(connection::Launch {
                rdp_profile: None,
                program,
                args,
                directory: None,
            }),
        );
    }
    pub(super) fn appearance_picker(&mut self, ui: &mut Ui) {
        let mut appearance = self.store.preferences.appearance;
        let label = ui.label("Theme");
        ComboBox::from_id_salt("appearance-theme")
            .width(72.)
            .selected_text(format!("{appearance:?}"))
            .show_ui(ui, |ui| {
                for (value, label) in [
                    (Appearance::System, "System"),
                    (Appearance::Light, "Light"),
                    (Appearance::Dark, "Dark"),
                ] {
                    ui.selectable_value(&mut appearance, value, label);
                }
            })
            .response
            .labelled_by(label.id);
        if appearance != self.store.preferences.appearance {
            self.store.preferences.appearance = appearance;
            self.apply_appearance(ui.ctx());
            self.persist();
        }
    }
    pub(super) fn settings_page(&mut self, ui: &mut Ui) {
        use SettingsCategory::*;
        fluent::page_scroll(ui, "settings_page", |ui| {
            ui.heading("Workspace settings");
            ui.add_space(8.);
            ui.horizontal_wrapped(|ui| {
                for (category, label) in [
                    (Appearance, "Appearance"),
                    (Terminal, "Terminal"),
                    (Safety, "Input & safety"),
                    (Storage, "Sessions & backups"),
                    (About, "About"),
                ] {
                    ui.selectable_value(&mut self.settings_category, category, label);
                }
            });
            ui.separator();
            if self.settings_category == Terminal {
                if ui.button("Configure global terminal profile").clicked() {
                    self.terminal_profile_editor =
                        Some(super::terminal_settings::ProfileEditor::from_preferences(
                            &self.store.preferences,
                        ));
                }
            }
            if self.settings_category == Safety {
                surface(ui).show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(fluent::emphasized(
                        ui.ctx(),
                        RichText::new("Paste and session safety").size(18.),
                    ));
                    ui.checkbox(
                        &mut self.store.preferences.paste.confirm_multiline,
                        "Confirm multiline paste",
                    );
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Delay between pasted lines");
                        ui.add(
                            DragValue::new(&mut self.store.preferences.paste.line_delay_ms)
                                .range(0..=5000)
                                .suffix(" ms"),
                        );
                    });
                    ui.checkbox(
                        &mut self.store.preferences.confirm_session_close,
                        "Confirm closing active sessions",
                    );
                });
                ui.add_space(12.);
                surface(ui).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(fluent::emphasized(ui.ctx(),RichText::new("Clipboard and selection").size(18.)));
                ui.checkbox(
                    &mut self.store.preferences.clipboard.copy_on_select,
                    "Copy selection automatically on release",
                );
                ui.horizontal_wrapped(|ui| {
                    ui.label("Right click");
                    ui.selectable_value(
                        &mut self.store.preferences.clipboard.right_click,
                        RightClickBehavior::ContextMenu,
                        "Context menu",
                    );
                    ui.selectable_value(
                        &mut self.store.preferences.clipboard.right_click,
                        RightClickBehavior::Paste,
                        "Paste (Shift-right-click for menu)",
                    );
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("Word delimiters");
                    ui.add(crate::ui::fluent::singleline(
                        &mut self.store.preferences.clipboard.word_delimiters,
                    ));
                });
                ui.menu_button("Selection help",|ui|{ui.label("Alt-drag selects a rectangle. Ctrl+Shift+C copies. Ctrl+Shift+V pastes. Hold Shift to select while a terminal program uses the mouse.");});
                });
                ui.add_space(12.);
                if primary(ui, "Save preferences").clicked() {
                    self.persist();
                }
            }
            if self.settings_category == Terminal {
                if ui.button("Configure terminal highlighting").clicked() {
                    self.highlight_editor = Some(super::highlighting::Editor::from_preferences(
                        &self.store.preferences,
                    ));
                }
            }
            if self.settings_category == Appearance {
                surface(ui).show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(fluent::emphasized(
                        ui.ctx(),
                        RichText::new("Appearance").size(18.),
                    ));
                    self.appearance_picker(ui);
                    let reduced_motion = ui.checkbox(
                        &mut self.store.preferences.reduced_motion,
                        "Reduce interface transitions",
                    );
                    let high_contrast = ui.checkbox(
                        &mut self.store.preferences.high_contrast,
                        "Increase interface contrast",
                    );
                    if reduced_motion.changed() || high_contrast.changed() {
                        self.apply_appearance(ui.ctx());
                        self.persist();
                    }
                });
            }
            if self.settings_category == Terminal {
                surface(ui).show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(crate::ui::fluent::emphasized(
                        ui.ctx(),
                        RichText::new("Terminal").size(18.),
                    ));
                    fluent::settings_grid(ui, "prefs", |ui| {
                        ui.label("Scrollback lines");
                        ui.add(
                            DragValue::new(&mut self.store.preferences.scrollback)
                                .range(100..=360000),
                        );
                        ui.end_row();
                        ui.label("History memory");
                        ui.label("128 MiB per terminal").on_hover_text("Older lines expire when either the line limit or memory limit is reached.");
                        ui.end_row();
                        ui.label("SSH keepalive interval");
                        ui.add(
                            DragValue::new(&mut self.store.preferences.ssh_defaults.keepalive_secs)
                                .range(1..=86400)
                                .suffix("s"),
                        );
                        ui.end_row();
                        ui.label("SSH connect timeout");
                        ui.add(
                            DragValue::new(
                                &mut self.store.preferences.ssh_defaults.connect_timeout_secs,
                            )
                            .range(1..=3600)
                            .suffix("s"),
                        );
                        ui.end_row();
                        field(
                            ui,
                            "Default shell executable",
                            &mut self.store.preferences.shell,
                            "Use the system default",
                        );
                        field(
                            ui,
                            "Download folder",
                            &mut self.store.preferences.downloads,
                            "Folder path",
                        );
                    });
                    if primary(ui, "Save preferences").clicked() {
                        self.persist();
                    }
                });
            }
            if self.settings_category == Storage {
                surface(ui).show(ui,|ui|{ui.set_min_width(ui.available_width());ui.label(crate::ui::fluent::emphasized(ui.ctx(), RichText::new("Sessions and backups").size(18.)));ui.label(RichText::new("Session settings, folders, tunnels and macros are stored locally. Exported backups contain hostnames and key-file paths, but no passwords.").color(muted(ui)));ui.horizontal_wrapped(|ui|{
    if icons::action(ui,"Export workspace").clicked(){if let Some(p)=rfd::FileDialog::new().set_file_name("termcraft-workspace.json").save_file(){match self.store.save(&p){Ok(())=>self.message("Workspace exported"),Err(e)=>self.message(e)}}}
    if icons::action(ui,"Import workspace").clicked(){if let Some(p)=rfd::FileDialog::new().add_filter("Workspace JSON",&["json"]).pick_file(){match Store::load_bounded(&p, 16 * 1024 * 1024).and_then(|mut imported| { let mut macros = self.store.macros.clone(); macros.append(&mut imported.macros); super::macro_events::validate_library(&macros)?; imported.macros = macros; Ok(imported) }) { Ok(imported) => { self.store.sessions.extend(imported.sessions.into_iter().map(|mut session| {session.id=harbor_workspace::model::new_session_id();session})); self.store.tunnels.extend(imported.tunnels.into_iter().take(5_usize.saturating_sub(self.store.tunnels.len()))); self.store.macros = imported.macros; self.persist(); }, Err(error) => self.message(error) }}}
   });if !self.store_ok{ui.colored_label(ui.visuals().error_fg_color,"The settings file is damaged. Automatic saving is disabled to preserve it.");if ui.button("Back up damaged file and start fresh").clicked(){match fs::copy(&self.store_path,self.store_path.with_extension("damaged-backup.json")){Ok(_)=>{self.store_ok=true;self.settings_load_error=None;self.persist();},Err(e)=>self.message(e.to_string())}}}});
            }
            if self.settings_category == About {
                surface(ui).show(ui,|ui|{ui.set_min_width(ui.available_width());ui.label(crate::ui::fluent::emphasized(ui.ctx(), RichText::new("About TermCraft").size(18.)));ui.label("An independently written Rust desktop remote workspace with a Fluent-inspired interface.");ui.label(RichText::new("Built with eframe/egui and OpenSSH. No original branding or bundled engines are reused.").size(12.).color(muted(ui)));ui.add_space(5.);ui.label(RichText::new("Feature and platform qualification is tracked in the project checklists. See Tools → Runtime for engine availability.").size(12.).color(muted(ui)));});
            }
        });
    }
}
pub(super) fn history_status(ui: &mut Ui, t: &TerminalTab) {
    let stats = t.screen.stats();
    ui.small(format!(
        "History: {} lines · {:.1} / {} MiB{}",
        t.screen.history().len(),
        stats.history_bytes as f64 / (1024. * 1024.),
        stats.history_budget / (1024 * 1024),
        if stats.discarded_lines > 0 {
            format!(" · {} older lines expired", stats.discarded_lines)
        } else {
            String::new()
        }
    ));
}
pub(super) fn terminal_view(ui: &mut Ui, t: &mut TerminalTab, p: &Preferences) -> Vec<u8> {
    if t.session.protocol == "Browser" && t.session.browser_options.embedded {
        super::web_browser::view(ui, t);
        return Vec::new();
    }
    if let Some(ftp) = &mut t.ftp {
        ftp.view(ui);
        return Vec::new();
    }
    if t.gateway.is_some() && t.desktop.is_none() && t.process.is_none() && t.legacy.is_none() {
        ui.label(&t.connection_status);
        if !t.error.is_empty() {
            ui.colored_label(ui.visuals().error_fg_color, &t.error);
        }
        super::remote_files::authentication_ui(ui, t);
        return Vec::new();
    }
    if t.desktop.is_some() {
        super::desktop::view(ui, t);
        return Vec::new();
    }
    if t.waiting_for_x11 {
        ui.label("Waiting for the TermCraft X server…");
        return Vec::new();
    }
    t.apply_terminal_preferences(p);
    t.highlights.update(&p.highlight_rules);
    if let Some(error) = &t.highlights.error {
        ui.colored_label(
            ui.visuals().error_fg_color,
            format!("Highlight settings rejected; retaining the previous valid rules: {error}"),
        );
    }
    let highlight_enabled = t.session.highlight_enabled.unwrap_or(p.highlight_enabled);
    if t.paste.is_some() || t.paste_end.is_some() || t.after_paste.is_some() {
        ui.horizontal(|ui| {
            ui.label("Pasting input…");
            if ui.button("Cancel remaining paste").clicked() {
                t.cancel_paste();
            }
        });
    }
    if t.codec.errors() > 0 {
        ui.small(format!(
            "{} decoding: {} chunk(s) contained invalid bytes; raw logs remain unchanged",
            t.codec.encoding().label(),
            t.codec.errors()
        ));
    }
    let scroll_action = super::shortcuts::terminal_actions(ui, t, p);
    let height = ui.available_height().max(150.);
    let font = FontId::new(
        t.terminal_settings.font_size,
        super::terminal_settings::font_family(t.terminal_settings.font_family),
    );
    let cell_width = ui.fonts_mut(|f| f.glyph_width(&font, 'M')).max(4.);
    let line_height = t.terminal_settings.font_size * 1.35;
    let dynamic_cols = ((ui.available_width() - 25.) / cell_width)
        .floor()
        .clamp(10., 500.) as usize;
    let dynamic_rows = ((height - 32.) / line_height).floor().clamp(3., 200.) as usize;
    let (cols, rows) = match t.terminal_settings.dimensions {
        harbor_workspace::terminal::settings::Dimensions::Dynamic => (dynamic_cols, dynamic_rows),
        harbor_workspace::terminal::settings::Dimensions::Fixed { cols, rows } => {
            (cols as usize, rows as usize)
        }
    };
    if t.screen.cols != cols || t.screen.rows != rows {
        t.screen.resize(cols, rows);
        t.resize_transport();
    }
    let dark = ui.visuals().dark_mode;
    let palette = if dark {
        &t.terminal_settings.dark_palette
    } else {
        &t.terminal_settings.light_palette
    };
    let bg = Color32::from_rgb(
        palette.background[0],
        palette.background[1],
        palette.background[2],
    );
    let mut clicked = false;
    let mut hovered = false;
    let mut any_selection = false;
    let mouse_mode = t.screen.mouse_mode();
    let shift = ui.input(|i| i.modifiers.shift);
    let reporting = mouse_mode.enabled() && !shift;
    let mut viewport = super::terminal_mouse::Viewport {
        clip: Rect::NOTHING,
        rows: Vec::new(),
        cell_width,
        cols,
        layer: ui.layer_id(),
    };
    let mut selection_viewport = viewport.clone();
    let frame = Frame::new()
        .fill(bg)
        .corner_radius(0)
        .inner_margin(0)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.;
            ui.set_min_height(height - 26.);
            let hist = if t.screen.is_alternate() {
                0
            } else {
                t.screen.history().len()
            };
            let total = hist + t.screen.rows;
            let mut area = ScrollArea::both()
                .id_salt(("terminal-scroll", t.id))
                .auto_shrink([false, false])
                .max_height(height - 30.)
                .stick_to_bottom(true);
            if t.screen.is_alternate()
                && matches!(
                    t.terminal_settings.dimensions,
                    harbor_workspace::terminal::settings::Dimensions::Dynamic
                )
            {
                area = area.scroll([false, false]);
            }
            area = area.scroll_source(egui::scroll_area::ScrollSource {
                mouse_wheel: !reporting,
                drag: egui::scroll_area::DragScroll::Never,
                ..Default::default()
            });
            if let Some(offset) = scroll_action.and_then(|action| {
                action.scroll_offset(
                    t.history_offset,
                    line_height,
                    height - 30.,
                    (total as f32 * line_height - (height - 30.)).max(0.),
                )
            }) {
                area = area.stick_to_bottom(false).vertical_scroll_offset(offset);
            }
            let scrolled = area.show_rows(ui, line_height, total, |ui, range| {
                viewport.clip = ui.clip_rect();
                selection_viewport.clip = ui.clip_rect();
                for index in range {
                    let history_cells;
                    let cells = if index < hist {
                        history_cells = t.screen.history().get(index).unwrap();
                        &history_cells
                    } else {
                        &t.screen.grid()[index - hist]
                    };
                    let mut measure = |text: &str, font: &FontId| {
                        ui.fonts_mut(|f| {
                            f.layout_no_wrap(text.to_owned(), font.clone(), Color32::WHITE)
                                .rows
                                .first()
                                .and_then(|row| row.glyphs.last())
                                .map(|glyph| glyph.max_x())
                                .unwrap_or(0.)
                        })
                    };
                    let selected = t
                        .selection
                        .selection
                        .as_ref()
                        .and_then(|s| s.columns_for_row(&t.screen, index, t.selection.mode));
                    let (job, backgrounds) = terminal_layout_profile(
                        cells,
                        &font,
                        dark,
                        &t.search,
                        cell_width,
                        Some(&mut measure),
                        highlight_enabled.then_some(&t.highlights.compiled),
                        selected,
                        palette,
                    );
                    let background_slot = ui.painter().add(egui::epaint::Shape::Noop);
                    let label = ui.add_sized(
                        vec2(cells.len() as f32 * cell_width, line_height),
                        Label::new(job)
                            .halign(Align::Min)
                            .extend()
                            .selectable(false)
                            .sense(Sense::click_and_drag()),
                    );
                    selection_viewport.rows.push((index, label.rect));
                    if index >= hist {
                        viewport.rows.push((index - hist, label.rect));
                    }
                    let shapes = backgrounds
                        .into_iter()
                        .map(|(col, width, color)| {
                            egui::epaint::Shape::rect_filled(
                                Rect::from_min_size(
                                    label.rect.min + vec2(col as f32 * cell_width, 0.),
                                    vec2(width as f32 * cell_width, line_height),
                                ),
                                0,
                                color,
                            )
                        })
                        .collect::<Vec<_>>();
                    ui.painter()
                        .set(background_slot, egui::epaint::Shape::Vec(shapes));
                    clicked |= label.clicked();
                    hovered |= label.hovered();
                    any_selection |= label.dragged();
                    if index == hist + t.screen.row && t.screen.cursor_visible && t.running() {
                        let cursor = Rect::from_min_size(
                            label.rect.min
                                + vec2(t.screen.col.min(t.screen.cols - 1) as f32 * cell_width, 0.),
                            vec2(cell_width, line_height - 2.),
                        );
                        ui.painter().rect_stroke(
                            cursor,
                            0.,
                            Stroke::new(1.5, accent(ui)),
                            StrokeKind::Inside,
                        );
                    }
                }
            });
            scrolled.state.offset.y
        });
    t.history_offset = frame.inner;
    let id = Id::new(("terminal-input", t.id));
    let response = ui.interact(frame.response.rect, id, Sense::click());
    if super::shortcuts::input_blocked(ui.ctx(), t.id)
        || !ui.memory(|m| m.allows_interaction(ui.layer_id()))
    {
        t.macro_state.terminal_focus(ui.ctx(), false);
        t.route_mouse(ui.ctx(), &viewport, false, &[], ui.input(|i| i.modifiers));
        return Vec::new();
    }
    let pointer_events = ui.input(|i| i.events.clone());
    let pointer_press=pointer_events.iter().any(|event| matches!(event,Event::PointerButton{pos,pressed:true,..} if selection_viewport.cell(ui.ctx(),*pos).is_some()));
    if response.clicked() || clicked || pointer_press {
        response.request_focus();
    }
    hovered |= response.hovered();
    if hovered {
        ui.ctx().set_cursor_icon(CursorIcon::Text);
    }
    let focused = response.has_focus();
    t.macro_state.terminal_focus(
        ui.ctx(),
        focused
            && ui.input(|input| input.focused)
            && !super::shortcuts::input_blocked(ui.ctx(), t.id),
    );
    let mut outgoing = Vec::new();
    let can_send = t.ssh.is_none()
        || (t.ssh_connected
            && t.auth.is_none()
            && t.ssh.as_ref().is_some_and(|s| !s.is_finished()));
    t.route_mouse(
        ui.ctx(),
        &viewport,
        focused && ui.input(|i| i.focused) && can_send,
        &pointer_events,
        ui.input(|i| i.modifiers),
    );
    let actions = t.selection.handle(
        ui.ctx(),
        &t.screen,
        &selection_viewport,
        &pointer_events,
        &p.clipboard,
        focused && ui.input(|i| i.focused),
        !reporting,
        ui.input(|i| i.modifiers),
    );
    let copied = actions.copy.is_some();
    if let Some(text) = actions.copy {
        ui.ctx().copy_text(text);
    }
    if let Some(error) = actions.error {
        t.error = error;
    }
    if actions.paste && can_send {
        response.request_focus();
        t.clipboard_request = true;
    }
    let mut menu_copy = false;
    let mut menu_paste = false;
    let mut clear_selection = false;
    Popup::menu(&response).open_memory(actions.menu.then_some(egui::SetOpenCommand::Bool(true))).at_pointer_fixed().show(|ui|{
        if ui.add_enabled(t.selection.selection.is_some(),Button::new("Copy selection")).clicked(){menu_copy=true;ui.close();}
        if ui.add_enabled(can_send,Button::new("Paste")).clicked(){menu_paste=true;ui.close();}
        if ui.button("Clear selection").clicked(){clear_selection=true;ui.close();}
        ui.small("Alt-drag: rectangle. Hold Shift when an application uses the mouse. Shift-right-click always opens this menu.");
    });
    if clear_selection {
        t.selection.selection = None;
    }
    if menu_copy {
        match t.selection.selected(&t.screen) {
            Ok(text) if !text.is_empty() => ui.ctx().copy_text(text),
            Err(error) => t.error = error,
            _ => {}
        }
    }
    if menu_paste {
        response.request_focus();
        t.clipboard_request = true;
    }
    let keyboard_modifiers = ui.input(|i| i.modifiers);
    let normalized_copy = pointer_events.iter().any(|e| matches!(e, Event::Copy));
    if focused
        && !any_selection
        && can_send
        && !t.input_busy()
        && !super::shortcuts::input_blocked(ui.ctx(), t.id)
    {
        let mut deferred_rejected = false;
        let mut application_text = false;
        for event in ui.input(|i| i.events.clone()) {
            if deferred_rejected {
                continue;
            }
            let mut event_bytes = Vec::new();
            let mut deferred_enter = false;
            (|| match event {
                Event::Copy
                    if !copied
                        && keyboard_modifiers.ctrl
                        && !keyboard_modifiers.shift
                        && !keyboard_modifiers.mac_cmd =>
                {
                    event_bytes.push(3)
                }
                Event::Text(s) => {
                    if !std::mem::take(&mut application_text) {
                        event_bytes.extend_from_slice(s.as_bytes());
                    }
                }
                Event::Paste(s) => {
                    if t.paste_request.is_none() {
                        t.paste_request = Some(super::session_confirmations::InputPaste {
                            text: s,
                            after: Default::default(),
                        });
                    } else {
                        t.error = "A paste is already pending for this terminal.".into();
                    }
                }
                Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    if normalized_copy && key == Key::C && modifiers.ctrl && !modifiers.shift {
                        return;
                    }
                    if super::shortcuts::is_application_key(key, modifiers, p) {
                        application_text = true;
                        return;
                    }
                    application_text = false;
                    if copied && key == Key::C && modifiers.ctrl {
                        return;
                    }
                    if modifiers.ctrl && !modifiers.shift {
                        let ch = match key {
                            Key::A => Some(1),
                            Key::B => Some(2),
                            Key::C => Some(3),
                            Key::D => Some(4),
                            Key::E => Some(5),
                            Key::F => Some(6),
                            Key::G => Some(7),
                            Key::H => Some(8),
                            Key::K => Some(11),
                            Key::L => Some(12),
                            Key::O => Some(15),
                            Key::P => Some(16),
                            Key::R => Some(18),
                            Key::S => Some(19),
                            Key::T => Some(20),
                            Key::U => Some(21),
                            Key::W => Some(23),
                            Key::X => Some(24),
                            Key::Y => Some(25),
                            Key::Z => Some(26),
                            Key::Space => Some(0),
                            _ => None,
                        };
                        if let Some(ch) = ch {
                            event_bytes.push(ch);
                            return;
                        }
                    }
                    if key == Key::Enter && t.legacy.is_some() {
                        if t.paste_request.is_some() {
                            deferred_enter = true;
                        } else {
                            if !outgoing.is_empty() {
                                let previous = std::mem::take(&mut outgoing);
                                if let Err(error) = t.send_user_bytes(&previous, true) {
                                    t.error = error;
                                    return;
                                }
                            }
                            if let Err(error) = t.send_legacy_enter() {
                                t.error = error;
                            }
                        }
                        return;
                    }
                    let arrow = if t.screen.app_cursor {
                        "\x1bO"
                    } else {
                        "\x1b["
                    };
                    let bytes = match key {
                        Key::Enter => "\r".into(),
                        Key::Backspace => {
                            String::from_utf8_lossy(t.terminal_settings.backspace.bytes())
                                .into_owned()
                        }
                        Key::Tab => {
                            if modifiers.shift {
                                "\x1b[Z".into()
                            } else {
                                "\t".into()
                            }
                        }
                        Key::Escape => "\x1b".into(),
                        Key::ArrowUp => format!("{arrow}A"),
                        Key::ArrowDown => format!("{arrow}B"),
                        Key::ArrowRight => format!("{arrow}C"),
                        Key::ArrowLeft => format!("{arrow}D"),
                        Key::Home => "\x1b[H".into(),
                        Key::End => "\x1b[F".into(),
                        Key::Delete => "\x1b[3~".into(),
                        Key::Insert => "\x1b[2~".into(),
                        Key::PageUp => "\x1b[5~".into(),
                        Key::PageDown => "\x1b[6~".into(),
                        Key::F1 => "\x1bOP".into(),
                        Key::F2 => "\x1bOQ".into(),
                        Key::F3 => "\x1bOR".into(),
                        Key::F4 => "\x1bOS".into(),
                        Key::F5 => "\x1b[15~".into(),
                        Key::F6 => "\x1b[17~".into(),
                        Key::F7 => "\x1b[18~".into(),
                        Key::F8 => "\x1b[19~".into(),
                        Key::F9 => "\x1b[20~".into(),
                        Key::F10 => "\x1b[21~".into(),
                        Key::F11 => "\x1b[23~".into(),
                        Key::F12 => "\x1b[24~".into(),
                        _ => String::new(),
                    };
                    event_bytes.extend(bytes.as_bytes());
                }
                _ => {}
            })();
            if let Some(paste) = &mut t.paste_request {
                use super::session_confirmations::DeferredInput;
                let input = if deferred_enter {
                    DeferredInput::LegacyEnter
                } else {
                    DeferredInput::Bytes(event_bytes)
                };
                if !paste.after.push(input) {
                    t.paste_request = None;
                    deferred_rejected = true;
                    t.error =
                        "Too much typed input followed the paste; the paste was cancelled.".into();
                }
            } else {
                outgoing.extend(event_bytes);
            }
        }
    }
    if !outgoing.is_empty() {
        if let Err(error) = t.send_user_bytes(&outgoing, true) {
            t.error = error;
            outgoing.clear();
        }
    }
    outgoing
}
#[cfg(test)]
fn terminal_job(cells: &[Cell], font: &FontId, dark: bool, search: &str) -> egui::text::LayoutJob {
    terminal_layout(cells, font, dark, search, 0., None).0
}
#[cfg(test)]
fn terminal_layout(
    cells: &[Cell],
    font: &FontId,
    dark: bool,
    search: &str,
    cell_width: f32,
    measure: Option<&mut dyn FnMut(&str, &FontId) -> f32>,
) -> (egui::text::LayoutJob, Vec<(usize, usize, Color32)>) {
    terminal_layout_highlight(cells, font, dark, search, cell_width, measure, None)
}
#[cfg(test)]
fn terminal_layout_highlight(
    cells: &[Cell],
    font: &FontId,
    dark: bool,
    search: &str,
    cell_width: f32,
    measure: Option<&mut dyn FnMut(&str, &FontId) -> f32>,
    rules: Option<&harbor_workspace::terminal::highlight::HighlightRules>,
) -> (egui::text::LayoutJob, Vec<(usize, usize, Color32)>) {
    terminal_layout_selected(cells, font, dark, search, cell_width, measure, rules, None)
}
#[cfg(test)]
fn terminal_layout_selected(
    cells: &[Cell],
    font: &FontId,
    dark: bool,
    search: &str,
    cell_width: f32,
    measure: Option<&mut dyn FnMut(&str, &FontId) -> f32>,
    rules: Option<&harbor_workspace::terminal::highlight::HighlightRules>,
    selection: Option<std::ops::Range<usize>>,
) -> (egui::text::LayoutJob, Vec<(usize, usize, Color32)>) {
    let palette = if dark {
        harbor_workspace::terminal::settings::TerminalPalette::dark()
    } else {
        harbor_workspace::terminal::settings::TerminalPalette::light()
    };
    terminal_layout_profile(
        cells, font, dark, search, cell_width, measure, rules, selection, &palette,
    )
}
fn terminal_layout_profile(
    cells: &[Cell],
    font: &FontId,
    dark: bool,
    search: &str,
    cell_width: f32,
    mut measure: Option<&mut dyn FnMut(&str, &FontId) -> f32>,
    rules: Option<&harbor_workspace::terminal::highlight::HighlightRules>,
    selection: Option<std::ops::Range<usize>>,
    palette: &harbor_workspace::terminal::settings::TerminalPalette,
) -> (egui::text::LayoutJob, Vec<(usize, usize, Color32)>) {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = f32::INFINITY;
    job.keep_trailing_whitespace = true;
    let mut backgrounds = Vec::new();
    let mut cursor_x = 0.;
    let text = cells.iter().map(|c| c.text.as_str()).collect::<String>();
    let highlights = rules.map(|r| r.spans(&text)).unwrap_or_default();
    let matches: Vec<_> = if search.is_empty() {
        vec![]
    } else {
        text.match_indices(search)
            .map(|(i, s)| i..i + s.len())
            .collect()
    };
    let mut offset = 0;
    for (col, c) in cells.iter().enumerate() {
        if c.continuation {
            continue;
        }
        let (fg, bg) = palette.colors(c);
        let mut fg = Color32::from_rgb(fg.0, fg.1, fg.2);
        let mut bg = Color32::from_rgb(bg.0, bg.1, bg.2);
        let end = offset + c.text.len();
        if let Some(span) = highlights
            .iter()
            .find(|span| span.range.start < end && span.range.end > offset)
        {
            let style = if dark { span.dark } else { span.light };
            if let Some(rgb) = style.foreground {
                fg = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
            }
            if let Some(rgb) = style.background {
                bg = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
            }
        }
        if matches.iter().any(|r| r.start < end && r.end > offset) {
            fg = Color32::from_rgb(32, 32, 32);
            bg = Color32::from_rgb(255, 222, 115);
        }
        let width = c.width.max(1) as usize;
        if selection
            .as_ref()
            .is_some_and(|range| range.start < col + width && range.end > col)
        {
            fg = Color32::WHITE;
            bg = Color32::from_rgb(15, 105, 73);
        }
        if let Some((start, previous_width, color)) = backgrounds.last_mut() {
            if *color == bg && *start + *previous_width == col {
                *previous_width += width;
            } else {
                backgrounds.push((col, width, bg));
            }
        } else {
            backgrounds.push((col, width, bg));
        }
        let mut cell_font = font.clone();
        let mut advance = cell_width * c.width.max(1) as f32;
        let measured = measure.is_some();
        if let Some(measure) = &mut measure {
            let known_monospace = font.family == FontFamily::Monospace
                || font.family == FontFamily::Name("DejaVu Sans Mono".into());
            if !c.text.is_ascii() || c.text.len() != 1 || !known_monospace {
                advance = measure(&c.text, &cell_font);
                let target = cell_width * c.width.max(1) as f32;
                if advance > target && advance > 0. {
                    cell_font.size *= target / advance;
                    advance = measure(&c.text, &cell_font);
                }
            }
        }
        let delta = col as f32 * cell_width - cursor_x;
        let leading = if measured && delta.abs() > 0.01 {
            delta
        } else {
            0.
        };
        job.append(
            &c.text,
            leading,
            TextFormat {
                font_id: cell_font,
                line_height: Some(font.size * 1.35),
                color: fg,
                background: bg,
                ..Default::default()
            },
        );
        cursor_x = col as f32 * cell_width + advance;
        offset = end;
    }
    (job, backgrounds)
}
pub(super) fn session_path(
    ui: &mut Ui,
    name: &str,
    value: &mut String,
    hint: &str,
    pick: fn(rfd::FileDialog) -> Option<PathBuf>,
) {
    fluent::form_row(ui, name, |ui, label| {
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 84.).max(1.);
            fluent::text_input(ui, label, value, hint, width);
            if ui.button("Browse").clicked()
                && let Some(path) = pick(rfd::FileDialog::new())
            {
                *value = path.to_string_lossy().into_owned();
            }
        });
    });
}

fn field(ui: &mut Ui, label: &str, value: &mut String, hint: &str) {
    super::fluent::field(ui, label, value, hint, 390.);
    ui.end_row();
}
pub(super) fn page_heading(ui: &mut Ui, title: &str, description: &str) {
    ui.add_space(20.);
    ui.label(fluent::emphasized(
        ui.ctx(),
        RichText::new(title).size(fluent::PAGE_TITLE),
    ));
    ui.label(RichText::new(description).color(muted(ui)));
    ui.add_space(20.);
}
fn empty_state(ui: &mut Ui, kind: &str, title: &str, description: &str) {
    ui.add_space(44.);
    ui.vertical_centered(|ui| {
        icon(ui, kind, 42., accent(ui));
        ui.add_space(10.);
        ui.label(crate::ui::fluent::emphasized(
            ui.ctx(),
            RichText::new(title).size(21.),
        ));
        ui.add_space(5.);
        ui.label(RichText::new(description).color(muted(ui)));
    });
}
fn valid_filename(s: &str) -> bool {
    !s.trim().is_empty() && s != "." && s != ".." && !s.contains(['/', '\\', '\n', '\r', '\0'])
}
fn size_text(n: u64) -> String {
    if n < 1024 {
        format!("{n} B")
    } else if n < 1048576 {
        format!("{:.1} KB", n as f64 / 1024.)
    } else {
        format!("{:.1} MB", n as f64 / 1048576.)
    }
}

#[cfg(test)]
mod appearance_tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn the_entire_live_screen_fits_the_terminal_viewport() {
        let ctx = egui::Context::default();
        let root = tempfile::tempdir().unwrap();
        let mut workspace = Workspace::new_context(&ctx, None, root.path().to_owned());
        workspace.start_with_launch(
            Session {
                protocol: "Local".into(),
                ..Default::default()
            },
            Some(connection::Launch {
                rdp_profile: None,
                program: "/definitely-missing-termcraft-test-program".into(),
                args: vec![],
                directory: None,
            }),
        );
        let preferences = workspace.store.preferences.clone();
        let input = || egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900., 600.))),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input(), |ui| {
            let _ = terminal_view(ui, &mut workspace.tabs[0], &preferences);
        });
        output.textures_delta.clear();
        let rows = workspace.tabs[0].screen.rows;
        for row in 0..rows {
            workspace.tabs[0]
                .screen
                .feed(format!("\x1b[{};1HROW_{row:02}", row + 1).as_bytes());
        }
        let mut output = ctx.run_ui(input(), |ui| {
            let _ = terminal_view(ui, &mut workspace.tabs[0], &preferences);
        });
        output.textures_delta.clear();
        fn text(shape: &egui::epaint::Shape, result: &mut String) {
            match shape {
                egui::epaint::Shape::Text(shape) => {
                    result.push_str(shape.galley.text());
                    result.push('\n');
                }
                egui::epaint::Shape::Vec(shapes) => {
                    for shape in shapes {
                        text(shape, result);
                    }
                }
                _ => {}
            }
        }
        let mut rendered = String::new();
        for shape in &output.shapes {
            text(&shape.shape, &mut rendered);
        }
        assert!(
            rendered.contains("ROW_00"),
            "The first live row was virtualized out of the viewport"
        );
        assert!(
            rendered.contains(&format!("ROW_{:02}", rows - 1)),
            "The last live row was virtualized out of the viewport"
        );
    }
    #[cfg(unix)]
    #[test]
    fn terminal_view_routes_mouse_without_broadcast_or_macro_recording() {
        let ctx = egui::Context::default();
        let root = tempfile::tempdir().unwrap();
        let mut workspace = Workspace::new_context(&ctx, None, root.path().to_owned());
        workspace.start_with_launch(
            Session {
                protocol: "Local".into(),
                ..Default::default()
            },
            Some(connection::Launch {
                rdp_profile: None,
                program: "/definitely-missing-termcraft-test-program".into(),
                args: vec![],
                directory: None,
            }),
        );
        let t = &mut workspace.tabs[0];
        t.process = Some(
            Process::spawn(
                "python3",
                &[
                    "-I".into(),
                    "-c".into(),
                    r#"import os,tty
 tty.setraw(0)
 os.write(1,b'\x1b[?1000h\x1b[?1006hREADY')
 while True:
  data=os.read(0,1024)
  if not data: break
  os.write(1,b'CAPTURE:'+data.hex().encode()+b'\r\n')
"#
                    .lines()
                    .map(|line| line.strip_prefix(' ').unwrap_or(line))
                    .collect::<Vec<_>>()
                    .join("\n"),
                ],
                None,
                110,
                30,
            )
            .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(3);
        super::super::terminal_review_tests::wait_for_fixture(
            t,
            deadline,
            "READY and SGR mouse mode",
            |tab| {
                tab.screen.plain().contains("READY")
                    && tab.screen.mouse_mode().enabled()
                    && tab.screen.mouse_mode().sgr
            },
        );
        t.recording = true;
        let preferences = workspace.store.preferences.clone();
        let mut origin = None;
        for _ in 0..2 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900., 600.))),
                    ..Default::default()
                },
                |ui| {
                    assert!(terminal_view(ui, t, &preferences).is_empty());
                },
            );
            output.textures_delta.clear();
            fn locate(shape: &egui::epaint::Shape) -> Option<Pos2> {
                match shape {
                    egui::epaint::Shape::Text(s) if s.galley.text().contains("READY") => {
                        Some(s.pos)
                    }
                    egui::epaint::Shape::Vec(s) => s.iter().find_map(locate),
                    _ => None,
                }
            }
            origin = output
                .shapes
                .iter()
                .find_map(|s| locate(&s.shape))
                .or(origin);
        }
        let pos = origin.expect("painted live row") + vec2(1., 2.);
        let events = vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::default(),
            },
            Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::default(),
            },
        ];
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900., 600.))),
                events,
                ..Default::default()
            },
            |ui| {
                assert!(
                    terminal_view(ui, t, &preferences).is_empty(),
                    "mouse must not enter keyboard broadcast"
                );
            },
        );
        output.textures_delta.clear();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !t.screen.plain().contains("CAPTURE:") && Instant::now() < deadline {
            t.poll();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            t.screen.plain().contains("1b5b3c303b313b314d"),
            "missing SGR press: {}",
            t.screen.plain()
        );
        assert!(
            t.screen.plain().contains("1b5b3c303b313b316d"),
            "missing SGR release"
        );
        assert!(
            t.recorded.is_empty(),
            "mouse events must not enter text macros"
        );
    }
    #[test]
    #[ignore = "CPU renderer benchmark; run explicitly to record frame timings"]
    fn benchmark_visible_terminal_rows_with_long_history() {
        let mut screen = Screen::new(110, 30, 360000);
        let chunk = b"owned terminal needle line\r\n".repeat(100);
        for _ in 0..3600 {
            screen.feed(&chunk);
        }
        let matches = screen.search_count("needle");
        let indexed = screen.stats().indexed_history_lines;
        let ctx = egui::Context::default();
        let mut samples = Vec::new();
        for frame in 0..120 {
            let start = std::time::Instant::now();
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let font = FontId::monospace(14.);
                let cw = ui.fonts_mut(|f| f.glyph_width(&font, 'M'));
                assert_eq!(screen.search_count("needle"), matches);
                for cells in screen.grid() {
                    let mut measure = |text: &str, font: &FontId| {
                        ui.fonts_mut(|f| {
                            f.layout_no_wrap(text.to_owned(), font.clone(), Color32::WHITE)
                                .rows
                                .first()
                                .and_then(|row| row.glyphs.last())
                                .map(|glyph| glyph.max_x())
                                .unwrap_or(0.)
                        })
                    };
                    let (job, _) =
                        terminal_layout(cells, &font, true, "needle", cw, Some(&mut measure));
                    ui.label(job);
                }
            });
            output.textures_delta.clear();
            if frame >= 20 {
                samples.push(start.elapsed().as_micros());
            }
        }
        samples.sort_unstable();
        assert_eq!(screen.stats().indexed_history_lines, indexed);
        println!(
            "T01 CPU egui renderer: profile={} input_lines=360000 visible_rows=30 cols=110 history_lines={} samples=100 frame_p50_us={} frame_p95_us={} (no native GPU/window timing)",
            if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
            screen.history().len(),
            samples[50],
            samples[95]
        );
    }
    #[test]
    fn wide_graphemes_reserve_exact_terminal_columns_in_the_galley() {
        let ctx = egui::Context::default();
        let mut screen = harbor_workspace::terminal::Screen::new(8, 3, 100);
        screen.feed("界X".as_bytes());
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let font = FontId::monospace(14.);
            let cw = ui.fonts_mut(|f| f.glyph_width(&font, 'M'));
            let mut measure = |text: &str, font: &FontId| {
                ui.fonts_mut(|f| {
                    f.layout_no_wrap(text.to_owned(), font.clone(), Color32::WHITE)
                        .rows
                        .first()
                        .and_then(|row| row.glyphs.last())
                        .map(|glyph| glyph.max_x())
                        .unwrap_or(0.)
                })
            };
            let (job, backgrounds) =
                terminal_layout(&screen.grid()[0], &font, false, "", cw, Some(&mut measure));
            assert_eq!(job.text, "界X     ");
            assert_eq!(backgrounds[0].0, 0);
            assert_eq!(backgrounds[0].1, 8);
            let galley = ui.fonts_mut(|f| f.layout_job(job));
            let x = galley.rows[0]
                .glyphs
                .iter()
                .find(|g| g.chr == 'X')
                .unwrap()
                .pos
                .x;
            assert!((x - 2. * cw).abs() <= 1., "X at {x}, expected {}", 2. * cw);
        });
        output.textures_delta.clear();
    }
    #[cfg(unix)]
    #[test]
    fn terminal_view_copies_cell_rectangle_and_opens_shift_right_click_menu() {
        let ctx = egui::Context::default();
        let root = tempfile::tempdir().unwrap();
        let mut workspace = Workspace::new_context(&ctx, None, root.path().to_owned());
        workspace.start_with_launch(
            Session {
                protocol: "Local".into(),
                ..Default::default()
            },
            Some(connection::Launch {
                rdp_profile: None,
                program: "/definitely-missing-termcraft-test-program".into(),
                args: vec![],
                directory: None,
            }),
        );
        let mut preferences = workspace.store.preferences.clone();
        preferences.clipboard.copy_on_select = true;
        preferences.clipboard.right_click = RightClickBehavior::Paste;
        let t = &mut workspace.tabs[0];
        t.screen.feed(b"ABCD\r\n1234");
        let mut positions = vec![];
        let mut width = 0.;
        fn find(shape: &egui::epaint::Shape, positions: &mut Vec<Pos2>) {
            match shape {
                egui::epaint::Shape::Text(s)
                    if s.galley.text().starts_with("ABCD")
                        || s.galley.text().starts_with("1234") =>
                {
                    positions.push(s.pos)
                }
                egui::epaint::Shape::Vec(shapes) => {
                    for shape in shapes {
                        find(shape, positions);
                    }
                }
                _ => {}
            }
        }
        for _ in 0..2 {
            positions.clear();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900., 600.))),
                    ..Default::default()
                },
                |ui| {
                    width = ui.fonts_mut(|f| {
                        f.glyph_width(&FontId::monospace(preferences.font_size), 'M')
                    });
                    assert!(terminal_view(ui, t, &preferences).is_empty());
                },
            );
            output.textures_delta.clear();
            for shape in &output.shapes {
                find(&shape.shape, &mut positions);
            }
        }
        assert_eq!(positions.len(), 2);
        let start = positions[0] + vec2(width * 1.5, 2.);
        let end = positions[1] + vec2(width * 2.5, 2.);
        let mods = Modifiers {
            alt: true,
            ..Default::default()
        };
        let events = vec![
            Event::PointerMoved(start),
            Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: mods,
            },
            Event::PointerMoved(end),
            Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: mods,
            },
        ];
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900., 600.))),
                events: {
                    let mut events = events;
                    events.insert(0, Event::ModifiersChanged(mods));
                    events
                },
                ..Default::default()
            },
            |ui| {
                assert!(terminal_view(ui, t, &preferences).is_empty());
            },
        );
        output.textures_delta.clear();
        assert!(
            output
                .platform_output
                .commands
                .iter()
                .any(|c| matches!(c,egui::OutputCommand::CopyText(s) if s=="BC\n23")),
            "missing terminal-specific clipboard copy"
        );
        let mods = Modifiers {
            shift: true,
            ..Default::default()
        };
        let events = vec![
            Event::PointerMoved(end),
            Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Secondary,
                pressed: true,
                modifiers: mods,
            },
            Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Secondary,
                pressed: false,
                modifiers: mods,
            },
        ];
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900., 600.))),
                events: {
                    let mut events = events;
                    events.insert(0, Event::ModifiersChanged(mods));
                    events
                },
                ..Default::default()
            },
            |ui| {
                assert!(terminal_view(ui, t, &preferences).is_empty());
            },
        );
        output.textures_delta.clear();
        fn menu(shape: &egui::epaint::Shape) -> bool {
            match shape {
                egui::epaint::Shape::Text(s) => s.galley.text().contains("Copy selection"),
                egui::epaint::Shape::Vec(s) => s.iter().any(menu),
                _ => false,
            }
        }
        // A newly opened egui Area needs a sizing frame before its first paint.
        assert!(Popup::is_any_open(&ctx), "context menu was not opened");
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900., 600.))),
                ..Default::default()
            },
            |ui| {
                assert!(terminal_view(ui, t, &preferences).is_empty());
            },
        );
        output.textures_delta.clear();
        assert!(
            output.shapes.iter().any(|s| menu(&s.shape)),
            "Shift-right-click must reach the menu in paste mode"
        );
        assert!(
            output
                .viewport_output
                .values()
                .all(|v| !v.commands.contains(&egui::ViewportCommand::RequestPaste))
        );
    }
    #[cfg(unix)]
    #[test]
    fn oversized_fixed_alternate_grid_can_reveal_its_far_controls() {
        use harbor_workspace::terminal::settings::{Dimensions, TerminalSettings};
        let ctx = egui::Context::default();
        let root = tempfile::tempdir().unwrap();
        let mut w = Workspace::new_context(&ctx, None, root.path().to_owned());
        w.start_with_launch(
            Session {
                protocol: "Local".into(),
                ..Default::default()
            },
            Some(connection::Launch {
                rdp_profile: None,
                program: "/definitely-missing-termcraft-test-program".into(),
                args: vec![],
                directory: None,
            }),
        );
        let mut p = w.store.preferences.clone();
        p.terminal = Some(TerminalSettings {
            dimensions: Dimensions::Fixed {
                cols: 300,
                rows: 80,
            },
            ..Default::default()
        });
        w.tabs[0].apply_terminal_preferences(&p);
        w.tabs[0].screen.feed(b"\x1b[?1049h\x1b[80;300HZ");
        let mut visible = false;
        for index in 0..20 {
            let events = if index == 3 {
                vec![
                    Event::PointerMoved(pos2(400., 200.)),
                    Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: vec2(-10000., 0.),
                        phase: egui::TouchPhase::Move,
                        modifiers: Modifiers::default(),
                    },
                ]
            } else {
                vec![]
            };
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900., 600.))),
                    time: Some(index as f64 / 10.),
                    events,
                    ..Default::default()
                },
                |ui| {
                    terminal_view(ui, &mut w.tabs[0], &p);
                },
            );
            out.textures_delta.clear();
            fn marker(shape: &egui::epaint::Shape, clip: Rect) -> bool {
                match shape {
                    egui::epaint::Shape::Text(s) => s
                        .galley
                        .rows
                        .iter()
                        .flat_map(|r| &r.glyphs)
                        .filter(|g| g.chr == 'Z')
                        .any(|g| {
                            clip.intersect(Rect::from_min_size(
                                s.pos + g.pos.to_vec2(),
                                vec2(6., 12.),
                            ))
                            .is_positive()
                        }),
                    egui::epaint::Shape::Vec(s) => s.iter().any(|s| marker(s, clip)),
                    _ => false,
                }
            }
            if index > 3 {
                visible |= out.shapes.iter().any(|s| marker(&s.shape, s.clip_rect));
            }
        }
        assert!(
            visible,
            "local scrolling must reveal fixed alternate controls outside the initial viewport"
        );
    }
    #[test]
    fn proportional_font_keeps_physical_terminal_columns() {
        let ctx = egui::Context::default();
        let root = tempfile::tempdir().unwrap();
        let _workspace = Workspace::new_context(&ctx, None, root.path().to_owned());
        let mut screen = Screen::new(8, 2, 100);
        screen.feed(b"iWiX");
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let font = FontId::new(18., FontFamily::Name("Liberation Sans".into()));
            let width = ui.fonts_mut(|f| f.glyph_width(&font, 'M'));
            let mut measure = |text: &str, font: &FontId| {
                ui.fonts_mut(|f| {
                    f.layout_no_wrap(text.into(), font.clone(), Color32::WHITE)
                        .rows[0]
                        .glyphs
                        .last()
                        .map(|g| g.max_x())
                        .unwrap_or(0.)
                })
            };
            let (job, _) = terminal_layout_profile(
                &screen.grid()[0],
                &font,
                false,
                "",
                width,
                Some(&mut measure),
                None,
                None,
                &harbor_workspace::terminal::settings::TerminalPalette::light(),
            );
            let galley = ui.fonts_mut(|f| f.layout_job(job));
            let x = galley.rows[0]
                .glyphs
                .iter()
                .find(|g| g.chr == 'X')
                .unwrap()
                .pos
                .x;
            assert!(
                (x - 3. * width).abs() < 1.,
                "X at {x}, physical column 3 is {}",
                3. * width
            );
        });
        output.textures_delta.clear();
    }
    #[cfg(unix)]
    #[test]
    fn configured_terminal_profile_changes_actual_paint_and_dimensions() {
        use harbor_workspace::terminal::settings::{Dimensions, TerminalFont, TerminalSettings};
        let ctx = egui::Context::default();
        let root = tempfile::tempdir().unwrap();
        let mut w = Workspace::new_context(&ctx, None, root.path().to_owned());
        w.start_with_launch(
            Session {
                protocol: "Local".into(),
                ..Default::default()
            },
            Some(connection::Launch {
                rdp_profile: None,
                program: "/definitely-missing-termcraft-test-program".into(),
                args: vec![],
                directory: None,
            }),
        );
        w.tabs[0].screen.feed(b"PROFILE");
        let mut preferences = w.store.preferences.clone();
        let mut profile = TerminalSettings {
            font_family: TerminalFont::LiberationSans,
            font_size: 18.,
            dimensions: Dimensions::Fixed { cols: 132, rows: 3 },
            ..Default::default()
        };
        profile.light_palette.foreground = [12, 34, 56];
        profile.light_palette.background = [240, 249, 242];
        preferences.terminal = Some(profile);
        let mut matched = false;
        for _ in 0..2 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900., 600.))),
                    ..Default::default()
                },
                |ui| {
                    terminal_view(ui, &mut w.tabs[0], &preferences);
                },
            );
            output.textures_delta.clear();
            fn inspect(shape: &egui::epaint::Shape) -> bool {
                match shape {
                    egui::epaint::Shape::Text(s) if s.galley.text().starts_with("PROFILE") => {
                        s.galley.job.sections[0].format.font_id.family
                            == FontFamily::Name("Liberation Sans".into())
                            && s.galley.job.sections[0].format.font_id.size == 18.
                            && s.galley.job.sections[0].format.color
                                == Color32::from_rgb(12, 34, 56)
                    }
                    egui::epaint::Shape::Vec(s) => s.iter().any(inspect),
                    _ => false,
                }
            }
            matched |= output.shapes.iter().any(|s| inspect(&s.shape));
        }
        assert_eq!(w.tabs[0].screen.cols, 132);
        assert_eq!(w.tabs[0].screen.rows, 3);
        assert!(
            matched,
            "profile font and palette did not reach the painted terminal"
        );
    }
    #[test]
    fn physical_selection_highlight_has_priority_and_covers_wide_cells() {
        use harbor_workspace::terminal::highlight::HighlightRules;
        let rules = HighlightRules::compile(&[HighlightRule {
            pattern: "界".into(),
            ..Default::default()
        }])
        .unwrap();
        let mut screen = Screen::new(8, 3, 100);
        screen.feed("A界B".as_bytes());
        let (job, _) = terminal_layout_selected(
            &screen.grid()[0],
            &FontId::monospace(14.),
            false,
            "界",
            0.,
            None,
            Some(&rules),
            Some(2..3),
        );
        let section = job
            .sections
            .iter()
            .find(|s| s.byte_range.start.0 <= 1 && s.byte_range.end.0 > 1)
            .unwrap();
        assert_eq!(section.format.background, Color32::from_rgb(15, 105, 73));
        assert_eq!(job.text, "A界B    ");
    }
    #[test]
    fn keyword_rendering_preserves_ansi_and_search_has_priority() {
        use harbor_workspace::terminal::highlight::HighlightRules;
        let rules = HighlightRules::compile(&[HighlightRule {
            pattern: "ERROR".into(),
            ..Default::default()
        }])
        .unwrap();
        let mut screen = Screen::new(20, 3, 100);
        screen.feed(b"\x1b[34mX ERROR Y");
        let before = screen.plain();
        for dark in [false, true] {
            let (job, _) = terminal_layout_highlight(
                &screen.grid()[0],
                &FontId::monospace(14.),
                dark,
                "",
                0.,
                None,
                Some(&rules),
            );
            assert_eq!(
                job.sections[0].format.color,
                Color32::from_rgb(18, 150, 190)
            );
            let expected = if dark {
                Color32::from_rgb(91, 28, 39)
            } else {
                Color32::from_rgb(255, 224, 226)
            };
            assert_eq!(
                job.sections
                    .iter()
                    .find(|s| s.byte_range.start.0 <= 2 && s.byte_range.end.0 > 2)
                    .unwrap()
                    .format
                    .background,
                expected
            );
            let (job, _) = terminal_layout_highlight(
                &screen.grid()[0],
                &FontId::monospace(14.),
                dark,
                "ERROR",
                0.,
                None,
                Some(&rules),
            );
            assert_eq!(
                job.sections
                    .iter()
                    .find(|s| s.byte_range.start.0 <= 2 && s.byte_range.end.0 > 2)
                    .unwrap()
                    .format
                    .background,
                Color32::from_rgb(255, 222, 115)
            );
        }
        assert_eq!(screen.plain(), before);
    }
    #[test]
    fn search_only_highlights_matching_text_and_inverse_stays_legible() {
        let mut screen = Screen::new(20, 3, 100);
        screen.feed(b"before match after");
        let job = terminal_job(&screen.grid()[0], &FontId::monospace(14.), false, "match");
        let highlighted: String = job
            .sections
            .iter()
            .filter(|s| s.format.background == Color32::from_rgb(255, 222, 115))
            .map(|s| &job.text[s.byte_range.start.0..s.byte_range.end.0])
            .collect();
        assert_eq!(highlighted, "match");
        screen.feed(b"\r\x1b[7mX");
        for dark in [false, true] {
            let job = terminal_job(&screen.grid()[0], &FontId::monospace(14.), dark, "");
            assert_ne!(
                job.sections[0].format.color,
                job.sections[0].format.background
            );
        }
    }
}
