use eframe::egui::{self, Button, Context, Id, ScrollArea, TextEdit, Ui};
use harbor_workspace::{
    credentials::Secret,
    protocols::{
        ftp::{FtpOptions, FtpTlsMode, FtpTransferType},
        ftp_actor::{SessionOptions, Workspace as FtpWorkspace},
    },
    remote_files::{
        EntryKind, RemoteEntry, RemotePath,
        editor::{CloseDecision, DocumentStatus, RemoteDocument, SavePolicy},
        transfer::{ConflictPolicy, RecoveryPath, TransferEvent, TransferOperation},
    },
};
use std::{collections::BTreeMap, path::PathBuf, sync::mpsc, time::Duration};
use zeroize::Zeroizing;

pub(super) struct State {
    connection: u64,
    host: String,
    port: u16,
    username: String,
    options: SessionOptions,
    recovery_root: PathBuf,
    password: Zeroizing<String>,
    actor: Option<FtpWorkspace>,
    address: String,
    selected: Option<RemoteEntry>,
    transfers: BTreeMap<u64, TransferEvent>,
    pending: Option<Pending>,
    dialog: Option<mpsc::Receiver<Option<Pending>>>,
    editor: Option<Editor>,
    recovery_candidate: Option<RemoteDocument>,
    comparison_dismissed: bool,
    generation: u64,
    close_cancelled: bool,
    message: Option<String>,
}

enum Pending {
    Upload {
        local: PathBuf,
        directory: RemotePath,
        name: String,
        folder: bool,
    },
    Download {
        remote: RemotePath,
        local: PathBuf,
        folder: bool,
    },
    Mkdir {
        directory: RemotePath,
        name: String,
    },
    Rename {
        source: RemotePath,
        name: String,
    },
    Delete {
        path: RemotePath,
        directory: bool,
        contents: bool,
        typed: String,
    },
}

impl State {
    pub(super) fn new(
        connection: u64,
        host: String,
        port: u16,
        username: String,
        options: SessionOptions,
        recovery_root: PathBuf,
    ) -> Self {
        Self {
            connection,
            host,
            port,
            username,
            options,
            recovery_root,
            password: Zeroizing::new(String::new()),
            actor: None,
            address: String::new(),
            selected: None,
            transfers: BTreeMap::new(),
            pending: None,
            dialog: None,
            editor: None,
            recovery_candidate: None,
            comparison_dismissed: false,
            generation: 0,
            close_cancelled: false,
            message: None,
        }
    }

    pub(super) fn status(&self) -> &str {
        self.actor
            .as_ref()
            .map_or("FTP ready to connect", FtpWorkspace::status)
    }

