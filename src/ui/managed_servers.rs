use super::*;
use harbor_workspace::{
    credentials::Secret,
    services::{
        ServiceAuth, ServiceConfig, ServiceId, ServiceProtocol, ServiceState, ServiceSupervisor,
    },
};
use std::{net::SocketAddr, path::Path};
use zeroize::{Zeroize, Zeroizing};

pub(super) struct ManagedServers {
    supervisor: ServiceSupervisor,
    forms: [Form; 4],
}

struct Form {
    protocol: ServiceProtocol,
    bind: String,
    root: String,
    executable: String,
    username: String,
    password: Zeroizing<String>,
    lifetime: u32,
    allow_write: bool,
    allow_non_loopback: bool,
    anonymous: bool,
    id: Option<ServiceId>,
    error: Option<String>,
}

impl Default for ManagedServers {
    fn default() -> Self {
        Self {
            supervisor: ServiceSupervisor::default(),
            forms: [
                ServiceProtocol::Ftp,
                ServiceProtocol::Tftp,
                ServiceProtocol::Telnet,
                ServiceProtocol::Nfs,
            ]
            .map(Form::new),
        }
    }
}

impl ManagedServers {
    pub(super) fn cards(&mut self, ui: &mut Ui) {
        for form in &mut self.forms {
            form.card(ui, &self.supervisor);
            ui.add_space(17.);
        }
    }
}

impl Form {
    fn new(protocol: ServiceProtocol) -> Self {
        let (address, lifetime) = managed_service_defaults(protocol);
        Self {
            protocol,
            bind: address.to_string(),
            root: String::new(),
            executable: String::new(),
            username: String::new(),
            password: Zeroizing::new(String::new()),
            lifetime,
            allow_write: false,
            allow_non_loopback: false,
            anonymous: false,
            id: None,
            error: None,
        }
    }

