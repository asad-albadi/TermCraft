use super::*;
mod image;
mod resize;
use harbor_workspace::desktop::{DesktopEvent, DesktopInput, DesktopSession, Frame, TrustDecision};
use harbor_workspace::macros::Action as MacroInput;
use std::{
    collections::{BTreeSet, VecDeque},
    sync::Arc,
};
use zeroize::Zeroizing;
struct RemoteCursor {
    width: u16,
    height: u16,
    hotspot: (u16, u16),
    image: Option<ColorImage>,
    texture: Option<TextureHandle>,
}
impl RemoteCursor {
    fn new(
        width: u16,
        height: u16,
        hotspot: (u16, u16),
        pixels: Vec<u8>,
    ) -> Result<Option<Self>, String> {
        if width == 0 && height == 0 && pixels.is_empty() {
            return Ok(None);
        }
        if width == 0
            || height == 0
            || width > 512
            || height > 512
            || hotspot.0 >= width
            || hotspot.1 >= height
            || pixels.len() != usize::from(width) * usize::from(height) * 4
        {
            return Err("Invalid or oversized remote cursor shape".into());
        }
        Ok(Some(Self {
            width,
            height,
            hotspot,
            image: Some(image::from_rgba(
                [width as usize, height as usize],
                &pixels,
                false,
            )),
            texture: None,
        }))
    }
    fn rect(&self, position: Pos2, scale: f32) -> Rect {
        Rect::from_min_size(
            position - vec2(self.hotspot.0 as f32, self.hotspot.1 as f32) * scale,
            vec2(self.width as f32, self.height as f32) * scale,
        )
    }
    fn paint(&mut self, ui: &Ui, canvas: Rect, position: Pos2, scale: f32) {
        if let Some(image) = self.image.take() {
            self.texture = Some(ui.ctx().load_texture(
                "remote-desktop-cursor",
                image,
                TextureOptions::NEAREST,
            ));
        }
        if let Some(texture) = &self.texture {
            ui.painter().with_clip_rect(canvas).image(
                texture.id(),
                self.rect(position, scale),
                Rect::from_min_max(Pos2::ZERO, pos2(1., 1.)),
                Color32::WHITE,
            );
        }
    }
}
#[derive(Default)]
struct ClipboardState {
    ready: bool,
    remote: Option<Zeroizing<String>>,
}
impl ClipboardState {
    fn receive(
        &mut self,
        policy: harbor_workspace::desktop::rdp_options::ClipboardPolicy,
        text: String,
    ) -> Result<(), String> {
        self.remote = None;
        let text = Zeroizing::new(text);
        if !policy.from_remote() || text.len() > 8192 {
            return Err("Unexpected or oversized remote clipboard text".into());
        }
        self.remote = Some(text);
        Ok(())
    }
    fn clear(&mut self) {
        self.ready = false;
        self.remote = None;
    }
    fn controls(&mut self, ui: &mut Ui) {
        if ui
            .add_enabled(self.remote.is_some(), Button::new("Copy remote clipboard"))
            .clicked()
        {
            if let Some(text) = &self.remote {
                ui.ctx().copy_text(text.to_string());
            }
        }
    }
}
pub(super) enum Form {
    Certificate {
        id: u64,
        fingerprint: String,
        previous: Option<String>,
        reason: String,
        changed: bool,
    },
    Credentials {
        id: u64,
        user: String,
        password: Zeroizing<String>,
        domain: String,
        fingerprint: String,
        host: String,
        port: u16,
        save_in_vault: bool,
        vault_filled: bool,
    },
}
pub(super) struct State {
    protocol: String,
    pub handle: DesktopSession,
    pub message: String,
    pub error: String,
    pub connected: bool,
    pub(super) macro_capture: super::macro_events::Capture,
    pub(super) macro_playing: bool,
    macro_buttons: u8,
    macro_queue: VecDeque<DesktopInput>,
    macro_release_pending: bool,
    pub fullscreen_pending: bool,
    pub frame: Option<Arc<Frame>>,
    texture: Option<TextureHandle>,
    texture_revision: u64,
    texture_damage: Option<harbor_workspace::desktop::framebuffer::DamageRect>,
    texture_full_pending: bool,
    pub form: Option<Form>,
    pub remember: Option<String>,
    focused: bool,
    system_cursor: bool,
    remote_cursor: Option<RemoteCursor>,
    modifiers: Modifiers,
    keys: BTreeSet<(u8, bool)>,
    buttons: BTreeSet<u8>,
    last_point: (u16, u16),
    wheel: Vec2,
    clipboard: ClipboardState,
    options: harbor_workspace::desktop::rdp_options::RdpOptions,
    pub canvas_rect: Option<Rect>,
    inherit_pane: bool,
    fixed_size: (u32, u32),
    pane_resize: resize::PaneResize,
}
impl State {
    pub fn new(mut handle: DesktopSession, session: &Session) -> Self {
        handle.set_partial_frames(true);
        Self {
            handle,
            protocol: session.protocol.clone(),
            message: "Connecting…".into(),
            error: String::new(),
            connected: false,
            macro_capture: Default::default(),
            macro_playing: false,
            macro_buttons: 0,
            macro_queue: VecDeque::new(),
            macro_release_pending: false,
            fullscreen_pending: session.fullscreen,
            frame: None,
            texture: None,
            texture_revision: 0,
            texture_damage: None,
            texture_full_pending: true,
            form: None,
            remember: None,
            focused: false,
            system_cursor: true,
            remote_cursor: None,
            modifiers: Modifiers::default(),
            keys: BTreeSet::new(),
            buttons: BTreeSet::new(),
            last_point: (0, 0),
            wheel: Vec2::ZERO,
            clipboard: ClipboardState::default(),
            options: if session.protocol == "VNC" {
                let mut options = harbor_workspace::desktop::rdp_options::RdpOptions::default();
                options.clipboard = session.vnc_options.clipboard;
                options.keyboard = session.vnc_options.keyboard;
                options
            } else {
                session.rdp_options.clone()
            },
            canvas_rect: None,
            inherit_pane: session.desktop_size_mode
                == harbor_workspace::model::DesktopSizeMode::InheritPane,
            fixed_size: (session.width, session.height),
            pane_resize: Default::default(),
        }
    }
    fn accept_frame(&mut self, frame: Arc<Frame>) -> bool {
        let changed = self
            .frame
            .as_ref()
            .is_none_or(|old| old.revision != frame.revision);
        self.frame = Some(frame);
        self.texture_damage = None;
        self.texture_full_pending = true;
        changed
    }
    fn accept_patch(
        &mut self,
        patch: harbor_workspace::desktop::framebuffer::FramePatch,
    ) -> Result<(), String> {
        let frame = self
            .frame
            .as_mut()
            .ok_or("Desktop damage arrived before its full framebuffer")?;
        patch.apply(Arc::make_mut(frame))?;
        self.texture_damage = Some(
            self.texture_damage
                .map_or(patch.rect, |old| old.union(patch.rect)),
        );
        Ok(())
    }
    pub fn poll(&mut self) -> bool {
        self.flush_macro_release();
        let mut changed = false;
        for event in self.handle.poll() {
            match event {
                DesktopEvent::Connected { .. } => {
                    self.connected = true;
                    self.form = None;
                    self.error.clear();
                    self.message = format!("Connected · {}", self.protocol);
                }
                DesktopEvent::Frame(frame) => changed |= self.accept_frame(frame),
                DesktopEvent::FramePatch(patch) => match self.accept_patch(patch) {
                    Ok(()) => changed = true,
                    Err(error) => self.error = error,
                },
                DesktopEvent::Certificate {
                    id,
                    fingerprint,
                    previous,
                    reason,
                    changed,
                    ..
                } => {
                    self.release();
                    self.form = Some(Form::Certificate {
                        id,
                        fingerprint,
                        previous,
                        reason,
                        changed,
                    });
                    self.message = "Verify the RDP server certificate".into();
                }
                DesktopEvent::Authentication {
                    id,
                    user,
                    fingerprint,
                    host,
                    port,
                } => {
                    self.release();
                    let verified_tls = !fingerprint.is_empty();
                    self.form = Some(Form::Credentials {
                        id,
                        user,
                        password: Zeroizing::new(String::new()),
                        domain: self.options.domain.clone(),
                        fingerprint,
                        host,
                        port,
                        save_in_vault: false,
                        vault_filled: false,
                    });
                    self.message = if !verified_tls {
                        "VNC sign-in · unencrypted connection".into()
                    } else {
                        format!("{} sign-in · verified TLS", self.protocol)
                    };
                }
                DesktopEvent::TrustConfirmed {
                    fingerprint,
                    remember: true,
                } => self.remember = Some(fingerprint),
                DesktopEvent::ClipboardReady => self.clipboard.ready = true,
                DesktopEvent::Clipboard { text } => {
                    if let Err(error) = self.clipboard.receive(self.options.clipboard, text) {
                        self.error = error;
                    }
                }
                DesktopEvent::CursorShape {
                    width,
                    height,
                    hotspot,
                    pixels,
                } => match RemoteCursor::new(width, height, hotspot, pixels) {
                    Ok(cursor) => {
                        self.remote_cursor = cursor;
                        self.system_cursor = false;
                        changed = true;
                    }
                    Err(error) => self.error = error,
                },
                DesktopEvent::Cursor { visible, .. } => {
                    self.system_cursor = visible;
                    if visible {
                        self.remote_cursor = None;
                    }
                }
                DesktopEvent::Status { message } => self.message = message,
                DesktopEvent::Disconnected { message } => {
                    self.connected = false;
                    self.clipboard.clear();
                    self.remote_cursor = None;
                    self.system_cursor = true;
                    self.form = None;
                    self.release();
                    self.error = message;
                    self.message = "Disconnected".into();
                }
                _ => {}
            }
        }
        changed
    }
    pub(super) fn vault_credential(
        &self,
        session: &Session,
    ) -> Option<harbor_workspace::ssh::ChallengeCredential> {
        let Form::Credentials {
            host,
            port,
            fingerprint,
            user,
            domain,
            ..
        } = self.form.as_ref()?
        else {
            return None;
        };
        Some(harbor_workspace::ssh::ChallengeCredential {
            reference: session.credential_ref.clone(),
            binding: harbor_workspace::desktop::rdp_options::credential_binding(
                session,
                host,
                *port,
                fingerprint,
                user,
                domain,
            )
            .ok()?,
        })
    }
    pub(super) fn vault_matches(
        &self,
        session: &Session,
        challenge: u64,
        binding: &harbor_workspace::credentials::CredentialBinding,
    ) -> bool {
        matches!(&self.form, Some(Form::Credentials { id, .. }) if *id == challenge)
            && self
                .vault_credential(session)
                .is_some_and(|c| &c.binding == binding)
    }
    pub(super) fn clear_vault_password(&mut self) {
        if let Some(Form::Credentials {
            password,
            vault_filled,
            save_in_vault,
            ..
        }) = &mut self.form
        {
            if *vault_filled {
                use zeroize::Zeroize;
                password.zeroize();
            }
            *vault_filled = false;
            *save_in_vault = false;
        }
    }
    pub(super) fn fill_vault_password(&mut self, value: Zeroizing<String>) {
        if let Some(Form::Credentials {
            password,
            vault_filled,
            ..
        }) = &mut self.form
        {
            *password = value;
            *vault_filled = true;
        }
    }
    fn send(&mut self, input: DesktopInput) -> bool {
        let recorded = if self.macro_capture.active() && self.macro_ready() {
            match &input {
                DesktopInput::Key {
                    code,
                    extended,
                    pressed,
                } => Some(MacroInput::Key {
                    code: u32::from(*code) | (u32::from(*extended) << 8),
                    pressed: *pressed,
                }),
                DesktopInput::Text { value } => Some(MacroInput::Text(value.clone())),
                DesktopInput::Pointer { x, y, .. } => {
                    self.frame.as_ref().map(|frame| MacroInput::Pointer {
                        x: scale_to_normalized(*x, frame.width),
                        y: scale_to_normalized(*y, frame.height),
                        buttons: self
                            .buttons
                            .iter()
                            .fold(0, |mask, button| mask | (1 << (button - 1))),
                    })
                }
                _ => None,
            }
        } else {
            None
        };
        match self.handle.send(input) {
            Ok(()) => {
                if let Some(action) = recorded {
                    self.macro_capture.record(action);
                }
                true
            }
            Err(error) => {
                self.error = error;
                self.macro_release_pending = true;
                self.flush_macro_release();
                self.focused = false;
                self.keys.clear();
                self.buttons.clear();
                self.modifiers = Modifiers::default();
                self.wheel = Vec2::ZERO;
                false
            }
        }
    }
    pub(super) fn macro_ready(&self) -> bool {
        self.connected && self.form.is_none() && !self.handle.is_finished() && self.frame.is_some()
    }

