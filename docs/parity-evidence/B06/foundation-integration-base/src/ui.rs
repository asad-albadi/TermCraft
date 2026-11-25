use eframe::egui::{self, epaint::PathShape, *};
use harbor_workspace::{
    connection,
    model::*,
    process::Process,
    serial::Serial,
    terminal::{Cell, Screen},
};
use std::{
    fs,
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};
mod activity;
mod appearance;
mod broadcast;
mod cli_dispatch;
mod clipboard;
#[cfg(all(test, unix))]
mod credential_tests;
mod credentials;
mod desktop;
mod desktop_gateway;
mod ftp;
mod ftp_settings;
mod highlighting;
mod http_share;
mod layout;
mod macro_events;
mod managed_cron;
mod managed_servers;
mod managed_ssh_iperf;
mod managed_vnc;
mod mosh_startup;
mod packages;
mod pages;
mod plugins;
mod rdp_settings;
mod remote_editor;
mod remote_files;
mod serial_settings;
mod servers;
mod session_confirmations;
mod session_groups;
mod session_import;
mod shared_sessions;
mod shortcuts;
#[cfg(all(test, unix))]
mod ssh_key_tests;
mod ssh_keys;
mod ssh_options;
mod terminal_mouse;
mod terminal_output;
mod terminal_selection;
mod terminal_settings;
mod tools;
#[cfg(all(test, unix))]
mod vnc_integration_tests;
mod vnc_settings;
mod web_browser;
mod windows;
mod xdmcp;
mod xserver;
fn accent(ui: &Ui) -> Color32 {
    ui.visuals().hyperlink_color
}
fn ink(ui: &Ui) -> Color32 {
    ui.visuals().text_color()
}
fn muted(ui: &Ui) -> Color32 {
    ui.visuals().weak_text_color()
}
fn line(ui: &Ui) -> Color32 {
    ui.visuals().widgets.noninteractive.bg_stroke.color
}
fn mist(ui: &Ui) -> Color32 {
    ui.visuals().window_fill
}
fn content(ui: &Ui) -> Color32 {
    ui.visuals().panel_fill
}
#[derive(Clone, Copy, PartialEq)]
enum Page {
    Home,
    Terminal,
    Files,
    Tunnels,
    Macros,
    Servers,
    Tools,
    Settings,
}
struct TerminalTab {
    id: u64,
    session: Session,
    screen: Screen,
    codec: harbor_workspace::terminal::settings::TerminalCodec,
    terminal_settings: harbor_workspace::terminal::settings::TerminalSettings,
    output_ended: bool,
    output_changed: bool,
    pending_output: Option<(Vec<u8>, usize)>,
    input_failed: bool,
    mouse: terminal_mouse::Router,
    pending_mouse: Option<terminal_mouse::Pending>,
    highlights: highlighting::Cache,
    selection: terminal_selection::Controller,
    paste: Option<harbor_workspace::terminal::paste::PasteController>,
    paste_end: Option<Vec<u8>>,
    paste_waiting: bool,
    paste_request: Option<session_confirmations::InputPaste>,
    clipboard_request: bool,
    after_paste: Option<session_confirmations::AfterPaste>,
    pending_replies: Vec<u8>,
    launch: Option<connection::Launch>,
    process: Option<Process>,
    // Drop the process before releasing its installed-version lease.
    portable_launch: Option<harbor_workspace::environment::Launch>,
    pending_mosh: Option<mosh_startup::Pending>,
    xdmcp: Option<harbor_workspace::x11::ManagedXdmcp>,
    pending_portable: Option<packages::PendingLaunch>,
    desktop: Option<desktop::State>,
    web_browser: Option<web_browser::State>,
    gateway: Option<desktop_gateway::Pending>,
    ssh: Option<harbor_workspace::ssh::ConnectionHandle>,
    browser: Option<harbor_workspace::remote_files::BrowserState>,
    directory_tracker: Option<harbor_workspace::ssh::shell_integration::ShellDirectoryTracker>,
    auth: Option<remote_files::AuthForm>,
    vault_request: Option<credentials::Request>,
    transfer_ui: remote_files::TransferUi,
    ssh_connected: bool,
    connection_status: String,
    waiting_for_x11: bool,
    serial: Option<Serial>,
    legacy: Option<harbor_workspace::protocols::Transport>,
    ftp: Option<ftp::State>,
    error: String,
    started: Instant,
    search: String,
    search_open: bool,
    search_focus: bool,
    history_offset: f32,
    remote_path: String,
    macro_state: macro_events::TabState,
    recording: bool,
    recorded: String,
    log: Option<fs::File>,
}
impl TerminalTab {
    fn start_embedded_ssh(&mut self, config: harbor_workspace::ssh::ConnectionConfig) {
        use harbor_workspace::ssh::ConnectionHandle;
        self.browser = Some(harbor_workspace::remote_files::BrowserState::new(self.id));
        self.directory_tracker = Some(
            harbor_workspace::ssh::shell_integration::ShellDirectoryTracker::new(
                &self.session.host,
            ),
        );
        match ConnectionHandle::connect_with_managed_x11(config) {
            Ok(ssh) => self.ssh = Some(ssh),
            Err(error) => {
                self.error = error;
                self.browser.as_mut().unwrap().loading = false;
            }
        }
    }

