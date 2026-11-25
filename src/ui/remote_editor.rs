use super::*;
use harbor_workspace::remote_files::{
    RemotePath,
    editor::{CloseDecision, DocumentStatus, RemoteDocument, SavePolicy},
};
pub(super) struct RemoteEditor {
    pub(super) document: RemoteDocument,
    pub(super) id: u64,
    pub(super) recover_requested: bool,
    connection: u64,
    path: RemotePath,
    endpoint: String,
    text: String,
    loaded: bool,
    closing: bool,
    replace_confirm: bool,
    error: Option<String>,
}
impl RemoteEditor {
    fn show(&mut self, ctx: &Context) -> bool {
        let (status, dirty, recovery) = self.document.details();
        if !self.loaded
            && !matches!(
                status,
                DocumentStatus::Opening | DocumentStatus::Failed(_) | DocumentStatus::Disconnected
            )
        {
            self.text = self.document.snapshot().text;
            self.loaded = true;
        }
        let mut open = true;
        let mut remove = false;
        Window::new(format!("{}{} — Remote editor", if dirty { "● " } else { "" }, self.path.display()))
            .id(Id::new(("remote-editor", self.id))).open(&mut open)
            .default_size([740., 520.]).show(ctx, |ui| {
                ui.label(&self.endpoint);
                if ui.button("Compare recovery in active SSH session…").clicked() { self.recover_requested = true; }
                ui.label(self.path.display());
                ui.small("UTF-8 text · 2 MiB limit · local recovery snapshots are retained");
                ui.label(match &status {
                    DocumentStatus::Opening => "Opening…",
                    DocumentStatus::Ready => "Ready",
                    DocumentStatus::Recovering => "Writing local recovery snapshot…",
                    DocumentStatus::Saving => "Uploading staged save…",
                    DocumentStatus::Committing => "Publishing save…",
                    DocumentStatus::Indeterminate(message) => message,
                    DocumentStatus::Saved => "Saved",
                    DocumentStatus::Conflict => "The remote version changed. Your edits and the remote version are preserved.",
                    DocumentStatus::Cancelled => "Operation cancelled. A save may already have reached the server; compare the remote version before retrying. Local recovery is retained.",
                    DocumentStatus::Disconnected => "This connection ended. Your saved recovery copy is on this computer. Open the file again after reconnecting; compare your edits before saving.",
                    DocumentStatus::Failed(message) => message,
                });
                if let Some(path) = &recovery {
                    ui.horizontal_wrapped(|ui| {
                        ui.small(format!("Recovery: {}", path.display()));
                        if ui.small_button("Copy path").clicked() { ui.ctx().copy_text(path.display().to_string()); }
                    });
                }
                let (remote_recovery, warning) = self.document.recovery_details();
                if let Some(path) = remote_recovery { ui.small(format!("Remote staging recovery: {}", path.display())); }
                if let Some(warning) = warning { ui.colored_label(ui.visuals().warn_fg_color, warning); }
                if let Some(error) = &self.error { ui.colored_label(Color32::LIGHT_RED, error); }
                ui.horizontal(|ui| {
                    if status.busy() && status != DocumentStatus::Committing && ui.button("Cancel operation").clicked() { self.document.cancel_operation(); }
                    if ui.add_enabled(self.loaded && !status.busy(), Button::new("Save")).clicked() {
                        self.error = self.document.save(SavePolicy::VerifyUnchanged).err();
                    }
                    if status == DocumentStatus::Conflict && ui.button("Replace remote version…").clicked() { self.replace_confirm = true; }
                });
                if self.replace_confirm {
                    ui.label("Replace the currently observed remote file with your edited text? A new remote change detected during upload still stops the save.");
                    ui.horizontal(|ui| {
                        if ui.add_enabled(!status.busy(), Button::new("Confirm replacement")).clicked() {
                            self.error = self.document.save(SavePolicy::Overwrite).err(); self.replace_confirm = false;
                        }
                        if ui.button("Cancel replacement").clicked() { self.replace_confirm = false; }
                    });
                }
                ScrollArea::both().id_salt(("remote-editor-text", self.id)).show(ui, |ui| {
                    if ui.add_enabled(self.loaded, TextEdit::multiline(&mut self.text).id(Id::new(("remote-editor-text-field", self.id))).font(TextStyle::Monospace).desired_width(f32::INFINITY).desired_rows(20)).changed() {
                        if let Err(error) = self.document.set_text(&self.text) {
                            self.error = Some(error); self.text = self.document.snapshot().text;
                        }
                    }
                });
                if self.closing {
                    if { let (status, dirty, _) = self.document.details(); !dirty && !status.busy() } { remove = true; }
                    else {
                        ui.separator(); ui.label("Save changes before closing this remote document?");
                        ui.horizontal(|ui| {
                            if ui.add_enabled(!status.busy(), Button::new("Save and close")).clicked() {
                                self.error = self.document.request_close(CloseDecision::Save).err();
                            }
                            if ui.add_enabled(!status.busy(), Button::new("Discard edits")).clicked() {
                                remove = self.document.request_close(CloseDecision::Discard).unwrap_or(false);
                            }
                            if ui.button("Cancel close").clicked() { self.closing = false; }
                        });
                    }
                }
            });
        if !open {
            let (current_status, current_dirty, _) = self.document.details();
            if !current_dirty && !current_status.busy() {
                remove = true;
            } else {
                self.closing = true;
            }
        }
        remove
    }
}
impl Workspace {
    pub(super) fn open_remote_editor(&mut self, connection: u64, path: RemotePath) {
        if self
            .remote_editors
            .iter()
            .any(|e| e.connection == connection && e.path == path)
        {
            return;
        }
        if self.remote_editors.len() >= 8 {
            self.message("Close a remote document before opening another (8 document limit).");
            return;
        }
        let result = self
            .tabs
            .iter()
            .find(|t| {
                t.id == connection && t.ssh_connected && t.transfer_ui.effective_capabilities().edit
            })
            .and_then(|tab| tab.ssh.as_ref().map(|ssh| (tab, ssh)))
            .ok_or_else(|| "Wait for the SSH connection before opening a file".to_owned())
            .and_then(|(tab, ssh)| {
                let document = RemoteDocument::open(
                    connection,
                    path.clone(),
                    &ssh.transfers,
                    self.store_path.parent().unwrap().join("remote-recovery"),
                )?;
                static NEXT_DOCUMENT: std::sync::atomic::AtomicU64 =
                    std::sync::atomic::AtomicU64::new(1);
                Ok(RemoteEditor {
                    id: NEXT_DOCUMENT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
                    recover_requested: false,
                    document,
                    connection,
                    path,
                    endpoint: format!(
                        "{} · {}@{}:{} · connection {}",
                        tab.session.name,
                        tab.session.user,
                        tab.session.host,
                        tab.session.port,
                        connection
                    ),
                    text: String::new(),
                    loaded: false,
                    closing: false,
                    replace_confirm: false,
                    error: None,
                })
            });
        match result {
            Ok(editor) => self.remote_editors.push(editor),
            Err(error) => self.message(error),
        }
    }
    fn ftp_documents_block_close(&self, connection: Option<u64>) -> bool {
        self.tabs
            .iter()
            .filter(|tab| connection.is_none_or(|id| tab.id == id))
            .any(|tab| {
                tab.ftp
                    .as_ref()
                    .is_some_and(|ftp| ftp.has_pending_documents())
            })
    }

