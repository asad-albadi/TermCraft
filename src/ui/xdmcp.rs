use super::*;
use harbor_workspace::x11::{
    ManagedXdmcp, XServerManager,
    xdmcp::{
        BroadcastNetwork, ClipboardPolicy, DiscoveredEndpoint, Discovery, DiscoveryRequest,
        QueryMode,
    },
    xdmcp_host::{HostRequest, Resolution},
};
#[derive(Default)]
pub(super) struct State {
    pending: Option<Discovery>,
    resolving: Option<(Resolution, bool)>,
    resolved: Vec<std::net::SocketAddr>,
    generation: u64,
    selection: Option<(HostRequest, std::net::SocketAddr)>,
    endpoints: Vec<DiscoveredEndpoint>,
    error: String,
    fingerprint: String,
    session_id: Option<String>,
    interface: String,
    prefix: u8,
    advertise: String,
}
impl State {
    pub(super) fn end_editor(&mut self) {
        if self.session_id.is_some() {
            *self = Self {
                generation: self.generation.wrapping_add(1),
                ..Self::default()
            };
        }
    }

    fn request(&self, s: &Session) -> HostRequest {
        HostRequest {
            host: s.host.clone(),
            port: s.port,
            mode: s.xdmcp_options.mode,
            generation: self.generation,
        }
    }

    fn clear_request_results(&mut self) {
        self.resolved.clear();
        self.endpoints.clear();
        self.error.clear();
    }

    fn start_resolution(&mut self, request: HostRequest, discover: bool) {
        self.clear_request_results();
        match Resolution::start(request) {
            Ok(resolution) => self.resolving = Some((resolution, discover)),
            Err(error) => self.error = error,
        }
    }

    // Call only after the dialog frame is known to retain this XDMCP editor.
    // A ready DNS answer can start UDP discovery, so paint cannot do this.
    pub(super) fn finish_resolution(&mut self, s: &mut Session, ctx: &Context) {
        let request = self.request(s);
        let Some((result, discover)) = self
            .resolving
            .as_ref()
            .and_then(|(job, discover)| job.try_finish(&request).map(|result| (result, *discover)))
        else {
            return;
        };
        self.resolving = None;
        ctx.request_repaint();
        match result {
            Ok(addresses) => {
                self.resolved = addresses;
                let preference = s
                    .xdmcp_options
                    .selected_endpoint
                    .filter(|ip| self.resolved.contains(ip));
                if preference.is_none() {
                    self.selection = None;
                    s.xdmcp_options.selected_endpoint = None;
                }
                if discover {
                    let target =
                        preference.or_else(|| (self.resolved.len() == 1).then(|| self.resolved[0]));
                    let result = target
                        .ok_or_else(|| "Hostname has several IPv4 endpoints. Resolve and select one before discovery".to_owned())
                        .and_then(|endpoint| {
                            Discovery::start(DiscoveryRequest {
                                endpoint,
                                mode: request.mode,
                                network: None,
                            })
                        });
                    match result {
                        Ok(pending) => self.pending = Some(pending),
                        Err(error) => self.error = error,
                    }
                }
            }
            Err(error) => self.error = error,
        }
    }
}
pub(super) fn sync_editor(
    s: &mut Session,
    state: &mut State,
) -> (HostRequest, Option<std::net::SocketAddr>) {
    if state.session_id.as_deref() != Some(s.id.as_str()) {
        state.end_editor();
        *state = State {
            session_id: Some(s.id.clone()),
            generation: state.generation,
            interface: s
                .xdmcp_options
                .discovery_network
                .map(|n| n.interface.to_string())
                .unwrap_or_default(),
            prefix: s
                .xdmcp_options
                .discovery_network
                .map(|n| n.prefix)
                .unwrap_or(24),
            advertise: s
                .xdmcp_options
                .advertise_address
                .map(|ip| ip.to_string())
                .unwrap_or_default(),
            ..Default::default()
        };
    }
    let fingerprint = format!(
        "{}:{}:{:?}:{:?}",
        s.host, s.port, s.xdmcp_options.mode, s.xdmcp_options.discovery_network
    );
    if state.fingerprint != fingerprint {
        state.pending = None;
        state.resolving = None;
        state.clear_request_results();
        state.selection = None;
        state.generation = state.generation.wrapping_add(1);
        state.fingerprint = fingerprint;
    }
    let request = state.request(s);
    if let Some(result) = state.pending.as_ref().and_then(Discovery::try_finish) {
        state.pending = None;
        match result {
            Ok(endpoints) => state.endpoints = endpoints,
            Err(error) => state.error = error,
        }
    }
    let endpoint = s
        .host
        .parse::<std::net::IpAddr>()
        .ok()
        .map(|ip| std::net::SocketAddr::new(ip, s.port));
    let bound = state.selection.as_ref().is_some_and(|(bound, ip)| {
        bound == &request && Some(*ip) == s.xdmcp_options.selected_endpoint
    });
    if s.xdmcp_options.selected_endpoint != endpoint && !bound {
        s.xdmcp_options.selected_endpoint = None;
    }
    (request, endpoint)
}

