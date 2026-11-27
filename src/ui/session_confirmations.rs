use super::*;
use harbor_workspace::terminal::paste::{
    CloseController, CloseDecision, PasteController, PasteDecision,
};
use std::collections::VecDeque;
pub(super) enum CloseTarget {
    Tab(u64),
    Reconnect(u64),
    Window,
}
pub(super) struct PendingClose {
    target: CloseTarget,
    controller: CloseController,
}
impl PendingClose {
    pub fn reconnect(id: u64, controller: CloseController) -> Self {
        Self {
            target: CloseTarget::Reconnect(id),
            controller,
        }
    }
}
const MAX_AFTER_PASTE: usize = 32 * 1024;

#[derive(Clone)]
pub(super) enum DeferredInput {
    Bytes(Vec<u8>),
    LegacyEnter,
}

#[derive(Clone, Default)]
pub(super) struct AfterPaste {
    actions: VecDeque<DeferredInput>,
    len: usize,
}

impl AfterPaste {
    pub(super) fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    pub(super) fn push(&mut self, input: DeferredInput) -> bool {
        let input_len = match &input {
            DeferredInput::Bytes(bytes) => bytes.len(),
            DeferredInput::LegacyEnter => 1,
        };
        if self.len.saturating_add(input_len) > MAX_AFTER_PASTE {
            return false;
        }
        self.len += input_len;
        match input {
            DeferredInput::Bytes(bytes) if !bytes.is_empty() => {
                if let Some(DeferredInput::Bytes(previous)) = self.actions.back_mut() {
                    previous.extend(bytes);
                } else {
                    self.actions.push_back(DeferredInput::Bytes(bytes));
                }
            }
            DeferredInput::Bytes(_) => {}
            DeferredInput::LegacyEnter => self.actions.push_back(DeferredInput::LegacyEnter),
        }
        true
    }

    fn validate_for(&self, tab: &TerminalTab) -> Result<(), String> {
        for action in &self.actions {
            match action {
                DeferredInput::Bytes(bytes) => {
                    let text = std::str::from_utf8(bytes)
                        .map_err(|_| "Queued text input is not valid UTF-8.")?;
                    tab.codec.encode(text)?;
                }
                DeferredInput::LegacyEnter if tab.legacy.is_none() => {
                    return Err("The original legacy terminal is no longer connected.".into());
                }
                DeferredInput::LegacyEnter => {}
            }
        }
        Ok(())
    }
}

impl From<Vec<u8>> for AfterPaste {
    fn from(bytes: Vec<u8>) -> Self {
        let len = bytes.len();
        let mut actions = VecDeque::new();
        if len != 0 {
            actions.push_back(DeferredInput::Bytes(bytes));
        }
        Self { actions, len }
    }
}