    fn ssh_documents_block_close(&self, connection: Option<u64>) -> bool {
        self.remote_editors
            .iter()
            .filter(|editor| connection.is_none_or(|id| editor.connection == id))
            .any(|editor| {
                let (status, dirty, _) = editor.document.details();
                dirty || status.busy()
            })
    }

    pub(super) fn remote_documents_block_close(&self, connection: Option<u64>) -> bool {
        self.ftp_documents_block_close(connection) || self.ssh_documents_block_close(connection)
    }

    pub(super) fn intercept_remote_close(&mut self, ctx: &Context) {
        if self.block_import_close(ctx) {
            return;
        }
        let blocked = self.remote_documents_block_close(None);
        if blocked {
            self.allow_remote_exit = false;
            self.allow_session_exit = false;
        }
        if ctx.input(|i| i.viewport().close_requested()) && blocked {
            self.begin_ftp_document_close(None);
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(ViewportCommand::Focus);
            self.pending_remote_exit = true;
        }
    }

    pub(super) fn begin_ftp_document_close(&mut self, connection: Option<u64>) {
        for tab in self
            .tabs
            .iter_mut()
            .filter(|tab| connection.is_none_or(|id| tab.id == id))
        {
            if let Some(ftp) = &mut tab.ftp {
                ftp.close_ready();
            }
        }
    }

    fn cancel_remote_close(&mut self, connection: Option<u64>) {
        self.pending_remote_tab_close = None;
        self.pending_remote_exit = false;
        for tab in self
            .tabs
            .iter_mut()
            .filter(|tab| connection.is_none_or(|id| tab.id == id))
        {
            if let Some(ftp) = &mut tab.ftp {
                ftp.cancel_close_request();
            }
        }
    }