pub(super) fn settings(ui: &mut Ui, s: &mut Session, state: &mut State) {
    ui.horizontal_wrapped(|ui| {
        ui.label("Query mode");
        ui.selectable_value(&mut s.xdmcp_options.mode, QueryMode::Direct, "Direct");
        ui.selectable_value(
            &mut s.xdmcp_options.mode,
            QueryMode::Indirect,
            "Indirect chooser",
        );
        ui.selectable_value(
            &mut s.xdmcp_options.mode,
            QueryMode::Broadcast,
            "Broadcast discovery",
        );
    });
    let (request, endpoint) = sync_editor(s, state);
    if s.xdmcp_options.mode == QueryMode::Broadcast {
        if state.interface.is_empty() {
            if let Some(network) = s.xdmcp_options.discovery_network {
                state.interface = network.interface.to_string();
                state.prefix = network.prefix;
            } else {
                state.prefix = 24;
            }
        }
        ui.horizontal_wrapped(|ui| {
            ui.label("Local interface IPv4");
            ui.add(crate::ui::fluent::singleline(&mut state.interface));
            ui.label("Prefix");
            ui.add(DragValue::new(&mut state.prefix).range(16..=30));
        });
        s.xdmcp_options.discovery_network =
            state
                .interface
                .parse()
                .ok()
                .map(|interface| BroadcastNetwork {
                    interface,
                    prefix: state.prefix,
                });
        ui.small("Broadcast sends at most three discovery packets on the network you choose, only when you click Discover. Advertisements are unauthenticated.");
    }
    ui.horizontal_wrapped(|ui| {
        if state.pending.is_some() || state.resolving.is_some() {
            ui.spinner();
            if ui.button("Cancel discovery / resolution").clicked() {
                state.resolving = None;
                state.pending = None;
                state.endpoints.clear();
            }
        } else if ui.button("Discover endpoints").clicked() {
            if endpoint.is_none() && request.mode != QueryMode::Broadcast {
                state.start_resolution(request.clone(), true);
            } else {
                state.clear_request_results();
                let target = if s.xdmcp_options.mode == QueryMode::Broadcast {
                    Some(std::net::SocketAddr::new(
                        std::net::Ipv4Addr::BROADCAST.into(),
                        s.port,
                    ))
                } else {
                    endpoint
                };
                match target
                    .ok_or_else(|| "Enter an XDMCP hostname or numeric address".to_owned())
                    .and_then(|endpoint| {
                        Discovery::start(DiscoveryRequest {
                            endpoint,
                            mode: s.xdmcp_options.mode,
                            network: s.xdmcp_options.discovery_network,
                        })
                    }) {
                    Ok(pending) => state.pending = Some(pending),
                    Err(error) => state.error = error,
                }
            }
        }
        if endpoint.is_none()
            && request.mode != QueryMode::Broadcast
            && state.resolving.is_none()
            && state.pending.is_none()
            && ui.button("Resolve hostname").clicked()
        {
            state.start_resolution(request.clone(), false);
        }
        if endpoint.is_some()
            && s.xdmcp_options.mode != QueryMode::Broadcast
            && ui.button("Use entered endpoint").clicked()
        {
            s.xdmcp_options.selected_endpoint = endpoint;
        }
    });
    if state.pending.is_some() || state.resolving.is_some() {
        ui.ctx().request_repaint_after(Duration::from_millis(40));
    }
    for address in &state.resolved {
        if ui
            .selectable_label(
                s.xdmcp_options.selected_endpoint == Some(*address),
                format!("Use resolved IPv4 endpoint {address}"),
            )
            .clicked()
        {
            s.xdmcp_options.selected_endpoint = Some(*address);
            state.selection = Some((request.clone(), *address));
        }
    }
    if endpoint.is_none() {
        ui.small("Resolution uses only IPv4 candidates and takes at most 2 seconds. Connect resolves the hostname again; DNS is not authentication.");
    }
    for endpoint in &state.endpoints {
        if ui
            .selectable_label(
                s.xdmcp_options.selected_endpoint == Some(endpoint.address),
                format!(
                    "{} — {} — {}",
                    endpoint.address, endpoint.hostname, endpoint.status
                ),
            )
            .clicked()
        {
            if request.mode == QueryMode::Broadcast {
                s.host = endpoint.address.ip().to_string();
                s.port = endpoint.address.port();
            } else {
                state.selection = Some((request.clone(), endpoint.address));
            }
            s.xdmcp_options.selected_endpoint = Some(endpoint.address);
            if s.xdmcp_options.mode == QueryMode::Broadcast {
                s.xdmcp_options.mode = QueryMode::Direct;
            }
        }
    }
    if !state.error.is_empty() {
        crate::ui::errors::show(ui, crate::ui::errors::Domain::X11, &state.error);
    }
    ui.small(format!(
        "Selected endpoint: {}",
        s.xdmcp_options
            .selected_endpoint
            .map(|v| v.to_string())
            .unwrap_or_else(|| "None".into())
    ));
    ui.horizontal_wrapped(|ui| {
        let mut automatic = s.xdmcp_options.display.is_none();
        if ui
            .checkbox(&mut automatic, "Automatic free display")
            .changed()
        {
            s.xdmcp_options.display = if automatic { None } else { Some(100) }
        }
        if let Some(display) = &mut s.xdmcp_options.display {
            ui.add(DragValue::new(display).range(1..=59535));
        }
    });
    let native_macos = cfg!(all(target_os = "macos", target_arch = "aarch64"));
    if native_macos {
        ui.small("Private XQuartz shows native windows using host geometry and keyboard. Full screen, monitor selection, layout overrides and host clipboard sharing are unavailable.");
        let options = &mut s.xdmcp_options;
        if (options.fullscreen
            || options.monitor.is_some()
            || !options.keyboard.is_empty()
            || options.clipboard != ClipboardPolicy::Disabled
            || options.clipboard_display.is_some()
            || options.clipboard_authority.is_some())
            && ui.button("Clear unsupported XDMCP options").clicked()
        {
            options.fullscreen = false;
            options.monitor = None;
            options.keyboard.clear();
            options.clipboard = ClipboardPolicy::Disabled;
            options.clipboard_display = None;
            options.clipboard_authority = None;
        }
    } else {
        ui.horizontal_wrapped(|ui| {
            ui.label("Desktop size");
            ui.add(DragValue::new(&mut s.width).range(200..=8192));
            ui.label("×");
            ui.add(DragValue::new(&mut s.height).range(200..=8192));
            ui.checkbox(&mut s.xdmcp_options.fullscreen, "Full screen");
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Monitor output");
            let mut monitor = s.xdmcp_options.monitor.clone().unwrap_or_default();
            let monitor_hint = if cfg!(all(windows, target_arch = "x86_64")) {
                "Default; e.g. 1"
            } else {
                "Default; e.g. HDMI-1"
            };
            if ui
                .add(crate::ui::fluent::singleline(&mut monitor).hint_text(monitor_hint))
                .changed()
            {
                s.xdmcp_options.monitor = if monitor.is_empty() {
                    None
                } else {
                    Some(monitor)
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
        ui.label("Session keyboard");
        let keyboard = &mut s.xdmcp_options.keyboard;
        ui.add(crate::ui::fluent::singleline(keyboard).hint_text("Host keyboard").desired_width(120.0))
            .fluent_hint("Enter an installed XKB layout ID, such as us, de, or fr. Leave empty to inherit the host keyboard.");
        if !keyboard.is_empty() && ui.button("Use host keyboard").clicked() {
            keyboard.clear();
        }
    });
        ui.horizontal_wrapped(|ui| {
            let windows = cfg!(all(windows, target_arch = "x86_64"));
            ui.label(if windows {
                "Windows text clipboard"
            } else {
                "Parent X11 text clipboard"
            });
            ui.selectable_value(
                &mut s.xdmcp_options.clipboard,
                ClipboardPolicy::Disabled,
                "Disabled",
            );
            #[cfg(target_os = "linux")]
            ui.selectable_value(
                &mut s.xdmcp_options.clipboard,
                ClipboardPolicy::Bidirectional,
                "Bidirectional text",
            );
            #[cfg(all(windows, target_arch = "x86_64"))]
            if ui
                .selectable_value(
                    &mut s.xdmcp_options.clipboard,
                    ClipboardPolicy::WindowsBidirectional,
                    "Bidirectional text",
                )
                .changed()
            {
                s.xdmcp_options.clipboard_display = None;
                s.xdmcp_options.clipboard_authority = None;
            }
            #[cfg(not(any(target_os = "linux", all(windows, target_arch = "x86_64"))))]
            ui.add_enabled(
                false,
                egui::Button::new("Bidirectional (unavailable for this engine)"),
            );
        });
        #[cfg(all(windows, target_arch = "x86_64"))]
        if s.xdmcp_options.clipboard == ClipboardPolicy::WindowsBidirectional {
            ui.small("Shares Windows clipboard text in both directions with this XDMCP desktop.");
        }
        #[cfg(target_os = "linux")]
        if s.xdmcp_options.clipboard == ClipboardPolicy::Bidirectional {
            ui.horizontal_wrapped(|ui| {
                ui.label("Selected local parent X11 display");
                ui.text_edit_singleline(
                    s.xdmcp_options
                        .clipboard_display
                        .get_or_insert_with(String::new),
                );
            });
            let mut authority = s
                .xdmcp_options
                .clipboard_authority
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            ui.horizontal_wrapped(|ui| {
                ui.label("Parent Xauthority (blank: inherited)");
                if ui
                    .add(crate::ui::fluent::singleline(&mut authority))
                    .changed()
                {
                    s.xdmcp_options.clipboard_authority = if authority.is_empty() {
                        None
                    } else {
                        Some(authority.into())
                    };
                }
            });
            ui.small("UTF-8 text, up to 1 MiB. Shares this X11 CLIPBOARD; Wayland compositor clipboard sharing depends on the compositor.");
        }
    }
    if state.advertise.is_empty() {
        if let Some(ip) = s.xdmcp_options.advertise_address {
            state.advertise = ip.to_string();
        }
    }
    ui.horizontal_wrapped(|ui| {
        ui.label("Advertised local IPv4");
        ui.add(
            crate::ui::fluent::singleline(&mut state.advertise).hint_text(if native_macos {
                "Automatic route address"
            } else {
                "Reachable non-loopback local IPv4"
            }),
        );
    });
    s.xdmcp_options.advertise_address = state.advertise.parse().ok();
    if !state.advertise.is_empty() && s.xdmcp_options.advertise_address.is_none() {
        crate::ui::errors::show(
            ui,
            crate::ui::errors::Domain::X11,
            "Enter a valid local IPv4 address",
        );
    }
    if cfg!(all(windows, target_arch = "x86_64")) {
        ui.checkbox(
            &mut s.xdmcp_options.allow_network_listener,
            "Allow unencrypted XDMCP/X11 and this session's TCP listener on the selected address",
        );
        ui.small("Use a trusted isolated network. The private Windows engine binds only the selected local IPv4 address and requires X11 authentication. The engine needs an approved native package.");
    } else {
        ui.checkbox(
            &mut s.xdmcp_options.allow_network_listener,
            if native_macos {
                "Allow unencrypted XDMCP/X11 and this session's TCP listener on one local IPv4 address"
            } else {
                "Allow unencrypted XDMCP/X11 and this session's TCP listener on all interfaces"
            },
        );
        if native_macos {
            ui.small("Use a trusted isolated network. XDMCP does not authenticate the manager. The listener requires the negotiated X11 cookie and is bound to the advertised address. Indirect mode can accept a forwarded manager. Requires the packaged Apple Silicon XQuartz runtime.");
        } else {
            ui.small("XDMCP and X11 are unencrypted. Use a trusted isolated network. The advertised address does not restrict the TCP listener. X11 access control remains enabled. Linux requires the pinned owned Xephyr payload and a host X11 display. Other platforms have no automatic engine fallback.");
        }
    }
}
pub(super) fn start(session: &Session) -> Result<ManagedXdmcp, String> {
    if session.host.parse::<std::net::IpAddr>().is_err() {
        return ManagedXdmcp::start_hostname(
            XServerManager::default(),
            session.xdmcp_options.clone(),
            session.host.clone(),
            session.port,
            session.width,
            session.height,
        );
    }
    let endpoint = session
        .xdmcp_options
        .selected_endpoint
        .ok_or("Choose an XDMCP endpoint in the session settings")?;
    if endpoint.port() != session.port
        || session.host.parse::<std::net::IpAddr>().ok() != Some(endpoint.ip())
    {
        return Err(
            "The selected XDMCP endpoint no longer matches this session. Select it again".into(),
        );
    }
    ManagedXdmcp::start(
        XServerManager::default(),
        session.xdmcp_options.clone(),
        session.width,
        session.height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn render(session: &mut Session, state: &mut State) {
        let ctx = Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| settings(ui, session, state));
        output.textures_delta.clear();
    }
    fn input(events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1400., 1000.))),
            events,
            ..Default::default()
        }
    }

    fn test_frame(
        ctx: &Context,
        input: egui::RawInput,
        run: impl FnMut(&mut Ui),
    ) -> egui::FullOutput {
        let mut output = ctx.run_ui(input, run);
        output.textures_delta.clear();
        output
    }

    fn text_pos(shapes: &[egui::epaint::ClippedShape], label: &str) -> Pos2 {
        fn scan(shape: &egui::epaint::Shape, label: &str) -> Option<Pos2> {
            match shape {
                egui::epaint::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.pos + text.galley.rect.center().to_vec2())
                }
                egui::epaint::Shape::Vec(items) => items.iter().find_map(|s| scan(s, label)),
                _ => None,
            }
        }
        shapes
            .iter()
            .find_map(|s| scan(&s.shape, label))
            .unwrap_or_else(|| panic!("Missing UI label: {label}"))
    }

    fn pointer(pos: Pos2, pressed: bool) -> egui::RawInput {
        input(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ])
    }

    fn hostname_session() -> Session {
        Session {
            protocol: "XDMCP".into(),
            host: "localhost".into(),
            port: 1177,
            ..Default::default()
        }
    }

    fn seed_candidates(session: &mut Session, state: &mut State, address: std::net::SocketAddr) {
        state.resolved = vec![address];
        state.endpoints = vec![DiscoveredEndpoint {
            address,
            hostname: "prior host".into(),
            status: "prior status".into(),
        }];
        state.selection = Some((
            HostRequest {
                host: session.host.clone(),
                port: session.port,
                mode: session.xdmcp_options.mode,
                generation: state.generation,
            },
            address,
        ));
        session.xdmcp_options.selected_endpoint = Some(address);
    }

    #[test]
    fn fix946_repeating_resolution_hides_both_old_lists_on_failure() {
        let ctx = Context::default();
        let mut session = hostname_session();
        // Deterministic synchronous resolver validation failure, with prior
        // results seeded to isolate the new-request action from input editing.
        session.port = 0;
        let mut state = State::default();
        let _ = test_frame(&ctx, input(vec![]), |ui| {
            settings(ui, &mut session, &mut state)
        });
        seed_candidates(&mut session, &mut state, "127.0.0.1:1177".parse().unwrap());
        let output = test_frame(&ctx, input(vec![]), |ui| {
            settings(ui, &mut session, &mut state)
        });
        let resolve = text_pos(&output.shapes, "Resolve hostname");
        for pressed in [true, false] {
            let _ = test_frame(&ctx, pointer(resolve, pressed), |ui| {
                settings(ui, &mut session, &mut state)
            });
        }
        assert!(state.error.contains("nonzero port"), "{}", state.error);
        assert!(state.resolved.is_empty(), "old DNS rows remain selectable");
        assert!(
            state.endpoints.is_empty(),
            "old discovery rows remain selectable"
        );
    }

    #[test]
    fn fix946_window_close_and_reopen_same_profile_discards_old_editor() {
        let ctx = Context::default();
        let mut w = Workspace::new_context(
            &ctx,
            None,
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/gui-test"),
        );
        let session = hostname_session();
        let address = "127.0.0.1:1177".parse().unwrap();
        w.session_form = Some(session.clone());
        let frame = |w: &mut Workspace, input| test_frame(&ctx, input, |_| w.session_dialog(&ctx));
        let _ = frame(&mut w, input(vec![]));
        seed_candidates(
            w.session_form.as_mut().unwrap(),
            &mut w.xdmcp_discovery,
            address,
        );
        let generation = w.xdmcp_discovery.generation;
        let _ = frame(&mut w, input(vec![]));
        let _ = frame(
            &mut w,
            input(vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]),
        );
        assert!(w.session_form.is_none(), "Escape did not cancel the editor");
        let mut original = session;
        original.xdmcp_options.selected_endpoint = Some(address);
        w.session_form = Some(original);
        let _ = frame(&mut w, input(vec![]));
        assert!(
            w.xdmcp_discovery.resolved.is_empty(),
            "same profile reused old DNS rows"
        );
        assert!(
            w.xdmcp_discovery.endpoints.is_empty(),
            "same profile reused old discovered rows"
        );
        assert!(
            w.xdmcp_discovery.selection.is_none(),
            "same profile reused old selection binding"
        );
        assert!(
            w.session_form
                .unwrap()
                .xdmcp_options
                .selected_endpoint
                .is_none()
        );
        assert_ne!(w.xdmcp_discovery.generation, generation);
    }

    #[test]
    fn ready_resolution_cannot_start_discovery_in_the_window_close_frame() {
        let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        socket
            .set_read_timeout(Some(Duration::from_millis(150)))
            .unwrap();
        let addr = socket.local_addr().unwrap();
        let ctx = Context::default();
        let mut w = Workspace::new_context(
            &ctx,
            None,
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/gui-test"),
        );
        let session = Session {
            host: addr.ip().to_string(),
            port: addr.port(),
            ..hostname_session()
        };
        w.open_session_form(session.clone());
        let frame = |w: &mut Workspace, input| test_frame(&ctx, input, |_| w.session_dialog(&ctx));
        let _ = frame(&mut w, input(vec![]));
        let output = frame(&mut w, input(vec![]));
        let close = text_pos(&output.shapes, "Cancel");
        let _ = frame(&mut w, pointer(close, true));
        // A numeric answer is ready synchronously, making the DNS-to-UDP
        // handoff deterministic without leaving a system resolver behind.
        let queue_ready = |w: &mut Workspace| {
            let request = w.xdmcp_discovery.request(&session);
            w.xdmcp_discovery.resolving = Some((Resolution::start(request).unwrap(), true));
        };
        queue_ready(&mut w);
        let _ = frame(&mut w, pointer(close, false));
        assert!(w.session_form.is_none());
        assert!(w.xdmcp_discovery.pending.is_none());
        assert!(w.xdmcp_discovery.resolving.is_none());

        w.open_session_form(session.clone());
        let _ = frame(&mut w, input(vec![]));
        assert!(w.xdmcp_discovery.pending.is_none());
        assert!(w.xdmcp_discovery.resolved.is_empty());
        let mut packet = [0; 128];
        let error = socket.recv_from(&mut packet).unwrap_err();
        assert!(matches!(
            error.kind(),
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
        ));

        // Positive control on the same socket and same profile. A retained
        // frame must consume its own ready answer and send an explicit query.
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        queue_ready(&mut w);
        let _ = frame(&mut w, input(vec![]));
        assert!(w.xdmcp_discovery.pending.is_some());
        let (n, _) = socket.recv_from(&mut packet).unwrap();
        assert_eq!(&packet[..n], &[0, 1, 0, 2, 0, 1, 0]);
        w.xdmcp_discovery.end_editor();
    }

    #[test]
    fn switching_editor_sections_retains_only_current_xdmcp_candidates() {
        use crate::ui::pages::SessionSection::{Advanced, Bookmark, Connection};
        let (ctx, _root, mut w) = crate::ui::terminal_review_tests::owned_workspace();
        w.open_session_form(hostname_session());
        let render =
            |w: &mut Workspace| test_frame(&ctx, input(vec![]), |_| w.session_dialog(&ctx));
        render(&mut w);
        let endpoint = "127.0.0.1:1177".parse().unwrap();
        seed_candidates(
            w.session_form.as_mut().unwrap(),
            &mut w.xdmcp_discovery,
            endpoint,
        );
        let generation = w.xdmcp_discovery.generation;
        for section in [Advanced, Bookmark, Connection] {
            w.session_section = section;
            render(&mut w);
            assert_eq!(w.xdmcp_discovery.generation, generation);
            assert_eq!(
                w.session_form
                    .as_ref()
                    .unwrap()
                    .xdmcp_options
                    .selected_endpoint,
                Some(endpoint)
            );
            assert!(w.xdmcp_discovery.pending.is_none() && w.xdmcp_discovery.resolving.is_none());
        }
        w.session_form.as_mut().unwrap().host = "different.invalid".into();
        render(&mut w);
        assert!(w.xdmcp_discovery.resolved.is_empty() && w.xdmcp_discovery.selection.is_none());
    }

    #[test]
    fn protocol_exit_and_explicit_replacement_end_the_current_editor() {
        let ctx = Context::default();
        let mut w = Workspace::new_context(
            &ctx,
            None,
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/gui-test"),
        );
        let mut original = hostname_session();
        let address = "127.0.0.1:1177".parse().unwrap();
        original.xdmcp_options.selected_endpoint = Some(address);
        w.open_session_form(original.clone());
        let frame = |w: &mut Workspace, input| test_frame(&ctx, input, |_| w.session_dialog(&ctx));
        let _ = frame(&mut w, input(vec![]));
        seed_candidates(
            w.session_form.as_mut().unwrap(),
            &mut w.xdmcp_discovery,
            address,
        );
        let generation = w.xdmcp_discovery.generation;
        w.open_session_form(original);
        let output = frame(&mut w, input(vec![]));
        assert_ne!(w.xdmcp_discovery.generation, generation);
        assert!(w.xdmcp_discovery.resolved.is_empty());
        assert!(
            w.session_form
                .as_ref()
                .unwrap()
                .xdmcp_options
                .selected_endpoint
                .is_none()
        );

        seed_candidates(
            w.session_form.as_mut().unwrap(),
            &mut w.xdmcp_discovery,
            address,
        );
        let ssh = text_pos(&output.shapes, "SSH");
        let _ = frame(&mut w, pointer(ssh, true));
        let _ = frame(&mut w, pointer(ssh, false));
        assert_eq!(w.session_form.as_ref().unwrap().protocol, "SSH");
        assert!(w.xdmcp_discovery.session_id.is_none());
        assert!(w.xdmcp_discovery.resolved.is_empty());
        assert!(w.xdmcp_discovery.selection.is_none());
    }

    #[test]
    fn hostname_editor_never_resolves_and_retains_original_name() {
        let mut session = Session::default();
        session.host = "localhost".into();
        session.port = 1177;
        session.xdmcp_options.selected_endpoint = Some("127.0.0.1:1177".parse().unwrap());
        let mut state = State::default();
        render(&mut session, &mut state);
        assert!(state.resolving.is_none());
        assert!(state.resolved.is_empty());
        assert_eq!(session.host, "localhost");
        assert!(session.xdmcp_options.selected_endpoint.is_none());
    }
    #[test]
    fn hostname_selection_is_bound_to_exact_host_port_mode_and_editor_generation() {
        let mut session = Session::default();
        session.host = "localhost".into();
        session.port = 1177;
        let mut state = State::default();
        render(&mut session, &mut state);
        let selected = "127.0.0.1:1177".parse().unwrap();
        state.selection = Some((
            HostRequest {
                host: session.host.clone(),
                port: session.port,
                mode: session.xdmcp_options.mode,
                generation: state.generation,
            },
            selected,
        ));
        session.xdmcp_options.selected_endpoint = Some(selected);
        render(&mut session, &mut state);
        assert_eq!(session.host, "localhost");
        assert_eq!(session.xdmcp_options.selected_endpoint, Some(selected));
        session.xdmcp_options.mode = QueryMode::Indirect;
        render(&mut session, &mut state);
        assert!(session.xdmcp_options.selected_endpoint.is_none());
        assert!(state.selection.is_none());
        session.xdmcp_options.selected_endpoint = Some(selected);
        session.host = "LOCALHOST".into();
        render(&mut session, &mut state);
        assert!(session.xdmcp_options.selected_endpoint.is_none());
        session.xdmcp_options.selected_endpoint = Some(selected);
        session.port = 1178;
        render(&mut session, &mut state);
        assert!(session.xdmcp_options.selected_endpoint.is_none());
    }
    #[test]
    fn opening_broadcast_settings_never_starts_discovery() {
        let mut session = Session::default();
        session.protocol = "XDMCP".into();
        session.xdmcp_options.mode = QueryMode::Broadcast;
        session.xdmcp_options.discovery_network = Some(BroadcastNetwork {
            interface: "192.0.2.1".parse().unwrap(),
            prefix: 30,
        });
        let mut state = State::default();
        render(&mut session, &mut state);
        assert!(state.pending.is_none());
        assert!(state.endpoints.is_empty());
        assert!(!session.xdmcp_options.allow_network_listener);
    }
    #[test]
    fn editor_invalidates_stale_selection_and_does_not_carry_network_choices_between_profiles() {
        let mut session = Session::default();
        session.host = "192.0.2.3".into();
        session.port = 1177;
        session.xdmcp_options.selected_endpoint = Some("192.0.2.3:1177".parse().unwrap());
        session.xdmcp_options.advertise_address = Some("192.0.2.1".parse().unwrap());
        let mut state = State::default();
        render(&mut session, &mut state);
        assert!(session.xdmcp_options.selected_endpoint.is_some());
        session.port = 1178;
        render(&mut session, &mut state);
        assert!(session.xdmcp_options.selected_endpoint.is_none());
        let mut other = Session::default();
        render(&mut other, &mut state);
        assert!(other.xdmcp_options.advertise_address.is_none());
        assert!(state.advertise.is_empty());
    }
    #[test]
    fn workspace_routes_xdmcp_to_owned_session_and_rejects_stale_endpoint() {
        let mut session = Session::default();
        session.protocol = "XDMCP".into();
        session.host = "192.0.2.3".into();
        session.port = 1178;
        session.xdmcp_options.selected_endpoint = Some("192.0.2.3:1177".parse().unwrap());
        assert!(start(&session).err().unwrap().contains("no longer matches"));
        assert!(
            connection::launch(&session, &Preferences::default())
                .unwrap_err()
                .contains("per-session owned engine")
        );
    }
}