    pub(super) fn macro_focused(&self) -> bool {
        self.focused
    }

    fn flush_macro_release(&mut self) {
        if self.macro_release_pending
            && (self.handle.is_finished() || self.handle.send(DesktopInput::ReleaseAll).is_ok())
        {
            self.macro_release_pending = false;
        }
    }

    pub(super) fn drain_macro_input(&mut self) -> Result<bool, String> {
        while let Some(input) = self.macro_queue.front() {
            let result = match input {
                DesktopInput::ReleaseAll => self.handle.release_after_input(),
                DesktopInput::Text { value } => self.handle.send(DesktopInput::Text {
                    value: value.clone(),
                }),
                DesktopInput::Key {
                    code,
                    extended,
                    pressed,
                } => self.handle.send(DesktopInput::Key {
                    code: *code,
                    extended: *extended,
                    pressed: *pressed,
                }),
                DesktopInput::Pointer {
                    x,
                    y,
                    button,
                    pressed,
                } => self.handle.send(DesktopInput::Pointer {
                    x: *x,
                    y: *y,
                    button: *button,
                    pressed: *pressed,
                }),
                _ => unreachable!("macro queue accepts only content input"),
            };
            match result {
                Ok(()) => {
                    self.macro_queue.pop_front();
                }
                Err(error)
                    if error == harbor_workspace::desktop::INPUT_QUEUE_BUSY
                        && !self.handle.is_finished() =>
                {
                    return Ok(false);
                }
                Err(error) => return Err(error),
            }
        }
        Ok(true)
    }

    pub(super) fn play_macro_action(&mut self, input: MacroInput) -> Result<bool, String> {
        if !self.macro_ready() || self.macro_release_pending {
            return Err("The desktop is not ready for macro input.".into());
        }
        debug_assert!(self.macro_queue.is_empty());
        match input {
            MacroInput::Text(value) => self.macro_queue.push_back(DesktopInput::Text { value }),
            MacroInput::Key { code, pressed } => self.macro_queue.push_back(DesktopInput::Key {
                code: (code & 0xff) as u8,
                extended: code & 0x100 != 0,
                pressed,
            }),
            MacroInput::Pointer { x, y, buttons } => {
                let frame = self.frame.as_ref().unwrap();
                let x = scale_from_normalized(x, frame.width);
                let y = scale_from_normalized(y, frame.height);
                self.macro_queue.push_back(DesktopInput::Pointer {
                    x,
                    y,
                    button: None,
                    pressed: false,
                });
                for button in 1..=3 {
                    let mask = 1 << (button - 1);
                    if (self.macro_buttons ^ buttons) & mask != 0 {
                        self.macro_queue.push_back(DesktopInput::Pointer {
                            x,
                            y,
                            button: Some(button),
                            pressed: buttons & mask != 0,
                        });
                    }
                }
                self.macro_buttons = buttons;
            }
            MacroInput::ReleaseAll => {
                self.macro_buttons = 0;
                self.macro_queue.push_back(DesktopInput::ReleaseAll);
            }
        }
        self.drain_macro_input()
    }