    fn send(&mut self, data: &[u8]) {
        if self.input_busy() {
            self.error = "Finish or cancel the current paste before sending more input.".into();
            return;
        }
        if let Err(error) = self.send_user_bytes(data, true) {
            self.error = error;
        }
    }
    fn send_generated_command(&mut self, data: &[u8]) {
        if self.input_busy() {
            self.error = "Finish or cancel the current paste before sending a file command.".into();
            return;
        }
        if let Err(error) = self.transmit(data, false) {
            self.error = error;
        }
    }
    fn transmit(&mut self, data: &[u8], record: bool) -> Result<(), String> {
        if self.ssh.is_some()
            && (!self.ssh_connected
                || self.auth.is_some()
                || self.ssh.as_ref().is_some_and(|s| s.is_finished()))
        {
            return Err("Use the sign-in form in Remote files. Terminal input is disabled until authentication completes.".into());
        }
        let result = if let Some(ssh) = &self.ssh {
            ssh.try_send(harbor_workspace::ssh::ConnectionCommand::Input(
                data.to_vec(),
            ))
        } else if let Some(legacy) = &self.legacy {
            legacy.write(data)
        } else if let Some(p) = &mut self.process {
            p.write(data)
        } else if let Some(s) = &mut self.serial {
            s.write(data)
        } else {
            Err("This session is disconnected. Choose Reconnect.".into())
        };
        if result.is_ok() && self.recording && record {
            self.recorded.push_str(&String::from_utf8_lossy(data));
        }
        result
    }
    fn poll(&mut self) {
        let _ = self.poll_with_budget(terminal_output::TICK_BYTES);
    }
    fn poll_with_budget(&mut self, budget: usize) -> usize {
        self.poll_with_ssh_batch(
            budget,
            harbor_workspace::ssh::ConnectionHandle::poll_with_output_budget,
        )
    }
    fn poll_with_ssh_batch(
        &mut self,
        budget: usize,
        read_ssh: impl FnOnce(
            &mut harbor_workspace::ssh::ConnectionHandle,
            usize,
        ) -> Vec<harbor_workspace::ssh::ConnectionEvent>,
    ) -> usize {
        if let Some(session) = &self.xdmcp {
            while let Some(status) = session.poll() {
                match status {
                    Ok(message) => self.connection_status = message,
                    Err(error) => {
                        self.connection_status = "XDMCP could not start".into();
                        self.error = error;
                    }
                }
            }
        }
        if let Some(startup) = &self.pending_portable {
            let Some(result) = startup.poll() else {
                return 0;
            };
            self.pending_portable = None;
            match result {
                Ok((process, launch)) => {
                    process.resize(self.screen.cols as u16, self.screen.rows as u16);
                    self.connection_status = format!("BusyBox {}", launch.version);
                    self.process = Some(process);
                    self.portable_launch = Some(launch);
                }
                Err(error) => {
                    self.connection_status = "Portable Unix could not start".into();
                    self.error = error;
                }
            }
        }
        if let Some(startup) = &self.pending_mosh {
            let Some(result) = startup.poll() else {
                return 0;
            };
            self.pending_mosh = None;
            match result {
                Ok(process) => {
                    process.resize(self.screen.cols as u16, self.screen.rows as u16);
                    self.process = Some(process);
                    self.connection_status = "Mosh started".into();
                }
                Err(error) => {
                    self.connection_status = "Mosh could not start".into();
                    self.error = error;
                }
            }
        }
        if let Some(ftp) = &mut self.ftp {
            ftp.poll();
            self.connection_status = ftp.status().into();
            return 0;
        }
        if self.gateway.is_some() {
            desktop_gateway::poll(self);
            if self.desktop.is_none() && self.process.is_none() && self.legacy.is_none() {
                return 0;
            }
        }
        if let Some(desktop) = &mut self.desktop {
            self.output_changed |= desktop.poll();
            self.connection_status = desktop.message.clone();
            self.error = desktop.error.clone();
            return 0;
        }

        let budget = if self.pending_replies.len() >= 4096 && self.ready_for_input() {
            0
        } else {
            budget
        };
        let mut output = Vec::new();
        let mut latest_shell_directory = None;
        if let Some(ssh) = &mut self.ssh {
            use harbor_workspace::ssh::ConnectionEvent;
            for event in read_ssh(ssh, budget) {
                match event {
                    ConnectionEvent::Output { connection, bytes } if connection == self.id => {
                        if let Some(tracker) = &mut self.directory_tracker {
                            if let Some(path) = tracker.feed(&bytes).pop() {
                                latest_shell_directory = Some(path);
                            }
                        }
                        output.extend(bytes)
                    }
                    ConnectionEvent::State {
                        connection,
                        message,
                    } if connection == self.id => self.connection_status = message,
                    ConnectionEvent::FilesCapabilities {
                        connection,
                        capabilities,
                    } if connection == self.id => {
                        self.transfer_ui.capabilities = Some(capabilities);
                    }
                    ConnectionEvent::NeedsXDisplay { connection } if connection == self.id => {
                        self.waiting_for_x11 = true;
                    }
                    ConnectionEvent::ResolvedEndpoint { connection, host }
                        if connection == self.id =>
                    {
                        self.directory_tracker = Some(
                            harbor_workspace::ssh::shell_integration::ShellDirectoryTracker::new(
                                &host,
                            ),
                        );
                    }
                    ConnectionEvent::Connected { connection } if connection == self.id => {
                        self.ssh_connected = true;
                        self.connection_status = "Connected".into();
                        self.error.clear();
                    }
                    ConnectionEvent::Challenge {
                        connection,
                        id,
                        title,
                        prompts,
                        host_key,
                        credential,
                    } if connection == self.id => {
                        let values = (0..prompts.len())
                            .map(|_| zeroize::Zeroizing::new(String::new()))
                            .collect();
                        self.auth = Some(remote_files::AuthForm {
                            id,
                            title,
                            prompts,
                            values,
                            host_key,
                            credential,
                            save_in_vault: false,
                            vault_filled: false,
                        });
                        if let Some(browser) = &mut self.browser {
                            browser.visible = true;
                        }
                    }
                    ConnectionEvent::Directory(snapshot) => {
                        if let Some(browser) = &mut self.browser {
                            if browser.apply_snapshot(snapshot) {
                                self.transfer_ui.listing_changed();
                            }
                        }
                    }
                    ConnectionEvent::FilesError {
                        connection,
                        request,
                        message,
                    } => {
                        if let Some(browser) = &mut self.browser {
                            browser.fail(connection, request, message);
                        }
                    }
                    ConnectionEvent::Exited {
                        connection,
                        message,
                    } if connection == self.id => {
                        self.ssh_connected = false;
                        self.waiting_for_x11 = false;
                        self.output_ended = true;
                        self.auth = None;
                        self.error = message;
                        if let Some(browser) = &mut self.browser {
                            browser.loading = false;
                        }
                    }
                    _ => {}
                }
            }
        }
        if let Some(path) = latest_shell_directory {
            self.follow_shell_directory(path);
        }
        let refresh_files = self
            .ssh
            .as_mut()
            .is_some_and(|ssh| self.transfer_ui.poll(ssh));
        if refresh_files {
            if let Some(path) = self
                .browser
                .as_ref()
                .and_then(|browser| browser.refresh_path())
            {
                self.navigate_remote(path);
            }
        }
        if let Some(process) = &mut self.process {
            self.output_ended |=
                terminal_output::drain(&mut output, budget, &mut self.pending_output, &process.rx);
            process.poll();
        }
        if let Some(serial) = &mut self.serial {
            self.output_ended |=
                terminal_output::drain(&mut output, budget, &mut self.pending_output, &serial.rx);
        }
        if let Some(legacy) = &self.legacy {
            self.output_ended |= legacy.with_output(|rx, flushed| {
                if flushed {
                    self.pending_output = None;
                    output.clear();
                }
                terminal_output::drain(&mut output, budget, &mut self.pending_output, rx)
            });
            if legacy.is_ready() {
                self.connection_status = "Connected".into();
            }
        }

        if let Some(error) = self
            .process
            .as_ref()
            .and_then(|p| p.input_error())
            .or_else(|| self.serial.as_ref().and_then(|s| s.input_error()))
            .or_else(|| self.legacy.as_ref().and_then(|s| s.input_error()))
        {
            self.input_failed = true;
            self.error = error;
            self.paste = None;
            self.paste_end = None;
        }
        if !output.is_empty() {
            if let Some(f) = &mut self.log {
                use std::io::Write;
                let _ = f.write_all(&output);
            }
            let decoded = self.codec.feed(&output);
            self.capture_macro_output(&decoded);
            self.feed_terminal_output(decoded.as_bytes());
        }
        if self.output_ended
            && output.is_empty()
            && self.pending_output.is_none()
            && self.ssh.as_ref().is_none_or(|ssh| ssh.output_drained())
        {
            let final_text = self.codec.finish();
            if !final_text.is_empty() {
                self.capture_macro_output(&final_text);
                self.feed_terminal_output(final_text.as_bytes());
            }
        }
        self.pending_replies.append(&mut self.screen.replies);
        self.flush_protocol_replies();
        output.len()
    }
    fn terminal_starting(&self) -> bool {
        self.pending_mosh.is_some() || self.pending_portable.is_some()
    }
    fn running(&self) -> bool {
        if self.terminal_starting() {
            return true;
        }
        if let Some(session) = &self.xdmcp {
            return !session.is_finished();
        }
        if let Some(browser) = &self.web_browser {
            return !browser.handle.state().closed;
        }
        if let Some(ftp) = &self.ftp {
            return ftp.running();
        }
        if let Some(legacy) = &self.legacy {
            return !legacy.is_finished();
        }
        if self
            .gateway
            .as_ref()
            .is_some_and(|gateway| !gateway.is_finished())
        {
            return true;
        }
        if let Some(desktop) = &self.desktop {
            return !desktop.handle.is_finished();
        }
        (self.ssh_connected && self.ssh.as_ref().is_some_and(|s| !s.is_finished()))
            || self
                .serial
                .as_ref()
                .is_some_and(|s| !self.output_ended && !s.disconnected())
            || self.process.as_ref().is_some_and(|p| !p.exited)
    }
}
#[derive(Clone)]
struct Entry {
    name: String,
    path: PathBuf,
    dir: bool,
    size: u64,
}
struct FileView {
    path: PathBuf,
    path_text: String,
    entries: Vec<Entry>,
    selected: Option<PathBuf>,
    query: String,
    new_name: String,
    show_new: bool,
    clipboard: Option<PathBuf>,
    error: String,
}
impl FileView {
    fn new() -> Self {
        let path = home_dir();
        let mut s = Self {
            path_text: path.to_string_lossy().into(),
            path,
            entries: Vec::new(),
            selected: None,
            query: String::new(),
            new_name: String::new(),
            show_new: false,
            clipboard: None,
            error: String::new(),
        };
        s.refresh();
        s
    }
    fn refresh(&mut self) {
        self.entries.clear();
        self.error.clear();
        match fs::read_dir(&self.path) {
            Ok(entries) => {
                for ent in entries.flatten() {
                    if let Ok(m) = ent.metadata() {
                        self.entries.push(Entry {
                            name: ent.file_name().to_string_lossy().into(),
                            path: ent.path(),
                            dir: m.is_dir(),
                            size: m.len(),
                        });
                    }
                }
                self.entries.sort_by(|a, b| {
                    b.dir
                        .cmp(&a.dir)
                        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                });
            }
            Err(e) => self.error = e.to_string(),
        }
        self.path_text = self.path.to_string_lossy().into();
        self.selected = None;
    }
}
struct Editor {
    path: PathBuf,
    text: String,
    dirty: bool,
}
pub struct Workspace {
    logo: TextureHandle,
    desktop_theme: Option<appearance::DesktopTheme>,
    store: Store,
    store_path: PathBuf,
    store_ok: bool,
    packages: packages::State,
    plugins: plugins::State,
    page: Page,
    tabs: Vec<TerminalTab>,
    active: usize,
    next_id: u64,
    session_search: String,
    session_form: Option<Session>,
    session_import: Option<session_import::State>,
    shared_sources: shared_sessions::State,
    session_groups: session_groups::State,
    dispatch: cli_dispatch::State,
    credentials: credentials::State,
    ssh_keys: ssh_keys::State,
    session_import_busy: std::sync::Arc<std::sync::atomic::AtomicBool>,
    highlight_editor: Option<highlighting::Editor>,
    shortcut_editor_open: bool,
    shortcut_capture: Option<harbor_workspace::workspace::shortcuts::Action>,
    shortcut_error: String,
    terminal_profile_editor: Option<terminal_settings::ProfileEditor>,
    editing: Option<usize>,
    form_error: String,
    save_session: bool,
    files: FileView,
    editor: Option<Editor>,
    remote_editors: Vec<remote_editor::RemoteEditor>,
    pending_remote_tab_close: Option<u64>,
    pending_remote_import: Option<(u64, u64)>,
    pending_remote_exit: bool,
    pending_session_close: Option<session_confirmations::PendingClose>,
    allow_session_exit: bool,
    pending_paste: Option<session_confirmations::PendingPaste>,
    clipboard_read: Option<clipboard::Pending>,
    clipboard_busy: std::sync::Arc<std::sync::atomic::AtomicBool>,
    allow_remote_exit: bool,
    delete_file: Option<PathBuf>,
    rename_file: Option<PathBuf>,
    rename_text: String,
    tunnel_form: Tunnel,
    tunnel_editing: Option<usize>,
    tunnel_open: bool,
    macro_form: Macro,
    macro_editing: Option<usize>,
    macro_open: bool,
    http: Option<harbor_workspace::http_server::Server>,
    http_start: Option<http_share::Pending>,
    http_auth_enabled: bool,
    http_auth_token: zeroize::Zeroizing<String>,
    http_uploads: bool,
    http_request_timeout: u32,
    http_root: String,
    http_bind: String,
    http_log: String,
    http_timeout: u32,
    http_started: Option<Instant>,
    xserver: xserver::State,
    xdmcp_discovery: xdmcp::State,
    managed_servers: managed_servers::ManagedServers,
    managed_cron: managed_cron::ManagedCron,
    managed_ssh_iperf: managed_ssh_iperf::ManagedSshIperf,
    managed_vnc: managed_vnc::ManagedVnc,
    status: String,
    status_time: Instant,
    quick_host: String,
    quick_protocol: String,
    layout_state: harbor_workspace::workspace::layout::LayoutState,
    pane_bounds: Vec<(u64, Rect)>,
    activity: harbor_workspace::workspace::activity::ActivityState,
    painted_terminals: std::collections::BTreeMap<u64, (u64, ViewportId)>,
    visible_terminals: std::collections::BTreeSet<u64>,
    windows: harbor_workspace::workspace::windows::WorkspaceWindows,
    window_active: std::collections::BTreeMap<u64, u64>,
    window_close_prompt: std::collections::BTreeSet<u64>,
    title_editor: Option<(u64, String)>,
    poll_cursor: usize,
    logic_polled: bool,
    last_paint_poll_frame: Option<u64>,
    broadcast: bool,
    broadcast_group: harbor_workspace::workspace::broadcast::BroadcastGroup,
    broadcast_report: harbor_workspace::workspace::broadcast::BroadcastReport,
    broadcast_page: usize,
    parity_open: bool,
    screenshot: Option<String>,
    frames: u32,
    shot_requested: bool,
    tools: tools::UiState,
    diagnostic_host: String,
    diagnostic_result: String,
    diagnostic_rx: Option<Receiver<String>>,
}
impl Workspace {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        screenshot: Option<String>,
        data_dir: PathBuf,
    ) -> Self {
        let workspace = Self::new_context(&cc.egui_ctx, screenshot, data_dir);
        #[cfg(target_os = "linux")]
        let workspace = {
            let mut workspace = workspace;
            workspace.desktop_theme = Some(appearance::DesktopTheme::watch(cc.egui_ctx.clone()));
            workspace
        };
        workspace
    }
    fn new_context(ctx: &Context, screenshot: Option<String>, data_dir: PathBuf) -> Self {
        let rgba = harbor_workspace::branding::logo(256);
        let logo = ctx.load_texture(
            "termcraft-logo",
            ColorImage::from_rgba_unmultiplied([256, 256], rgba.as_raw()),
            TextureOptions::LINEAR,
        );
        // Linux winit supplies None, and an unavailable portal must not imply dark.
        ctx.options_mut(|options| options.fallback_theme = Theme::Light);
        let mut fonts = FontDefinitions::default();
        fonts.font_data.insert(
            "interface".into(),
            std::sync::Arc::new(FontData::from_static(include_bytes!(
                "../assets/Interface.ttf"
            ))),
        );
        fonts.font_data.insert(
            "terminal".into(),
            std::sync::Arc::new(FontData::from_static(include_bytes!(
                "../assets/Terminal.ttf"
            ))),
        );
        fonts
            .families
            .get_mut(&FontFamily::Proportional)
            .unwrap()
            .insert(0, "interface".into());
        fonts
            .families
            .get_mut(&FontFamily::Monospace)
            .unwrap()
            .insert(0, "terminal".into());
        fonts.families.insert(
            FontFamily::Name("DejaVu Sans Mono".into()),
            vec!["terminal".into()],
        );
        fonts.families.insert(
            FontFamily::Name("Liberation Sans".into()),
            vec!["interface".into()],
        );
        ctx.set_fonts(fonts);
        for theme in [Theme::Light, Theme::Dark] {
            let mut style = (*ctx.style_of(theme)).clone();
            let dark = theme == Theme::Dark;
            style.visuals = if dark {
                Visuals::dark()
            } else {
                Visuals::light()
            };
            let accent = if dark {
                Color32::from_rgb(52, 211, 153)
            } else {
                Color32::from_rgb(0, 120, 87)
            };
            let fill = if dark {
                Color32::from_rgb(19, 25, 23)
            } else {
                Color32::from_rgb(251, 253, 252)
            };
            style.visuals.override_text_color = Some(if dark {
                Color32::from_rgb(234, 242, 237)
            } else {
                Color32::from_rgb(24, 39, 32)
            });
            style.visuals.panel_fill = fill;
            style.visuals.window_fill = if dark {
                Color32::from_rgb(27, 35, 31)
            } else {
                Color32::from_rgb(239, 245, 242)
            };
            style.visuals.extreme_bg_color = fill;
            style.visuals.window_corner_radius = CornerRadius::same(8);
            style.visuals.hyperlink_color = accent;
            style.visuals.weak_text_color = Some(if dark {
                Color32::from_rgb(180, 180, 180)
            } else {
                Color32::from_rgb(96, 96, 96)
            });
            style.visuals.selection.bg_fill = if dark {
                Color32::from_rgb(22, 78, 59)
            } else {
                Color32::from_rgb(204, 238, 223)
            };
            style.visuals.selection.stroke = Stroke::new(
                1.,
                if dark {
                    Color32::WHITE
                } else {
                    Color32::from_rgb(0, 64, 46)
                },
            );
            for widget in [
                &mut style.visuals.widgets.inactive,
                &mut style.visuals.widgets.hovered,
                &mut style.visuals.widgets.active,
            ] {
                widget.corner_radius = CornerRadius::same(4);
            }
            style.visuals.widgets.active.bg_stroke = Stroke::new(1., accent);
            style.visuals.widgets.active.bg_fill = style.visuals.selection.bg_fill;
            style.visuals.widgets.hovered.bg_stroke = Stroke::new(1., accent);
            style.visuals.widgets.hovered.bg_fill = if dark {
                Color32::from_rgb(35, 55, 45)
            } else {
                Color32::from_rgb(224, 239, 231)
            };
            style.spacing.item_spacing = vec2(8., 8.);
            style.spacing.button_padding = vec2(10., 6.);
            style.spacing.interact_size.y = 30.;
            style
                .text_styles
                .insert(TextStyle::Body, FontId::proportional(14.));
            style
                .text_styles
                .insert(TextStyle::Button, FontId::proportional(14.));
            style
                .text_styles
                .insert(TextStyle::Heading, FontId::proportional(24.));
            ctx.set_style_of(theme, style);
        }
        let store_path = data_dir.join("workspace.json");
        let (store, store_ok, status) = match Store::load(&store_path) {
            Ok(s) => (s, true, "Ready".into()),
            Err(e) => (Store::default(), false, e),
        };
        ctx.set_theme(theme_preference(store.preferences.appearance));
        Self {
            logo,
            desktop_theme: None,
            store,
            store_path,
            store_ok,
            packages: Default::default(),
            plugins: Default::default(),
            page: Page::Home,
            tabs: Vec::new(),
            active: 0,
            next_id: 1,
            session_search: String::new(),
            session_form: None,
            session_import: None,
            shared_sources: Default::default(),
            session_groups: Default::default(),
            dispatch: Default::default(),
            credentials: Default::default(),
            ssh_keys: Default::default(),
            session_import_busy: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            highlight_editor: None,
            shortcut_editor_open: false,
            shortcut_capture: None,
            shortcut_error: String::new(),
            terminal_profile_editor: None,
            editing: None,
            form_error: String::new(),
            save_session: true,
            files: FileView::new(),
            editor: None,
            remote_editors: Vec::new(),
            pending_remote_tab_close: None,
            pending_remote_import: None,
            pending_remote_exit: false,
            pending_session_close: None,
            allow_session_exit: false,
            pending_paste: None,
            clipboard_read: None,
            clipboard_busy: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            allow_remote_exit: false,
            delete_file: None,
            rename_file: None,
            rename_text: String::new(),
            tunnel_form: Tunnel::default(),
            tunnel_editing: None,
            tunnel_open: false,
            macro_form: Macro::default(),
            macro_editing: None,
            macro_open: false,
            http: None,
            http_start: None,
            http_auth_enabled: false,
            http_auth_token: zeroize::Zeroizing::new(String::new()),
            http_uploads: false,
            http_request_timeout: 30,
            http_root: home_dir().to_string_lossy().into(),
            http_bind: "127.0.0.1:8080".into(),
            http_log: String::new(),
            http_timeout: 3600,
            http_started: None,
            xserver: xserver::State::for_context(ctx),
            xdmcp_discovery: Default::default(),
            managed_servers: Default::default(),
            managed_cron: Default::default(),
            managed_ssh_iperf: Default::default(),
            managed_vnc: Default::default(),
            status,
            status_time: Instant::now(),
            quick_host: String::new(),
            quick_protocol: "SSH".into(),
            layout_state: harbor_workspace::workspace::layout::LayoutState::default(),
            pane_bounds: Vec::new(),
            activity: Default::default(),
            painted_terminals: Default::default(),
            visible_terminals: Default::default(),
            windows: Default::default(),
            window_active: Default::default(),
            window_close_prompt: Default::default(),
            title_editor: None,
            poll_cursor: 0,
            logic_polled: false,
            last_paint_poll_frame: None,
            broadcast: false,
            broadcast_group: Default::default(),
            broadcast_report: Default::default(),
            broadcast_page: 0,
            parity_open: false,
            screenshot,
            frames: 0,
            shot_requested: false,
            tools: tools::UiState::default(),
            diagnostic_host: "localhost".into(),
            diagnostic_result: String::new(),
            diagnostic_rx: None,
        }
    }
    fn message(&mut self, s: impl Into<String>) {
        self.status = s.into();
        self.status_time = Instant::now();
    }
    fn persist(&mut self) {
        if self.import_applying() {
            self.message("Session import is saving; other settings writes are deferred.");
            return;
        }
        if !self.store_ok {
            self.message(
                "Settings are damaged; export a backup before resetting them in Settings.",
            );
            return;
        }
        self.shared_sources.dirty = true;
        match self.store.save(&self.store_path) {
            Ok(()) => self.message("Workspace saved"),
            Err(e) => self.message(format!("Could not save workspace: {e}")),
        }
    }
    fn new_session(&mut self, protocol: &str) {
        self.editing = None;
        self.session_form = Some(Session {
            protocol: protocol.into(),
            x11_source: Self::default_x11_source(),
            port: protocol_port(protocol),
            browser_options: harbor_workspace::browser::Options {
                embedded: protocol == "Browser",
                runtime: if protocol == "Browser" {
                    harbor_workspace::browser::Runtime::BundledChromium
                } else {
                    harbor_workspace::browser::Runtime::SystemChromium
                },
                ..Default::default()
            },
            ftp_options: if protocol == "FTP" {
                harbor_workspace::protocols::ftp_actor::SessionOptions::embedded()
            } else {
                Default::default()
            },
            ..Default::default()
        });
        self.form_error.clear();
        self.save_session = true;
    }
    fn start(&mut self, s: Session) {
        self.start_with_launch(s, None);
    }
    fn start_with_launch(&mut self, s: Session, launch: Option<connection::Launch>) {
        self.start_with_launch_placement(s, launch, None);
    }
    fn start_with_launch_placement(
        &mut self,
        s: Session,
        launch: Option<connection::Launch>,
        placement: Option<u64>,
    ) {
        let id = self.next_id;
        self.next_id += 1;
        let mut tab = TerminalTab {
            id,
            session: s.clone(),
            screen: Screen::new(110, 30, self.store.preferences.scrollback),
            codec: harbor_workspace::terminal::settings::TerminalCodec::new(
                harbor_workspace::terminal::settings::CharacterEncoding::Utf8,
            ),
            terminal_settings: harbor_workspace::terminal::settings::TerminalSettings::default(),
            output_ended: false,
            output_changed: false,
            pending_output: None,
            input_failed: false,
            mouse: terminal_mouse::Router::default(),
            pending_mouse: None,
            highlights: highlighting::Cache::default(),
            selection: terminal_selection::Controller::default(),
            paste: None,
            paste_end: None,
            paste_waiting: false,
            paste_request: None,
            clipboard_request: false,
            after_paste: None,
            pending_replies: Vec::new(),
            launch: None,
            process: None,
            portable_launch: None,
            pending_mosh: None,
            xdmcp: None,
            pending_portable: None,
            desktop: None,
            web_browser: None,
            gateway: None,
            ssh: None,
            browser: None,
            directory_tracker: None,
            auth: None,
            vault_request: None,
            transfer_ui: remote_files::TransferUi::default(),
            ssh_connected: false,
            connection_status: "Connecting…".into(),
            waiting_for_x11: false,
            serial: None,
            legacy: None,
            ftp: None,
            error: String::new(),
            started: Instant::now(),
            search: String::new(),
            search_open: false,
            search_focus: false,
            history_offset: 0.,
            remote_path: String::new(),
            macro_state: macro_events::TabState::default(),
            recording: false,
            recorded: String::new(),
            log: None,
        };
        tab.apply_terminal_preferences(&self.store.preferences);
        tab.transfer_ui.backend = s.remote_backend;
        let owned_x11 = s.protocol == "Local"
            && s.x11
            && s.x11_source == harbor_workspace::x11::XDisplaySource::Owned;
        if let Err(error) = connection::validate_system_x11_launch(&s) {
            tab.error = error;
        } else if s.protocol == packages::PORTABLE_SESSION {
            match self.start_portable_worker(tab.screen.cols as u16, tab.screen.rows as u16) {
                Ok(startup) => tab.pending_portable = Some(startup),
                Err(error) => tab.error = error,
            }
        } else if owned_x11 {
            if let Err(error) = self.xserver.request_session(id) {
                tab.error = error;
            } else {
                tab.waiting_for_x11 = true;
                tab.launch = launch;
            }
        } else if s.protocol == "XDMCP" && launch.is_none() {
            match xdmcp::start(&s) {
                Ok(session) => tab.xdmcp = Some(session),
                Err(error) => tab.error = error,
            }
        } else if s.protocol == "Browser" && s.browser_options.embedded && launch.is_none() {
            let config = harbor_workspace::browser::Config {
                generation: id,
                address: s.host.clone(),
                proxy: s.browser_options.proxy.clone(),
                size: [960, 640],
            };
            match harbor_workspace::browser::Session::start_with_runtime(
                config,
                s.browser_options.runtime,
            ) {
                Ok(handle) => tab.web_browser = Some(web_browser::State::new(handle)),
                Err(error) => tab.error = error,
            }
        } else if ((s.protocol == "RDP"
            && s.desktop_engine == harbor_workspace::model::DesktopEngine::Embedded)
            || (s.protocol == "VNC" && s.vnc_options.embedded))
            && launch.is_none()
        {
            let mut config = harbor_workspace::desktop::DesktopConfig::new(
                id,
                s.clone(),
                harbor_workspace::desktop::DesktopConfig::default_engine_path(),
            );
            config.remembered_certificate = self
                .store
                .preferences
                .rdp_certificates
                .get(&desktop::endpoint(&s))
                .cloned();
            if s.jump.trim().is_empty() {
                match harbor_workspace::desktop::DesktopSession::connect(config) {
                    Ok(handle) => tab.desktop = Some(desktop::State::new(handle, &s)),
                    Err(error) => tab.error = error,
                }
            } else {
                let ssh = harbor_workspace::ssh::ConnectionConfig {
                    id,
                    agent: self.session_agent_endpoint(&s),
                    defaults: self.store.preferences.ssh_defaults.clone(),
                    clock: None,
                    session: s.clone(),
                    app_known_hosts: self.store_path.parent().unwrap().join("ssh/known_hosts"),
                    user_known_hosts: vec![home_dir().join(".ssh/known_hosts")],
                };
                if let Err(error) = desktop_gateway::start(&mut tab, ssh, config) {
                    tab.error = error;
                }
            }
        } else if s.protocol == "SSH" && s.ssh_engine == SshEngine::Embedded && launch.is_none() {
            tab.start_embedded_ssh(self.terminal_ssh_config(id, &s));
        } else if s.protocol == "Telnet" && !s.jump.trim().is_empty() && launch.is_none() {
            let ssh = harbor_workspace::ssh::ConnectionConfig {
                id,
                agent: self.session_agent_endpoint(&s),
                defaults: self.store.preferences.ssh_defaults.clone(),
                clock: None,
                session: s.clone(),
                app_known_hosts: self.store_path.parent().unwrap().join("ssh/known_hosts"),
                user_known_hosts: vec![home_dir().join(".ssh/known_hosts")],
            };
            if let Err(error) = desktop_gateway::start_telnet(&mut tab, ssh) {
                tab.error = error;
            }
        } else if s.protocol == "FTP"
            && s.ftp_options.launch == harbor_workspace::protocols::ftp_actor::LaunchMode::Embedded
            && launch.is_none()
        {
            tab.ftp = Some(ftp::State::new(
                id,
                s.host.clone(),
                s.port,
                s.user.clone(),
                s.ftp_options.clone(),
                self.store_path.parent().unwrap().join("ftp-recovery"),
            ));
        } else if matches!(s.protocol.as_str(), "Telnet" | "Rlogin" | "Rsh") && launch.is_none() {
            let connection = harbor_workspace::protocols::LegacyProtocolConfig::from_session(
                &s,
                tab.screen.cols as u16,
                tab.screen.rows as u16,
            )
            .and_then(harbor_workspace::protocols::Transport::connect);
            match connection {
                Ok(connection) => tab.legacy = Some(connection),
                Err(error) => tab.error = error,
            }
        } else if s.protocol == "Serial" {
            match Serial::open_with_settings(&s.serial_device, s.serial_line.settings(s.baud)) {
                Ok(p) => tab.serial = Some(p),
                Err(e) => tab.error = e,
            }
        } else if s.protocol == "Mosh" {
            match mosh_startup::Pending::start(
                s,
                self.store.preferences.clone(),
                tab.screen.cols as u16,
                tab.screen.rows as u16,
            ) {
                Ok(startup) => tab.pending_mosh = Some(startup),
                Err(error) => tab.error = error,
            }
        } else {
            match launch
                .map(Ok)
                .unwrap_or_else(|| connection::launch(&s, &self.store.preferences))
            {
                Ok(l) => {
                    tab.launch = Some(l.clone());
                    match Process::spawn(
                        &l.program,
                        &l.args,
                        l.directory.as_deref(),
                        tab.screen.cols as u16,
                        tab.screen.rows as u16,
                    ) {
                        Ok(p) => tab.process = Some(p),
                        Err(e) => tab.error = e,
                    }
                }
                Err(e) => tab.error = e,
            }
        }
        if !tab.error.is_empty() {
            self.message(tab.error.clone());
        }
        tab.resize_transport();
        self.tabs.push(tab);
        if !owned_x11 {
            self.queue_startup_macro(id);
        }
        self.active = self.tabs.len() - 1;
        if placement.is_none() {
            self.add_layout_terminal(self.tabs[self.active].id);
        }
        self.page = Page::Terminal;
    }
    fn terminal_ssh_config(
        &self,
        id: u64,
        session: &Session,
    ) -> harbor_workspace::ssh::ConnectionConfig {
        harbor_workspace::ssh::ConnectionConfig {
            id,
            agent: self.session_agent_endpoint(session),
            defaults: self.store.preferences.ssh_defaults.clone(),
            clock: None,
            session: session.clone(),
            app_known_hosts: self.store_path.parent().unwrap().join("ssh/known_hosts"),
            user_known_hosts: vec![home_dir().join(".ssh/known_hosts")],
        }
    }

    fn duplicate_saved_session(&mut self, index: usize) {
        let mut session = self.store.sessions[index].clone();
        session.name = format!("{} copy", session.title());
        session.id = harbor_workspace::model::new_session_id();
        self.store.sessions.push(session);
        self.persist();
    }
    fn reconnect_active(&mut self) {
        if self.tabs[self.active]
            .ftp
            .as_mut()
            .is_some_and(|ftp| !ftp.close_ready())
        {
            self.message("Save or discard the FTP document before reconnecting");
            return;
        }
        if self.remote_documents_block_close(Some(self.tabs[self.active].id))
            && !self.tabs[self.active]
                .ssh
                .as_ref()
                .is_some_and(|ssh| ssh.is_finished())
        {
            self.message("Save or close the remote editors for this connection before reconnecting. Recovery files stay on this computer.");
            return;
        }
        let mut controller = harbor_workspace::terminal::paste::CloseController::default();
        if controller.begin(
            self.tabs[self.active].running(),
            self.store.preferences.confirm_session_close,
        ) == harbor_workspace::terminal::paste::CloseDecision::Confirm
        {
            self.pending_session_close = Some(session_confirmations::PendingClose::reconnect(
                self.tabs[self.active].id,
                controller,
            ));
            return;
        }
        self.reconnect_tab_now(self.active);
    }
    fn reconnect_tab_now(&mut self, index: usize) {
        if self.tabs[index]
            .ftp
            .as_mut()
            .is_some_and(|ftp| !ftp.close_ready())
        {
            self.message("Save or discard the FTP document before reconnecting");
            return;
        }
        if self.remote_documents_block_close(Some(self.tabs[index].id))
            && !self.tabs[index]
                .ssh
                .as_ref()
                .is_some_and(|s| s.is_finished())
        {
            self.message("Save or close the remote editors before reconnecting this session.");
            return;
        }
        let old_id = self.tabs[index].id;
        self.cancel_macro(old_id);
        self.finish_macro_recording(index);
        let window = self.windows.window(old_id);
        self.activity.remove(old_id);
        self.painted_terminals.remove(&old_id);
        self.visible_terminals.remove(&old_id);
        self.broadcast_group.select(old_id, false);
        let old = self.tabs.remove(index);
        let session = old.session.clone();
        let launch = old.launch.clone();
        drop(old);
        self.start_with_launch_placement(session, launch, Some(window));
        let id = self.tabs[self.active].id;
        self.windows.replace_terminal(old_id, id);
        if window == 0 {
            let replaced = self
                .layout_state
                .root
                .as_mut()
                .is_some_and(|tree| tree.replace(old_id, id));
            if self.layout_state.active == Some(old_id) {
                self.layout_state.active = Some(id);
            }
            if !replaced {
                self.add_layout_terminal(id);
            }
        } else {
            self.window_active.insert(window, id);
        }
        self.sync_root_selection();
    }

    fn local(&mut self) {
        self.start(Session {
            protocol: "Local".into(),
            name: "Local terminal".into(),
            ..Default::default()
        })
    }
    fn top(&mut self, root: &mut Ui) {
        Panel::top("title")
            .frame(
                Frame::new()
                    .fill(mist(root))
                    .inner_margin(Margin::symmetric(12, 6)),
            )
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.add(Image::new(&self.logo).fit_to_exact_size(vec2(24., 24.)));
                    ui.label(
                        RichText::new("TermCraft — Personal edition")
                            .size(13.)
                            .strong(),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        self.appearance_picker(ui);
                    });
                });
            });
        Panel::top("menus")
            .frame(
                Frame::new()
                    .fill(mist(root))
                    .inner_margin(Margin::symmetric(8, 0)),
            )
            .show(root, |ui| {
                egui::MenuBar::new().ui(ui, |ui| {
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
                        if ui.button("Keyboard shortcuts…").clicked() {
                            self.shortcut_editor_open = true;
                            ui.close();
                        }
                        self.layout_picker(ui);
                        self.broadcast_toggle(ui);
                    });
                    ui.menu_button("X server", |ui| {
                        ui.label("Use an installed system X server.");
                        if ui.button("X server settings").clicked() {
                            self.page = Page::Servers;
                            ui.close();
                        }
                    });
                    ui.menu_button("Tools", |ui| {
                        for (label, page) in [
                            ("Network tools and plugins", Page::Tools),
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
                        ui.label("Games are not available in this build.");
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
                        ui.label(RichText::new("TermCraft personal workspace").strong());
                        if ui.button("Feature parity and known differences").clicked() {
                            self.parity_open = true;
                            ui.close();
                        }
                        ui.hyperlink_to(
                            "Original documentation",
                            "https://mobaxterm.mobatek.net/documentation.html",
                        );
                        ui.label("Create a Session to connect to a remote host.");
                        ui.label("Double-click a saved session to reconnect.");
                        ui.label("Ctrl+N: new session · Ctrl+Shift+T: local terminal");
                        ui.separator();
                        ui.label("System clients provide remote protocols and X11.");
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
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.;
                    if tool_button(ui, "desktop", "Session")
                        .on_hover_text("New session (Ctrl+N)")
                        .clicked()
                    {
                        self.new_session("SSH");
                    }
                    if tool_button(ui, "desktop", "Servers").clicked() {
                        self.page = Page::Servers;
                    }
                    if tool_button(ui, "tools", "Tools").clicked() {
                        self.page = Page::Tools;
                    }
                    ui.add_enabled_ui(false, |ui| {
                        tool_button(ui, "games", "Games")
                            .on_hover_text("Games are not available in this build");
                    });
                    if tool_button(ui, "folder", "Sessions").clicked() {
                        self.store.preferences.sidebar_visible =
                            !self.store.preferences.sidebar_visible;
                        self.persist();
                    }
                    ui.separator();
                    if tool_button(ui, "terminal", "Split")
                        .on_hover_text("Toggle split terminals")
                        .clicked()
                    {
                        self.toggle_layout();
                        if !self.tabs.is_empty() {
                            self.page = Page::Terminal;
                        }
                    }
                    if tool_button(ui, "terminal", "MultiExec")
                        .on_hover_text(if self.broadcast {
                            "MultiExec enabled — click to disable"
                        } else {
                            "Choose MultiExec targets"
                        })
                        .clicked()
                    {
                        self.set_broadcast_enabled(!self.broadcast);
                    }
                    if tool_button(ui, "link", "Tunneling").clicked() {
                        self.page = Page::Tunnels;
                    }
                    ui.add_enabled_ui(false, |ui| {
                        tool_button(ui, "folder", "Packages")
                            .on_hover_text("Package management is not available in this build");
                    });
                    if tool_button(ui, "settings", "Settings").clicked() {
                        self.page = Page::Settings;
                    }
                    let help = tool_button(ui, "help", "Help");
                    egui::Popup::menu(&help).show(|ui| {
                        ui.label(RichText::new("Keyboard shortcuts").strong());
                        ui.label("Ctrl+N — new session");
                        ui.label("Ctrl+Shift+T — local terminal");
                        ui.label("Double-click a saved session to connect.");
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if tool_button(ui, "exit", "Exit").clicked() {
                            ui.ctx().send_viewport_cmd(ViewportCommand::Close);
                        }
                        if tool_button(ui, "desktop", "X server")
                            .on_hover_text("Configure the system X server")
                            .clicked()
                        {
                            self.page = Page::Servers;
                        }
                    });
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
                    dot(ui, Color32::from_rgb(20, 150, 106));
                    let text =
                        if self.status_time.elapsed() > Duration::from_secs(12) && self.store_ok {
                            "Ready"
                        } else {
                            &self.status
                        };
                    ui.add(Label::new(RichText::new(text).size(11.).color(muted(ui))).truncate());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!(
                                "{} connections    •    {} saved sessions",
                                self.tabs.iter().filter(|t| t.running()).count(),
                                self.store.sessions.len()
                            ))
                            .size(11.)
                            .color(muted(ui)),
                        );
                    });
                });
            });
    }
    fn sidebar(&mut self, root: &mut Ui) {
        Panel::left("navigation-rail")
            .exact_size(38.)
            .resizable(false)
            .frame(
                Frame::new()
                    .fill(mist(root))
                    .inner_margin(Margin::symmetric(3, 9)),
            )
            .show(root, |ui| {
                for (kind, label, page) in [
                    ("folder", "Sessions", Page::Home),
                    ("tools", "Tools", Page::Tools),
                    ("folder", "Local files", Page::Files),
                    ("macro", "Macros", Page::Macros),
                ] {
                    let (r, response) = ui.allocate_exact_size(vec2(32., 36.), Sense::click());
                    if self.page == page || response.hovered() {
                        ui.painter()
                            .rect_filled(r, 4., ui.visuals().selection.bg_fill);
                    }
                    if self.page == page {
                        ui.painter().rect_filled(
                            Rect::from_min_size(r.min + vec2(0., 10.), vec2(3., 16.)),
                            2.,
                            accent(ui),
                        );
                    }
                    draw_icon(
                        ui.painter(),
                        Rect::from_center_size(r.center(), vec2(18., 18.)),
                        kind,
                        if self.page == page {
                            accent(ui)
                        } else {
                            muted(ui)
                        },
                    );
                    response.widget_info(|| {
                        WidgetInfo::selected(
                            WidgetType::SelectableLabel,
                            ui.is_enabled(),
                            self.page == page,
                            label,
                        )
                    });
                    if response.on_hover_text(label).clicked() {
                        self.page = page;
                        if page == Page::Home && !self.store.preferences.sidebar_visible {
                            self.store.preferences.sidebar_visible = true;
                            self.persist();
                        }
                    }
                }
            });
        if !self.store.preferences.sidebar_visible {
            return;
        }
        let width = self.store.preferences.sidebar_width;
        let width = if width.is_finite() {
            width.clamp(190., 480.)
        } else {
            226.
        };
        let panel = Panel::left("sidebar")
            .default_size(width)
            .min_size(190.)
            .max_size(480.)
            .resizable(true)
            .frame(
                Frame::new()
                    .fill(mist(root))
                    .inner_margin(Margin::symmetric(10, 12)),
            )
            .show(root, |ui| {
                ui.label(RichText::new("Quick connect").size(12.).color(muted(ui)));
                let host = ui.add_sized(
                    [ui.available_width(), 30.],
                    TextEdit::singleline(&mut self.quick_host).hint_text("Host or IP address"),
                );
                let enter = host.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                ui.horizontal(|ui| {
                    ComboBox::from_id_salt("quick_protocol")
                        .width(95.)
                        .selected_text(&self.quick_protocol)
                        .show_ui(ui, |ui| {
                            for protocol in ["SSH", "Telnet", "SFTP", "FTP"] {
                                ui.selectable_value(
                                    &mut self.quick_protocol,
                                    protocol.into(),
                                    protocol,
                                );
                            }
                        });
                    if ui.button("Connect").clicked() || enter {
                        let session = Session {
                            host: self.quick_host.trim().into(),
                            protocol: self.quick_protocol.clone(),
                            port: protocol_port(&self.quick_protocol),
                            ..Default::default()
                        };
                        if connection::launch(&session, &self.store.preferences).is_ok() {
                            self.start(session);
                        } else {
                            self.new_session(&self.quick_protocol.clone());
                            self.session_form = Some(session);
                        }
                    }
                });
                ui.add_space(10.);
                ui.separator();
                ui.add_space(5.);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Sessions").strong().size(15.));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .small_button("+")
                            .on_hover_text("Add a saved session")
                            .clicked()
                        {
                            self.new_session("SSH");
                        }
                    });
                });
                ui.add_space(3.);
                ui.add(
                    TextEdit::singleline(&mut self.session_search)
                        .hint_text("Search sessions")
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(8.);
                let mut launch = None;
                let mut launch_folder = None;
                let mut shortcut = None;
                let mut edit = None;
                let mut remove = None;
                let mut duplicate = None;
                ScrollArea::vertical()
                    .id_salt("sessions")
                    .max_height((ui.available_height() - 80.).max(150.))
                    .show(ui, |ui| {
                        let mut folders = self
                            .store
                            .sessions
                            .iter()
                            .map(|s| s.folder.clone())
                            .collect::<Vec<_>>();
                        folders.sort();
                        folders.dedup();
                        if folders.is_empty() {
                            ui.add_space(12.);
                            ui.horizontal(|ui| {
                                icon(ui, "folder", 16., muted(ui));
                                ui.label(RichText::new("My sessions").color(muted(ui)).size(13.));
                            });
                            ui.add_space(10.);
                            ui.label(
                                RichText::new("Your saved connections\nwill appear here.")
                                    .size(12.)
                                    .color(muted(ui)),
                            );
                            ui.add_space(9.);
                            if ui.link("Create your first session").clicked() {
                                self.new_session("SSH");
                            }
                        }
                        for folder in folders {
                            let header = CollapsingHeader::new(RichText::new(&folder).size(13.).strong())
                                .default_open(true)
                                .show(ui, |ui| {
                                    for (i, s) in self.store.sessions.iter().enumerate() {
                                        if s.folder != folder
                                            || !format!("{} {} {}", s.name, s.host, s.protocol)
                                                .to_lowercase()
                                                .contains(&self.session_search.to_lowercase())
                                        {
                                            continue;
                                        }
                                        let r = ui.add_sized(
                                            [ui.available_width(), 32.],
                                            Button::new(RichText::new(s.title()).size(13.))
                                                .frame(false),
                                        );
                                        if r.double_clicked() {
                                            launch = Some(s.clone());
                                        }
                                        r.on_hover_text(format!(
                                            "{}  {}:{}\nDouble-click to connect",
                                            s.protocol, s.host, s.port
                                        ))
                                        .context_menu(
                                            |ui| {
                                                if ui.button("Connect").clicked() {
                                                    launch = Some(s.clone());
                                                    ui.close();
                                                }
                                                if s.protocol == "SSH"
                                                    && ui
                                                        .button("Open SFTP with these settings")
                                                        .clicked()
                                                {
                                                    launch = s.sftp_session().ok();
                                                    ui.close();
                                                }
                                                if ui.button("Create launch shortcut…").clicked() {
                                                    shortcut = Some((harbor_workspace::platform::shortcuts::ShortcutTarget::Session(s.id.clone()), s.title()));
                                                    ui.close();
                                                }
                                                if ui.button("Edit").clicked() {
                                                    edit = Some(i);
                                                    ui.close();
                                                }
                                                if ui.button("Duplicate").clicked() {
                                                    duplicate = Some(i);
                                                    ui.close();
                                                }
                                                if ui.button("Remove saved session").clicked() {
                                                    remove = Some(i);
                                                    ui.close();
                                                }
                                            },
                                        );
                                    }
                                });
                            header.header_response.context_menu(|ui| {
                                if ui.button("Launch folder and subfolders").clicked() {launch_folder=Some(folder.clone()); ui.close();}
                                if ui.button("Create group launch shortcut…").clicked() {shortcut=Some((harbor_workspace::platform::shortcuts::ShortcutTarget::Group(folder.clone()),folder.clone()));ui.close();}
                            });
                        }
                        self.shared_session_tree(ui);
                    });
                if let Some(s) = launch {
                    self.start(s);
                }
                if let Some(folder)=launch_folder {self.launch_saved_group(&folder);}
                if let Some((target,name))=shortcut {self.create_launch_shortcut(target,name,ui.ctx());}
                if let Some(i) = edit {
                    self.session_form = Some(self.store.sessions[i].clone());
                    self.editing = Some(i);
                    self.form_error.clear();
                }
                if let Some(i) = duplicate {
                    self.duplicate_saved_session(i);
                }
                if let Some(i) = remove {
                    self.store.sessions.remove(i);
                    self.persist();
                }
                ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                    ui.label(
                        RichText::new("Double-click a session to connect")
                            .size(11.)
                            .color(muted(ui)),
                    );
                    ui.separator();
                });
            });
        let new_width = panel.response.rect.width();
        if (new_width - self.store.preferences.sidebar_width).abs() > 0.5
            && !root.ctx().input(|i| i.pointer.any_down())
        {
            self.store.preferences.sidebar_width = new_width;
            self.persist();
        }
    }
    fn apply_appearance(&mut self, ctx: &Context) {
        let previous = ctx.theme();
        let desktop = self.desktop_theme.as_mut().and_then(|theme| theme.poll());
        ctx.options_mut(|options| options.fallback_theme = desktop.unwrap_or(Theme::Light));
        ctx.set_theme(theme_preference(self.store.preferences.appearance));
        if ctx.theme() != previous {
            ctx.request_repaint();
        }
    }
    fn close_tab(&mut self, index: usize) {
        if self.tabs[index]
            .ftp
            .as_mut()
            .is_some_and(|ftp| !ftp.close_ready())
        {
            self.pending_remote_tab_close = Some(self.tabs[index].id);
            return;
        }
        if self.remote_documents_block_close(Some(self.tabs[index].id)) {
            self.pending_remote_tab_close = Some(self.tabs[index].id);
            return;
        }
        self.request_active_tab_close(index);
    }
    fn force_close_tab(&mut self, index: usize) {
        let id = self.tabs[index].id;
        self.cancel_macro(id);
        self.finish_macro_recording(index);
        if self
            .clipboard_read
            .as_ref()
            .is_some_and(|r| r.source == id || r.targets.contains(&id))
        {
            self.clipboard_read = None;
        }
        self.windows.remove(id);
        self.activity.remove(id);
        self.painted_terminals.remove(&id);
        self.visible_terminals.remove(&id);
        self.broadcast_group.select(id, false);
        self.layout_state.remove(id);
        self.tabs.remove(index);
        if self.tabs.is_empty() {
            self.active = 0;
            if self.page == Page::Terminal {
                self.page = Page::Home;
            }
        } else if index < self.active {
            self.active -= 1;
        } else {
            self.active = self.active.min(self.tabs.len() - 1);
        }
        if let Some(id) = self.layout_state.active {
            if let Some(index) = self.tabs.iter().position(|t| t.id == id) {
                self.active = index;
            }
        }
        if self.layout_state.root.is_none() {
            if let Some(tab) = self.tabs.iter().find(|t| self.windows.window(t.id) == 0) {
                self.layout_state.focus(tab.id);
            }
        }
    }
    fn tab_bar(&mut self, ui: &mut Ui) {
        Frame::new()
            .fill(mist(ui))
            .inner_margin(Margin {
                left: 8,
                right: 8,
                top: 6,
                bottom: 0,
            })
            .show(ui, |ui| {
                ScrollArea::horizontal()
                    .id_salt("terminal-tabs")
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 3.;
                            let (home, _) =
                                workspace_tab(ui, "Home", self.page == Page::Home, None, false, 0);
                            if home.clicked() {
                                self.page = Page::Home;
                            }
                            let mut close = None;
                            let mut action = None;
                            for (i, t) in self.tabs.iter().enumerate() {
                                let selected = self.page == Page::Terminal && self.active == i;
                                let (response, closed) = workspace_tab(
                                    ui,
                                    &self.terminal_tab_label(t.id),
                                    selected,
                                    Some(t.running()),
                                    true,
                                    t.id,
                                );
                                response.context_menu(|ui| {
                                    if ui.button("Rename tab…").clicked() {
                                        action = Some((t.id, 0));
                                        ui.close();
                                    }
                                    if ui
                                        .button(if self.windows.window(t.id) == 0 {
                                            "Detach to window"
                                        } else {
                                            "Reattach here"
                                        })
                                        .clicked()
                                    {
                                        action = Some((t.id, 1));
                                        ui.close();
                                    }
                                });
                                if closed || response.clicked_by(PointerButton::Middle) {
                                    close = Some(i);
                                } else if response.clicked() {
                                    self.page = Page::Terminal;
                                    if self.windows.window(t.id) == 0 {
                                        self.active = i;
                                        self.layout_state.focus(t.id);
                                    } else {
                                        self.window_active.insert(self.windows.window(t.id), t.id);
                                        ui.ctx().send_viewport_cmd_to(
                                            windows::viewport_id(self.windows.window(t.id)),
                                            ViewportCommand::Focus,
                                        );
                                    }
                                }
                            }
                            if ui
                                .add(Button::new("+").frame(false).min_size(vec2(32., 34.)))
                                .on_hover_text("New local terminal")
                                .clicked()
                            {
                                self.local();
                            }
                            if let Some((id, kind)) = action {
                                if kind == 0 {
                                    self.title_editor = Some((
                                        id,
                                        self.windows.override_title(id).unwrap_or("").to_owned(),
                                    ));
                                } else if self.windows.window(id) == 0 {
                                    self.detach_terminal(id);
                                } else {
                                    self.reattach_terminal(id);
                                }
                            }
                            if let Some(i) = close {
                                self.close_tab(i);
                            }
                        });
                    });
            });
    }
    fn home(&mut self, ui: &mut Ui) {
        ScrollArea::vertical().show(ui, |ui| {
            ui.add_space((ui.available_height() * 0.15).clamp(36., 100.));
            ui.vertical_centered(|ui| {
                ui.add(Image::new(&self.logo).fit_to_exact_size(vec2(112., 112.)));
                ui.add_space(14.);
                ui.label(RichText::new("TermCraft").size(36.).strong());
                ui.add_space(2.);
                ui.label(RichText::new("Personal edition · Remote terminal workspace").size(13.).color(muted(ui)));
                ui.add_space(28.);
                if primary(ui, "Start local terminal").clicked() { self.local(); }
                ui.add_space(10.);
                if ui.link("Create a new remote session").clicked() { self.new_session("SSH"); }
                ui.add_space(30.);
            });
            let width = ui.available_width().min(500.);
            ui.horizontal(|ui| {
                ui.add_space(((ui.available_width() - width) / 2.).max(0.));
                ui.vertical(|ui| {
                    ui.set_width(width);
                    ui.separator();
                    ui.add_space(12.);
                    ui.label(RichText::new("Saved sessions").size(13.).strong());
                    ui.add_space(6.);
                    if self.store.sessions.is_empty() {
                        ui.label(RichText::new("Create a session using the Session toolbar button.\nYour connections will appear in the session tree on the left.").size(12.).color(muted(ui)));
                    }
                    let mut launch = None;
                    for session in self.store.sessions.iter().take(6) {
                        if ui.link(format!("{}   ·   {}", session.title(), session.protocol)).clicked() { launch = Some(session.clone()); }
                    }
                    if let Some(s) = launch { self.start(s); }
                    ui.add_space(24.);
                    ui.horizontal(|ui| {
                        icon(ui, "lock", 14., muted(ui));
                        ui.label(RichText::new("Passwords are saved only by choice in the encrypted vault.").size(11.).color(muted(ui)));
                    });
                });
            });
        });
    }
    fn dialogs(&mut self, ctx: &Context) {
        self.session_dialog(ctx);
        self.highlighting_dialog(ctx);
        self.terminal_profile_dialog(ctx);
        self.file_dialogs(ctx);
        self.tunnel_dialog(ctx);
        self.macro_dialog(ctx);
    }
}
impl Workspace {
    fn render(&mut self, root: &mut Ui) {
        self.poll_http_start();
        if self.http_start.is_some() {
            root.ctx().request_repaint_after(Duration::from_millis(30));
        }
        let ctx = root.ctx().clone();
        self.poll_session_import(&ctx);
        self.poll_shared_sources(&ctx);
        self.poll_dispatch(&ctx);
        self.poll_vault(&ctx);
        self.poll_ssh_keys(&ctx);
        self.poll_desktop_certificates();
        if self.import_applying() {
            root.disable();
        }
        self.apply_appearance(&ctx);
        self.set_shortcut_context(&ctx);
        self.frames += 1;
        self.update_activity_visibility(&ctx);
        let frame = ctx.cumulative_frame_nr();
        if self.logic_polled {
            self.logic_polled = false;
            self.last_paint_poll_frame = Some(frame);
        } else if self.last_paint_poll_frame != Some(frame) {
            self.pump_terminals();
            self.last_paint_poll_frame = Some(frame);
        }
        self.painted_terminals.clear();
        self.poll_clipboard();
        if let Some(h) = &self.http {
            while let Ok(msg) = h.events.try_recv() {
                self.http_log.push_str(&msg);
                self.http_log.push('\n');
            }
            if self.http_log.len() > 64000 {
                let end = self
                    .http_log
                    .char_indices()
                    .find(|(i, _)| *i >= 32000)
                    .map(|(i, _)| i)
                    .unwrap_or(self.http_log.len());
                self.http_log.drain(..end);
            }
            if self.http_timeout > 0
                && self
                    .http_started
                    .is_some_and(|t| t.elapsed().as_secs() >= self.http_timeout as u64)
            {
                self.http = None;
                self.message("HTTP server stopped after its configured duration.");
            }
        }
        if let Some(rx) = &self.diagnostic_rx {
            if let Ok(result) = rx.try_recv() {
                self.diagnostic_result = result;
                self.diagnostic_rx = None;
            }
        }
        Window::new("Feature parity and known differences")
            .open(&mut self.parity_open)
            .default_size([850., 580.])
            .show(&ctx, |ui| {
                ui.label("Full MobaXterm parity is the target. This build is incomplete.");
                ScrollArea::both().show(ui, |ui| {
                    Grid::new("parity_matrix")
                        .striped(true)
                        .spacing([16., 12.])
                        .show(ui, |ui| {
                            for line in include_str!("../FEATURE_PARITY.md")
                                .lines()
                                .filter(|line| line.starts_with('|') && !line.starts_with("| ---"))
                            {
                                for (index, cell) in line.trim_matches('|').split('|').enumerate() {
                                    let width = [170., 100., 210., 350.][index.min(3)];
                                    ui.add_sized([width, 0.], Label::new(cell.trim()).wrap());
                                }
                                ui.end_row();
                            }
                        });
                });
            });
        self.top(root);
        self.sidebar(root);
        CentralPanel::default()
            .frame(Frame::new().fill(content(root)).inner_margin(0))
            .show(root, |ui| {
                self.tab_bar(ui);
                self.remote_files_panel(ui);
                Frame::new()
                    .fill(content(ui))
                    .inner_margin(12)
                    .show(ui, |ui| match self.page {
                        Page::Home => self.home(ui),
                        Page::Terminal => self.terminals(ui),
                        Page::Files => self.files_page(ui),
                        Page::Tunnels => self.tunnels_page(ui),
                        Page::Macros => self.macros_page(ui),
                        Page::Servers => self.servers_page(ui),
                        Page::Tools => self.tools_page(ui),
                        Page::Settings => self.settings_page(ui),
                    });
            });
        self.detached_windows(&ctx);
        self.collect_terminal_requests(&ctx);
        self.window_title_editor(&ctx);
        self.shortcut_editor(&ctx);
        self.handle_shortcuts(&ctx, 0);
        self.dialogs(&ctx);
        self.remote_editor_windows(&ctx);
        self.session_confirmations(&ctx);
        self.shared_sessions_windows(&ctx);
        self.session_group_windows(&ctx);
        self.credential_windows(&ctx);
        self.ssh_key_windows(&ctx);
        self.session_import_window(&ctx);
        if let Some(path) = &self.screenshot {
            ctx.request_repaint();
            if self.frames >= 120 && !self.shot_requested {
                ctx.send_viewport_cmd(ViewportCommand::Screenshot(Default::default()));
                self.shot_requested = true;
            }
            let events = ctx.input(|i| i.events.clone());
            for event in events {
                if let Event::Screenshot { image, .. } = event {
                    let path = PathBuf::from(path);
                    if let Some(p) = path.parent() {
                        let _ = fs::create_dir_all(p);
                    }
                    let result = image::save_buffer(
                        &path,
                        &image.as_raw(),
                        image.size[0] as u32,
                        image.size[1] as u32,
                        image::ColorType::Rgba8,
                    );
                    match result {
                        Ok(()) => {
                            println!("Screenshot saved: {}", path.display());
                            ctx.send_viewport_cmd(ViewportCommand::Close);
                        }
                        Err(e) => {
                            eprintln!("Screenshot failed: {e}");
                            ctx.send_viewport_cmd(ViewportCommand::Close);
                        }
                    }
                }
            }
        } else if !self.tabs.is_empty() || self.http.is_some() || self.diagnostic_rx.is_some() {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
    }
}
fn theme_preference(appearance: Appearance) -> ThemePreference {
    match appearance {
        Appearance::System => ThemePreference::System,
        Appearance::Light => ThemePreference::Light,
        Appearance::Dark => ThemePreference::Dark,
    }
}
fn workspace_tab(
    ui: &mut Ui,
    title: &str,
    selected: bool,
    running: Option<bool>,
    closable: bool,
    id: u64,
) -> (Response, bool) {
    let font = FontId::proportional(13.);
    let galley = ui
        .painter()
        .layout_no_wrap(title.into(), font.clone(), ink(ui));
    let width = (galley.size().x + if closable { 64. } else { 30. }).clamp(76., 240.);
    let (rect, response) = ui.allocate_exact_size(vec2(width, 38.), Sense::click());
    let close_rect =
        Rect::from_center_size(pos2(rect.right() - 18., rect.center().y), vec2(24., 24.));
    let close = if closable {
        Some(ui.interact(close_rect, ui.id().with(("close-tab", id)), Sense::click()))
    } else {
        None
    };
    let fill = if selected {
        content(ui)
    } else if response.hovered() {
        ui.visuals().widgets.hovered.bg_fill
    } else {
        mist(ui)
    };
    ui.painter().rect_filled(
        rect,
        CornerRadius {
            nw: 7,
            ne: 7,
            sw: 0,
            se: 0,
        },
        fill,
    );
    if selected {
        ui.painter().line_segment(
            [
                rect.left_top() + vec2(8., 1.),
                rect.right_top() + vec2(-8., 1.),
            ],
            Stroke::new(2., accent(ui)),
        );
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect.shrink(3.),
            4.,
            Stroke::new(1., accent(ui)),
            StrokeKind::Inside,
        );
    }
    let text_left = if let Some(running) = running {
        ui.painter().circle_filled(
            pos2(rect.left() + 14., rect.center().y),
            3.,
            if running {
                Color32::from_rgb(20, 165, 115)
            } else {
                muted(ui)
            },
        );
        rect.left() + 26.
    } else {
        rect.left() + 14.
    };
    let text_right = if closable {
        close_rect.left() - 4.
    } else {
        rect.right() - 10.
    };
    ui.painter()
        .with_clip_rect(Rect::from_min_max(
            pos2(text_left, rect.top()),
            pos2(text_right, rect.bottom()),
        ))
        .text(
            pos2(text_left, rect.center().y),
            Align2::LEFT_CENTER,
            title,
            font,
            if selected { ink(ui) } else { muted(ui) },
        );
    let mut closed = false;
    if let Some(close) = close {
        if close.hovered() {
            ui.painter()
                .rect_filled(close_rect, 4., ui.visuals().widgets.hovered.bg_fill);
        }
        let c = close_rect.center();
        for (a, b) in [
            (vec2(-3., -3.), vec2(3., 3.)),
            (vec2(-3., 3.), vec2(3., -3.)),
        ] {
            ui.painter()
                .line_segment([c + a, c + b], Stroke::new(1.3, muted(ui)));
        }
        close.widget_info(|| {
            WidgetInfo::labeled(WidgetType::Button, true, format!("Close {title}"))
        });
        closed = close.on_hover_text(format!("Close {title}")).clicked();
    }
    response
        .widget_info(|| WidgetInfo::selected(WidgetType::SelectableLabel, true, selected, title));
    (response.on_hover_text(title), closed)
}
fn primary(ui: &mut Ui, label: &str) -> Response {
    ui.add(
        Button::new(RichText::new(label).color(if ui.visuals().dark_mode {
            Color32::from_rgb(6, 34, 24)
        } else {
            Color32::WHITE
        }))
        .fill(accent(ui))
        .stroke(Stroke::NONE)
        .corner_radius(6),
    )
}
fn surface(ui: &Ui) -> Frame {
    Frame::new()
        .fill(content(ui))
        .stroke(Stroke::new(1., line(ui)))
        .corner_radius(9)
        .inner_margin(17)
}
fn dot(ui: &mut Ui, color: Color32) {
    let (r, _) = ui.allocate_exact_size(vec2(7., 7.), Sense::hover());
    ui.painter().circle_filled(r.center(), 3., color);
}
fn tool_button(ui: &mut Ui, kind: &str, label: &str) -> Response {
    let (r, response) = ui.allocate_exact_size(vec2(56., 54.), Sense::click());
    if response.hovered() || response.has_focus() {
        ui.painter()
            .rect_filled(r, 4., ui.visuals().widgets.hovered.bg_fill);
        ui.painter()
            .rect_stroke(r, 4., Stroke::new(1., line(ui)), StrokeKind::Inside);
    }
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), label));
    draw_icon(
        ui.painter(),
        Rect::from_center_size(r.center_top() + vec2(0., 18.), vec2(23., 23.)),
        kind,
        if ui.is_enabled() {
            accent(ui)
        } else {
            Color32::from_gray(160)
        },
    );
    ui.painter().text(
        r.center_bottom() - vec2(0., 9.),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(10.5),
        if ui.is_enabled() {
            ink(ui)
        } else {
            Color32::from_gray(160)
        },
    );
    response
}
fn icon(ui: &mut Ui, kind: &str, size: f32, color: Color32) {
    let (r, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    draw_icon(ui.painter(), r, kind, color);
}
fn draw_icon(p: &Painter, r: Rect, kind: &str, color: Color32) {
    let q = |x: f32, y: f32| r.min + vec2(x * r.width() / 24., y * r.height() / 24.);
    let stroke = Stroke::new(1.6, color);
    let line = |a: (f32, f32), b: (f32, f32)| {
        p.line_segment([q(a.0, a.1), q(b.0, b.1)], stroke);
    };
    match kind {
        "exit" => {
            p.add(PathShape::line(
                vec![q(10., 3.), q(4., 3.), q(4., 21.), q(10., 21.)],
                stroke,
            ));
            line((9., 12.), (22., 12.));
            line((17., 7.), (22., 12.));
            line((22., 12.), (17., 17.));
        }
        "help" => {
            p.circle_stroke(q(12., 12.), r.width() * 0.43, stroke);
            p.text(
                q(12., 12.),
                Align2::CENTER_CENTER,
                "?",
                FontId::proportional(r.height() * 0.8),
                color,
            );
        }
        "games" => {
            p.rect_stroke(
                Rect::from_min_max(q(2., 6.), q(22., 19.)),
                4.,
                stroke,
                StrokeKind::Inside,
            );
            line((5., 12.), (11., 12.));
            line((8., 9.), (8., 15.));
            p.circle_filled(q(16., 10.), r.width() * 0.055, color);
            p.circle_filled(q(19., 14.), r.width() * 0.055, color);
        }
        "terminal" => {
            p.rect_stroke(
                Rect::from_min_max(q(2., 4.), q(22., 20.)),
                3.,
                stroke,
                StrokeKind::Inside,
            );
            line((6., 9.), (10., 12.));
            line((10., 12.), (6., 15.));
            line((13., 15.), (18., 15.));
        }
        "folder" => {
            let pts = vec![
                q(2., 7.),
                q(2., 20.),
                q(22., 20.),
                q(22., 7.),
                q(11., 7.),
                q(9., 4.),
                q(2., 4.),
                q(2., 7.),
            ];
            p.add(PathShape::closed_line(pts, stroke));
        }
        "desktop" => {
            p.rect_stroke(
                Rect::from_min_max(q(2., 3.), q(22., 17.)),
                2.,
                stroke,
                StrokeKind::Inside,
            );
            line((12., 17.), (12., 21.));
            line((7., 21.), (17., 21.));
        }
        "lock" => {
            p.rect_stroke(
                Rect::from_min_max(q(5., 10.), q(19., 22.)),
                2.,
                stroke,
                StrokeKind::Inside,
            );
            p.add(PathShape::line(
                vec![
                    q(8., 10.),
                    q(8., 5.),
                    q(10., 3.),
                    q(14., 3.),
                    q(16., 5.),
                    q(16., 10.),
                ],
                stroke,
            ));
            p.circle_filled(q(12., 15.), 1.4, color);
            line((12., 15.), (12., 18.));
        }
        "link" => {
            p.rect_stroke(
                Rect::from_min_max(q(2., 3.), q(10., 11.)),
                3.,
                stroke,
                StrokeKind::Inside,
            );
            p.rect_stroke(
                Rect::from_min_max(q(14., 13.), q(22., 21.)),
                3.,
                stroke,
                StrokeKind::Inside,
            );
            line((8., 9.), (16., 15.));
        }
        "home" => {
            line((2., 11.), (12., 3.));
            line((12., 3.), (22., 11.));
            p.add(PathShape::line(
                vec![q(5., 9.), q(5., 21.), q(19., 21.), q(19., 9.)],
                stroke,
            ));
            line((10., 21.), (10., 14.));
            line((10., 14.), (14., 14.));
            line((14., 14.), (14., 21.));
        }
        "serial" => {
            p.rect_stroke(
                Rect::from_min_max(q(3., 6.), q(21., 18.)),
                3.,
                stroke,
                StrokeKind::Inside,
            );
            for i in 0..3 {
                p.circle_filled(q(7. + i as f32 * 5., 12.), 1., color);
            }
            line((8., 3.), (8., 6.));
            line((16., 3.), (16., 6.));
            line((8., 18.), (8., 21.));
            line((16., 18.), (16., 21.));
        }
        "macro" => {
            p.add(PathShape::closed_line(
                vec![q(7., 3.), q(21., 12.), q(7., 21.)],
                stroke,
            ));
        }
        "settings" => {
            p.circle_stroke(r.center(), r.width() * 0.30, stroke);
            p.circle_stroke(r.center(), r.width() * 0.10, stroke);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.;
                let v = vec2(a.cos(), a.sin());
                p.line_segment(
                    [
                        r.center() + v * r.width() * 0.31,
                        r.center() + v * r.width() * 0.45,
                    ],
                    stroke,
                );
            }
        }
        _ => {
            line((5., 3.), (21., 19.));
            line((3., 5.), (19., 21.));
            line((19., 3.), (3., 19.));
            p.circle_stroke(q(7., 17.), 2., stroke);
        }
    }
}

