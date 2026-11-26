use super::*;
use harbor_workspace::x11::{
    XCapabilities, XClientDisplay, XDisplayMode, XDisplaySource, XServerManager, XServerOptions,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{SyncSender, TryRecvError},
};
use std::thread::JoinHandle;

#[derive(Default)]
pub(super) struct State {
    options: XServerOptions,
    context: Option<Context>,
    bundle_root: Option<PathBuf>,
    probe_executable: Option<PathBuf>,
    worker: Option<Worker>,
    display: Option<XClientDisplay>,
    pending: Vec<u64>,
    error: Option<String>,
    capabilities: Option<Result<XCapabilities, String>>,
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    clipboard_pending: bool,
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    clipboard_status: Option<String>,
}

enum Event {
    Capabilities(Result<XCapabilities, String>),
    Ready(XClientDisplay),
    Ended(Result<(), String>),
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    Clipboard(Result<Option<String>, String>),
}

enum Request {
    Stop,
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    Clipboard(ClipboardDirection),
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
enum ClipboardDirection {
    ToX11,
    ToMac,
}

struct Worker {
    events: Receiver<Event>,
    requests: SyncSender<Request>,
    cancelled: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    fn request_stop(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
        let _ = self.requests.try_send(Request::Stop);
    }

    fn stopping(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.request_stop();
    }
}

impl State {
    pub(super) fn for_context(ctx: &Context) -> Self {
        Self {
            context: Some(ctx.clone()),
            ..Self::default()
        }
    }

    fn install_manager(&self) -> XServerManager {
        match &self.bundle_root {
            Some(root) => XServerManager::for_installation(root.clone()),
            None => XServerManager::default(),
        }
    }

    fn start(&mut self, ctx: &Context) {
        if self.worker.is_some() {
            return;
        }
        let manager = self.install_manager();
        let probe_executable = self.probe_executable.clone();
        let options = self.options.clone();
        let (sender, events) = mpsc::sync_channel(3);
        let (requests, requested) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let ctx = ctx.clone();
        self.error = None;
        self.capabilities = None;
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            self.clipboard_pending = false;
            self.clipboard_status = None;
        }
        match std::thread::Builder::new()
            .name("termcraft-xserver".into())
            .spawn(move || {
                let report = |event| {
                    let delivered = sender.send(event).is_ok();
                    ctx.request_repaint();
                    delivered
                };
                let result = manager
                    .start_cancellable(options, &worker_cancelled)
                    .and_then(|mut lease| {
                        // Probe before clients are released: an active RandR check may
                        // briefly switch the newly owned display and then restore it.
                        let capabilities = match &probe_executable {
                            Some(executable) => XCapabilities::probe_with_executable(
                                &lease.client_display(),
                                executable,
                                &worker_cancelled,
                            ),
                            None => {
                                XCapabilities::probe(&lease.client_display(), &worker_cancelled)
                            }
                        };
                        // A crashed or timed-out driver may have interrupted a mode
                        // transition. Discard this fresh display before exposing clients.
                        if let Err(error) = &capabilities {
                            let error = error.clone();
                            report(Event::Capabilities(capabilities));
                            lease.stop_owned()?;
                            return Err(error);
                        }
                        if !report(Event::Capabilities(capabilities)) {
                            return lease.stop_owned();
                        }
                        if worker_cancelled.load(Ordering::Relaxed)
                            || !report(Event::Ready(lease.client_display()))
                        {
                            return lease.stop_owned();
                        }
                        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
                        let mut clipboard: Option<
                            harbor_workspace::x11::MacosClipboard,
                        > = None;
                        loop {
                            use mpsc::RecvTimeoutError;

                            if worker_cancelled.load(Ordering::Relaxed) {
                                return lease.stop_owned();
                            }
                            let interval = Duration::from_millis(250);
                            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
                            let interval = if clipboard.is_some() {
                                Duration::from_millis(25)
                            } else {
                                interval
                            };
                            match requested.recv_timeout(interval) {
                                Ok(Request::Stop) | Err(RecvTimeoutError::Disconnected) => {
                                    return lease.stop_owned();
                                }
                                #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
                                Ok(Request::Clipboard(direction)) => {
                                    let result = transfer_clipboard(
                                        &mut clipboard,
                                        direction,
                                        &lease,
                                        &worker_cancelled,
                                    );
                                    if !worker_cancelled.load(Ordering::Relaxed) {
                                        let _ = sender.try_send(Event::Clipboard(result));
                                        ctx.request_repaint();
                                    }
                                }
                                Err(RecvTimeoutError::Timeout) => {
                                    if !lease.is_running()? {
                                        return Err("The packaged X server exited".into());
                                    }
                                    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
                                    if let Some(bridge) = &mut clipboard
                                        && bridge.poll().is_err()
                                    {
                                        clipboard = None;
                                    }
                                }
                            }
                        }
                    });
                report(Event::Ended(result));
            }) {
            Ok(thread) => {
                self.worker = Some(Worker {
                    events,
                    requests,
                    cancelled,
                    thread: Some(thread),
                });
            }
            Err(error) => {
                self.error = Some(format!("Could not start the X server worker: {error}"))
            }
        }
    }