    pub(super) fn poll(&mut self) {
        let mut refresh = false;
        if let Some(actor) = &mut self.actor {
            let previous = actor.browser.path.clone();
            for event in actor.poll() {
                refresh |= event.id != 0 && event.outcome.is_some();
                self.transfers.insert(event.id, event);
            }
            if previous != actor.browser.path {
                self.address =
                    String::from_utf8(actor.browser.path.as_bytes().to_vec()).unwrap_or_default();
                self.selected = None;
            }
            while self.transfers.len() > 48 {
                let Some(id) = self
                    .transfers
                    .values()
                    .find(|event| event.outcome.is_some())
                    .map(|event| event.id)
                else {
                    break;
                };
                self.transfers.remove(&id);
            }
            if refresh
                && actor.ready()
                && let Some(path) = actor.browser.refresh_path()
            {
                let _ = actor.navigate(path);
            }
        }
        if let Some(dialog) = &self.dialog {
            match dialog.try_recv() {
                Ok(result) => {
                    self.pending = result;
                    self.dialog = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.dialog = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
    }

    fn connect(&mut self) {
        let password = std::mem::take(&mut *self.password);
        let options = FtpOptions {
            host: self.host.clone(),
            port: self.port,
            username: self.username.clone(),
            password: (!password.is_empty()).then(|| Secret::new(password)),
            tls: self.options.tls,
            protect_data: self.options.protect_data,
            data_mode: self.options.data_mode,
            certificate_authority: self.options.certificate_authority.clone(),
            timeout: Duration::from_secs(20),
        };
        match FtpWorkspace::start(self.connection, options, self.options.transfer_type) {
            Ok(actor) => {
                self.actor = Some(actor);
                self.generation = self.generation.wrapping_add(1);
                self.comparison_dismissed = false;
                self.recovery_candidate = None;
                self.pending = None;
                self.dialog = None;
                self.selected = None;
                self.message = None;
            }
            Err(error) => self.message = Some(error),
        }
    }

    pub(super) fn running(&self) -> bool {
        self.actor.as_ref().is_some_and(|actor| !actor.closed())
    }

    pub(super) fn disconnect(&mut self) {
        if let Some(actor) = &self.actor {
            actor.disconnect();
        }
    }

    pub(super) fn has_pending_documents(&self) -> bool {
        self.editor.as_ref().is_some_and(|editor| {
            let (status, dirty, _) = editor.document.details();
            dirty || status.busy()
        })
    }

    pub(super) fn close_ready(&mut self) -> bool {
        let pending = self.has_pending_documents();
        if pending && let Some(editor) = &mut self.editor {
            editor.closing = true;
        }
        !pending
    }

    pub(super) fn cancel_close_request(&mut self) {
        if let Some(editor) = &mut self.editor {
            editor.closing = false;
        }
    }

    pub(super) fn take_close_cancelled(&mut self) -> bool {
        std::mem::take(&mut self.close_cancelled)
    }

    pub(super) fn document_window(&mut self, ctx: &Context) {
        if let Some(editor) = &mut self.editor {
            let closing = editor.closing;
            if editor.show(ctx, self.connection, &mut self.comparison_dismissed) {
                self.editor = None;
                self.comparison_dismissed = false;
                self.recovery_candidate = None;
            } else if closing && !editor.closing {
                self.close_cancelled = true;
            }
        }
        self.recovery_window(ctx);
    }

    pub(super) fn view(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.heading(format!("{}@{}:{}", self.username, self.host, self.port));
            ui.label(self.status());
            ui.small(match (self.options.tls, self.options.protect_data) {
                (FtpTlsMode::Plain, _) => "Plain FTP · credentials and data are unencrypted",
                (FtpTlsMode::Explicit, true) => "Explicit FTPS · encrypted data",
                (FtpTlsMode::Explicit, false) => "Explicit FTPS · unencrypted data",
                (FtpTlsMode::Implicit, true) => "Implicit FTPS · encrypted data",
                (FtpTlsMode::Implicit, false) => "Implicit FTPS · unencrypted data",
            });
        });
        ui.small(match self.options.transfer_type {
            FtpTransferType::Binary => "Binary transfers preserve file bytes.",
            FtpTransferType::Ascii => {
                "ASCII transfers convert line endings. Use Binary for non-text files."
            }
        });
        if let Some(fingerprint) = self
            .actor
            .as_ref()
            .and_then(FtpWorkspace::certificate_fingerprint)
        {
            ui.collapsing("Verified server certificate", |ui| {
                ui.monospace(fingerprint);
            });
        }
        if self.actor.as_ref().is_none_or(FtpWorkspace::closed) {
            ui.horizontal(|ui| {
                ui.label("Password");
                let field = ui.add(TextEdit::singleline(&mut *self.password).password(true));
                if let Some(mut state) = TextEdit::load_state(ui.ctx(), field.id) {
                    state.clear_undoer();
                    TextEdit::store_state(ui.ctx(), field.id, state);
                }
                if ui.button("Connect").clicked() {
                    self.connect();
                }
            });
        } else if ui.button("Disconnect").clicked() {
            self.disconnect();
        }
        if let Some(error) = &self.message {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if self.actor.as_ref().is_some_and(FtpWorkspace::ready) {
            self.browser(ui);
        }
        self.confirmation(ui);
        self.transfer_list(ui);
        if self.actor.as_ref().is_some_and(|actor| !actor.closed()) || self.dialog.is_some() {
            ui.ctx().request_repaint_after(Duration::from_millis(60));
        }
    }

    fn browser(&mut self, ui: &mut Ui) {
        let actor = self.actor.as_mut().unwrap();
        let page_path = actor.browser.path.clone();
        let mut requested_path = None;
        ui.horizontal(|ui| {
            if ui.button("Up").clicked() {
                requested_path = Some(Ok(page_path.parent()));
            }
            if ui.button("Refresh").clicked() {
                requested_path = Some(Ok(page_path.clone()));
            }
            let address =
                ui.add(TextEdit::singleline(&mut self.address).hint_text("Absolute remote path"));
            if ui.button("Go").clicked()
                || address.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
            {
                requested_path = Some(RemotePath::from_bytes(self.address.as_bytes().to_vec()));
            }
        });
        ui.small(page_path.display());
        if actor.browser.loading {
            ui.spinner();
        }
        if let Some(error) = &actor.browser.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        ScrollArea::vertical()
            .id_salt("ftp-list")
            .max_height(360.)
            .show_rows(ui, 24.0, actor.browser.entries.len(), |ui, rows| {
                for row in rows {
                    let entry = &actor.browser.entries[row];
                    let selected = self
                        .selected
                        .as_ref()
                        .is_some_and(|selected| selected.path == entry.path);
                    ui.horizontal(|ui| {
                        let response = ui.selectable_label(selected, &entry.display_name);
                        if response.clicked() || response.double_clicked() {
                            self.selected = Some(entry.clone());
                        }
                        if response.double_clicked() && entry.kind == EntryKind::Directory {
                            requested_path = Some(Ok(entry.path.clone()));
                        }
                        match entry.kind {
                            EntryKind::Directory => {
                                ui.small("Folder");
                            }
                            EntryKind::File => {
                                if let Some(size) = entry.size {
                                    ui.small(format!("{size} B"));
                                }
                            }
                            EntryKind::Symlink => {
                                ui.small("Link");
                            }
                            EntryKind::Other => {}
                        }
                    });
                }
            });
        if let Some(path) = requested_path {
            self.message = path.and_then(|path| actor.navigate(path)).err();
            self.selected = None;
        }
        let available = !actor.browser.loading
            && actor.browser.error.is_none()
            && self.pending.is_none()
            && self.dialog.is_none();
        let selected = self.selected.clone();
        ui.add_enabled_ui(available, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Upload…").clicked() {
                    self.file_dialog(None, page_path.clone(), false);
                }
                if ui.button("Upload folder…").clicked() {
                    self.file_dialog(None, page_path.clone(), true);
                }
                if ui.button("New folder…").clicked() {
                    self.pending = Some(Pending::Mkdir {
                        directory: page_path,
                        name: String::new(),
                    });
                }
                if let Some(entry) = selected {
                    if entry.kind == EntryKind::File {
                        if ui.button("Download…").clicked() {
                            self.file_dialog(Some(entry.clone()), entry.path.parent(), false);
                        }
                        if ui
                            .add_enabled(self.editor.is_none(), Button::new("Edit…"))
                            .clicked()
                        {
                            self.edit(entry.path.clone());
                        }
                    }
                    if entry.kind == EntryKind::Directory && ui.button("Download folder…").clicked()
                    {
                        self.file_dialog(Some(entry.clone()), entry.path.parent(), true);
                    }
                    if ui.button("Rename…").clicked() {
                        self.pending = Some(Pending::Rename {
                            source: entry.path.clone(),
                            name: String::new(),
                        });
                    }
                    if ui.button("Delete…").clicked() {
                        self.pending = Some(Pending::Delete {
                            path: entry.path,
                            directory: entry.kind == EntryKind::Directory,
                            contents: false,
                            typed: String::new(),
                        });
                    }
                }
            });
        });
    }

    fn edit(&mut self, path: RemotePath) {
        let Some(actor) = &self.actor else {
            return;
        };
        match actor.open_document(path.clone(), self.recovery_root.clone()) {
            Ok(document) => {
                self.editor = Some(Editor {
                    document,
                    generation: self.generation,
                    path,
                    text: String::new(),
                    loaded: false,
                    closing: false,
                    replacing: false,
                    error: None,
                })
            }
            Err(error) => self.message = Some(error),
        }
    }

    fn recovery_window(&mut self, ctx: &Context) {
        if self.comparison_dismissed {
            return;
        }
        let Some(source) = &mut self.editor else {
            return;
        };
        let Some(actor) = self.actor.as_ref().filter(|actor| actor.ready()) else {
            return;
        };
        if source.generation == self.generation {
            return;
        }
        let (source_status, dirty, _) = source.document.details();
        if source_status.busy() || !dirty {
            return;
        }

        let mut import = false;
        let mut cancel = false;
        let size = super::fluent::dialog_size(ctx, 520., 130.);
        super::fluent::modeless(ctx, Id::new(("ftp-recovery", self.connection)), "Compare FTP edits", size)
            .show(ctx, |ui| {
                super::fluent::dialog_body(ui, size, |ui| {
                ui.label(source.path.display());
                if actor.certificate_fingerprint().is_none() {
                    ui.label("This connection has no verified certificate. Use the retained local copy to compare edits manually.");
                    return;
                }
                if let Some(target) = &self.recovery_candidate {
                    let (status, _, _) = target.details();
                    if status.busy() {
                        ui.spinner();
                        ui.ctx().request_repaint_after(Duration::from_millis(60));
                    } else if target.can_import_from(&source.document) {
                        ui.label("The verified FTP endpoint and file path match. Review the freshly loaded version below.");
                        ScrollArea::vertical().max_height(180.).show(ui, |ui| {
                            ui.monospace(target.snapshot().text);
                        });
                        import = ui.button("Use my edits in the fresh editor").clicked();
                    } else {
                        ui.label("The verified endpoint or open file does not match the original. Your edits remain in the original editor.");
                    }
                } else if ui.button("Open the current server version").clicked() {
                    match actor.open_document(source.path.clone(), self.recovery_root.clone()) {
                        Ok(document) => self.recovery_candidate = Some(document),
                        Err(error) => self.message = Some(error),
                    }
                }
                });
                cancel = ui.button("Cancel comparison").clicked();
            });
        if import && let Some(mut target) = self.recovery_candidate.take() {
            match target.import_recovery_from(&source.document) {
                Ok(()) => {
                    source.text = target.snapshot().text;
                    source.document = target;
                    source.generation = self.generation;
                    source.error = None;
                    source.replacing = false;
                }
                Err(error) => source.error = Some(error),
            }
        }
        if cancel {
            self.comparison_dismissed = true;
            self.recovery_candidate = None;
        }
    }

    fn file_dialog(&mut self, remote: Option<RemoteEntry>, directory: RemotePath, folder: bool) {
        let (send, recv) = mpsc::sync_channel(1);
        let result = std::thread::Builder::new()
            .name("termcraft-ftp-file-dialog".into())
            .spawn(move || {
                let dialog = rfd::FileDialog::new();
                let pending = if let Some(entry) = remote {
                    if folder {
                        dialog
                            .set_title("Choose a parent folder for the downloaded FTP folder")
                            .pick_folder()
                            .and_then(|parent| {
                                let local =
                                    harbor_workspace::remote_files::transfer::local_download_path(
                                        &parent,
                                        &entry.path,
                                    )
                                    .ok()?;
                                Some(Pending::Download {
                                    remote: entry.path,
                                    local,
                                    folder,
                                })
                            })
                    } else {
                        // Pass only a validated UTF-8 basename to the native picker.
                        let mut dialog = dialog;
                        if let Ok(local) =
                            harbor_workspace::remote_files::transfer::local_download_path(
                                std::path::Path::new("."),
                                &entry.path,
                            )
                            && let Some(name) = local.file_name().and_then(|name| name.to_str())
                        {
                            dialog = dialog.set_file_name(name);
                        }
                        dialog.save_file().map(|local| Pending::Download {
                            remote: entry.path,
                            local,
                            folder,
                        })
                    }
                } else {
                    let selected = if folder {
                        dialog.pick_folder()
                    } else {
                        dialog.pick_file()
                    };
                    selected.map(|local| {
                        let name = local
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or_default()
                            .to_owned();
                        Pending::Upload {
                            local,
                            directory,
                            name,
                            folder,
                        }
                    })
                };
                let _ = send.send(pending);
            });
        match result {
            Ok(_) => self.dialog = Some(recv),
            Err(error) => self.message = Some(error.to_string()),
        }
    }

    fn confirmation(&mut self, ui: &mut Ui) {
        let Some(mut pending) = self.pending.take() else {
            return;
        };
        let ready = self.actor.as_ref().is_some_and(FtpWorkspace::ready);
        let mut chosen = None;
        let mut dismiss = false;
        ui.group(|ui| {
            match &mut pending {
                Pending::Upload { directory, name, folder, .. } => {
                    let action = if *folder { "Upload folder into" } else { "Upload into" };
                    ui.label(format!("{action} {}", directory.display()));
                    if *folder { ui.small("Existing folders merge one entry at a time. New entries can be copied before a conflict stops the merge."); }
                    ui.text_edit_singleline(name);
                    ui.small("FTP checks the name before publishing. Changes by another client during publication can affect the result.");
                }
                Pending::Download { remote, local, folder } => {
                    ui.label(format!("Download {} to {}", remote.identity(), local.display()));
                    if *folder { ui.small("Existing folders merge one entry at a time. New entries can be copied before a conflict stops the merge."); }
                }
                Pending::Mkdir { directory, name } => {
                    ui.label(format!("Create a folder in {}", directory.display()));
                    ui.text_edit_singleline(name);
                }
                Pending::Rename { source, name } => {
                    ui.label(format!("Rename {}", source.identity()));
                    ui.text_edit_singleline(name);
                    ui.small("FTP checks the name before renaming. Changes by another client during publication can affect the result.");
                }
                Pending::Delete { path, directory, contents, typed } => {
                    ui.label(format!("Delete {}?", path.identity()));
                    if *directory {
                        if ui.checkbox(contents, "Also delete its contents").changed() { typed.clear(); }
                        if *contents {
                            ui.label(format!("Type {} to confirm deletion of the folder and everything inside it.", path.identity()));
                            ui.text_edit_singleline(typed);
                        } else {
                            ui.small("The server can remove this folder if it is empty.");
                        }
                    }
                }
            }
            ui.horizontal_wrapped(|ui| {
                let label = match pending {
                    Pending::Delete { contents: true, .. } => "Delete folder and contents",
                    Pending::Delete { .. } => "Confirm deletion",
                    Pending::Mkdir { .. } => "Create folder",
                    _ => "Continue with conflict check",
                };
                let confirmed = match &pending {
                    Pending::Delete { path, contents: true, typed, .. } => *typed == path.identity(),
                    _ => true,
                };
                if ui.add_enabled(ready && confirmed, Button::new(label)).clicked() { chosen = Some(ConflictPolicy::Ask); }
                if matches!(pending, Pending::Upload { .. } | Pending::Download { .. } | Pending::Rename { .. }) {
                    if ui.add_enabled(ready, Button::new("Skip existing")).clicked() { chosen = Some(ConflictPolicy::Skip); }
                    if ui.add_enabled(ready, Button::new("Allow replacing existing file")).clicked() { chosen = Some(ConflictPolicy::Overwrite); }
                }
                let safe = ui.button("Cancel");
                if matches!(pending, Pending::Delete { .. }) { super::fluent::focus_on_entry(ui, &safe, ui.id()); }
                dismiss = safe.clicked();
            });
        });
        if let Some(policy) = chosen {
            let result = pending
                .operation(policy)
                .and_then(|operation| self.actor.as_mut().unwrap().submit(operation));
            match result {
                Ok(_) => self.message = None,
                Err(error) => {
                    self.message = Some(error);
                    self.pending = Some(pending);
                }
            }
        } else if !dismiss {
            self.pending = Some(pending);
        }
    }

    fn transfer_list(&mut self, ui: &mut Ui) {
        if self.transfers.is_empty() {
            return;
        }
        ui.label("File operations");
        ScrollArea::vertical()
            .id_salt("ftp-transfers")
            .max_height(150.)
            .show(ui, |ui| {
                for event in self.transfers.values().rev() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(&event.summary);
                        if event.bytes > 0 {
                            ui.small(format!("{} B", event.bytes));
                        }
                        if event.bulk {
                            ui.small(format!("{} entries", event.completed_entries));
                        }
                        if let Some(outcome) = &event.outcome {
                            use harbor_workspace::remote_files::transfer::TransferOutcome::*;
                            let description = match outcome {
                                Completed => "Finished",
                                CompletedWithWarning(message)
                                | Indeterminate(message)
                                | Failed(message) => message,
                                Skipped => "Skipped existing destination",
                                Conflict => "Destination exists. Review it before retrying.",
                                Cancelled => "Cancelled",
                            };
                            ui.small(description);
                        } else {
                            if event.committing {
                                ui.small("Publishing…");
                            }
                            let label = if event.committing {
                                "Cancel after this entry"
                            } else {
                                "Cancel"
                            };
                            if (!event.committing || event.bulk)
                                && ui.small_button(label).clicked()
                                && let Some(actor) = &self.actor
                            {
                                actor.cancel_transfer(event.id);
                            }
                        }
                    });
                    if let Some(path) = &event.recovery {
                        let path = match path {
                            RecoveryPath::Local(path) => path.display().to_string(),
                            RecoveryPath::Remote(path) => path.display(),
                        };
                        ui.small(format!("Staging recovery: {path}"));
                    }
                }
            });
    }
}