    pub(super) fn cancel_macro_input(&mut self, action: MacroInput) {
        if action == MacroInput::ReleaseAll {
            self.macro_queue.clear();
            self.macro_buttons = 0;
            self.macro_release_pending = true;
            self.flush_macro_release();
        }
    }

    pub fn release(&mut self) {
        if !self.macro_playing && (!self.keys.is_empty() || !self.buttons.is_empty()) {
            self.macro_capture.record(MacroInput::ReleaseAll);
            self.macro_release_pending = true;
            self.flush_macro_release();
        }
        self.focused = false;
        self.modifiers = Modifiers::default();
        self.wheel = Vec2::ZERO;
        self.keys.clear();
        self.buttons.clear();
    }
}
fn scale_to_normalized(value: u16, length: u16) -> u16 {
    let max = length.saturating_sub(1);
    (u32::from(value.min(max)) * u32::from(u16::MAX) / u32::from(max.max(1))) as u16
}

fn scale_from_normalized(value: u16, length: u16) -> u16 {
    let max = u32::from(length.saturating_sub(1));
    ((u32::from(value) * max + 32767) / u32::from(u16::MAX)) as u16
}

pub(super) fn endpoint(session: &Session) -> String {
    format!(
        "{}:{}",
        session
            .host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_ascii_lowercase(),
        session.port
    )
}
impl Workspace {
    pub(super) fn poll_desktop_certificates(&mut self) {
        if self.import_applying() {
            return;
        }
        let mut changed = false;
        for tab in &mut self.tabs {
            if let Some(desktop) = &mut tab.desktop {
                if !self.visible_terminals.contains(&tab.id) {
                    desktop.release();
                }
                if let Some(fingerprint) = desktop.remember.take() {
                    let endpoint = endpoint(&tab.session);
                    if self.store.preferences.rdp_certificates.len() < 512
                        || self
                            .store
                            .preferences
                            .rdp_certificates
                            .contains_key(&endpoint)
                    {
                        changed |= self.store.preferences.rdp_certificates.get(&endpoint)
                            != Some(&fingerprint);
                        self.store
                            .preferences
                            .rdp_certificates
                            .insert(endpoint, fingerprint);
                    } else {
                        desktop.error="Certificate accepted for this connection; the remembered-certificate list is full".into();
                    }
                }
            }
        }
        if changed {
            self.persist();
        }
    }
}
pub(super) fn view(ui: &mut Ui, tab: &mut TerminalTab) {
    let state = tab.desktop.as_mut().unwrap();
    if !state.error.is_empty() {
        crate::ui::errors::show(
            ui,
            crate::ui::errors::protocol(&tab.session.protocol),
            &state.error,
        );
        if !state.connected && state.form.is_none() && state.handle.is_finished() {
            return;
        }
    }
    let mut answer = None;
    let mut vault_use = false;
    let mut vault_save = false;
    if let Some(form) = &mut state.form {
        match form {
            Form::Certificate {
                id,
                fingerprint,
                previous,
                reason,
                changed,
            } => {
                ui.heading(if *changed {
                    "RDP certificate changed"
                } else {
                    "Verify the RDP server"
                });
                ui.label(format!("{}:{}", tab.session.host, tab.session.port));
                ui.label(reason.as_str());
                if let Some(previous) = previous {
                    ui.label("Previously remembered SHA-256");
                    ui.monospace(previous.as_str());
                }
                ui.label("Presented certificate SHA-256");
                ui.monospace(fingerprint.as_str());
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Trust once").clicked() {
                        answer = Some(DesktopInput::Trust {
                            id: *id,
                            decision: TrustDecision::Once,
                        });
                    }
                    if ui
                        .button(if *changed {
                            "Replace remembered certificate"
                        } else {
                            "Remember this certificate"
                        })
                        .clicked()
                    {
                        answer = Some(DesktopInput::Trust {
                            id: *id,
                            decision: TrustDecision::Remember,
                        });
                    }
                    let reject = ui.button("Reject");
                    super::fluent::focus_on_entry(ui, &reject, Id::new((tab.id, *id)));
                    if reject.clicked() {
                        answer = Some(DesktopInput::Trust {
                            id: *id,
                            decision: TrustDecision::Reject,
                        });
                    }
                });
            }
            Form::Credentials {
                id,
                user,
                password,
                domain,
                fingerprint,
                save_in_vault,
                vault_filled,
                ..
            } => {
                let is_rdp = tab.session.protocol == "RDP";
                ui.heading(if is_rdp {
                    "RDP sign-in"
                } else {
                    "VNC password"
                });
                if is_rdp {
                    ui.small(format!(
                        "Verified TLS certificate {}…",
                        fingerprint.get(..16).unwrap_or(fingerprint)
                    ));
                } else {
                    if fingerprint.is_empty() {
                        ui.small("Classic VNC does not encrypt your desktop or verify server identity. Passwords are not saved for this transport.");
                    } else {
                        ui.small(format!(
                            "Verified VNC TLS certificate {}…",
                            fingerprint.get(..16).unwrap_or(fingerprint)
                        ));
                        ui.small("VNC vault binding is not implemented; passwords are not saved.");
                    }
                }
                Grid::new(("rdp-auth", tab.id))
                    .num_columns(2)
                    .show(ui, |ui| {
                        if is_rdp {
                            ui.label("Username");
                            let user_changed = ui.text_edit_singleline(user).changed();
                            ui.end_row();
                            ui.label("Domain (optional)");
                            let domain_changed = ui.text_edit_singleline(domain).changed();
                            if *vault_filled && (user_changed || domain_changed) {
                                use zeroize::Zeroize;
                                password.zeroize();
                                *vault_filled = false;
                            }
                            ui.end_row();
                        }
                        ui.label("Password");
                        super::remote_files::auth_field_scoped(
                            ui,
                            "rdp-auth",
                            tab.id,
                            *id,
                            0,
                            &harbor_workspace::ssh::Prompt {
                                label: "Password".into(),
                                echo: false,
                            },
                            password,
                        );
                        ui.end_row();
                    });
                if is_rdp && tab.session.credential_ref.is_some() {
                    vault_use = ui.button("Use saved credential").clicked();
                }
                if is_rdp {
                    ui.checkbox(save_in_vault, "Save entered password in vault (optional)");
                }
                if ui
                    .button(if is_rdp || !fingerprint.is_empty() {
                        "Connect securely"
                    } else {
                        "Connect to VNC"
                    })
                    .clicked()
                {
                    vault_save = *save_in_vault;
                    answer = Some(DesktopInput::Authenticate {
                        id: *id,
                        user: user.clone(),
                        password: harbor_workspace::credentials::Secret::new(password.as_str()),
                        domain: domain.clone(),
                    });
                }
                if is_rdp {
                    ui.small("Credentials are sent over the verified RDP TLS connection.");
                }
            }
        }
    }
    if vault_use || vault_save {
        if let (Some(credential), Some(Form::Credentials { id, password, .. })) =
            (state.vault_credential(&tab.session), state.form.as_ref())
        {
            let destination = super::credentials::Destination::Desktop {
                connection: tab.id,
                challenge: *id,
            };
            if vault_use {
                tab.vault_request = Some(super::credentials::Request::Use {
                    destination,
                    credential,
                });
            } else if vault_save {
                tab.vault_request = Some(super::credentials::Request::Save {
                    destination,
                    credential,
                    values: vec![Zeroizing::new(password.to_string())],
                });
            }
        }
    }
    if let Some(answer) = answer {
        if state.send(answer) {
            state.form = None;
        }
    }
    let available = ui.available_size().max(vec2(1., 1.));
    let (rect, _) = ui.allocate_exact_size(available, Sense::hover());
    if state.connected && state.form.is_none() && !tab.session.view_only {
        let now = Instant::now();
        let (width, height, density) = if state.inherit_pane {
            (rect.width(), rect.height(), ui.ctx().pixels_per_point())
        } else {
            (state.fixed_size.0 as f32, state.fixed_size.1 as f32, 1.)
        };
        if let Some((width, height)) = state.pane_resize.observe(width, height, density, now) {
            // A full bounded input queue is retried; resizing never displaces key releases.
            if state
                .handle
                .send(DesktopInput::Resize { width, height })
                .is_ok()
            {
                state.pane_resize.sent((width, height), now);
            }
        }
    }
    ui.painter()
        .rect_filled(rect, 0., ui.visuals().extreme_bg_color);
    let Some(frame) = state.frame.clone() else {
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            "Waiting for the remote desktop…",
            FontId::proportional(15.),
            ui.visuals().weak_text_color(),
        );
        ui.ctx().request_repaint_after(Duration::from_millis(16));
        return;
    };
    if state.texture.is_none() || state.texture_revision != frame.revision {
        let damage = if state.texture_full_pending {
            None
        } else {
            state.texture_damage.take()
        };
        if let (Some(texture), Some(rect)) = (&mut state.texture, damage) {
            let mut pixels =
                Vec::with_capacity(usize::from(rect.width) * usize::from(rect.height) * 4);
            for y in usize::from(rect.y)..usize::from(rect.y + rect.height) {
                let start = (y * usize::from(frame.width) + usize::from(rect.x)) * 4;
                pixels.extend_from_slice(&frame.pixels[start..start + usize::from(rect.width) * 4]);
            }
            let image = image::from_rgba(
                [rect.width as usize, rect.height as usize],
                &pixels,
                state.protocol == "RDP",
            );
            texture.set_partial(
                [rect.x as usize, rect.y as usize],
                image,
                TextureOptions::LINEAR,
            );
        } else {
            state.texture_damage = None;
            let image = image::from_rgba(
                [frame.width as usize, frame.height as usize],
                &frame.pixels,
                state.protocol == "RDP",
            );
            if let Some(texture) = &mut state.texture {
                texture.set(image, TextureOptions::LINEAR);
            } else {
                state.texture = Some(ui.ctx().load_texture(
                    format!("rdp-desktop-{}", tab.id),
                    image,
                    TextureOptions::LINEAR,
                ));
            }
        }
        state.texture_revision = frame.revision;
        state.texture_full_pending = false;
    }
    let scale = (rect.width() / frame.width as f32).min(rect.height() / frame.height as f32);
    let remote = Rect::from_center_size(
        rect.center(),
        vec2(frame.width as f32 * scale, frame.height as f32 * scale),
    );
    state.canvas_rect = Some(remote);
    let texture = state.texture.as_ref().unwrap();
    ui.painter().image(
        texture.id(),
        remote,
        Rect::from_min_max(Pos2::ZERO, pos2(1., 1.)),
        Color32::WHITE,
    );
    let response = ui.interact(
        remote,
        Id::new(("rdp-canvas", tab.id)),
        Sense::click_and_drag(),
    );
    ui.memory_mut(|memory| {
        memory.set_focus_lock_filter(
            response.id,
            egui::EventFilter {
                tab: true,
                horizontal_arrows: true,
                vertical_arrows: true,
                escape: true,
            },
        )
    });
    if response.hovered() && state.connected && !state.system_cursor {
        ui.ctx().set_cursor_icon(CursorIcon::None);
    }
    let allowed = ui.is_enabled()
        && !super::shortcuts::input_blocked(ui.ctx(), tab.id)
        && ui.memory(|memory| memory.allows_interaction(ui.layer_id()));
    let point_ctx = ui.ctx().clone();
    let layer = ui.layer_id();
    let clip = ui.clip_rect();
    let owns_point = |position: Pos2| {
        allowed
            && remote.contains(position)
            && clip.contains(position)
            && point_ctx.layer_id_at(position) == Some(layer)
    };
    if allowed && (response.clicked() || response.drag_started()) && !response.has_focus() {
        response.request_focus();
    }
    let keyboard_remote = state.options.keyboard
        == harbor_workspace::desktop::rdp_options::KeyboardRouting::Remote
        && !tab.session.view_only;
    let focused = allowed
        && response.has_focus()
        && ui.input(|i| i.focused)
        && state.connected
        && state.form.is_none();
    if !focused && state.focused {
        state.release();
    }
    state.focused = focused;
    let events = if state.macro_playing || state.macro_release_pending {
        Vec::new()
    } else {
        ui.input(|i| i.events.clone())
    };
    for event in coalesce_pointer_motion(events) {
        match event {
            Event::PointerMoved(position)
                if allowed
                    && state.connected
                    && !tab.session.view_only
                    && state.form.is_none()
                    && (owns_point(position) || !state.buttons.is_empty()) =>
            {
                let (x, y) = point(remote, &frame, position);
                state.last_point = (x, y);
                state.send(DesktopInput::Pointer {
                    x,
                    y,
                    button: None,
                    pressed: false,
                });
            }
            Event::PointerButton {
                pos,
                button,
                pressed,
                modifiers: event_modifiers,
                ..
            } if allowed && state.connected && !tab.session.view_only && state.form.is_none() => {
                let button = match button {
                    PointerButton::Primary => 1,
                    PointerButton::Secondary => 2,
                    PointerButton::Middle => 3,
                    _ => continue,
                };
                if pressed && !owns_point(pos) {
                    continue;
                }
                if !pressed && !state.buttons.contains(&button) {
                    continue;
                }
                if !sync_modifiers(state, event_modifiers) {
                    continue;
                }
                let (x, y) = point(remote, &frame, pos);
                state.last_point = (x, y);
                if pressed {
                    state.buttons.insert(button);
                    if !response.has_focus() {
                        response.request_focus();
                    }
                    state.focused = true;
                } else {
                    state.buttons.remove(&button);
                }
                state.send(DesktopInput::Pointer {
                    x,
                    y,
                    button: Some(button),
                    pressed,
                });
            }
            Event::MouseWheel {
                unit,
                delta,
                modifiers: event_modifiers,
                ..
            } if focused
                && !tab.session.view_only
                && ui.input(|i| {
                    i.pointer
                        .hover_pos()
                        .is_some_and(|position| remote.contains(position))
                }) =>
            {
                if !sync_modifiers(state, event_modifiers) {
                    continue;
                }
                let units = match unit {
                    MouseWheelUnit::Point => 3.,
                    MouseWheelUnit::Line => 120.,
                    MouseWheelUnit::Page => 360.,
                };
                state.wheel += vec2(-delta.x, delta.y) * units;
                state.wheel = state.wheel.clamp(vec2(-2400., -2400.), vec2(2400., 2400.));
                for horizontal in [false, true] {
                    let pending = if horizontal {
                        &mut state.wheel.x
                    } else {
                        &mut state.wheel.y
                    };
                    let count = pending.trunc() as i32;
                    *pending -= count as f32;
                    let mut remaining = count;
                    while remaining != 0 {
                        let delta = remaining.clamp(-240, 240) as i16;
                        let (x, y) = state.last_point;
                        if !state.send(DesktopInput::Scroll {
                            x,
                            y,
                            delta,
                            horizontal,
                        }) {
                            break;
                        }
                        remaining -= i32::from(delta);
                    }
                }
            }
            Event::ModifiersChanged(value) if state.focused && keyboard_remote => {
                sync_modifiers(state, value);
            }
            Event::Key {
                key,
                physical_key,
                pressed,
                modifiers,
                repeat,
                ..
            } if state.focused && keyboard_remote => {
                if !sync_modifiers(state, modifiers) {
                    continue;
                }
                let Some((code, extended)) = scancode(physical_key.unwrap_or(key)) else {
                    continue;
                };
                let special = matches!(
                    key,
                    Key::ArrowLeft
                        | Key::ArrowRight
                        | Key::ArrowUp
                        | Key::ArrowDown
                        | Key::Escape
                        | Key::Tab
                        | Key::Enter
                        | Key::Backspace
                        | Key::Insert
                        | Key::Delete
                        | Key::Home
                        | Key::End
                        | Key::PageUp
                        | Key::PageDown
                        | Key::F1
                        | Key::F2
                        | Key::F3
                        | Key::F4
                        | Key::F5
                        | Key::F6
                        | Key::F7
                        | Key::F8
                        | Key::F9
                        | Key::F10
                        | Key::F11
                        | Key::F12
                );
                if special
                    || modifiers.ctrl
                    || modifiers.alt
                    || modifiers.mac_cmd
                    || state.keys.contains(&(code, extended))
                {
                    if pressed {
                        state.keys.insert((code, extended));
                    } else {
                        state.keys.remove(&(code, extended));
                    }
                    if !repeat || pressed {
                        state.send(DesktopInput::Key {
                            code,
                            extended,
                            pressed,
                        });
                    }
                }
            }
            Event::Text(value)
                if state.focused
                    && keyboard_remote
                    && !(state.modifiers.ctrl
                        || state.modifiers.alt
                        || state.modifiers.mac_cmd) =>
            {
                if !value.chars().any(char::is_control) {
                    state.send(DesktopInput::Text { value });
                }
            }
            Event::Paste(value) if state.focused => {
                if state.clipboard.ready
                    && state.options.clipboard.to_remote()
                    && !tab.session.view_only
                {
                    state.send(DesktopInput::Clipboard { text: value });
                } else {
                    state.error = "Clipboard upload is disabled or its channel is not ready".into();
                }
            }
            Event::WindowFocused(false) => state.release(),
            _ => {}
        }
    }
    if state.focused && keyboard_remote && !state.macro_playing && !state.macro_release_pending {
        sync_modifiers(state, ui.input(|i| i.modifiers));
    }
    if allowed
        && state.connected
        && state.form.is_none()
        && !tab.session.view_only
        && response.hovered()
    {
        if let Some(position) = ui
            .input(|i| i.pointer.hover_pos())
            .filter(|position| owns_point(*position))
        {
            if let Some(cursor) = &mut state.remote_cursor {
                ui.ctx().set_cursor_icon(CursorIcon::None);
                cursor.paint(ui, remote, position, scale);
            }
        }
    }
    ui.ctx().request_repaint_after(Duration::from_millis(16));
}
// Keep the latest motion in each uninterrupted run; never move a button/key event
// across motion. This prevents high-polling mice from filling the ordered queue.
fn coalesce_pointer_motion(events: Vec<Event>) -> Vec<Event> {
    let mut result = Vec::with_capacity(events.len());
    for event in events {
        if matches!(event, Event::PointerMoved(_))
            && result
                .last()
                .is_some_and(|last| matches!(last, Event::PointerMoved(_)))
        {
            result.pop();
        }
        result.push(event);
    }
    result
}