    fn name(&self) -> &'static str {
        match self.protocol {
            ServiceProtocol::Ftp => "FTP",
            ServiceProtocol::Tftp => "TFTP",
            ServiceProtocol::Nfs => "NFS",
            ServiceProtocol::Telnet => "Telnet",
            _ => "Other",
        }
    }

    fn card(&mut self, ui: &mut Ui, supervisor: &ServiceSupervisor) {
        let status = self.id.and_then(|id| supervisor.status(id));
        let state = status
            .as_ref()
            .map_or(ServiceState::Stopped, |status| status.state);
        let busy = matches!(
            state,
            ServiceState::Starting | ServiceState::Running | ServiceState::Stopping
        );
        let name = self.name();
        let title = match self.protocol {
            ServiceProtocol::Ftp => "FTP file server",
            ServiceProtocol::Tftp => "TFTP file server",
            ServiceProtocol::Nfs => "NFS file server",
            ServiceProtocol::Telnet => "Telnet shell server",
            _ => "Other server",
        };
        ui.push_id(name, |ui| {
            surface(ui).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(crate::ui::fluent::emphasized(ui.ctx(), RichText::new(title).size(18.)));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        dot(ui, if state == ServiceState::Running { accent(ui) } else { muted(ui) });
                        ui.label(match state {
                            ServiceState::Starting => "Starting",
                            ServiceState::Running => "Running",
                            ServiceState::Stopping => "Stopping",
                            ServiceState::Stopped => "Stopped",
                            ServiceState::Failed => "Failed",
                        });
                    });
                });
                ui.label(RichText::new(match self.protocol {
                    ServiceProtocol::Ftp => "Clients sign in to browse or transfer files. FTP sends credentials and files in clear text.",
                    ServiceProtocol::Nfs => "NFSv3 /share over TCP. Use this port for both NFS and MOUNT; no portmapper. AUTH_SYS and anonymous clients share your account’s access without a password or encryption.",
                    ServiceProtocol::Tftp => "Clients can download files without signing in. Uploads are optional. Use octet mode.",
                    _ => "Signed-in clients can use the selected shell with your workspace account’s permissions. Telnet sends credentials and shell traffic in clear text.",
                }).color(muted(ui)));
                ui.add_space(5.);
                ui.add_enabled_ui(!busy, |ui| self.fields(ui));
                if let Some(address) = status.as_ref().and_then(|status| status.address).filter(|_| state == ServiceState::Running) {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(address.to_string()).monospace());
                        if ui.button("Copy address").clicked() {
                            ui.ctx().copy_text(address.to_string());
                        }
                    });
                }
                if let Some(error) = self.error.as_ref().or_else(|| status.as_ref().and_then(|status| status.error.as_ref())) {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                if busy {
                    ui.ctx().request_repaint_after(Duration::from_millis(125));
                    let action = if state == ServiceState::Starting { format!("Cancel {name} startup") } else { format!("Stop {name} server") };
                    if ui.add_enabled(state != ServiceState::Stopping, Button::new(action)).clicked() {
                        if let Some(id) = self.id {
                            self.error = supervisor.stop(id).err();
                        }
                    }
                } else {
                    let label = if state == ServiceState::Stopped && status.as_ref().is_some_and(|status| status.address.is_some()) {
                        format!("Restart {name} server")
                    } else {
                        format!("Start {name} server")
                    };
                    if primary(ui, &label).clicked() {
                        self.start(supervisor);
                        ui.ctx().request_repaint();
                    }
                }
                if let Some(status) = status.filter(|status| !status.log.is_empty()) {
                    CollapsingHeader::new("Service log").show(ui, |ui| {
                        ScrollArea::vertical().max_height(110.).show(ui, |ui| {
                            for event in status.log {
                                ui.label(event);
                            }
                        });
                    });
                }
            });
        });
    }

    fn fields(&mut self, ui: &mut Ui) {
        fluent::settings_grid(ui, "configuration", |ui| {
            if self.protocol == ServiceProtocol::Telnet {
                let label = ui.label("Shell executable");
                ui.horizontal_wrapped(|ui| {
                    fluent::text_input(ui, label.id, &mut self.executable, "Absolute path", 300.)
                        .fluent_hint(&self.executable);
                    if ui.button("Browse").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_file() {
                            self.executable = path.to_string_lossy().into_owned();
                        }
                    }
                });
            } else {
                let label = ui.label("Shared folder");
                ui.horizontal_wrapped(|ui| {
                    fluent::text_input(ui, label.id, &mut self.root, "", 300.)
                        .fluent_hint(&self.root);
                    if ui.button("Browse").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            self.root = path.to_string_lossy().into_owned();
                        }
                    }
                });
            }
            ui.end_row();
            let label = ui.label("Listen address");
            fluent::text_input(ui, label.id, &mut self.bind, "", 210.);
            ui.end_row();
            ui.label("Network access");
            ui.checkbox(&mut self.allow_non_loopback, "Allow a non-loopback address");
            ui.end_row();
            if self.protocol == ServiceProtocol::Ftp {
                ui.label("Access");
                if ui
                    .checkbox(&mut self.anonymous, "Allow anonymous sign-in (read-only)")
                    .changed()
                    && self.anonymous
                {
                    self.allow_write = false;
                    self.password.zeroize();
                }
                ui.end_row();
            }
            if self.protocol == ServiceProtocol::Telnet
                || (self.protocol == ServiceProtocol::Ftp && !self.anonymous)
            {
                let label = ui.label("Username");
                fluent::text_input(ui, label.id, &mut self.username, "", 210.);
                ui.end_row();
                let label = ui.label("Password");
                service_password_field(ui, &mut self.password, 210.).labelled_by(label.id);
                ui.end_row();
            }
            if self.protocol != ServiceProtocol::Telnet && !self.anonymous {
                ui.label("Permissions");
                ui.checkbox(
                    &mut self.allow_write,
                    match self.protocol {
                        ServiceProtocol::Ftp => "Allow uploads, renames and deletes",
                        ServiceProtocol::Nfs => "Allow writes, replacement, renames and deletes",
                        _ => "Allow uploads",
                    },
                );
                ui.end_row();
            }
            ui.label("Stop after");
            ui.add(
                DragValue::new(&mut self.lifetime)
                    .range(0..=86400)
                    .suffix(" seconds (0 = until stopped)"),
            );
            ui.end_row();
        });
    }

    fn start(&mut self, supervisor: &ServiceSupervisor) {
        match self.config().and_then(|config| supervisor.begin(config)) {
            Ok(id) => {
                self.id = Some(id);
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn config(&self) -> Result<ServiceConfig, String> {
        let bind = self
            .bind
            .trim()
            .parse::<SocketAddr>()
            .map_err(|_| "Enter an IP address and port, for example 127.0.0.1:2121.")?;
        let make_password = || Secret::new(self.password.as_str());
        let mut config = match self.protocol {
            ServiceProtocol::Vnc
            | ServiceProtocol::Ssh
            | ServiceProtocol::Iperf
            | ServiceProtocol::Cron => {
                return Err("Use the SSH or Iperf service card.".into());
            }
            ServiceProtocol::Ftp => ServiceConfig::ftp(
                PathBuf::from(&self.root),
                self.username.clone(),
                make_password(),
            ),
            ServiceProtocol::Tftp => ServiceConfig::tftp(PathBuf::from(&self.root)),
            ServiceProtocol::Nfs => ServiceConfig::nfs(PathBuf::from(&self.root)),
            ServiceProtocol::Telnet => ServiceConfig::telnet(
                PathBuf::from(&self.executable),
                self.username.clone(),
                make_password(),
            ),
        };
        if config
            .root
            .as_ref()
            .is_some_and(|root| root == Path::new(""))
        {
            return Err("Choose a shared folder.".into());
        }
        if self.anonymous {
            config.auth = ServiceAuth::Anonymous;
        }
        config.bind = bind;
        config.allow_non_loopback = self.allow_non_loopback;
        config.allow_write = self.allow_write;
        config.lifetime = (self.lifetime != 0).then(|| Duration::from_secs(self.lifetime.into()));
        Ok(config)
    }
}

// Masking a TextEdit does not prevent egui from keeping plaintext undo snapshots.
pub(super) fn service_password_field(ui: &mut Ui, password: &mut String, width: f32) -> Response {
    let id = ui.next_auto_id();
    ui.skip_ahead_auto_ids(1);
    if let Some(mut state) = egui::text_edit::TextEditState::load(ui.ctx(), id) {
        state.clear_undoer();
        state.store(ui.ctx(), id);
    }
    let response = crate::ui::fluent::singleline(password)
        .id(id)
        .password(true)
        .desired_width(width.min(ui.available_width()).max(1.))
        .show(ui)
        .response
        .response;
    if let Some(mut state) = egui::text_edit::TextEditState::load(ui.ctx(), id) {
        state.clear_undoer();
        state.store(ui.ctx(), id);
    }
    response
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use std::net::{TcpStream, UdpSocket};

    #[test]
    fn actual_service_fields_reference_their_labels_at_both_form_widths() {
        use egui::accesskit::Role;
        for width in [360., 820.] {
            for service in ["HTTP", "FTP", "Telnet"] {
                let (ctx, _root, mut w) = crate::ui::terminal_review_tests::owned_workspace();
                ctx.enable_accesskit();
                let mut form = Form::new(if service == "Telnet" {
                    ServiceProtocol::Telnet
                } else {
                    ServiceProtocol::Ftp
                });
                form.anonymous = false;
                form.password.push_str("test-secret");
                let mut draw = || {
                    crate::ui::gui_tests::shell_frame(&ctx, vec2(width, 1500.), 1., vec![], |ui| {
                        if service == "HTTP" {
                            w.servers_page(ui);
                        } else {
                            form.fields(ui);
                        }
                    })
                };
                draw();
                let nodes = draw().platform_output.accesskit_update.unwrap().nodes;
                let labels: &[&str] = match service {
                    "HTTP" => &["Shared folder", "Listen address"],
                    "FTP" => &["Shared folder", "Listen address", "Username", "Password"],
                    _ => &["Shell executable", "Listen address", "Username", "Password"],
                };
                for &label in labels {
                    let (id, _) = nodes
                        .iter()
                        .find(|(_, n)| {
                            n.role() == Role::Label
                                && (n.value() == Some(label) || n.label() == Some(label))
                        })
                        .unwrap();
                    assert!(
                        nodes.iter().any(|(_, n)| n.labelled_by().contains(id)
                            && n.role()
                                == if label == "Password" {
                                    Role::PasswordInput
                                } else {
                                    Role::TextInput
                                }),
                        "{service} {label} at {width}"
                    );
                }
                assert!(!nodes.iter().any(|(_, n)| {
                    [n.value(), n.label()]
                        .into_iter()
                        .flatten()
                        .any(|s| s.contains("test-secret"))
                }));
            }
        }
    }

    fn label_position(shape: &egui::epaint::Shape, value: &str) -> Option<Pos2> {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text() == value => {
                Some(text.pos + vec2(5., 5.))
            }
            egui::epaint::Shape::Vec(shapes) => {
                shapes.iter().find_map(|shape| label_position(shape, value))
            }
            _ => None,
        }
    }

    pub(in crate::ui) fn frame(
        ctx: &Context,
        workspace: &mut Workspace,
        events: Vec<Event>,
    ) -> FullOutput {
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1450., 2500.))),
                events,
                ..Default::default()
            },
            |ui| workspace.render(ui),
        );
        output.textures_delta.clear();
        output
    }

    pub(in crate::ui) fn click(ctx: &Context, workspace: &mut Workspace, label: &str) {
        let target = |output: &FullOutput| {
            output.shapes.iter().find_map(|shape| {
                label_position(&shape.shape, label).filter(|point| shape.clip_rect.contains(*point))
            })
        };
        let mut output = frame(ctx, workspace, vec![]);
        let mut pos = target(&output);
        for offset in [-700., -700., 700., 700., 700.] {
            if pos.is_some() {
                break;
            }
            frame(
                ctx,
                workspace,
                vec![
                    Event::PointerMoved(pos2(720., 520.)),
                    Event::MouseWheel {
                        unit: MouseWheelUnit::Point,
                        delta: vec2(0., offset),
                        phase: TouchPhase::Move,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
            // Let wheel smoothing finish before locating the pointer target.
            for _ in 0..40 {
                output = frame(ctx, workspace, vec![]);
            }
            pos = target(&output);
        }
        let pos = pos.unwrap_or_else(|| {
            let labels: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::epaint::Shape::Text(text) => Some(text.galley.text()),
                    _ => None,
                })
                .collect();
            panic!("Servers UI did not paint {label}. Painted text: {labels:?}")
        });
        frame(ctx, workspace, vec![Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            frame(
                ctx,
                workspace,
                vec![Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                }],
            );
        }
    }

    fn wait_state(
        ctx: &Context,
        work: &mut Workspace,
        index: usize,
        expected: ServiceState,
    ) -> SocketAddr {
        wait_state_until(
            ctx,
            work,
            index,
            expected,
            Instant::now() + Duration::from_secs(4),
        )
    }

    fn wait_state_until(
        ctx: &Context,
        work: &mut Workspace,
        index: usize,
        expected: ServiceState,
        deadline: Instant,
    ) -> SocketAddr {
        let id = work.managed_servers.forms[index]
            .id
            .expect("Servers UI did not start a service");
        loop {
            frame(ctx, work, vec![]);
            let status = work.managed_servers.supervisor.status(id).unwrap();
            assert!(
                Instant::now() < deadline,
                "Service state observed after deadline: {status:?}"
            );
            if status.state == expected {
                return status.address.unwrap();
            }
            assert_ne!(status.state, ServiceState::Failed, "{status:?}");
            std::thread::sleep(
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(10)),
            );
        }
    }

    fn wait_refusal_until(
        deadline: Instant,
        mut connect: impl FnMut(Duration) -> std::io::Result<()>,
    ) {
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                !remaining.is_zero(),
                "Telnet stop deadline expired before probe"
            );
            let outcome = connect(remaining.min(Duration::from_millis(50)));
            // Even immediate refusal is success only inside the shared deadline.
            assert!(
                Instant::now() < deadline,
                "Telnet stop probe completed after deadline"
            );
            match outcome {
                Err(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => return,
                Ok(()) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::TimedOut
                            | std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(error) => panic!("Telnet stop probe failed without proving refusal: {error}"),
            }
            std::thread::sleep(
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(10)),
            );
        }
    }

    #[test]
    fn refusal_probe_rejects_late_refusal_and_retries_timeout() {
        let late = std::panic::catch_unwind(|| {
            let deadline = Instant::now() + Duration::from_millis(2);
            wait_refusal_until(deadline, |_| {
                // Deliberately return a real refusal only after the budget expired.
                std::thread::sleep(Duration::from_millis(3));
                Err(std::io::ErrorKind::ConnectionRefused.into())
            });
        });
        assert!(
            late.is_err(),
            "a refusal observed after the deadline was accepted"
        );
        let mut attempts = 0;
        wait_refusal_until(Instant::now() + Duration::from_secs(1), |remaining| {
            assert!(!remaining.is_zero());
            attempts += 1;
            Err(if attempts == 1 {
                std::io::ErrorKind::TimedOut
            } else {
                std::io::ErrorKind::ConnectionRefused
            }
            .into())
        });
        assert_eq!(attempts, 2, "timeout must not count as listener refusal");
    }

    // Removing the password field's undo clearing must fail this regression.
    pub(in crate::ui) fn password_history<T>(
        form: &mut T,
        mut render: impl FnMut(&mut T, &mut Ui),
        mut clear: impl FnMut(&mut T),
        label: &str,
    ) {
        let ctx = Context::default();
        let mut frame = |form: &mut T, time: f64, events: Vec<Event>| {
            let mut out = ctx.run_ui(
                RawInput {
                    time: Some(time),
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200., 1200.))),
                    events,
                    ..Default::default()
                },
                |ui| render(form, ui),
            );
            out.textures_delta.clear();
            out
        };
        frame(form, 0., vec![]);
        let out = frame(form, 0.01, vec![]);
        let label_pos = out
            .shapes
            .iter()
            .find_map(|s| label_position(&s.shape, label))
            .expect("password label");
        let pos = label_pos + vec2(245., 0.);
        frame(form, 0.02, vec![Event::PointerMoved(pos)]);
        for (time, pressed) in [(0.03, true), (0.04, false)] {
            frame(
                form,
                time,
                vec![Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                }],
            );
        }
        let id = ctx.memory(|m| m.focused()).expect("password field focused");
        frame(form, 0.1, vec![Event::Text("FAKE_SERVICE_PASSWORD".into())]);
        frame(form, 2., vec![]);
        clear(form);
        frame(form, 4., vec![]);
        let state = egui::text_edit::TextEditState::load(&ctx, id).expect("text editor state");
        let empty = (egui::text::CCursorRange::default(), String::new());
        let recovered_secret = state
            .undoer()
            .undo(&empty)
            .is_some_and(|(_, text)| text == "FAKE_SERVICE_PASSWORD");
        assert!(
            !recovered_secret,
            "TextEdit undo recovered the cleared fixture password"
        );
        assert!(
            !state.undoer().has_undo(&empty),
            "cleared service password remains in TextEdit undo history"
        );
    }

    fn check_protocol_password(protocol: ServiceProtocol) {
        let mut form = Form::new(protocol);
        password_history(
            &mut form,
            |form, ui| form.fields(ui),
            |form| {
                assert_eq!(form.password.as_str(), "FAKE_SERVICE_PASSWORD");
                form.password.zeroize();
            },
            "Password",
        );
    }
    #[test]
    fn ftp_service_password_field_leaves_no_undo_history() {
        check_protocol_password(ServiceProtocol::Ftp);
    }
    #[test]
    fn telnet_service_password_field_leaves_no_undo_history() {
        check_protocol_password(ServiceProtocol::Telnet);
    }

    #[test]
    fn nfs_servers_page_pointer_start_stop() {
        let ctx = Context::default();
        let root = tempfile::tempdir().unwrap();
        let mut work = Workspace::new_context(&ctx, None, root.path().to_owned());
        work.page = Page::Servers;
        let form = &mut work.managed_servers.forms[3];
        form.root = root.path().to_string_lossy().into_owned();
        form.bind = "127.0.0.1:0".into();
        assert!(!form.allow_write);
        assert!(!form.allow_non_loopback);
        click(&ctx, &mut work, "Start NFS server");
        let address = wait_state(&ctx, &mut work, 3, ServiceState::Running);
        assert!(TcpStream::connect(address).is_ok());
        click(&ctx, &mut work, "Stop NFS server");
        wait_state(&ctx, &mut work, 3, ServiceState::Stopped);
        assert!(TcpStream::connect(address).is_err());
    }

    #[test]
    fn servers_page_buttons_own_real_listeners_and_restart() {
        let ctx = Context::default();
        let root = tempfile::tempdir().unwrap();
        let mut work = Workspace::new_context(&ctx, None, root.path().to_owned());
        // Required credentials are ephemeral UI data. Button events below go through the actual page.
        work.page = Page::Servers;
        for form in &mut work.managed_servers.forms {
            form.bind = "127.0.0.1:0".into();
            form.root = root.path().to_string_lossy().into_owned();
            form.username = "fixture".into();
            *form.password = "ephemeral service password".into();
        }
        #[cfg(unix)]
        {
            work.managed_servers.forms[2].executable = "/bin/sh".into();
        }
        click(&ctx, &mut work, "Start FTP server");
        let first = wait_state(&ctx, &mut work, 0, ServiceState::Running);
        assert!(TcpStream::connect(first).is_ok());
        click(&ctx, &mut work, "Stop FTP server");
        assert_eq!(wait_state(&ctx, &mut work, 0, ServiceState::Stopped), first);
        assert!(TcpStream::connect(first).is_err());
        let old_id = work.managed_servers.forms[0].id;
        click(&ctx, &mut work, "Restart FTP server");
        let restarted = wait_state(&ctx, &mut work, 0, ServiceState::Running);
        assert_ne!(old_id, work.managed_servers.forms[0].id);
        assert!(TcpStream::connect(restarted).is_ok());
        click(&ctx, &mut work, "Start TFTP server");
        let tftp = wait_state(&ctx, &mut work, 1, ServiceState::Running);
        assert!(UdpSocket::bind(tftp).is_err());
        #[cfg(unix)]
        {
            click(&ctx, &mut work, "Start Telnet server");
            let telnet = wait_state(&ctx, &mut work, 2, ServiceState::Running);
            assert!(TcpStream::connect(telnet).is_ok());
            let deadline = Instant::now() + Duration::from_secs(4);
            click(&ctx, &mut work, "Stop Telnet server");
            wait_state_until(&ctx, &mut work, 2, ServiceState::Stopped, deadline);
            // Another fixture can be between fork and exec with a copied socket
            // descriptor even after this worker has closed its own listener.
            // Observe OS completion within the same stop deadline.
            wait_refusal_until(deadline, |remaining| {
                TcpStream::connect_timeout(&telnet, remaining).map(drop)
            });
        }
        click(&ctx, &mut work, "Stop TFTP server");
        wait_state(&ctx, &mut work, 1, ServiceState::Stopped);
        assert!(UdpSocket::bind(tftp).is_ok());
        let store = serde_json::to_string(&work.store).unwrap();
        assert!(!store.contains("ephemeral service password"));
        drop(work);
        assert!(TcpStream::connect(restarted).is_err());
    }
}