impl eframe::App for Workspace {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.stop_plugin_job();
        #[cfg(windows)]
        {
            for tab in &mut self.tabs {
                drop(tab.web_browser.take());
            }
            harbor_workspace::browser::wait_for_shutdown();
        }
        self.xserver.shutdown();
    }

    fn ui(&mut self, root: &mut Ui, _frame: &mut eframe::Frame) {
        self.render(root);
    }
    fn logic(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.poll_session_import(ctx);
        self.poll_shared_sources(ctx);
        self.poll_dispatch(ctx);
        self.poll_vault(ctx);
        self.poll_ssh_keys(ctx);
        self.poll_plugins(ctx);
        self.poll_xserver_sessions(ctx);
        self.poll_desktop_certificates();
        self.intercept_remote_close(ctx);
        self.intercept_session_close(ctx);
        self.apply_appearance(ctx);
        if self.screenshot.is_some() {
            ctx.request_repaint();
        }
        self.update_activity_visibility(ctx);
        self.pump_terminals();
        self.poll_clipboard();
        self.logic_polled = true;
    }
}
#[cfg(test)]
mod gui_tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn scp_pane_never_identifies_itself_as_sftp() {
        let _process_slot = terminal_review_tests::owned_process_fixture_slot();
        use harbor_workspace::remote_files::capabilities::RemoteBackendKind;
        let server = remote_fixture::SshServer::start(false);
        let mut config = server.config(99, true);
        config.session.remote_backend = RemoteBackendKind::Scp;
        let data = server.root.join("scp-ui-data");
        fs::create_dir_all(data.join("ssh")).unwrap();
        fs::copy(&config.app_known_hosts, data.join("ssh/known_hosts")).unwrap();
        let ctx = Context::default();
        let mut w = Workspace::new_context(&ctx, None, data);
        w.start(config.session);
        let deadline = Instant::now() + Duration::from_secs(8);
        while w.tabs[0].browser.as_ref().unwrap().loading {
            w.tabs[0].poll();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1360., 880.))),
                ..Default::default()
            },
            |ui| w.render(ui),
        );
        fn text(shape: &egui::epaint::Shape, result: &mut String) {
            match shape {
                egui::epaint::Shape::Text(t) => {
                    result.push_str(t.galley.text());
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
        output.textures_delta.clear();
        let mut labels = String::new();
        for shape in output.shapes {
            text(&shape.shape, &mut labels);
        }
        assert!(labels.contains("SCP"), "Transport label missing");
        assert!(
            !labels.contains("Embedded SFTP"),
            "Explicit SCP pane claimed another transport: {labels}"
        );
        let remote = server.root.join("scp-editor-ui.txt");
        fs::write(&remote, b"editor UI").unwrap();
        w.open_remote_editor(
            w.tabs[0].id,
            harbor_workspace::remote_files::RemotePath::from_bytes(
                remote.to_str().unwrap().as_bytes().to_vec(),
            )
            .unwrap(),
        );
        assert_eq!(
            w.remote_editors.len(),
            1,
            "Negotiated SCP editing must be usable from the pane"
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while w.remote_editors[0].document.details().0
            != harbor_workspace::remote_files::editor::DocumentStatus::Ready
        {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(w.remote_editors[0].document.snapshot().text, "editor UI");
        frame(&ctx, &mut w, vec![]);
    }
    #[cfg(unix)]
    #[test]
    fn dirty_remote_editor_blocks_tab_close_and_reconnect() {
        use harbor_workspace::remote_files::{RemotePath, editor::DocumentStatus};
        let server = remote_fixture::SshServer::start(true);
        let config = server.config(99, true);
        let data = server.root.join("editor-ui-data");
        fs::create_dir_all(data.join("ssh")).unwrap();
        fs::copy(&config.app_known_hosts, data.join("ssh/known_hosts")).unwrap();
        let ctx = Context::default();
        let mut w = Workspace::new_context(&ctx, None, data);
        w.start(config.session);
        let deadline = Instant::now() + Duration::from_secs(8);
        while w.tabs[0].browser.as_ref().unwrap().loading {
            w.tabs[0].poll();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        let remote = server.root.join("edited.txt");
        fs::write(&remote, b"v1").unwrap();
        let path = RemotePath::from_bytes(remote.to_str().unwrap().as_bytes().to_vec()).unwrap();
        w.open_remote_editor(w.tabs[0].id, path);
        let deadline = Instant::now() + Duration::from_secs(8);
        while w.remote_editors[0].document.snapshot().status != DocumentStatus::Ready {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        w.remote_editors[0].document.set_text("mine").unwrap();
        let connection = w.tabs[0].id;
        w.close_tab(0);
        assert_eq!(w.tabs.len(), 1);
        assert_eq!(w.pending_remote_tab_close, Some(connection));
        w.pending_remote_tab_close = None; // Cancel the close prompt.
        w.reconnect_active();
        assert_eq!(w.tabs[0].id, connection);
        assert_eq!(w.remote_editors[0].document.snapshot().text, "mine");
        frame(&ctx, &mut w, vec![]);
        assert_eq!(fs::read(remote).unwrap(), b"v1");
        let mut raw = RawInput::default();
        raw.viewports
            .get_mut(&ViewportId::ROOT)
            .unwrap()
            .events
            .push(ViewportEvent::Close);
        let output = ctx.run_ui(raw, |ui| {
            eframe::App::logic(&mut w, ui.ctx(), &mut eframe::Frame::_new_kittest());
        });
        assert!(
            output.viewport_output[&ViewportId::ROOT]
                .commands
                .iter()
                .any(|command| matches!(command, ViewportCommand::CancelClose)),
            "A minimized-window logic-only close must retain dirty remote text"
        );
        assert!(w.pending_remote_exit);
        let first_id = w.remote_editors[0].id;
        let second = server.root.join("second.txt");
        fs::write(&second, b"second document").unwrap();
        w.open_remote_editor(
            connection,
            RemotePath::from_bytes(second.to_str().unwrap().as_bytes().to_vec()).unwrap(),
        );
        let second_id = w.remote_editors[1].id;
        assert_ne!(first_id, second_id);
        w.remote_editors.remove(0);
        frame(&ctx, &mut w, vec![]);
        assert_eq!(
            w.remote_editors[0].id, second_id,
            "A surviving editor must keep its own undo identity when its vector index changes"
        );
        w.pending_remote_exit = false; // Cancel the application-close prompt.
        let deadline = Instant::now() + Duration::from_secs(8);
        while w.remote_editors[0].document.details().0 != DocumentStatus::Ready {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        w.remote_editors[0]
            .document
            .set_text("second mine")
            .unwrap();
        w.tabs[0].ssh.as_ref().unwrap().cancel();
        let deadline = Instant::now() + Duration::from_secs(8);
        while !w.tabs[0].ssh.as_ref().unwrap().is_finished() {
            w.tabs[0].poll();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        w.reconnect_active();
        assert_ne!(
            w.tabs[0].id, connection,
            "A disconnected tab must reconnect while retaining its dirty recovery editor"
        );
        assert_eq!(w.remote_editors[0].document.snapshot().text, "second mine");
        let deadline = Instant::now() + Duration::from_secs(8);
        while !w.tabs[0].ssh_connected {
            w.tabs[0].poll();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        w.remote_editors[0].recover_requested = true;
        frame(&ctx, &mut w, vec![]);
        let target_id = w.remote_editors[1].id;
        assert_eq!(w.pending_remote_import, Some((second_id, target_id)));
        let deadline = Instant::now() + Duration::from_secs(8);
        while w.remote_editors[1].document.details().0 != DocumentStatus::Ready {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            w.remote_editors[1]
                .document
                .can_import_from(&w.remote_editors[0].document)
        );
        assert_eq!(fs::read(second).unwrap(), b"second document");
    }
    fn frame(ctx: &Context, w: &mut Workspace, events: Vec<Event>) {
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1360., 880.))),
                events,
                ..Default::default()
            },
            |ui| w.render(ui),
        );
        output.textures_delta.clear();
    }
    #[test]
    fn all_workspace_pages_and_dialogs_render() {
        let ctx = Context::default();
        let mut w = Workspace::new_context(
            &ctx,
            None,
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/gui-test"),
        );
        for page in [
            Page::Home,
            Page::Files,
            Page::Tunnels,
            Page::Macros,
            Page::Servers,
            Page::Tools,
            Page::Settings,
        ] {
            w.page = page;
            frame(&ctx, &mut w, vec![]);
        }
        for protocol in PROTOCOLS {
            w.new_session(protocol);
            frame(&ctx, &mut w, vec![]);
            assert_eq!(&w.session_form.as_ref().unwrap().protocol, protocol);
        }
        w.session_form = None;
        w.tunnel_open = true;
        frame(&ctx, &mut w, vec![]);
        w.tunnel_open = false;
        w.macro_open = true;
        frame(&ctx, &mut w, vec![]);
        w.macro_open = false;
        w.parity_open = true;
        frame(&ctx, &mut w, vec![]);
        w.store.preferences.sidebar_visible = false;
        frame(&ctx, &mut w, vec![]);
    }
    #[cfg(unix)]
    #[allow(dead_code)]
    mod remote_fixture {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/ssh_server.rs"
        ));
    }
    #[cfg(unix)]
    #[test]
    fn native_drop_on_local_zone_does_not_upload() {
        struct Dropped(PathBuf);
        impl std::fmt::Debug for Dropped {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("FixtureDroppedFile")
            }
        }
        impl egui::DroppedFile for Dropped {
            fn path(&self) -> &std::path::Path {
                &self.0
            }
            fn bytes(&self) -> Result<Vec<u8>, String> {
                fs::read(&self.0).map_err(|e| e.to_string())
            }
        }
        fn text_position(shape: &egui::epaint::Shape, needle: &str) -> Option<Pos2> {
            match shape {
                egui::epaint::Shape::Text(t) if t.galley.text() == needle => {
                    Some(t.pos + vec2(10., 5.))
                }
                egui::epaint::Shape::Vec(shapes) => {
                    shapes.iter().find_map(|shape| text_position(shape, needle))
                }
                _ => None,
            }
        }
        let server = remote_fixture::SshServer::start(true);
        let remote = server.root.join("remote-target");
        fs::create_dir(&remote).unwrap();
        let local = server.root.join("local-drop-sentinel");
        fs::write(&local, b"private-fixture").unwrap();
        let mut config = server.config(99, true);
        config.session.directory = remote.to_string_lossy().into();
        let data = server.root.join("ui-data");
        fs::create_dir_all(data.join("ssh")).unwrap();
        fs::copy(&config.app_known_hosts, data.join("ssh/known_hosts")).unwrap();
        let ctx = Context::default();
        let mut workspace = Workspace::new_context(&ctx, None, data);
        workspace.start(config.session);
        let deadline = Instant::now() + Duration::from_secs(6);
        while workspace.tabs[0].browser.as_ref().unwrap().loading {
            workspace.tabs[0].poll();
            assert!(Instant::now() < deadline, "UI fixture did not connect");
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1360., 1000.))),
                ..Default::default()
            },
            |ui| workspace.render(ui),
        );
        let point = output
            .shapes
            .iter()
            .find_map(|shape| text_position(&shape.shape, "Local download folder"))
            .expect("Local download zone must be visible");
        let upload_point = output
            .shapes
            .iter()
            .find_map(|shape| {
                text_position(&shape.shape, "Drop local files or folders here to upload.")
            })
            .expect("Remote upload zone must be visible");
        output.textures_delta.clear();
        let mut drop_output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1360., 1000.))),
                events: vec![Event::PointerMoved(point)],
                dropped_files: vec![std::sync::Arc::new(Dropped(local.clone()))],
                ..Default::default()
            },
            |ui| workspace.render(ui),
        );
        drop_output.textures_delta.clear();
        let deadline = Instant::now() + Duration::from_millis(500);
        while Instant::now() < deadline {
            workspace.tabs[0].poll();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            !remote.join("local-drop-sentinel").exists(),
            "Native drop on the LOCAL zone uploaded data to SSH"
        );
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1360., 1000.))),
                events: vec![Event::PointerMoved(upload_point)],
                dropped_files: vec![std::sync::Arc::new(Dropped(local))],
                ..Default::default()
            },
            |ui| workspace.render(ui),
        );
        output.textures_delta.clear();
        let deadline = Instant::now() + Duration::from_secs(3);
        while !remote.join("local-drop-sentinel").exists() {
            workspace.tabs[0].poll();
            assert!(
                Instant::now() < deadline,
                "Drop onto the explicit upload zone did not upload"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            fs::read(remote.join("local-drop-sentinel")).unwrap(),
            b"private-fixture"
        );
    }
    #[cfg(unix)]
    #[test]
    fn ssh_directory_notices_follow_only_after_opt_in() {
        use harbor_workspace::{
            remote_files::BrowserFollowMode,
            ssh::shell_integration::{ShellFlavor, integration_script},
        };
        fn quoted(path: &std::path::Path) -> String {
            format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"))
        }
        let server = remote_fixture::SshServer::start(true);
        let child = server.root.join("shell child ' é");
        fs::create_dir(&child).unwrap();
        let child = fs::canonicalize(child).unwrap();
        let mut config = server.config(99, true);
        config.session.command =
            "exec /usr/bin/env -u PROMPT_COMMAND -u BASH_ENV /bin/bash --noprofile --norc -i"
                .into();
        let command = config.session.command.clone();
        let data = server.root.join("follow-ui-data");
        fs::create_dir_all(data.join("ssh")).unwrap();
        fs::copy(&config.app_known_hosts, data.join("ssh/known_hosts")).unwrap();
        let ctx = Context::default();
        let mut workspace = Workspace::new_context(&ctx, None, data);
        workspace.start(config.session);
        let deadline = Instant::now() + Duration::from_secs(8);
        while workspace.tabs[0].browser.as_ref().unwrap().loading {
            workspace.tabs[0].poll();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        let initial = workspace.tabs[0].browser.as_ref().unwrap().path.clone();
        let script = integration_script(ShellFlavor::Bash, "127.0.0.1").unwrap();
        workspace.tabs[0].send(
            format!(
                "cd -- {}
{script}
cd -- {}
",
                quoted(&server.root),
                quoted(&child)
            )
            .as_bytes(),
        );
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            workspace.tabs[0].poll();
            let latest = workspace.tabs[0]
                .directory_tracker
                .as_ref()
                .unwrap()
                .last_directory();
            if latest.is_some_and(|p| p.as_bytes() == child.to_str().unwrap().as_bytes()) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "No shell directory notice: {}",
                workspace.tabs[0].error
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(workspace.tabs[0].browser.as_ref().unwrap().path, initial);
        workspace.tabs[0]
            .browser
            .as_mut()
            .unwrap()
            .set_follow_mode(BrowserFollowMode::FollowShell);
        let path = workspace.tabs[0]
            .directory_tracker
            .as_ref()
            .unwrap()
            .last_directory()
            .unwrap()
            .clone();
        workspace.tabs[0].follow_shell_directory(path);
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            workspace.tabs[0].poll();
            let browser = workspace.tabs[0].browser.as_ref().unwrap();
            if !browser.loading && browser.path.as_bytes() == child.to_str().unwrap().as_bytes() {
                break;
            }
            assert!(Instant::now() < deadline, "Follow did not navigate");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(server.accepted_authentications(), 1);
        assert_eq!(workspace.tabs[0].session.command, command);
    }
    #[test]
    fn embedded_ssh_creates_browser_even_when_sidebar_collapsed() {
        let ctx = Context::default();
        let mut w = Workspace::new_context(
            &ctx,
            None,
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/ssh-pane-test"),
        );
        w.store.preferences.sidebar_visible = false;
        w.start(Session {
            host: "127.0.0.1".into(),
            port: 1,
            ..Default::default()
        });
        assert!(w.tabs[0].ssh.is_some());
        assert!(w.tabs[0].browser.as_ref().unwrap().visible);
        for appearance in [Appearance::Light, Appearance::Dark] {
            w.store.preferences.appearance = appearance;
            frame(&ctx, &mut w, vec![]);
            assert!(w.tabs[0].browser.as_ref().unwrap().visible);
        }
    }
    #[test]
    fn unauthenticated_terminal_input_cannot_be_recorded_or_queued() {
        let ctx = Context::default();
        let mut w = Workspace::new_context(
            &ctx,
            None,
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/ssh-input-test"),
        );
        w.start(Session {
            host: "127.0.0.1".into(),
            port: 1,
            ..Default::default()
        });
        let tab = &mut w.tabs[0];
        tab.recording = true;
        tab.send(b"SENTINEL_PASSWORD");
        assert!(tab.recorded.is_empty());
        assert!(!tab.error.contains("SENTINEL_PASSWORD"));
    }
    #[test]
    fn appearance_tracks_system_and_honors_overrides() {
        let ctx = Context::default();
        let mut w = Workspace::new_context(
            &ctx,
            None,
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/theme-test"),
        );
        assert_eq!(w.store.preferences.appearance, Appearance::System);
        for (appearance, system, expected) in [
            (Appearance::System, Theme::Dark, true),
            (Appearance::System, Theme::Light, false),
            (Appearance::Light, Theme::Dark, false),
            (Appearance::Dark, Theme::Light, true),
        ] {
            w.store.preferences.appearance = appearance;
            let mut output = ctx.run_ui(
                RawInput {
                    system_theme: Some(system),
                    ..Default::default()
                },
                |ui| {
                    w.apply_appearance(ui.ctx());
                    assert_eq!(ui.ctx().theme() == Theme::Dark, expected);
                },
            );
            output.textures_delta.clear();
        }
    }
    #[test]
    fn system_appearance_without_native_theme_defaults_to_light() {
        let ctx = Context::default();
        let mut w = Workspace::new_context(
            &ctx,
            None,
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/theme-fallback-test"),
        );
        for appearance in [Appearance::Dark, Appearance::System] {
            w.store.preferences.appearance = appearance;
            let mut output = ctx.run_ui(RawInput::default(), |ui| {
                w.apply_appearance(ui.ctx());
                assert_eq!(
                    ui.ctx().theme(),
                    if appearance == Appearance::Dark {
                        Theme::Dark
                    } else {
                        Theme::Light
                    }
                );
            });
            output.textures_delta.clear();
        }
    }
    #[test]
    fn integrated_tab_close_does_not_select_the_tab() {
        let ctx = Context::default();
        let mut rect = Rect::NOTHING;
        let mut selected = false;
        let mut closed = false;
        for step in 0..3 {
            let pos = pos2(rect.right() - 18., rect.center().y);
            let events = if step == 0 {
                vec![]
            } else {
                vec![
                    Event::PointerMoved(pos),
                    Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed: step == 1,
                        modifiers: Modifiers::NONE,
                    },
                ]
            };
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(600., 400.))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    let (response, close) =
                        workspace_tab(ui, "Local terminal", false, Some(true), true, 1);
                    rect = response.rect;
                    selected |= response.clicked();
                    closed |= close;
                },
            );
            output.textures_delta.clear();
        }
        assert!(closed);
        assert!(!selected);
    }
    #[cfg(unix)]
    #[test]
    fn disconnected_serial_aborts_pending_broadcast_before_other_target_writes() {
        use std::os::fd::{AsRawFd, FromRawFd};
        let mut master = -1;
        let mut slave = -1;
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut master,
                    &mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    std::ptr::null(),
                )
            },
            0
        );
        let master = unsafe { fs::File::from_raw_fd(master) };
        let slave = unsafe { fs::File::from_raw_fd(slave) };
        unsafe {
            assert_eq!(
                libc::fcntl(master.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC),
                0
            );
            assert_eq!(
                libc::fcntl(slave.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC),
                0
            );
        }
        let mut name = [0i8; 256];
        assert_eq!(
            unsafe { libc::ttyname_r(slave.as_raw_fd(), name.as_mut_ptr(), name.len()) },
            0
        );
        let name = unsafe { std::ffi::CStr::from_ptr(name.as_ptr()) }
            .to_str()
            .unwrap()
            .to_owned();
        let ctx = Context::default();
        let root = tempfile::tempdir().unwrap();
        let mut w = Workspace::new_context(&ctx, None, root.path().to_owned());
        w.start(Session {
            protocol: "Serial".into(),
            serial_device: name,
            baud: 9600,
            ..Default::default()
        });
        assert!(w.tabs[0].running());
        w.start_with_launch(Session{protocol:"Local".into(),..Default::default()},Some(connection::Launch{rdp_profile:None,program:"/usr/bin/python3".into(),args:vec!["-c".into(),"import os,tty\ntty.setraw(0)\nos.write(1,b'READY')\ndata=os.read(0,4096)\nos.write(1,b'CAP:'+data.hex().encode())\nos.read(0,1)".into()],directory:None}));
        let deadline = Instant::now() + Duration::from_secs(3);
        while !w.tabs[1].screen.plain().contains("READY") {
            w.tabs[1].poll();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        w.enable_broadcast_all();
        w.queue_paste(w.tabs[1].id, "one\ntwo".into());
        assert!(w.pending_paste.is_some());
        drop(master);
        drop(slave);
        // Observe OS hangup before asserting zero-budget processing. Concurrent
        // fork/exec fixtures can briefly inherit the PTY before CLOEXEC runs.
        let deadline = Instant::now() + Duration::from_secs(3);
        while !w.tabs[0].serial.as_ref().unwrap().disconnected() {
            assert!(Instant::now() < deadline, "PTY hangup was not observed");
            std::thread::sleep(Duration::from_millis(5));
        }
        w.tabs[0].poll_with_budget(0);
        assert!(
            !w.tabs[0].ready_for_input(),
            "disconnect readiness must not wait for the output parse budget"
        );
        let deadline = Instant::now() + Duration::from_secs(3);
        while !w.tabs[0].output_ended && Instant::now() < deadline {
            w.tabs[0].poll();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(w.tabs[0].output_ended);
        assert!(
            !w.tabs[0].running(),
            "observed serial disconnect must retire readiness"
        );
        w.resolve_paste(true);
        assert!(w.tabs[1].paste.is_none());
        w.tabs[1].tick_paste(Instant::now());
        w.tabs[1].poll();
        assert!(!w.tabs[1].screen.plain().contains("CAP:"));
    }
    #[cfg(unix)]
    #[test]
    fn generated_sftp_filename_keeps_native_bytes_outside_keyboard_codec() {
        use harbor_workspace::terminal::settings::{CharacterEncoding, TerminalOverrides};
        let ctx = Context::default();
        let root = tempfile::tempdir().unwrap();
        let mut w = Workspace::new_context(&ctx, None, root.path().to_owned());
        let program = "import os,tty\ntty.setraw(0)\nos.write(1,b'READY')\ndata=os.read(0,4096)\nos.write(1,b'CAP:'+data.hex().encode())\nos.read(0,1)";
        w.start_with_launch(
            Session {
                protocol: "SFTP".into(),
                terminal: TerminalOverrides {
                    encoding: Some(CharacterEncoding::Windows1252),
                    ..Default::default()
                },
                ..Default::default()
            },
            Some(connection::Launch {
                rdp_profile: None,
                program: "/usr/bin/python3".into(),
                args: vec!["-c".into(), program.into()],
                directory: None,
            }),
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        while !w.tabs[0].screen.plain().contains("READY") {
            w.tabs[0].poll();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        let path = root.path().join("café.txt");
        fs::write(&path, b"owned").unwrap();
        let mut command = b"put -p ".to_vec();
        command.extend(connection::sftp_quote_path(&path).unwrap());
        command.push(b'\n');
        w.tabs[0].send_generated_command(&command);
        let deadline = Instant::now() + Duration::from_secs(2);
        while !w.tabs[0].screen.plain().contains("CAP:") {
            w.tabs[0].poll();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            w.tabs[0].screen.plain().contains("636166c3a92e747874"),
            "{}",
            w.tabs[0].screen.plain()
        );
    }
    #[cfg(unix)]
    #[test]
    fn new_remote_revision_blocks_stale_active_window_close_approval() {
        use harbor_workspace::remote_files::{
            RemotePath,
            editor::{DocumentStatus, SavePolicy},
        };
        let server = remote_fixture::SshServer::start(true);
        let config = server.config(99, true);
        let data = server.root.join("stale-close-data");
        fs::create_dir_all(data.join("ssh")).unwrap();
        fs::copy(&config.app_known_hosts, data.join("ssh/known_hosts")).unwrap();
        let ctx = Context::default();
        let mut w = Workspace::new_context(&ctx, None, data);
        w.start(config.session);
        let deadline = Instant::now() + Duration::from_secs(8);
        while w.tabs[0].browser.as_ref().unwrap().loading {
            w.tabs[0].poll();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        let remote = server.root.join("revision.txt");
        fs::write(&remote, b"v1").unwrap();
        w.open_remote_editor(
            w.tabs[0].id,
            RemotePath::from_bytes(remote.as_os_str().as_encoded_bytes().to_vec()).unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(8);
        while w.remote_editors[0].document.details().0 != DocumentStatus::Ready {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        w.remote_editors[0].document.set_text("v2").unwrap();
        w.remote_editors[0]
            .document
            .save(SavePolicy::VerifyUnchanged)
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(8);
        while w.remote_documents_block_close(None) {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        w.pending_remote_exit = true;
        let mut output = ctx.run_ui(RawInput::default(), |ui| w.remote_editor_windows(ui.ctx()));
        output.textures_delta.clear();
        assert!(w.allow_remote_exit);
        let mut raw = RawInput::default();
        raw.viewports
            .get_mut(&ViewportId::ROOT)
            .unwrap()
            .events
            .push(ViewportEvent::Close);
        let mut output = ctx.run_ui(raw, |ui| {
            eframe::App::logic(&mut w, ui.ctx(), &mut eframe::Frame::_new_kittest())
        });
        output.textures_delta.clear();
        assert!(w.pending_session_close.is_some());
        w.remote_editors[0].document.set_text("new v3").unwrap();
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            w.resolve_session_close(true, ui.ctx())
        });
        output.textures_delta.clear();
        assert!(
            !output
                .viewport_output
                .values()
                .any(|v| v.commands.contains(&ViewportCommand::Close)),
            "stale approval must not close a newer dirty revision"
        );
        assert!(w.pending_remote_exit);
        assert!(!w.allow_remote_exit);
        assert_eq!(w.remote_editors[0].document.snapshot().text, "new v3");
        assert_eq!(fs::read(&remote).unwrap(), b"v2");
    }
    #[cfg(unix)]
    #[test]
    fn selected_legacy_transport_decodes_once_and_encodes_keyboard_input() {
        use harbor_workspace::terminal::settings::{
            CharacterEncoding, Dimensions, TerminalOverrides,
        };
        let ctx = Context::default();
        let root = tempfile::tempdir().unwrap();
        let mut w = Workspace::new_context(&ctx, None, root.path().to_owned());
        let session = Session {
            protocol: "Local".into(),
            terminal: TerminalOverrides {
                encoding: Some(CharacterEncoding::Windows1252),
                dimensions: Some(Dimensions::Fixed {
                    cols: 132,
                    rows: 40,
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        let program = "import os,tty\ntty.setraw(0)\nos.write(1,b'\\x80READY')\ndata=os.read(0,1024)\nos.write(1,b'CAP:'+data.hex().encode())\nos.read(0,1)";
        w.start_with_launch(
            session,
            Some(connection::Launch {
                rdp_profile: None,
                program: "/usr/bin/python3".into(),
                args: vec!["-c".into(), program.into()],
                directory: None,
            }),
        );
        let log = root.path().join("owned-wire.log");
        w.tabs[0].log = Some(fs::File::create(&log).unwrap());
        let deadline = Instant::now() + Duration::from_secs(3);
        while !w.tabs[0].screen.plain().contains("READY") && Instant::now() < deadline {
            w.tabs[0].poll();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            w.tabs[0].screen.plain().contains("€READY"),
            "{}",
            w.tabs[0].screen.plain()
        );
        assert_eq!(w.tabs[0].screen.cols, 132);
        assert_eq!(w.tabs[0].screen.rows, 40);
        assert!(fs::read(&log).unwrap().starts_with(&[0x80]));
        w.tabs[0].send("€".as_bytes());
        let deadline = Instant::now() + Duration::from_secs(2);
        while !w.tabs[0].screen.plain().contains("CAP:") && Instant::now() < deadline {
            w.tabs[0].poll();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(w.tabs[0].screen.plain().contains("CAP:80"));
        assert_eq!(w.tabs[0].screen.plain().matches('€').count(), 1);
    }
    #[cfg(unix)]
    #[test]
    fn broadcast_paste_cancel_and_stale_target_send_no_payload() {
        let ctx = Context::default();
        let root = tempfile::tempdir().unwrap();
        let mut w = Workspace::new_context(&ctx, None, root.path().to_owned());
        let program = "import os,tty\ntty.setraw(0)\nos.write(1,b'\\x1b[?2004hREADY')\nwhile True:\n data=os.read(0,1024)\n if not data: break\n os.write(1,b'CAP:'+data.hex().encode()+b'\\r\\n')";
        for name in ["owned-one", "owned-two"] {
            w.start_with_launch(
                Session {
                    name: name.into(),
                    protocol: "Local".into(),
                    ..Default::default()
                },
                Some(connection::Launch {
                    rdp_profile: None,
                    program: "/usr/bin/python3".into(),
                    args: vec!["-c".into(), program.into()],
                    directory: None,
                }),
            );
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        while w.tabs.iter().any(|t| !t.screen.plain().contains("READY"))
            && Instant::now() < deadline
        {
            for tab in &mut w.tabs {
                tab.poll();
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(w.tabs.iter().all(|t| t.screen.bracketed_paste));
        w.enable_broadcast_all();
        let source = w.tabs[0].id;
        w.queue_paste(source, "one\ntwo".into());
        assert!(w.pending_paste.is_some());
        w.resolve_paste(false);
        for tab in &mut w.tabs {
            tab.tick_paste(Instant::now());
            tab.poll();
            assert!(!tab.screen.plain().contains("CAP:"));
            assert!(!tab.input_busy());
        }
        w.queue_paste(source, "one\ntwo".into());
        w.force_close_tab(1);
        w.resolve_paste(true);
        assert!(w.tabs[0].paste.is_none());
        assert!(w.tabs[0].running());
        w.tabs[0].poll();
        assert!(!w.tabs[0].screen.plain().contains("CAP:"));
        w.store.preferences.paste.line_delay_ms = 100;
        w.tabs[0].screen.bracketed_paste = false;
        w.queue_paste(source, "one\ntwo".into());
        w.tabs[0].screen.bracketed_paste = true;
        w.resolve_paste(true);
        let start = Instant::now();
        w.tabs[0].tick_paste(start);
        w.tabs[0].send(b"WRONG");
        let deadline = Instant::now() + Duration::from_secs(2);
        while !w.tabs[0].screen.plain().contains("CAP:") && Instant::now() < deadline {
            w.tabs[0].poll();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            w.tabs[0].screen.plain().contains("1b5b3230307e6f6e650a"),
            "{}",
            w.tabs[0].screen.plain()
        );
        w.tabs[0].tick_paste(start + Duration::from_millis(99));
        assert!(w.tabs[0].paste.is_some());
        w.tabs[0].tick_paste(start + Duration::from_millis(100));
        let deadline = Instant::now() + Duration::from_secs(2);
        while !w.tabs[0].screen.plain().contains("74776f1b5b3230317e") && Instant::now() < deadline
        {
            w.tabs[0].poll();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(w.tabs[0].screen.plain().contains("74776f1b5b3230317e"));
        assert!(!w.tabs[0].screen.plain().contains("57524f4e47"));
        let mut raw = RawInput::default();
        raw.viewports
            .get_mut(&ViewportId::ROOT)
            .unwrap()
            .events
            .push(ViewportEvent::Close);
        let mut output = ctx.run_ui(raw, |ui| {
            eframe::App::logic(&mut w, ui.ctx(), &mut eframe::Frame::_new_kittest());
        });
        output.textures_delta.clear();
        assert!(
            output.viewport_output[&ViewportId::ROOT]
                .commands
                .contains(&ViewportCommand::CancelClose)
        );
        w.resolve_session_close(false, &ctx);
        assert!(w.tabs[0].running());
    }
    #[cfg(unix)]
    #[test]
    fn cancel_active_tab_close_preserves_real_process() {
        let ctx = Context::default();
        let root = tempfile::tempdir().unwrap();
        let mut w = Workspace::new_context(&ctx, None, root.path().to_owned());
        w.start_with_launch(
            Session {
                protocol: "Local".into(),
                ..Default::default()
            },
            Some(connection::Launch {
                rdp_profile: None,
                program: "/bin/sh".into(),
                args: vec![
                    "-c".into(),
                    "printf READY; read answer; printf 'REPLY:%s' \"$answer\"; read hold".into(),
                ],
                directory: None,
            }),
        );
        assert!(w.tabs[0].running());
        let id = w.tabs[0].id;
        w.close_tab(0);
        assert_eq!(w.tabs.len(), 1, "active close must await confirmation");
        w.resolve_session_close(false, &ctx);
        assert_eq!(w.tabs[0].id, id);
        assert!(w.tabs[0].running());
        w.tabs[0].send(b"STILL_ALIVE\n");
        let deadline = Instant::now() + Duration::from_secs(2);
        while !w.tabs[0].screen.plain().contains("REPLY:STILL_ALIVE") && Instant::now() < deadline {
            w.tabs[0].poll();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(w.tabs[0].screen.plain().contains("REPLY:STILL_ALIVE"));
        w.reconnect_active();
        assert_eq!(
            w.tabs[0].id, id,
            "reconnect must await active-session approval"
        );
        w.resolve_session_close(false, &ctx);
        assert_eq!(w.tabs[0].id, id);
        assert!(w.tabs[0].running());
    }
    #[test]
    fn closing_an_earlier_tab_preserves_active_session() {
        let ctx = Context::default();
        let mut w = Workspace::new_context(
            &ctx,
            None,
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/tab-test"),
        );
        for _ in 0..3 {
            w.start(Session {
                protocol: "Unavailable".into(),
                ..Default::default()
            });
        }
        let active_id = w.tabs[w.active].id;
        w.close_tab(0);
        assert_eq!(w.tabs[w.active].id, active_id);
        w.close_tab(w.active);
        assert_eq!(w.active, 0);
        w.close_tab(0);
        assert!(w.tabs.is_empty());
        assert!(w.page == Page::Home);
    }
    #[test]
    fn sidebar_keyboard_toggle_persists_and_restores() {
        let ctx = Context::default();
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/gui-sidebar-test");
        let _ = fs::remove_file(root.join("workspace.json"));
        let mut w = Workspace::new_context(&ctx, None, root.clone());
        frame(&ctx, &mut w, vec![]);
        let modifiers = Modifiers {
            ctrl: true,
            shift: true,
            ..Default::default()
        };
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1360., 880.))),
                events: vec![
                    Event::ModifiersChanged(modifiers),
                    Event::Key {
                        key: Key::B,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers,
                    },
                ],
                ..Default::default()
            },
            |ui| w.render(ui),
        );
        output.textures_delta.clear();
        assert!(!w.store.preferences.sidebar_visible);
        let loaded = Store::load(&root.join("workspace.json")).unwrap();
        assert!(!loaded.preferences.sidebar_visible);
        let restored = Workspace::new_context(&ctx, None, root.clone());
        assert!(!restored.store.preferences.sidebar_visible);
        fs::remove_file(root.join("workspace.json")).unwrap();
    }
    #[test]
    #[cfg(unix)]
    fn terminal_widget_writes_keyboard_input_into_real_shell() {
        let ctx = Context::default();
        let mut w = Workspace::new_context(
            &ctx,
            None,
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/gui-terminal-test"),
        );
        w.local();
        assert!(w.tabs[0].running());
        frame(&ctx, &mut w, vec![]);
        let id = Id::new(("terminal-input", w.tabs[0].id));
        ctx.memory_mut(|m| m.request_focus(id));
        frame(&ctx,&mut w,vec![Event::Text("printf '\\107\\125\\111\\137\\124\\105\\122\\115\\111\\116\\101\\114\\137\\117\\113\\n'".into()),Event::Key{key:Key::Enter,physical_key:None,pressed:true,repeat:false,modifiers:Modifiers::NONE}]);
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(30));
            w.tabs[0].poll();
            if w.tabs[0]
                .screen
                .plain()
                .lines()
                .any(|l| l.trim() == "GUI_TERMINAL_OK")
            {
                break;
            }
        }
        assert!(
            w.tabs[0]
                .screen
                .plain()
                .lines()
                .any(|l| l.trim() == "GUI_TERMINAL_OK")
        );
    }
    #[test]
    #[cfg(unix)]
    fn reconnect_keeps_tool_program_and_arguments() {
        let ctx = Context::default();
        let mut w = Workspace::new_context(
            &ctx,
            None,
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/gui-reconnect-test"),
        );
        w.start_tool(
            "Tool test",
            "/bin/sh",
            vec!["-c".into(), "printf TOOL_RECONNECTED".into()],
        );
        assert!(w.tabs[0].error.is_empty());
        w.reconnect_active();
        assert!(w.tabs[0].error.is_empty(), "{}", w.tabs[0].error);
    }
}

#[cfg(test)]
#[path = "ui/terminal_review_tests.rs"]
mod terminal_review_tests;

#[cfg(all(test, unix))]
mod workspace_review_tests;