    pub(super) fn poll(&mut self) {
        while let Some(worker) = &self.worker {
            match worker.events.try_recv() {
                Ok(Event::Capabilities(report)) => self.capabilities = Some(report),
                Ok(Event::Ready(display)) => self.display = Some(display),
                #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
                Ok(Event::Clipboard(result)) => {
                    self.clipboard_pending = false;
                    if !worker.stopping() {
                        self.clipboard_status = Some(match result {
                            Ok(Some(text)) => {
                                if let Some(ctx) = &self.context {
                                    ctx.copy_text(text);
                                }
                                "Copied X11 text to macOS.".into()
                            }
                            Ok(None) => "Copied macOS text to X11.".into(),
                            Err(error) => error,
                        });
                    }
                }
                Ok(Event::Ended(result)) => {
                    self.error = if worker.stopping() {
                        let cancelled = (!self.pending.is_empty())
                            .then(|| "TermCraft X server startup was cancelled.".into());
                        if self.display.is_some() {
                            result.err().or(cancelled)
                        } else {
                            cancelled
                        }
                    } else {
                        result.err()
                    };
                    self.display = None;
                    self.worker = None;
                    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
                    {
                        self.clipboard_pending = false;
                        self.clipboard_status = None;
                    }
                }
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.error = Some("The X server worker exited unexpectedly".into());
                    self.display = None;
                    self.worker = None;
                }
            }
        }
    }

    pub(super) fn request_session(&mut self, id: u64) -> Result<(), String> {
        if self.worker.as_ref().is_some_and(Worker::stopping) {
            return Err(
                "The TermCraft X server is stopping. Reconnect when it has stopped.".into(),
            );
        }
        if self.worker.is_none() {
            self.error = None;
        }
        self.pending.push(id);
        if let Some(ctx) = &self.context {
            ctx.request_repaint();
        }
        Ok(())
    }

    fn ready_sessions(
        &mut self,
        ctx: &Context,
    ) -> Option<(Vec<u64>, Result<XClientDisplay, String>)> {
        self.poll();
        if self.pending.is_empty() {
            return None;
        }

        let display = if self.worker.as_ref().is_some_and(Worker::stopping) {
            Err("TermCraft X server startup was cancelled.".into())
        } else if let Some(display) = self.display.clone() {
            Ok(display)
        } else if let Some(error) = &self.error {
            Err(error.clone())
        } else if self.worker.is_none() {
            self.start(ctx);
            if let Some(error) = &self.error {
                Err(error.clone())
            } else {
                return None;
            }
        } else {
            return None;
        };
        Some((std::mem::take(&mut self.pending), display))
    }

    pub(super) fn shutdown(&mut self) {
        if let Some(mut worker) = self.worker.take() {
            worker.request_stop();
            // The app has stopped rendering. Wait for the worker to reap its child
            // before leaving the process so the X server cannot be orphaned.
            if let Some(thread) = worker.thread.take() {
                let _ = thread.join();
            }
        }
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn transfer_clipboard(
    clipboard: &mut Option<harbor_workspace::x11::MacosClipboard>,
    direction: ClipboardDirection,
    lease: &harbor_workspace::x11::XServerLease,
    cancelled: &Arc<AtomicBool>,
) -> Result<Option<String>, String> {
    use harbor_workspace::x11::MacosClipboard;
    if cancelled.load(Ordering::Relaxed) {
        return Err("The private X display is stopping.".into());
    }
    if clipboard.is_none() {
        *clipboard = Some(MacosClipboard::open(
            lease.client_display(),
            cancelled.clone(),
        )?);
    }
    let clipboard = clipboard.as_mut().unwrap();
    match direction {
        ClipboardDirection::ToX11 => {
            let text = super::clipboard::read_os_text()?;
            clipboard.write_text(text)?;
            Ok(None)
        }
        ClipboardDirection::ToMac => clipboard.read_text().map(Some),
    }
}

pub(super) fn session_settings(ui: &mut Ui, session: &mut Session) {
    let local = session.protocol == "Local";
    ui.label("X11");
    ui.checkbox(
        &mut session.x11,
        if local {
            "Select an X display"
        } else {
            "Trusted X11 forwarding"
        },
    );
    ui.end_row();
    if session.protocol == "SSH" {
        use harbor_workspace::x11::remote_environment::{DesktopPreset, RemoteEnvironment};
        ui.label("Remote desktop");
        let mut mode = match session.remote_environment {
            RemoteEnvironment::None => 0,
            RemoteEnvironment::Preset(_) => 1,
            RemoteEnvironment::Custom(_) => 2,
        };
        let before = mode;
        ui.horizontal(|ui| {
            ui.selectable_value(&mut mode, 0, "None");
            ui.selectable_value(&mut mode, 1, "Desktop preset");
            ui.selectable_value(&mut mode, 2, "Custom command");
        });
        if mode != before {
            session.remote_environment = match mode {
                1 => RemoteEnvironment::Preset(DesktopPreset::Xfce),
                2 => RemoteEnvironment::Custom(String::new()),
                _ => RemoteEnvironment::None,
            };
        }
        ui.end_row();
        match &mut session.remote_environment {
            RemoteEnvironment::Preset(preset) => {
                ui.label("Desktop environment");
                egui::ComboBox::from_id_salt("remote-desktop-preset")
                    .selected_text(preset.label())
                    .show_ui(ui, |ui| {
                        for value in DesktopPreset::ALL {
                            ui.selectable_value(preset, value, value.label());
                        }
                    });
                ui.end_row();
                ui.label("Requires dbus-run-session and the selected desktop installed on the remote host. GNOME requires an X11-capable installation.");
                ui.end_row();
            }
            RemoteEnvironment::Custom(command) => {
                ui.label("Remote command");
                ui.text_edit_singleline(command);
                ui.end_row();
                ui.label("Runs once per connection in a separate SSH channel. Enter a full desktop startup command; a single X application is not a full desktop.");
                ui.end_row();
            }
            RemoteEnvironment::None => {}
        }
        if !session.remote_environment.is_none() {
            ui.label("Requires Embedded SSH and trusted X11 forwarding. Desktop compatibility depends on the remote host and selected display.");
            ui.end_row();
        }
    }
    if !session.x11 {
        return;
    }
    if !local {
        ui.label("Trusted programs share access to the selected X display.");
        ui.end_row();
    }
    ui.label("X display");
    ui.horizontal_wrapped(|ui| {
        let owned_supported =
            owned_x11_supported() && (local || session.ssh_engine == SshEngine::Embedded);
        ui.add_enabled_ui(owned_supported, |ui| {
            ui.selectable_value(&mut session.x11_source, XDisplaySource::Owned, "TermCraft");
        });
        ui.selectable_value(&mut session.x11_source, XDisplaySource::System, "System");
    });
    ui.end_row();
    if session.x11_source == XDisplaySource::Owned {
        ui.label(if !local && session.ssh_engine != SshEngine::Embedded {
            "The TermCraft display requires Embedded SSH."
        } else {
            "Starts the private display on demand."
        });
        ui.end_row();
    }
}

fn owned_x11_supported() -> bool {
    cfg!(any(
        all(
            target_os = "linux",
            target_arch = "x86_64",
            target_env = "gnu"
        ),
        all(target_os = "macos", target_arch = "aarch64")
    ))
}

impl Workspace {
    pub(super) fn poll_xserver_sessions(&mut self, ctx: &Context) {
        self.xserver.pending.retain(|id| {
            self.tabs
                .iter()
                .any(|tab| tab.id == *id && tab.waiting_for_x11)
        });
        for tab in &mut self.tabs {
            if !tab.waiting_for_x11 || self.xserver.pending.contains(&tab.id) {
                continue;
            }
            let Some(ssh) = &mut tab.ssh else {
                continue;
            };
            if let Err(error) = self.xserver.request_session(tab.id) {
                tab.waiting_for_x11 = false;
                ssh.supply_x_display(Err(error));
            }
        }
        let Some((sessions, display)) = self.xserver.ready_sessions(ctx) else {
            return;
        };
        for id in sessions {
            let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
                continue;
            };
            let result = display.as_ref().map_err(|error| {
                format!("X11 startup failed: {error} Choose System display in the session settings to use your desktop's X server.")
            });
            let session = self.tabs[index].session.clone();
            let tab = &mut self.tabs[index];
            tab.waiting_for_x11 = false;
            if let Some(ssh) = &mut tab.ssh {
                ssh.supply_x_display(result.cloned());
                continue;
            }
            match result {
                Ok(display) => {
                    tab.connection_status = "Connecting…".into();
                    let launch = tab
                        .launch
                        .clone()
                        .map_or_else(|| connection::launch(&session, &self.store.preferences), Ok);
                    match launch.and_then(|launch| {
                        let environment = display.environment()?;
                        let process = Process::spawn_with_environment(
                            &launch.program,
                            &launch.args,
                            launch.directory.as_deref(),
                            tab.screen.cols as u16,
                            tab.screen.rows as u16,
                            &environment,
                        )?;
                        Ok((launch, process))
                    }) {
                        Ok((launch, process)) => {
                            tab.launch = Some(launch);
                            tab.process = Some(process);
                        }
                        Err(error) => tab.error = error,
                    }
                }
                Err(error) => tab.error = error,
            }
            tab.resize_transport();
            if tab.error.is_empty() {
                self.queue_startup_macro(id);
            } else {
                self.message(self.tabs[index].error.clone());
            }
        }
    }

    pub(super) fn default_x11_source() -> XDisplaySource {
        if cfg!(all(
            target_os = "linux",
            target_arch = "x86_64",
            target_env = "gnu"
        )) {
            XDisplaySource::Owned
        } else {
            XDisplaySource::System
        }
    }
    pub(super) fn xserver_card(&mut self, ui: &mut Ui) {
        surface(ui).show(ui, |ui| {
            ui.label(RichText::new("X11 server").size(18.).strong());
            if !owned_x11_supported() {
                ui.label("A managed X11 server is unavailable on this platform.");
                return;
            }
            let description = if cfg!(target_os = "macos") {
                "Open an authenticated private XQuartz display in native windows."
            } else {
                "Open a private X display in your Wayland session. Its clipboard is shared between apps on that display."
            };
            ui.label(RichText::new(description).color(muted(ui)));
            let state = &mut self.xserver;
            if let Some(worker) = &state.worker {
                if worker.stopping() {
                    ui.label(if state.display.is_some() {
                        "Stopping X server…"
                    } else {
                        "Cancelling startup…"
                    });
                    return;
                }
                if let Some(display) = &state.display {
                    ui.label(format!("Running on display {}", display.display()));
                    if let Some(capabilities) = &state.capabilities {
                        match capabilities {
                            Ok(report) => {
                                if report.core_x11_pixels {
                                    ui.label("Core X11 pixel readback verified");
                                }
                                ui.label(format!("GLX: {} · Composite: {} · RandR: {}",
                                    report.glx_version.as_deref().unwrap_or("unavailable"),
                                    report.composite_version.as_deref().unwrap_or("unavailable"),
                                    report.randr_version.as_deref().unwrap_or("unavailable")));
                                if let Some(gl) = &report.opengl {
                                    ui.label(format!("{} · {} · pixels {}/{}", gl.renderer,
                                        if gl.software { "software rendering" } else { "driver rendering (hardware unqualified)" },
                                        gl.matched_pixels, gl.sampled_pixels));
                                } else { ui.label("OpenGL rendering unavailable or unverified"); }
                                ui.label(if report.composite_pixels { "Composite redirect/readback verified" } else { "Composite rendering unverified" });
                                ui.label(format!("Screen {}×{} · resolution change {}", report.dimensions[0], report.dimensions[1], if report.randr_change.is_some() { "verified with event; initial size restored" } else { "unavailable or unverified" }));
                                for error in &report.errors { ui.colored_label(Color32::from_rgb(200, 120, 50), error); }
                            }
                            Err(error) => { ui.label(format!("Graphics capability check failed: {error}")); }
                        }
                    }
                    ui.label(format!(
                        "Authority file {}",
                        display.authority_path().display()
                    ));
                    if ui
                        .button("Copy client environment")
                        .on_hover_text("Paste before an X application command to use this display.")
                        .clicked()
                    {
                        let path = display
                            .authority_path()
                            .to_string_lossy()
                            .replace('\'', "'\"'\"'");
                        ui.ctx().copy_text(format!(
                            "DISPLAY={} XAUTHORITY='{path}'",
                            display.display()
                        ));
                    }
                    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
                    {
                        ui.horizontal_wrapped(|ui| {
                            for (label, direction) in [
                                ("Copy macOS text to X11", ClipboardDirection::ToX11),
                                ("Copy X11 text to macOS", ClipboardDirection::ToMac),
                            ] {
                                if ui.add_enabled(!state.clipboard_pending, egui::Button::new(label))
                                    .on_hover_text("Transfer UTF-8 clipboard text once, up to 64 KiB.")
                                    .clicked()
                                {
                                    state.clipboard_pending = worker.requests
                                        .try_send(Request::Clipboard(direction)).is_ok();
                                    state.clipboard_status = None;
                                }
                            }
                        });
                        if let Some(status) = &state.clipboard_status {
                            ui.small(status);
                        }
                    }
                    if ui.button("Stop X server").clicked() {
                        worker.request_stop();
                    }
                } else {
                    ui.label("Starting X server…");
                    if ui.button("Cancel X server startup").clicked() {
                        worker.request_stop();
                    }
                }
                return;
            }

            if cfg!(target_os = "macos") {
                ui.label("Windowed display. macOS chooses the monitor.");
            } else {
            ui.horizontal(|ui| {
                ui.label("Display mode");
                ComboBox::from_id_salt("xserver_display_mode")
                    .selected_text(match state.options.display_mode {
                        XDisplayMode::Windowed => "Windowed",
                        XDisplayMode::Fullscreen => "Fullscreen",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut state.options.display_mode,
                            XDisplayMode::Windowed,
                            "Windowed",
                        );
                        ui.selectable_value(
                            &mut state.options.display_mode,
                            XDisplayMode::Fullscreen,
                            "Fullscreen",
                        );
                    });
            });
            ui.label(RichText::new("The compositor chooses the monitor.").color(muted(ui)));
            }
            ui.horizontal(|ui| {
                if ui.button("Choose bundle folder").clicked()
                    && let Some(root) = rfd::FileDialog::new().pick_folder()
                {
                    state.bundle_root = Some(root);
                }
                if state.bundle_root.is_some() && ui.button("Use application folder").clicked() {
                    state.bundle_root = None;
                }
            });
            if let Some(root) = &state.bundle_root {
                ui.label(root.display().to_string());
            }
            if primary(ui, "Start X server").clicked() {
                state.start(ui.ctx());
            }
            if let Some(error) = &state.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
        });
    }
}