pub(super) struct InputPaste {
    pub text: String,
    pub after: AfterPaste,
}
pub(super) struct PendingPaste {
    jobs: Vec<(u64, PasteController)>,
    after: AfterPaste,
}
impl TerminalTab {
    /// External network clients own an opaque interactive authentication exchange.
    /// Their terminal remains usable privately, but cannot join a broadcast batch.
    pub(super) fn broadcast_allowed(&self) -> bool {
        self.ssh.is_some() || matches!(self.session.protocol.as_str(), "Local" | "Serial")
    }
    pub(super) fn ready_for_input(&self) -> bool {
        if self.terminal_starting()
            || self.web_browser.is_some()
            || self.ftp.is_some()
            || self.desktop.is_some()
            || self.auth.is_some()
            || self
                .legacy
                .as_ref()
                .is_some_and(|legacy| !legacy.is_ready())
            || (self.gateway.is_some() && self.legacy.is_none())
        {
            return false;
        }
        self.running()
            && !self.input_failed
            && (self.ssh.is_none()
                || self.ssh_connected
                    && self.auth.is_none()
                    && !self.ssh.as_ref().unwrap().is_finished())
    }
    pub(super) fn input_busy(&self) -> bool {
        self.paste.is_some()
            || self.paste_end.is_some()
            || self.paste_waiting
            || self.pending_mouse.is_some()
            || self.after_paste.is_some()
            || !self.pending_replies.is_empty()
    }
    pub(super) fn tick_paste(&mut self, now: Instant) {
        if !self.ready_for_input() {
            self.paste = None;
            self.paste_end = None;
            self.after_paste = None;
            self.pending_replies.clear();
            return;
        }
        if let Some(bytes) = self.paste_end.take() {
            match self.transmit(&bytes, false) {
                Ok(()) => {}
                Err(error) if harbor_workspace::terminal::paste::retryable_input_error(&error) => {
                    self.paste_end = Some(bytes);
                    return;
                }
                Err(error) => {
                    self.after_paste = None;
                    self.error = error;
                    return;
                }
            }
        }
        let Some(mut paste) = self.paste.take() else {
            self.flush_after_paste();
            return;
        };
        let bytes = paste.tick(now);
        if !bytes.is_empty() {
            if let Err(error) = self.send_user_bytes(&bytes, false) {
                paste.retry_last_chunk();
                self.error = error.clone();
                if !harbor_workspace::terminal::paste::retryable_input_error(&error) {
                    self.macro_state.reject_paste();
                    self.paste_end = None;
                    self.after_paste = None;
                    return;
                }
            } else if self.recording {
                let text = bytes.strip_prefix(b"\x1b[200~").unwrap_or(&bytes);
                let text = text.strip_suffix(b"\x1b[201~").unwrap_or(text);
                let text = std::str::from_utf8(text).expect("paste chunks preserve UTF-8");
                self.recorded.push_str(text);
            }
        }
        if !paste.finished() {
            self.paste = Some(paste);
        } else {
            if self.macro_state.owns_paste() {
                self.macro_state.input_submitted();
            } else if self.macro_state.records_terminal() {
                self.record_macro_text(paste.text());
            }
            self.flush_after_paste();
        }
    }
    pub(super) fn flush_protocol_replies(&mut self) {
        if !self.ready_for_input() {
            self.pending_replies.clear();
            return;
        }
        if self.paste.is_some()
            || self.paste_end.is_some()
            || self.after_paste.is_some()
            || self.pending_mouse.is_some()
            || self.pending_replies.is_empty()
        {
            return;
        }
        let bytes = std::mem::take(&mut self.pending_replies);
        if let Err(error) = self.transmit(&bytes, false) {
            if harbor_workspace::terminal::paste::retryable_input_error(&error) {
                self.pending_replies = bytes;
            }
            self.error = error;
        }
    }
    pub(super) fn send_legacy_enter(&mut self) -> Result<(), String> {
        self.legacy
            .as_ref()
            .ok_or("The original legacy terminal is no longer connected.")?
            .enter()?;
        if self.recording {
            self.recorded.push('\r');
        }
        Ok(())
    }