    fn remote_import_dialog(&mut self, ctx: &Context) {
        let Some((source_id, target_id)) = self.pending_remote_import else {
            return;
        };
        let Some(source) = self.remote_editors.iter().position(|e| e.id == source_id) else {
            self.pending_remote_import = None;
            return;
        };
        let Some(target) = self.remote_editors.iter().position(|e| e.id == target_id) else {
            self.pending_remote_import = None;
            return;
        };
        let (status, _, _) = self.remote_editors[target].document.details();
        if status.busy() {
            return;
        }
        let permitted = self.remote_editors[target]
            .document
            .can_import_from(&self.remote_editors[source].document);
        let mut import = false;
        let mut cancel = false;
        Window::new("Compare recovery with freshly opened remote file").id(Id::new("remote-editor-import")).collapsible(false).show(ctx, |ui| {
            ui.label(&self.remote_editors[source].endpoint);
            ui.label(&self.remote_editors[target].endpoint);
            if permitted { ui.label("The authenticated SSH endpoint, verified server keys, username, gateway and path match. Review both editor windows before importing. Import copies your text into the fresh editor; Save is a separate action."); }
            else { ui.label("The authenticated endpoints or verified server keys do not match, a document did not open successfully, or the destination has unsaved edits. Recovery cannot be imported into this connection."); }
            ui.horizontal(|ui| { import = ui.add_enabled(permitted, Button::new("Import recovered edits")).clicked(); cancel = ui.button("Cancel import").clicked(); });
        });
        if import {
            let (target_editor, source_editor) = if target < source {
                let (left, right) = self.remote_editors.split_at_mut(source);
                (&mut left[target], &right[0])
            } else {
                let (left, right) = self.remote_editors.split_at_mut(target);
                (&mut right[0], &left[source])
            };
            match target_editor
                .document
                .import_recovery_from(&source_editor.document)
            {
                Ok(()) => target_editor.loaded = false,
                Err(error) => target_editor.error = Some(error),
            }
        }
        if import || cancel {
            self.pending_remote_import = None;
        }
    }
    pub(super) fn remote_editor_windows(&mut self, ctx: &Context) {
        self.intercept_remote_close(ctx);
        let mut ftp_close_cancelled = false;
        for tab in &mut self.tabs {
            if let Some(ftp) = &mut tab.ftp {
                ftp.document_window(ctx);
                if ftp.take_close_cancelled()
                    && (self.pending_remote_exit || self.pending_remote_tab_close == Some(tab.id))
                {
                    ftp_close_cancelled = true;
                }
            }
        }
        if ftp_close_cancelled {
            let connection = if self.pending_remote_exit {
                None
            } else {
                self.pending_remote_tab_close
            };
            self.cancel_remote_close(connection);
        }
        self.remote_editors.retain_mut(|editor| !editor.show(ctx));
        let request = self
            .remote_editors
            .iter_mut()
            .find(|editor| editor.recover_requested)
            .map(|editor| {
                editor.recover_requested = false;
                (editor.id, editor.path.clone())
            });
        if let Some((source, path)) = request {
            if let Some(tab) = self.tabs.get(self.active).filter(|t| t.ssh_connected) {
                let connection = tab.id;
                self.open_remote_editor(connection, path.clone());
                if let Some(target) = self
                    .remote_editors
                    .iter()
                    .find(|e| e.connection == connection && e.path == path && e.id != source)
                {
                    self.pending_remote_import = Some((source, target.id));
                } else {
                    self.message("Choose a newly connected SSH tab before comparing recovery.");
                }
            } else {
                self.message(
                    "Reconnect or open the matching SSH session before comparing recovery.",
                );
            }
        }
        self.remote_import_dialog(ctx);
        if self.pending_remote_tab_close.is_none() && !self.pending_remote_exit {
            return;
        }
        let connection = if self.pending_remote_exit {
            None
        } else {
            self.pending_remote_tab_close
        };
        let mut discard = false;
        let mut cancel = false;
        let ftp_pending = self.ftp_documents_block_close(connection);
        if self.ssh_documents_block_close(connection) {
            Window::new("Remote documents have unsaved or pending work").id(Id::new("remote-documents-close"))
            .collapsible(false).show(ctx, |ui| {
                ui.label("Save the documents below or explicitly discard their edits before closing this connection or app. Failed and conflicting saves keep this prompt open.");
                if ftp_pending {
                    ui.label("Use each FTP editor's close controls for its pending document.");
                }
                let mut busy = false;
                for editor in self.remote_editors.iter().filter(|e| connection.is_none_or(|id| e.connection == id)) {
                    let (status, dirty, _) = editor.document.details(); busy |= status.busy();
                    ui.label(format!("{} · {:?}{}", editor.path.display(), status, if dirty { " · unsaved" } else { "" }));
                }
                ui.horizontal(|ui| {
                    if ui.add_enabled(!busy, Button::new("Save all and close")).clicked() {
                        let mut errors = Vec::new();
                        for editor in self.remote_editors.iter_mut().filter(|e| connection.is_none_or(|id| e.connection == id)) {
                            if editor.document.details().1 { if let Err(e) = editor.document.save(SavePolicy::VerifyUnchanged) { errors.push(e); } }
                        }
                        if !errors.is_empty() { self.message(errors.join("; ")); }
                    }
                    discard = ui.add_enabled(!busy, Button::new("Discard edits and close")).clicked();
                    cancel = ui.button("Cancel close").clicked();
                });
            });
        }
        if cancel {
            self.cancel_remote_close(connection);
            return;
        }
        if discard {
            self.remote_editors
                .retain(|editor| connection.is_some_and(|id| editor.connection != id));
        }
        if !self.remote_documents_block_close(connection) {
            if self.pending_remote_exit {
                self.allow_remote_exit = true;
                self.request_window_session_close(ctx);
            } else if let Some(index) = self.tabs.iter().position(|t| Some(t.id) == connection) {
                self.request_active_tab_close(index);
            }
            self.pending_remote_tab_close = None;
            self.pending_remote_exit = false;
        }
    }
}