#[cfg(all(test, target_os = "macos", target_arch = "aarch64"))]
mod macos_tests {
    use super::*;

    #[test]
    fn apple_silicon_workspace_offers_the_private_xserver() {
        let (ctx, _root, mut workspace) = crate::ui::terminal_review_tests::owned_workspace();
        let mut output = ctx.run_ui(RawInput::default(), |ui| workspace.xserver_card(ui));
        output.textures_delta.clear();
        assert!(output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::epaint::Shape::Text(text)
                if text.galley.text() == "Start X server")
        }));
        assert_eq!(Workspace::default_x11_source(), XDisplaySource::System);
    }

    #[test]
    #[ignore = "changes the general pasteboard; run only in the disposable macOS GUI fixture"]
    fn private_xserver_worker_transfers_the_native_pasteboard_on_request() {
        use harbor_workspace::x11::MacosClipboard;
        assert_eq!(
            std::env::var("TERMCRAFT_TEST_DISPOSABLE_PASTEBOARD").as_deref(),
            Ok("1")
        );
        let root = PathBuf::from(std::env::var_os("TERMCRAFT_TEST_XQUARTZ_ROOT").unwrap());
        let (ctx, _root, mut workspace) = crate::ui::terminal_review_tests::owned_workspace();
        let state = &mut workspace.xserver;
        state.probe_executable = Some(root.with_file_name("MacOS").join("harbor-workspace"));
        state.bundle_root = Some(root);
        state.start(&ctx);
        let deadline = Instant::now() + Duration::from_secs(30);
        let display = loop {
            state.poll();
            if let Some(display) = state.display.clone() {
                break display;
            }
            assert!(state.error.is_none(), "{:?}", state.error);
            assert!(Instant::now() < deadline, "Private XQuartz did not start");
            std::thread::sleep(Duration::from_millis(15));
        };
        let capabilities = state.capabilities.as_ref().unwrap().as_ref().unwrap();
        assert!(capabilities.core_x11_pixels);
        let mut peer = MacosClipboard::open(display, Arc::new(AtomicBool::new(false))).unwrap();
        assert!(peer.read_text().unwrap_err().contains("empty"));

        let mac_text = "native Sonoma clipboard café\n✓";
        arboard::Clipboard::new()
            .unwrap()
            .set_text(mac_text)
            .unwrap();
        request(state, ClipboardDirection::ToX11);
        let deadline = Instant::now() + Duration::from_secs(5);
        while state.clipboard_pending {
            state.poll();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(15));
        }
        assert_eq!(
            state.clipboard_status.as_deref(),
            Some("Copied macOS text to X11.")
        );
        assert_eq!(peer.read_text().unwrap(), mac_text);

        let x11_text = "private X11 clipboard 漢字\n🦀";
        peer.write_text(x11_text.into()).unwrap();
        assert_eq!(
            arboard::Clipboard::new().unwrap().get_text().unwrap(),
            mac_text
        );
        request(state, ClipboardDirection::ToMac);
        let deadline = Instant::now() + Duration::from_secs(5);
        let text = loop {
            peer.poll().unwrap();
            let mut output = ctx.run_ui(RawInput::default(), |_| state.poll());
            output.textures_delta.clear();
            if let Some(egui::OutputCommand::CopyText(text)) = output
                .platform_output
                .commands
                .into_iter()
                .find(|command| matches!(command, egui::OutputCommand::CopyText(_)))
            {
                break text;
            }
            assert!(Instant::now() < deadline, "{:?}", state.clipboard_status);
            std::thread::sleep(Duration::from_millis(15));
        };
        assert_eq!(text, x11_text);
        state.shutdown();
        assert!(peer.read_text().is_err());
    }

    fn request(state: &mut State, direction: ClipboardDirection) {
        assert!(!state.clipboard_pending);
        assert!(
            state
                .worker
                .as_ref()
                .unwrap()
                .requests
                .try_send(Request::Clipboard(direction))
                .is_ok()
        );
        state.clipboard_pending = true;
    }
}