fn point(rect: Rect, frame: &Frame, position: Pos2) -> (u16, u16) {
    (
        ((position.x - rect.left()) / rect.width() * frame.width as f32)
            .floor()
            .clamp(0., (frame.width - 1) as f32) as u16,
        ((position.y - rect.top()) / rect.height() * frame.height as f32)
            .floor()
            .clamp(0., (frame.height - 1) as f32) as u16,
    )
}
fn sync_modifiers(state: &mut State, next: Modifiers) -> bool {
    if state.options.keyboard == harbor_workspace::desktop::rdp_options::KeyboardRouting::Local {
        return true;
    }
    for (was, now, code, extended) in [
        (state.modifiers.ctrl, next.ctrl, 0x1d, false),
        (state.modifiers.shift, next.shift, 0x2a, false),
        (state.modifiers.alt, next.alt, 0x38, false),
        (state.modifiers.mac_cmd, next.mac_cmd, 0x5b, true),
    ] {
        if was != now {
            if !state.send(DesktopInput::Key {
                code,
                extended,
                pressed: now,
            }) {
                return false;
            }
        }
    }
    state.modifiers = next;
    true
}
fn scancode(key: Key) -> Option<(u8, bool)> {
    Some(match key {
        Key::A => (30, false),
        Key::B => (48, false),
        Key::C => (46, false),
        Key::D => (32, false),
        Key::E => (18, false),
        Key::F => (33, false),
        Key::G => (34, false),
        Key::H => (35, false),
        Key::I => (23, false),
        Key::J => (36, false),
        Key::K => (37, false),
        Key::L => (38, false),
        Key::M => (50, false),
        Key::N => (49, false),
        Key::O => (24, false),
        Key::P => (25, false),
        Key::Q => (16, false),
        Key::R => (19, false),
        Key::S => (31, false),
        Key::T => (20, false),
        Key::U => (22, false),
        Key::V => (47, false),
        Key::W => (17, false),
        Key::X => (45, false),
        Key::Y => (21, false),
        Key::Z => (44, false),
        Key::Num1 => (2, false),
        Key::Num2 => (3, false),
        Key::Num3 => (4, false),
        Key::Num4 => (5, false),
        Key::Num5 => (6, false),
        Key::Num6 => (7, false),
        Key::Num7 => (8, false),
        Key::Num8 => (9, false),
        Key::Num9 => (10, false),
        Key::Num0 => (11, false),
        Key::Space => (57, false),
        Key::Enter => (28, false),
        Key::Tab => (15, false),
        Key::Escape => (1, false),
        Key::Backspace => (14, false),
        Key::Minus => (12, false),
        Key::Equals | Key::Plus => (13, false),
        Key::OpenBracket => (26, false),
        Key::CloseBracket => (27, false),
        Key::Semicolon => (39, false),
        Key::Quote => (40, false),
        Key::Backtick => (41, false),
        Key::Backslash => (43, false),
        Key::Comma => (51, false),
        Key::Period => (52, false),
        Key::Slash => (53, false),
        Key::ArrowLeft => (75, true),
        Key::ArrowRight => (77, true),
        Key::ArrowUp => (72, true),
        Key::ArrowDown => (80, true),
        Key::Home => (71, true),
        Key::End => (79, true),
        Key::Insert => (82, true),
        Key::Delete => (83, true),
        Key::PageUp => (73, true),
        Key::PageDown => (81, true),
        Key::F1 => (59, false),
        Key::F2 => (60, false),
        Key::F3 => (61, false),
        Key::F4 => (62, false),
        Key::F5 => (63, false),
        Key::F6 => (64, false),
        Key::F7 => (65, false),
        Key::F8 => (66, false),
        Key::F9 => (67, false),
        Key::F10 => (68, false),
        Key::F11 => (87, false),
        Key::F12 => (88, false),
        _ => return None,
    })
}
#[cfg(all(test, unix))]
#[allow(dead_code)]
#[path = "../../tests/support/rdp_server.rs"]
mod fixture;
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::ui::terminal_review_tests::owned_workspace;
    fn paint(ctx: &Context, w: &mut Workspace, events: Vec<Event>) -> egui::FullOutput {
        paint_size(ctx, w, events, vec2(1400., 930.))
    }
    fn paint_size(
        ctx: &Context,
        w: &mut Workspace,
        events: Vec<Event>,
        size: Vec2,
    ) -> egui::FullOutput {
        ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |ui| w.render(ui),
        )
    }
    fn text(shape: &egui::epaint::Shape, label: &str) -> Option<Pos2> {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text() == label => {
                Some(text.pos + text.galley.size() / 2.)
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(|shape| text(shape, label)),
            _ => None,
        }
    }
    fn click(ctx: &Context, w: &mut Workspace, position: Pos2) -> bool {
        click_size(ctx, w, position, vec2(1400., 930.))
    }
    fn click_size(ctx: &Context, w: &mut Workspace, position: Pos2, size: Vec2) -> bool {
        let mut fullscreen = false;
        for pressed in [true, false] {
            let mut output = paint_size(
                ctx,
                w,
                vec![
                    Event::PointerMoved(position),
                    Event::PointerButton {
                        pos: position,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::default(),
                    },
                ],
                size,
            );
            fullscreen |= output.viewport_output.values().any(|v| {
                v.commands
                    .iter()
                    .any(|c| matches!(c, ViewportCommand::Fullscreen(true)))
            });
            output.textures_delta.clear();
        }
        fullscreen
    }
    #[test]
    fn replacement_frame_then_patch_before_paint_requires_full_texture_upload() {
        use harbor_workspace::desktop::framebuffer::{DamageRect, FramePatch};
        let (ctx, root, mut w) = owned_workspace();
        crate::ui::terminal_review_tests::raw_terminal(&mut w, "texture fixture");
        let session = Session {
            protocol: "RDP".into(),
            host: "localhost".into(),
            view_only: true,
            ..Default::default()
        };
        let handle = DesktopSession::connect(harbor_workspace::desktop::DesktopConfig::new(
            9201,
            session.clone(),
            root.path().join("absent-texture-helper"),
        ))
        .unwrap();
        w.tabs[0].process = None;
        w.tabs[0].session = session.clone();
        let mut state = State::new(handle, &session);
        state.connected = true;
        state.accept_frame(Arc::new(Frame {
            width: 2,
            height: 2,
            revision: 1,
            pixels: vec![255, 0, 0, 255].repeat(4),
        }));
        w.tabs[0].desktop = Some(state);
        let draw = |w: &mut Workspace| {
            ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800., 600.))),
                    ..Default::default()
                },
                |ui| view(ui, &mut w.tabs[0]),
            )
        };
        let mut initial = draw(&mut w);
        initial.textures_delta.clear();
        let state = w.tabs[0].desktop.as_mut().unwrap();
        state.accept_frame(Arc::new(Frame {
            width: 4,
            height: 4,
            revision: 2,
            pixels: vec![0, 255, 0, 255].repeat(16),
        }));
        let patch = |revision| FramePatch {
            width: 4,
            height: 4,
            revision,
            rect: DamageRect {
                x: 1,
                y: 1,
                width: 1,
                height: 1,
            },
            pixels: vec![255, 0, 0, 255],
        };
        state.accept_patch(patch(3)).unwrap();
        let mut full = draw(&mut w);
        assert!(
            full.textures_delta
                .set
                .iter()
                .flat_map(|(_, d)| d)
                .any(|d| {
                    let egui::ImageData::Color(image) = &d.image;
                    d.pos.is_none()
                        && image.size == [4, 4]
                        && image.pixels[0] == Color32::GREEN
                        && image.pixels[5] == Color32::RED
                })
        );
        full.textures_delta.clear();
        w.tabs[0]
            .desktop
            .as_mut()
            .unwrap()
            .accept_patch(patch(4))
            .unwrap();
        let mut partial = draw(&mut w);
        assert!(
            partial
                .textures_delta
                .set
                .iter()
                .flat_map(|(_, d)| d)
                .any(|d| {
                    let egui::ImageData::Color(image) = &d.image;
                    d.pos == Some([1, 1]) && image.size == [1, 1]
                })
        );
        partial.textures_delta.clear();
    }
    #[test]
    fn failed_desktop_connection_paints_one_error_without_waiting_message() {
        let (ctx, root, mut w) = owned_workspace();
        crate::ui::terminal_review_tests::raw_terminal(&mut w, "error fixture");
        let session = Session {
            protocol: "RDP".into(),
            host: "localhost".into(),
            ..Default::default()
        };
        let config = harbor_workspace::desktop::DesktopConfig::new(
            9101,
            session.clone(),
            root.path().join("absent-rdp-helper"),
        );
        w.tabs[0].process = None;
        w.tabs[0].session = session.clone();
        w.tabs[0].desktop = Some(State::new(
            DesktopSession::connect(config).unwrap(),
            &session,
        ));
        let end = Instant::now() + Duration::from_secs(3);
        while w.tabs[0].desktop.as_ref().unwrap().error.is_empty() {
            w.tabs[0].poll();
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(5));
        }
        let diagnostic = harbor_workspace::diagnostics::Diagnostic::new(
            harbor_workspace::diagnostics::Context::Rdp,
            &w.tabs[0].error,
        );
        let mut first = paint(&ctx, &mut w, vec![]);
        first.textures_delta.clear();
        let mut out = paint(&ctx, &mut w, vec![]);
        assert_eq!(
            out.shapes
                .iter()
                .filter(|s| text(&s.shape, &diagnostic.code).is_some())
                .count(),
            1
        );
        assert!(
            !out.shapes
                .iter()
                .any(|s| text(&s.shape, "Waiting for the remote desktop…").is_some())
        );
        out.textures_delta.clear();
    }
    #[test]
    fn remote_motion_coalescing_preserves_button_and_key_boundaries() {
        let button = Event::PointerButton {
            pos: pos2(2., 2.),
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        };
        let events = coalesce_pointer_motion(vec![
            Event::PointerMoved(pos2(1., 1.)),
            Event::PointerMoved(pos2(2., 2.)),
            button.clone(),
            Event::PointerMoved(pos2(3., 3.)),
            Event::PointerMoved(pos2(4., 4.)),
            Event::Text("a".into()),
            Event::PointerMoved(pos2(5., 5.)),
        ]);
        assert_eq!(events.len(), 5);
        assert_eq!(events[0], Event::PointerMoved(pos2(2., 2.)));
        assert_eq!(events[1], button);
        assert_eq!(events[2], Event::PointerMoved(pos2(4., 4.)));
        assert_eq!(events[3], Event::Text("a".into()));
        assert_eq!(events[4], Event::PointerMoved(pos2(5., 5.)));
    }
    #[test]
    fn real_rdp_vault_save_use_delete_and_session_fullscreen() {
        let server = fixture::Server::start();
        let _ = fixture::engine_binary();
        let (ctx, _root, mut w) = owned_workspace();
        w.create_vault(
            harbor_workspace::credentials::Secret::new("owned desktop master"),
            &ctx,
        );
        let end = Instant::now() + Duration::from_secs(20);
        while w.vault_busy() {
            w.poll_vault(&ctx);
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut session = server.config(8400).session;
        session.fullscreen = true;
        w.store.sessions.push(session.clone());
        w.start(session);
        let end = Instant::now() + Duration::from_secs(15);
        let mut submitted = false;
        let mut fullscreen = false;
        while !w.tabs[0].desktop.as_ref().unwrap().connected || w.vault_busy() {
            let mut output = paint(&ctx, &mut w, vec![]);
            fullscreen |= output.viewport_output.values().any(|v| {
                v.commands
                    .iter()
                    .any(|c| matches!(c, ViewportCommand::Fullscreen(true)))
            });
            if let Some(pos) = output
                .shapes
                .iter()
                .find_map(|s| text(&s.shape, "Remember this certificate"))
            {
                output.textures_delta.clear();
                fullscreen |= click(&ctx, &mut w, pos);
            }
            if !submitted {
                if let Some(Form::Credentials {
                    password,
                    save_in_vault,
                    ..
                }) = &mut w.tabs[0].desktop.as_mut().unwrap().form
                {
                    *password = Zeroizing::new("fixture-password".into());
                    *save_in_vault = true;
                    if let Some(pos) = output
                        .shapes
                        .iter()
                        .find_map(|s| text(&s.shape, "Connect securely"))
                    {
                        output.textures_delta.clear();
                        fullscreen |= click(&ctx, &mut w, pos);
                        submitted = true;
                    }
                }
            }
            output.textures_delta.clear();
            assert!(Instant::now() < end, "RDP/vault did not connect");
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut output = paint(&ctx, &mut w, vec![]);
        fullscreen |= output.viewport_output.values().any(|v| {
            v.commands
                .iter()
                .any(|c| matches!(c, ViewportCommand::Fullscreen(true)))
        });
        output.textures_delta.clear();
        assert!(fullscreen);
        let reference = w.store.sessions[0]
            .credential_ref
            .clone()
            .expect("Explicit RDP save links a reference");
        assert!(
            !std::fs::read(w.vault_path())
                .unwrap()
                .windows(b"fixture-password".len())
                .any(|b| b == b"fixture-password")
        );
        w.start(w.store.sessions[0].clone());
        let end = Instant::now() + Duration::from_secs(12);
        while !matches!(
            w.tabs[1].desktop.as_ref().unwrap().form,
            Some(Form::Credentials { .. })
        ) {
            let mut output = paint(&ctx, &mut w, vec![]);
            output.textures_delta.clear();
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut output = paint(&ctx, &mut w, vec![]);
        let pos = output
            .shapes
            .iter()
            .find_map(|s| text(&s.shape, "Use saved credential"))
            .expect("Use saved credential button");
        output.textures_delta.clear();
        click(&ctx, &mut w, pos);
        w.poll_vault(&ctx);
        assert!(
            matches!(&w.tabs[1].desktop.as_ref().unwrap().form, Some(Form::Credentials { password, .. }) if password.as_str() == "fixture-password")
        );
        assert!(
            !w.tabs[1].desktop.as_ref().unwrap().connected,
            "Vault use must not submit authentication"
        );
        w.lock_vault();
        assert!(
            matches!(&w.tabs[1].desktop.as_ref().unwrap().form, Some(Form::Credentials { password, .. }) if password.is_empty())
        );
        w.unlock_vault(
            harbor_workspace::credentials::Secret::new("owned desktop master"),
            &ctx,
        );
        let end = Instant::now() + Duration::from_secs(20);
        while w.vault_busy() {
            w.poll_vault(&ctx);
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(5));
        }
        w.delete_vault_credential(reference, &ctx);
        let end = Instant::now() + Duration::from_secs(20);
        while w.vault_busy() {
            w.poll_vault(&ctx);
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(w.store.sessions[0].credential_ref.is_none());
    }
    #[test]
    fn real_rdp_surface_paints_inside_workspace_and_routes_focused_input() {
        let server = fixture::Server::start();
        let _ = fixture::engine_binary();
        let (ctx, _root, mut w) = owned_workspace();
        let session = server.config(8100).session;
        w.start(session);
        assert!(w.tabs[0].desktop.is_some());
        assert!(w.tabs[0].process.is_none());
        assert!(w.tabs[0].launch.is_none());
        let end = Instant::now() + Duration::from_secs(12);
        let mut trusted = false;
        let mut signed_in = false;
        let mut painted = false;
        while !painted {
            let mut output = paint(&ctx, &mut w, vec![]);
            if w.tabs[0].desktop.as_ref().unwrap().form.is_some() {
                assert!(w.start_macro_recording(0).is_err());
            }
            if !trusted {
                if let Some(position) = output
                    .shapes
                    .iter()
                    .find_map(|shape| text(&shape.shape, "Remember this certificate"))
                {
                    output.textures_delta.clear();
                    click(&ctx, &mut w, position);
                    trusted = true;
                }
            }
            if !signed_in {
                if matches!(
                    w.tabs[0].desktop.as_ref().unwrap().form,
                    Some(Form::Credentials { .. })
                ) {
                    if let Some(position) = output
                        .shapes
                        .iter()
                        .find_map(|shape| text(&shape.shape, "Connect securely"))
                    {
                        // Exercise the real focused password field and both entry paths,
                        // rather than assigning the form value behind the widget.
                        let mut typed = paint(&ctx, &mut w, vec![Event::Text("fixture-".into())]);
                        typed.textures_delta.clear();
                        let mut pasted = paint(&ctx, &mut w, vec![Event::Paste("password".into())]);
                        pasted.textures_delta.clear();
                        assert!(matches!(&w.tabs[0].desktop.as_ref().unwrap().form,
                            Some(Form::Credentials {password,..}) if password.as_str()=="fixture-password"));
                        output.textures_delta.clear();
                        click(&ctx, &mut w, position);
                        signed_in = true;
                    }
                }
            }
            for (_, deltas) in &output.textures_delta.set {
                for delta in deltas {
                    let egui::ImageData::Color(image) = &delta.image;
                    if image.pixels.first() == Some(&Color32::RED)
                        && (image.size == [640, 480]
                            || (delta.pos == Some([0, 0]) && image.size == [64, 64]))
                    {
                        painted = true;
                    }
                }
            }
            output.textures_delta.clear();
            assert!(
                w.tabs[0].desktop.as_ref().unwrap().error.is_empty(),
                "{}",
                w.tabs[0].desktop.as_ref().unwrap().error
            );
            assert!(Instant::now() < end, "RDP surface did not paint");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(trusted && signed_in);
        let before = w.tabs[0].desktop.as_ref().unwrap().canvas_rect.unwrap();
        let resized = vec2(1120., 720.);
        let mut output = paint_size(&ctx, &mut w, vec![], resized);
        output.textures_delta.clear();
        let canvas = w.tabs[0].desktop.as_ref().unwrap().canvas_rect.unwrap();
        assert!(
            canvas.width() < before.width(),
            "Resizing the app must resize its fitted desktop surface"
        );
        assert!((canvas.width() / canvas.height() - 640. / 480.).abs() < 0.001);
        let position = pos2(
            canvas.left() + canvas.width() * 45.5 / 640.,
            canvas.top() + canvas.height() * 27.5 / 480.,
        );
        click_size(&ctx, &mut w, position, resized);
        let mut output = paint_size(
            &ctx,
            &mut w,
            vec![
                Event::Key {
                    key: Key::Escape,
                    physical_key: Some(Key::Escape),
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::default(),
                },
                Event::Key {
                    key: Key::Escape,
                    physical_key: Some(Key::Escape),
                    pressed: false,
                    repeat: false,
                    modifiers: Modifiers::default(),
                },
                Event::Key {
                    key: Key::Tab,
                    physical_key: Some(Key::Tab),
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::default(),
                },
                Event::Key {
                    key: Key::Tab,
                    physical_key: Some(Key::Tab),
                    pressed: false,
                    repeat: false,
                    modifiers: Modifiers::default(),
                },
                Event::Key {
                    key: Key::ArrowRight,
                    physical_key: Some(Key::ArrowRight),
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::default(),
                },
                Event::Key {
                    key: Key::ArrowRight,
                    physical_key: Some(Key::ArrowRight),
                    pressed: false,
                    repeat: false,
                    modifiers: Modifiers::default(),
                },
                Event::Text("z".into()),
                Event::MouseWheel {
                    unit: MouseWheelUnit::Line,
                    delta: vec2(0., 1.),
                    phase: TouchPhase::Move,
                    modifiers: Modifiers::default(),
                },
            ],
            resized,
        );
        output.textures_delta.clear();
        let end = Instant::now() + Duration::from_secs(3);
        while !server.observations().contains("UnicodePressed(122)") {
            assert!(Instant::now() < end, "{}", server.observations());
            std::thread::sleep(Duration::from_millis(5));
        }
        let end = Instant::now() + Duration::from_secs(3);
        while !server
            .observations()
            .contains("mouse:VerticalScroll { value: 120 }")
        {
            assert!(Instant::now() < end, "{}", server.observations());
            std::thread::sleep(Duration::from_millis(5));
        }
        let ctrl = Modifiers {
            ctrl: true,
            ..Default::default()
        };
        let mut output = paint_size(
            &ctx,
            &mut w,
            vec![
                Event::ModifiersChanged(ctrl),
                Event::Key {
                    key: Key::C,
                    physical_key: Some(Key::C),
                    pressed: true,
                    repeat: false,
                    modifiers: ctrl,
                },
                Event::Key {
                    key: Key::C,
                    physical_key: Some(Key::C),
                    pressed: false,
                    repeat: false,
                    modifiers: ctrl,
                },
                Event::ModifiersChanged(Modifiers::default()),
            ],
            resized,
        );
        output.textures_delta.clear();
        let expected = [
            "key:Pressed { code: 29, extended: false }",
            "key:Pressed { code: 46, extended: false }",
            "key:Released { code: 46, extended: false }",
            "key:Released { code: 29, extended: false }",
        ];
        let end = Instant::now() + Duration::from_secs(3);
        loop {
            let observed = server.observations();
            if observed
                .lines()
                .collect::<Vec<_>>()
                .windows(4)
                .any(|window| window == expected)
            {
                break;
            }
            assert!(
                Instant::now() < end,
                "Ctrl+C ordering was not preserved: {observed}"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        // A floating app window owns pointer hits over the desktop beneath it.
        let overlay_pos = canvas.center() - vec2(80., 50.);
        let overlay = |ctx: &Context, w: &mut Workspace, events: Vec<Event>| {
            let mut clicked = false;
            let output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, resized)),
                    events,
                    ..Default::default()
                },
                |ui| {
                    w.render(ui);
                    Window::new("Owned overlay")
                        .id(Id::new("owned-rdp-overlay"))
                        .fixed_pos(overlay_pos)
                        .fixed_size(vec2(160., 80.))
                        .resizable(false)
                        .show(ui.ctx(), |ui| {
                            clicked = ui.button("Owned overlay button").clicked();
                        });
                },
            );
            (output, clicked)
        };
        let before_clicks = server
            .observations()
            .lines()
            .filter(|line| *line == "mouse:LeftPressed")
            .count();
        let (mut first, _) = overlay(&ctx, &mut w, vec![]);
        first.textures_delta.clear();
        let (mut output, _) = overlay(&ctx, &mut w, vec![]);
        let position = output
            .shapes
            .iter()
            .find_map(|shape| text(&shape.shape, "Owned overlay button"))
            .unwrap();
        assert!(canvas.contains(position));
        output.textures_delta.clear();
        let mut clicked = false;
        for pressed in [true, false] {
            let (mut output, did_click) = overlay(
                &ctx,
                &mut w,
                vec![
                    Event::PointerMoved(position),
                    Event::PointerButton {
                        pos: position,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::default(),
                    },
                ],
            );
            clicked |= did_click;
            output.textures_delta.clear();
        }
        assert!(clicked);
        let desktop = w.tabs[0].desktop.as_ref().unwrap();
        desktop
            .handle
            .send(DesktopInput::Key {
                code: 88,
                extended: false,
                pressed: true,
            })
            .unwrap();
        desktop
            .handle
            .send(DesktopInput::Key {
                code: 88,
                extended: false,
                pressed: false,
            })
            .unwrap();
        let end = Instant::now() + Duration::from_secs(3);
        while !server
            .observations()
            .contains("key:Released { code: 88, extended: false }")
        {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            server
                .observations()
                .lines()
                .filter(|line| *line == "mouse:LeftPressed")
                .count(),
            before_clicks,
            "A button in a covering window clicked the remote desktop underneath it"
        );
        let id = w.tabs[0].id;
        w.queue_macro(
            id,
            &Macro {
                name: "RDP macro".into(),
                script: Some(harbor_workspace::macros::Script {
                    version: 1,
                    events: vec![harbor_workspace::macros::Event::Input(MacroInput::Key {
                        code: 59,
                        pressed: true,
                    })],
                }),
                ..Macro::default()
            },
        )
        .unwrap();
        w.poll_macros(&ctx);
        let end = Instant::now() + Duration::from_secs(3);
        loop {
            let observations = server.observations();
            if observations
                .lines()
                .collect::<Vec<_>>()
                .windows(2)
                .any(|pair| {
                    pair == [
                        "key:Pressed { code: 59, extended: false }",
                        "key:Released { code: 59, extended: false }",
                    ]
                })
            {
                break;
            }
            assert!(
                Instant::now() < end,
                "RDP macro input did not arrive in order: {observations}"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let observed = server.observations();
        assert!(observed.contains("mouse:Move { x: 45, y: 27 }"));
        assert!(observed.contains("key:Pressed { code: 77, extended: true }"));
        assert!(observed.contains("key:Pressed { code: 1, extended: false }"));
        assert!(observed.contains("key:Pressed { code: 15, extended: false }"));
        let mut output = paint(&ctx, &mut w, vec![]);
        output.textures_delta.clear();
        assert_eq!(w.store.preferences.rdp_certificates.len(), 1);
        assert!(
            !std::fs::read_to_string(&w.store_path)
                .unwrap()
                .contains("fixture-password")
        );
        let prior_layouts = server
            .observations()
            .lines()
            .filter(|s| s.starts_with("client_resize:"))
            .count();
        let mut partial = false;
        for _ in 0..60 {
            let mut out = paint_size(&ctx, &mut w, vec![], resized);
            for (_, deltas) in &out.textures_delta.set {
                for delta in deltas {
                    let egui::ImageData::Color(image) = &delta.image;
                    partial |= delta.pos.is_some() && image.size == [64, 64];
                }
            }
            assert!(w.tabs[0].desktop.as_ref().unwrap().error.is_empty());
            out.textures_delta.clear();
            std::thread::sleep(Duration::from_millis(16));
        }
        assert!(
            partial,
            "ordinary bitmap changes must update only their texture region"
        );
        assert!(
            server
                .observations()
                .lines()
                .filter(|s| s.starts_with("client_resize:"))
                .count()
                <= prior_layouts + 1,
            "a stationary pane repeatedly renegotiated its display"
        );
        w.force_close_tab(0);
    }
}

#[cfg(test)]
mod clipboard_tests {
    use super::*;
    use harbor_workspace::desktop::rdp_options::ClipboardPolicy;
    #[test]
    fn remote_clipboard_is_bounded_directional_and_cleared_on_disconnect() {
        let mut state = ClipboardState::default();
        state
            .receive(ClipboardPolicy::FromRemote, "Remote 🐧".into())
            .unwrap();
        assert_eq!(
            state.remote.as_deref().map(|v| v.as_str()),
            Some("Remote 🐧")
        );
        assert!(
            state
                .receive(ClipboardPolicy::ToRemote, "private".into())
                .is_err()
        );
        assert!(state.remote.is_none());
        assert!(
            state
                .receive(ClipboardPolicy::FromRemote, "x".repeat(8193))
                .is_err()
        );
        state
            .receive(ClipboardPolicy::Bidirectional, "new text".into())
            .unwrap();
        state.ready = true;
        state.clear();
        assert!(state.remote.is_none());
        assert!(!state.ready);
    }
}

#[cfg(test)]
mod cursor_tests {
    use super::*;
    #[test]
    fn cursor_shape_is_bounded_and_hotspot_tracks_scaled_canvas() {
        assert!(RemoteCursor::new(2, 2, (0, 0), vec![0; 15]).is_err());
        assert!(RemoteCursor::new(513, 1, (0, 0), vec![0; 513 * 4]).is_err());
        assert!(RemoteCursor::new(2, 2, (2, 0), vec![0; 16]).is_err());
        assert!(RemoteCursor::new(0, 0, (0, 0), vec![]).unwrap().is_none());
        let cursor = RemoteCursor::new(8, 6, (3, 2), vec![255; 8 * 6 * 4])
            .unwrap()
            .unwrap();
        let rect = cursor.rect(pos2(100., 200.), 2.);
        assert_eq!(rect.min, pos2(94., 196.));
        assert_eq!(rect.size(), vec2(16., 12.));
    }
}

pub(super) fn toolbar(ui: &mut Ui, tab: &mut TerminalTab) {
    let Some(state) = &mut tab.desktop else {
        return;
    };
    ui.horizontal_wrapped(|ui| {
        ui.label(fluent::emphasized(ui.ctx(), RichText::new(&state.message)));
        ui.small(format!("{} · embedded display", tab.session.protocol));
    });
    if state.options.clipboard != harbor_workspace::desktop::rdp_options::ClipboardPolicy::Disabled
    {
        ui.horizontal_wrapped(|ui| {
            state.clipboard.controls(ui);
            ui.small(if state.clipboard.ready {
                "Text clipboard ready"
            } else {
                "Waiting for server clipboard channel…"
            }).on_hover_text("Paste with Ctrl+V in the desktop. Clipboard directions follow the saved session settings.");
        });
    }
}