impl Pending {
    fn operation(&self, policy: ConflictPolicy) -> Result<TransferOperation, String> {
        Ok(match self {
            Self::Upload {
                local,
                directory,
                name,
                folder,
            } => {
                let local = local.clone();
                let remote = directory.child(name.as_bytes())?;
                if *folder {
                    TransferOperation::UploadFolder {
                        local,
                        remote,
                        policy,
                    }
                } else {
                    TransferOperation::Upload {
                        local,
                        remote,
                        policy,
                    }
                }
            }
            Self::Download {
                remote,
                local,
                folder,
            } => {
                let remote = remote.clone();
                let local = local.clone();
                if *folder {
                    TransferOperation::DownloadFolder {
                        remote,
                        local,
                        policy,
                    }
                } else {
                    TransferOperation::Download {
                        remote,
                        local,
                        policy,
                    }
                }
            }
            Self::Mkdir { directory, name } => TransferOperation::Mkdir {
                remote: directory.child(name.as_bytes())?,
            },
            Self::Rename { source, name } => TransferOperation::Rename {
                source: source.clone(),
                destination: source.parent().child(name.as_bytes())?,
                policy,
            },
            Self::Delete {
                path,
                directory,
                contents,
                typed,
            } => {
                if *contents {
                    if !directory || *typed != path.identity() {
                        return Err(
                            "Confirm the exact folder path before deleting its contents.".into(),
                        );
                    }
                    TransferOperation::RemoveTree {
                        remote: path.clone(),
                        confirmed: path.clone(),
                    }
                } else {
                    TransferOperation::Remove {
                        remote: path.clone(),
                        confirmed: path.clone(),
                    }
                }
            }
        })
    }
}