#[cfg(all(test, target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
mod tests {
    use super::*;

    #[allow(dead_code)]
    mod fixture {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/x11_ssh.rs"
        ));
    }

    #[test]
    #[ignore = "requires the isolated Wayland fixture and a qualified Xwayland bundle"]
    fn embedded_config_controls_owned_display_startup() {
        assert_eq!(
            std::env::var("TERMCRAFT_TEST_XSERVER_ISOLATED").as_deref(),
            Ok("1")
        );
        let package = PathBuf::from(std::env::var_os("TERMCRAFT_TEST_XSERVER_PACKAGE").unwrap());
        for enabled in [true, false] {
            let fixture = fixture::Fixture::start();
            let config = fixture.config(1);
            let ctx = Context::default();
            let root = tempfile::tempdir().unwrap();
            let mut workspace = Workspace::new_context(&ctx, None, root.path().into());
            workspace.xserver.probe_executable = Some(
                std::env::var_os("TERMCRAFT_TEST_X11_PROBE")
                    .expect("set the qualified app probe executable")
                    .into(),
            );
            workspace.xserver.bundle_root = Some(if enabled {
                package.clone()
            } else {
                root.path().into()
            });
            let known_hosts = workspace
                .store_path
                .parent()
                .unwrap()
                .join("ssh/known_hosts");
            std::fs::create_dir_all(known_hosts.parent().unwrap()).unwrap();
            std::fs::copy(&config.app_known_hosts, &known_hosts).unwrap();
            let config_file = root.path().join("ssh_config");
            std::fs::write(
                &config_file,
                if enabled {
                    "Host *\n ForwardX11 yes\n ForwardX11Trusted yes\n"
                } else {
                    "Host *\n ForwardX11 no\n"
                },
            )
            .unwrap();
            let mut session = config.session;
            session.x11 = !enabled;
            session.ssh_options.prefer_config = true;
            session.ssh_options.config_file = Some(config_file.clone());
            workspace.start(session);
            let index = workspace.active;
            let deadline = Instant::now() + Duration::from_secs(8);
            while !workspace.tabs[index].ssh_connected {
                assert!(
                    workspace.tabs[index].error.is_empty(),
                    "{}",
                    workspace.tabs[index].error
                );
                assert!(Instant::now() < deadline, "SSH did not connect");
                workspace.poll_xserver_sessions(&ctx);
                workspace.tabs[index].poll();
                if workspace.tabs[index].waiting_for_x11 {
                    // The worker already resolved this profile. Changing the file
                    // while the display starts cannot alter its forwarding choice.
                    std::fs::write(&config_file, "Host *\n ForwardX11 no\n").unwrap();
                }
                std::thread::sleep(Duration::from_millis(10));
            }

            assert_eq!(fixture.logins.load(Ordering::Relaxed), 1);
            if enabled {
                let request = fixture.take_request().expect("SSH must request forwarding");
                assert!(fixture.take_request().is_none());
                let display = workspace.xserver.display.as_ref().unwrap();
                assert!(display.authority_path().is_file());
                assert!(fixture.x_client(&request, true).is_ok());
            } else {
                assert!(workspace.xserver.worker.is_none());
                assert!(fixture.take_request().is_none());
            }
            workspace.xserver.shutdown();
        }
    }

    #[test]
    fn openssh_without_display_blocks_only_requested_forwarding() {
        const CHILD: &str = "TERMCRAFT_TEST_X11_NO_DISPLAY";
        if std::env::var_os(CHILD).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "ui::xserver::tests::openssh_without_display_blocks_only_requested_forwarding",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .env("DISPLAY", "")
                .status()
                .unwrap();
            assert!(status.success());
            return;
        }
        let ctx = Context::default();
        let root = tempfile::tempdir().unwrap();
        let mut workspace = Workspace::new_context(&ctx, None, root.path().into());
        let session = Session {
            protocol: "SSH".into(),
            ssh_engine: SshEngine::OpenSsh,
            host: "127.0.0.1".into(),
            x11: true,
            x11_source: XDisplaySource::System,
            ..Default::default()
        };
        workspace.start(session.clone());
        let tab = &workspace.tabs[workspace.active];
        assert!(tab.error.contains("System display"), "{}", tab.error);
        assert!(tab.process.is_none());

        let fixture = fixture::Fixture::start();
        let fixture_config = fixture.config(1);
        let config_file = root.path().join("openssh_config");
        std::fs::write(
            &config_file,
            format!(
                "Host *\n UserKnownHostsFile {}\n StrictHostKeyChecking yes\n ForwardX11 yes\n",
                fixture_config.app_known_hosts.display(),
            ),
        )
        .unwrap();
        let mut session = session;
        session.x11 = false;
        session.port = fixture_config.session.port;
        session.user = fixture_config.session.user;
        session.ssh_options.config_file = Some(config_file);
        workspace.start(session);
        let tab = &mut workspace.tabs[workspace.active];
        assert!(tab.error.is_empty(), "{}", tab.error);
        let args = &tab.launch.as_ref().unwrap().args;
        assert!(args.iter().any(|arg| arg == "ForwardX11=no"));
        let deadline = Instant::now() + Duration::from_secs(5);
        while !tab.screen.plain().contains("FIXTURE_SHELL_READY") {
            tab.poll();
            assert!(
                Instant::now() < deadline,
                "OpenSSH shell did not start: {}",
                tab.screen.plain()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(fixture.logins.load(Ordering::Relaxed), 1);
        assert!(fixture.take_request().is_none());
    }

    fn poll_until_stopped(state: &mut State) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while state.worker.is_some() {
            assert!(Instant::now() < deadline, "X server worker did not finish");
            state.poll();
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn xserver_ui_reports_missing_bundle() {
        let root = tempfile::tempdir().unwrap();
        let mut state = State {
            bundle_root: Some(root.path().into()),
            ..State::default()
        };
        state.start(&Context::default());
        poll_until_stopped(&mut state);
        assert!(
            state
                .error
                .as_deref()
                .unwrap()
                .contains("Missing packaged X server")
        );
        assert!(state.display.is_none());
    }

    #[test]
    fn requested_display_failure_blocks_local_shell() {
        let ctx = Context::default();
        let root = tempfile::tempdir().unwrap();
        let mut workspace = Workspace::new_context(&ctx, None, root.path().to_owned());
        workspace.xserver.bundle_root = Some(root.path().into());
        workspace.start(Session {
            protocol: "Local".into(),
            x11: true,
            x11_source: XDisplaySource::Owned,
            ..Default::default()
        });
        let index = workspace.active;
        let deadline = Instant::now() + Duration::from_secs(5);
        while workspace.tabs[index].waiting_for_x11 {
            assert!(
                Instant::now() < deadline,
                "missing bundle did not resolve startup"
            );
            workspace.poll_xserver_sessions(&ctx);
            std::thread::sleep(Duration::from_millis(10));
        }
        let tab = &workspace.tabs[index];
        assert!(tab.process.is_none());
        assert!(tab.error.contains("Missing packaged X server"));
        assert!(tab.error.contains("Choose System display"));
    }

    #[test]
    #[ignore = "requires the isolated Wayland fixture and a qualified Xwayland bundle"]
    fn local_tab_starts_owned_server_and_only_its_shell_gets_client_environment() {
        assert_eq!(
            std::env::var("TERMCRAFT_TEST_XSERVER_ISOLATED").as_deref(),
            Ok("1")
        );
        let inherited_display = std::env::var_os("DISPLAY");
        let inherited_authority = std::env::var_os("XAUTHORITY");
        let ctx = Context::default();
        let root = tempfile::tempdir().unwrap();
        let mut workspace = Workspace::new_context(&ctx, None, root.path().to_owned());
        workspace.xserver.probe_executable = Some(
            std::env::var_os("TERMCRAFT_TEST_X11_PROBE")
                .expect("set the qualified app probe executable")
                .into(),
        );
        workspace.xserver.bundle_root = Some(
            std::env::var_os("TERMCRAFT_TEST_XSERVER_PACKAGE")
                .unwrap()
                .into(),
        );

        workspace.start_with_launch(
            Session {
                protocol: "Local".into(),
                x11: true,
                x11_source: XDisplaySource::Owned,
                ..Default::default()
            },
            Some(connection::Launch {
                program: "/bin/sh".into(),
                args: vec![
                    "-c".into(),
                    r#"printf '%s\n%s\n' "$DISPLAY" "$XAUTHORITY""#.into(),
                ],
                directory: None,
                rdp_profile: None,
            }),
        );
        let index = workspace.active;
        let deadline = Instant::now() + Duration::from_secs(8);
        while workspace.tabs[index].waiting_for_x11 {
            assert!(
                Instant::now() < deadline,
                "local tab did not get its X display"
            );
            workspace.poll_xserver_sessions(&ctx);
            std::thread::sleep(Duration::from_millis(10));
        }
        let tab = &workspace.tabs[index];
        assert!(tab.error.is_empty(), "{}", tab.error);
        let display = workspace.xserver.display.as_ref().unwrap();
        let process = tab.process.as_ref().unwrap();
        let expected = format!(
            "{}\r\n{}\r\n",
            display.display(),
            display.authority_path().display()
        );
        let mut output = Vec::new();
        while output.len() < expected.len() {
            output.extend(process.rx.recv_timeout(Duration::from_secs(3)).unwrap());
        }
        assert_eq!(output, expected.into_bytes());
        assert_eq!(std::env::var_os("DISPLAY"), inherited_display);
        assert_eq!(std::env::var_os("XAUTHORITY"), inherited_authority);
        workspace.xserver.shutdown();
    }

    #[test]
    #[ignore = "requires the isolated Wayland fixture and a qualified Xwayland bundle"]
    fn crashed_probe_discards_fresh_display_before_releasing_clients() {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::env::var("TERMCRAFT_TEST_XSERVER_ISOLATED").as_deref(),
            Ok("1")
        );
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("crashing-probe");
        let authority = directory.path().join("authority-path");
        std::fs::write(
            &executable,
            format!(
                "#!/bin/sh\nprintf '%s' \"$XAUTHORITY\" > '{}'\nexit 139\n",
                authority.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut state = State {
            bundle_root: Some(
                std::env::var_os("TERMCRAFT_TEST_XSERVER_PACKAGE")
                    .unwrap()
                    .into(),
            ),
            probe_executable: Some(executable),
            ..State::default()
        };
        state.start(&Context::default());
        poll_until_stopped(&mut state);
        assert!(state.display.is_none());
        assert!(
            state
                .error
                .as_deref()
                .unwrap()
                .contains("X capability probe failed")
        );
        assert!(state.capabilities.as_ref().unwrap().is_err());
        let authority = std::fs::read_to_string(authority).unwrap();
        assert!(
            !std::path::Path::new(&authority).exists(),
            "failed probe display must be discarded"
        );
    }

    #[test]
    fn xserver_ui_cancels_in_background() {
        let root = tempfile::tempdir().unwrap();
        let mut state = State {
            bundle_root: Some(root.path().into()),
            ..State::default()
        };
        state.start(&Context::default());
        state.worker.as_ref().unwrap().request_stop();
        poll_until_stopped(&mut state);
        assert!(state.error.is_none());
        assert!(state.display.is_none());
    }

    #[test]
    #[ignore = "requires the isolated Wayland fixture and a qualified Xwayland bundle"]
    fn xserver_ui_starts_and_stops_packaged_engine() {
        assert_eq!(
            std::env::var("TERMCRAFT_TEST_XSERVER_ISOLATED").as_deref(),
            Ok("1"),
            "run this test through tests/support/run_xserver_isolated.sh"
        );
        let root = std::env::var_os("TERMCRAFT_TEST_XSERVER_PACKAGE").unwrap();
        let mut state = State {
            bundle_root: Some(PathBuf::from(root)),
            probe_executable: Some(
                std::env::var_os("TERMCRAFT_TEST_X11_PROBE")
                    .expect("set the qualified app probe executable")
                    .into(),
            ),
            ..State::default()
        };
        state.start(&Context::default());
        let deadline = Instant::now() + Duration::from_secs(8);
        while state.display.is_none() && state.worker.is_some() {
            assert!(
                Instant::now() < deadline,
                "X server UI never reported readiness"
            );
            state.poll();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(state.error.is_none(), "{:?}", state.error);
        let authority = state.display.as_ref().unwrap().authority_path().to_owned();
        assert!(authority.is_file());
        state.worker.as_ref().unwrap().request_stop();
        poll_until_stopped(&mut state);
        assert!(state.error.is_none());
        assert!(!authority.exists());
    }
}