    fn flush_after_paste(&mut self) {
        if self.paste.is_some() || self.paste_end.is_some() {
            return;
        }
        if let Some(mut after) = self.after_paste.take() {
            while let Some(action) = after.actions.pop_front() {
                let result = match &action {
                    DeferredInput::Bytes(bytes) => self.send_user_bytes(bytes, true),
                    DeferredInput::LegacyEnter => self.send_legacy_enter(),
                };
                if let Err(error) = result {
                    if harbor_workspace::terminal::paste::retryable_input_error(&error) {
                        after.actions.push_front(action);
                        self.after_paste = Some(after);
                    }
                    self.error = error;
                    return;
                }
            }
        }
        self.flush_protocol_replies();
    }
    pub(super) fn cancel_paste(&mut self) {
        self.macro_state.reject_paste();
        self.after_paste = None;
        if let Some(mut paste) = self.paste.take() {
            let bytes = paste.cancel();
            if !bytes.is_empty() {
                self.paste_end = Some(bytes);
            }
        }
    }
}
impl Workspace {
    pub(super) fn request_active_tab_close(&mut self, index: usize) {
        let mut controller = CloseController::default();
        if controller.begin(
            self.tabs[index].running(),
            self.store.preferences.confirm_session_close,
        ) == CloseDecision::Confirm
        {
            self.pending_session_close = Some(PendingClose {
                target: CloseTarget::Tab(self.tabs[index].id),
                controller,
            });
        } else {
            self.force_close_tab(index);
        }
    }
    pub(super) fn request_window_session_close(&mut self, ctx: &Context) {
        if self.intercept_game_exit(ctx) || self.confirm_local_document_exit(ctx) {
            return;
        }
        let mut controller = CloseController::default();
        if controller.begin(
            self.tabs.iter().any(TerminalTab::running),
            self.store.preferences.confirm_session_close,
        ) == CloseDecision::Confirm
        {
            self.pending_session_close = Some(PendingClose {
                target: CloseTarget::Window,
                controller,
            });
        } else {
            self.allow_session_exit = true;
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
    }
    pub(super) fn intercept_session_close(&mut self, ctx: &Context) {
        if self.block_import_close(ctx) {
            return;
        }
        if ctx.input(|i| i.viewport().close_requested()) && self.intercept_game_exit(ctx) {
            return;
        }
        if ctx.input(|i| i.viewport().close_requested())
            && !self.pending_remote_exit
            && !self.remote_documents_block_close(None)
            && self.confirm_local_document_exit(ctx)
        {
            return;
        }
        if ctx.input(|i| i.viewport().close_requested())
            && !self.allow_session_exit
            && !self.pending_remote_exit
            && !self.remote_documents_block_close(None)
        {
            let mut controller = CloseController::default();
            if controller.begin(
                self.tabs.iter().any(TerminalTab::running),
                self.store.preferences.confirm_session_close,
            ) == CloseDecision::Confirm
            {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(ViewportCommand::Focus);
                self.pending_session_close = Some(PendingClose {
                    target: CloseTarget::Window,
                    controller,
                });
            }
        }
    }
    pub(super) fn resolve_session_close(&mut self, approve: bool, ctx: &Context) {
        let Some(mut pending) = self.pending_session_close.take() else {
            return;
        };
        if pending.controller.resolve(approve) != CloseDecision::Close {
            self.allow_session_exit = false;
            self.allow_remote_exit = false;
            return;
        }
        match pending.target {
            CloseTarget::Tab(id) => {
                if let Some(index) = self.tabs.iter().position(|t| t.id == id) {
                    if self.remote_documents_block_close(Some(id)) {
                        self.begin_ftp_document_close(Some(id));
                        self.pending_remote_tab_close = Some(id);
                    } else {
                        self.force_close_tab(index);
                    }
                }
            }
            CloseTarget::Reconnect(id) => {
                if let Some(index) = self.tabs.iter().position(|t| t.id == id) {
                    self.reconnect_tab_now(index);
                }
            }
            CloseTarget::Window => {
                if self.intercept_game_exit(ctx) {
                    return;
                }
                if self.remote_documents_block_close(None) {
                    self.begin_ftp_document_close(None);
                    self.allow_remote_exit = false;
                    self.allow_session_exit = false;
                    self.pending_remote_exit = true;
                    ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                    return;
                }
                if self.confirm_local_document_exit(ctx) {
                    return;
                }
                self.allow_session_exit = true;
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        }
    }
    pub(super) fn paste_targets(&self, source: u64) -> Vec<u64> {
        if self.broadcast
            && self
                .tabs
                .iter()
                .any(|t| t.id == source && t.broadcast_allowed())
        {
            self.broadcast_group
                .targets(source)
                .into_iter()
                .filter(|id| {
                    self.tabs
                        .iter()
                        .any(|t| t.id == *id && t.broadcast_allowed())
                })
                .collect()
        } else {
            vec![source]
        }
    }
    pub(super) fn queue_paste(&mut self, source: u64, text: String) {
        self.queue_paste_to(self.paste_targets(source), text, AfterPaste::default());
    }
    pub(super) fn queue_paste_to(
        &mut self,
        mut ids: Vec<u64>,
        text: String,
        after: impl Into<AfterPaste>,
    ) {
        let after = after.into();
        ids.sort_unstable();
        ids.dedup();
        if after.len > MAX_AFTER_PASTE {
            self.message("Too much typed input followed the paste; the paste was cancelled.");
            return;
        }
        if self.pending_paste.is_some() {
            self.message("Finish or cancel the pending paste confirmation first.");
            return;
        }
        let targets: Vec<_> = ids
            .iter()
            .filter_map(|id| self.tabs.iter().find(|t| t.id == *id))
            .collect();
        if targets.len() != ids.len() {
            self.message("Paste cancelled: an original target has closed.");
            return;
        }
        if targets.len() > 1 && targets.iter().any(|t| !t.broadcast_allowed()) {
            self.message("Paste cancelled: compatibility clients require private input because their authentication is not observable.");
            return;
        }
        if targets.is_empty()
            || targets.iter().any(|t| {
                !t.ready_for_input()
                    || t.input_busy()
                    || t.ssh.is_some() && (!t.ssh_connected || t.auth.is_some())
            })
        {
            self.message("Paste was not started: every target must be connected and ready, with no paste already active.");
            return;
        }
        if text
            .len()
            .saturating_add(after.len)
            .saturating_mul(targets.len())
            > 16 * 1024 * 1024
        {
            self.message(
                "Paste target batch exceeds 16 MiB. Select fewer targets or paste less text.",
            );
            return;
        }
        let mut jobs = Vec::new();
        let mut confirmation = false;
        for tab in targets {
            if let Err(error) = tab
                .codec
                .encode(&text)
                .and_then(|_| after.validate_for(tab))
            {
                self.message(error);
                return;
            }
            let settings = harbor_workspace::terminal::paste::PasteSettings {
                bracketed: tab.screen.bracketed_paste,
                ..self.store.preferences.paste
            };
            match PasteController::begin(text.clone(), settings) {
                PasteDecision::Send(c) => jobs.push((tab.id, c)),
                PasteDecision::Confirm(c) => {
                    confirmation = true;
                    jobs.push((tab.id, c));
                }
                PasteDecision::Reject(error) => {
                    self.message(error);
                    return;
                }
            }
        }
        for (id, _) in &jobs {
            if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == *id) {
                tab.paste_waiting = true;
            }
        }
        self.pending_paste = Some(PendingPaste { jobs, after });
        if !confirmation {
            self.resolve_paste(true);
        }
    }
    pub(super) fn resolve_paste(&mut self, approve: bool) {
        let Some(mut pending) = self.pending_paste.take() else {
            return;
        };
        let valid = pending.jobs.iter().all(|(id, paste)| {
            self.tabs.iter().find(|t| t.id == *id).is_some_and(|t| {
                t.ready_for_input()
                    && t.codec.encode(paste.text()).is_ok()
                    && pending.after.validate_for(t).is_ok()
                    && t.after_paste.is_none()
                    && t.pending_replies.is_empty()
                    && t.paste.is_none()
                    && t.paste_end.is_none()
                    && t.pending_mouse.is_none()
                    && (t.ssh.is_none() || t.ssh_connected && t.auth.is_none())
            })
        });
        for (id, _) in &pending.jobs {
            if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == *id) {
                tab.paste_waiting = false;
                if !approve || !valid {
                    tab.macro_state.reject_paste();
                }
            }
        }
        if !approve {
            return;
        }
        if !valid {
            self.message(
                "Paste cancelled: its original target set is no longer ready. No paste was sent.",
            );
            return;
        }
        let now = Instant::now();
        for (id, mut paste) in pending.jobs.drain(..) {
            if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id) {
                paste.confirm_for_terminal(now, tab.screen.bracketed_paste);
                let release = tab.mouse.suspend(tab.screen.mouse_mode());
                if !release.is_empty() {
                    tab.paste_end = Some(release);
                }
                tab.after_paste = (!pending.after.is_empty()).then(|| pending.after.clone());
                tab.paste = Some(paste);
            }
        }
    }
    fn confirmation_target(&self, id: u64) -> String {
        let Some(tab) = self.tabs.iter().find(|t| t.id == id) else {
            return format!("Closed terminal #{id}");
        };
        let endpoint = if tab.session.host.is_empty() {
            "local".to_owned()
        } else {
            format!("{}:{}", tab.session.host, tab.session.port)
        };
        format!(
            "{} · {} {} · #{id}",
            self.terminal_title(id),
            tab.session.protocol,
            endpoint
        )
    }
    pub(super) fn session_confirmations(&mut self, ctx: &Context) {
        if let Some(pending) = &self.pending_paste {
            let mut approve = false;
            let mut cancel = false;
            let size = super::fluent::dialog_size(ctx, 480., 114.);
            let response = super::fluent::modal(
                ctx,
                Id::new("paste-confirmation"),
                "Confirm multiline paste",
                size,
                |ui| {
                    super::fluent::dialog_body(ui, size, |ui| {
                        ui.label(format!("Send this text to {} original terminal target(s)? Newlines can execute commands.", pending.jobs.len()));
                        for (id, _) in &pending.jobs {
                            if self.tabs.iter().any(|tab| tab.id == *id) {
                                ui.label(self.confirmation_target(*id));
                            }
                        }
                        if let Some((_, paste)) = pending.jobs.first() {
                            ui.label(RichText::new(paste.preview()).monospace());
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        approve = ui.button("Paste to these targets").clicked();
                        let safe = primary(ui, "Cancel paste");
                        super::fluent::focus_on_entry(ui, &safe, ui.id());
                        cancel = safe.clicked();
                    });
                },
            );
            cancel |= response.should_close();
            if approve || cancel {
                self.resolve_paste(approve && !cancel);
            }
            return;
        }
        if let Some(pending) = &self.pending_session_close {
            let mut approve = false;
            let mut cancel = false;
            let reconnect = matches!(pending.target, CloseTarget::Reconnect(_));
            let size = super::fluent::dialog_size(ctx, 480., 114.);
            let response = super::fluent::modal(
                ctx,
                Id::new("active-session-close"),
                if reconnect {
                    "Reconnect active session?"
                } else {
                    "Close active sessions?"
                },
                size,
                |ui| {
                    super::fluent::dialog_body(ui, size, |ui| {
                        ui.label(if matches!(pending.target,CloseTarget::Reconnect(_)){"Reconnect terminates the current process or connection before starting a new session."}else{"Closing terminates the session processes and connections."});
                        for tab in self.tabs.iter().filter(|t| match pending.target {
                            CloseTarget::Window => t.running(),
                            CloseTarget::Tab(id) | CloseTarget::Reconnect(id) => t.id == id,
                        }) {
                            ui.label(self.confirmation_target(tab.id));
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        approve = ui
                            .button(if reconnect {
                                "Reconnect session"
                            } else {
                                "Close sessions"
                            })
                            .clicked();
                        let safe = primary(
                            ui,
                            if reconnect {
                                "Stay connected"
                            } else {
                                "Keep sessions open"
                            },
                        );
                        super::fluent::focus_on_entry(ui, &safe, ui.id());
                        cancel = safe.clicked();
                    });
                },
            );
            cancel |= response.should_close();
            if approve || cancel {
                self.resolve_session_close(approve && !cancel, ctx);
            }
        }
    }
}