struct Editor {
    document: RemoteDocument,
    generation: u64,
    path: RemotePath,
    text: String,
    loaded: bool,
    closing: bool,
    replacing: bool,
    error: Option<String>,
}

impl Editor {
    fn show(&mut self, ctx: &Context, connection: u64, comparison_dismissed: &mut bool) -> bool {
        let (status, dirty, recovery) = self.document.details();
        if !self.loaded && status == DocumentStatus::Ready {
            self.text = self.document.snapshot().text;
            self.loaded = true;
        }
        let mut open = true;
        let mut remove = self.closing && !dirty && !status.busy();
        let size = super::fluent::dialog_size(ctx, 660., 155.);
        super::fluent::modeless(ctx, Id::new(("ftp-editor",connection)), if dirty { "FTP editor · Unsaved" } else { "FTP editor" }, size)
            .open(&mut open).show(ctx, |ui| {
                ScrollArea::vertical().id_salt(("ftp-document-details",connection)).max_height(size.body_height*0.45).show(ui, |ui| {
                ui.label(self.path.display());
                ui.label(match &status {
                    DocumentStatus::Opening => "Opening…",
                    DocumentStatus::Ready => "Ready",
                    DocumentStatus::Recovering => "Saving a local recovery copy…",
                    DocumentStatus::Saving => "Uploading staged save…",
                    DocumentStatus::Committing => "Publishing save…",
                    DocumentStatus::Indeterminate(message) | DocumentStatus::Failed(message) => message,
                    DocumentStatus::Saved => "Saved",
                    DocumentStatus::Conflict => "The remote file changed. Your edits are still in this window.",
                    DocumentStatus::Cancelled => "Operation cancelled.",
                    DocumentStatus::Disconnected => "Connection ended. Local recovery is available.",
                });
                if let Some(path) = recovery {
                    ui.horizontal_wrapped(|ui| {
                        ui.small(format!("Local recovery: {}", path.display()));
                        if ui.small_button("Copy path").clicked() { ui.ctx().copy_text(path.display().to_string()); }
                    });
                }
                let (staging, warning) = self.document.recovery_details();
                if let Some(path) = staging { ui.small(format!("Remote staging: {}", path.display())); }
                if let Some(warning) = warning { ui.colored_label(ui.visuals().warn_fg_color, warning); }
                if let Some(error) = &self.error { ui.colored_label(ui.visuals().error_fg_color, error); }
                });
                if *comparison_dismissed && ui.button("Compare retained FTP edits…").clicked() { *comparison_dismissed = false; }
                ui.small("FTP checks whether the file changed before publishing. The server controls replacement and file permissions.");
                ui.horizontal_wrapped(|ui| {
                    if ui.add_enabled(self.loaded && dirty && !status.busy(), Button::new("Save")).clicked() {
                        self.error = self.document.save(SavePolicy::VerifyUnchanged).err();
                    }
                    if status == DocumentStatus::Conflict && ui.button("Replace remote version…").clicked() { self.replacing = true; }
                    if status.busy() && status != DocumentStatus::Committing && ui.button("Cancel operation").clicked() { self.document.cancel_operation(); }
                });
                if self.replacing {
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Confirm replacing the observed remote version").clicked() {
                            self.error = self.document.save(SavePolicy::Overwrite).err();
                            self.replacing = false;
                        }
                        if ui.button("Cancel replacement").clicked() { self.replacing = false; }
                    });
                }
                if self.closing {
                    ui.label("Save or discard changes before closing this FTP document.");
                    ui.horizontal_wrapped(|ui| {
                        if ui.add_enabled(!status.busy(), Button::new("Save and close")).clicked() {
                            self.error = self.document.request_close(CloseDecision::Save).err();
                        }
                        if ui.add_enabled(!status.busy(), Button::new("Discard and close")).clicked() {
                            remove = self.document.request_close(CloseDecision::Discard).unwrap_or(false);
                        }
                        if ui.button("Keep editing").clicked() { self.closing = false; remove = false; }
                    });
                }
                ScrollArea::both().show(ui, |ui| {
                    if ui.add_enabled(self.loaded, TextEdit::multiline(&mut self.text)
                        .font(egui::TextStyle::Monospace).desired_width(f32::INFINITY).desired_rows(16)).changed()
                        && let Err(error) = self.document.set_text(&self.text) {
                        self.error = Some(error);
                        self.text = self.document.snapshot().text;
                    }
                });
            });
        if !open {
            self.closing = dirty || status.busy();
            remove = !self.closing;
        }
        if status.busy() {
            ctx.request_repaint_after(Duration::from_millis(60));
        }
        remove
    }
}

#[cfg(test)]
#[path = "ftp_tests.rs"]
mod tests;
