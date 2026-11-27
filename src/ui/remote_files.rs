use super::*;
use harbor_workspace::remote_files::{
    RemoteEntry,
    transfer::{ConflictPolicy, RecoveryPath, TransferEvent, TransferOperation, TransferOutcome},
};
use harbor_workspace::{
    remote_files::{EntryKind, RemotePath},
    ssh::{ChallengeAnswer, ConnectionCommand, TrustDecision},
};
use zeroize::Zeroizing;
fn upload_operation(
    local: PathBuf,
    directory: &RemotePath,
    policy: ConflictPolicy,
) -> Result<TransferOperation, String> {
    let name = local
        .file_name()
        .ok_or("Choose a file or folder with a filename")?;
    #[cfg(unix)]
    let bytes = {
        use std::os::unix::ffi::OsStrExt;
        name.as_bytes()
    };
    #[cfg(not(unix))]
    let bytes = name
        .to_str()
        .ok_or("Filename cannot be represented by SFTP")?
        .as_bytes();
    let remote = directory.child(bytes)?;
    Ok(TransferOperation::Upload {
        local,
        remote,
        policy,
    })
}
#[derive(Clone)]
struct RemoteDragPayload {
    connection: u64,
    path: RemotePath,
    kind: EntryKind,
}
enum FileActionForm {
    NewFolder {
        parent: RemotePath,
        name: String,
    },
    Rename {
        source: RemotePath,
        name: String,
    },
    Delete {
        target: RemotePath,
        directory: bool,
        include_contents: bool,
    },
}
impl FileActionForm {
    fn operation(&self, policy: ConflictPolicy) -> Result<TransferOperation, String> {
        match self {
            Self::NewFolder { parent, name } => Ok(TransferOperation::Mkdir {
                remote: parent.child(name.as_bytes())?,
            }),
            Self::Rename { source, name } => Ok(TransferOperation::Rename {
                source: source.clone(),
                destination: source.parent().child(name.as_bytes())?,
                policy,
            }),
            Self::Delete {
                target,
                directory,
                include_contents,
            } => {
                if *directory && *include_contents {
                    Ok(TransferOperation::RemoveTree {
                        remote: target.clone(),
                        confirmed: target.clone(),
                    })
                } else {
                    Ok(TransferOperation::Remove {
                        remote: target.clone(),
                        confirmed: target.clone(),
                    })
                }
            }
        }
    }
}
#[derive(Default)]
pub(super) struct TransferUi {
    pub(super) backend: harbor_workspace::remote_files::capabilities::RemoteBackendKind,
    pub(super) capabilities:
        Option<harbor_workspace::remote_files::capabilities::RemoteCapabilities>,
    selected: Option<RemoteEntry>,
    form: Option<FileActionForm>,
    overwrite: bool,
    local_download: Option<PathBuf>,
    local_path_text: String,
    local_picker: Option<std::sync::mpsc::Receiver<Option<PathBuf>>>,
    picker: Option<std::sync::mpsc::Receiver<Option<Result<TransferOperation, String>>>>,
    events: Vec<TransferEvent>,
    error: Option<String>,
}
impl TransferUi {
    pub(super) fn blocking_action(&self) -> bool {
        matches!(self.form, Some(FileActionForm::Delete { .. }))
    }
    pub(super) fn effective_capabilities(
        &self,
    ) -> harbor_workspace::remote_files::capabilities::RemoteCapabilities {
        self.capabilities
            .unwrap_or_else(|| self.backend.capabilities())
    }
    pub(super) fn listing_changed(&mut self) {
        self.selected = None;
    }
    fn policy(&self) -> ConflictPolicy {
        if self.overwrite {
            ConflictPolicy::Overwrite
        } else {
            ConflictPolicy::Ask
        }
    }
    fn download_operation(&self, local: PathBuf) -> Result<TransferOperation, String> {
        let selected = self
            .selected
            .as_ref()
            .filter(|e| matches!(e.kind, EntryKind::File | EntryKind::Directory))
            .ok_or("Select a remote file or folder to download")?;
        Ok(TransferOperation::Download {
            remote: selected.path.clone(),
            local,
            policy: self.policy(),
        })
    }
    fn download_drop_operation(
        &self,
        payload: &RemoteDragPayload,
        connection: u64,
    ) -> Result<TransferOperation, String> {
        if payload.connection != connection {
            return Err("This remote drag belongs to a different connection".into());
        }
        let capabilities = self.effective_capabilities();
        if !capabilities.download_files
            || (payload.kind == EntryKind::Directory && !capabilities.download_folders)
        {
            return Err(
                "Use SFTP for folder downloads or this local platform's download support".into(),
            );
        }
        if !matches!(payload.kind, EntryKind::File | EntryKind::Directory) {
            return Err("Drag a regular remote file or folder".into());
        }
        let directory = self
            .local_download
            .as_ref()
            .ok_or("Choose a local download folder first")?;
        let local = harbor_workspace::remote_files::transfer::local_download_path(
            directory,
            &payload.path,
        )?;
        Ok(TransferOperation::Download {
            remote: payload.path.clone(),
            local,
            policy: self.policy(),
        })
    }
    fn local_drop_zone(
        &mut self,
        ui: &mut Ui,
        connection: u64,
        ready: bool,
        ssh: Option<&mut harbor_workspace::ssh::ConnectionHandle>,
    ) {
        ui.separator();
        ui.label("Local download folder");
        ui.horizontal(|ui| {
            if ui
                .add(
                    TextEdit::singleline(&mut self.local_path_text)
                        .desired_width(145.)
                        .hint_text("Choose a local folder"),
                )
                .changed()
            {
                self.local_download = (!self.local_path_text.is_empty())
                    .then(|| PathBuf::from(&self.local_path_text));
            }
            if ui
                .add_enabled(
                    self.local_picker.is_none() && self.picker.is_none(),
                    Button::new("…"),
                )
                .clicked()
            {
                let (tx, rx) = std::sync::mpsc::channel();
                self.local_picker = Some(rx);
                std::thread::spawn(move || {
                    let _ = tx.send(rfd::FileDialog::new().pick_folder());
                });
            }
        });
        let (_, payload) = ui.dnd_drop_zone::<RemoteDragPayload, _>(
            Frame::new()
                .inner_margin(8)
                .stroke(Stroke::new(1., line(ui))),
            |ui| {
                ui.label("Drop a remote entry here to download into this local folder.");
            },
        );
        if let Some(payload) = payload {
            let result = if ready {
                self.download_drop_operation(&payload, connection)
                    .and_then(|op| {
                        ssh.ok_or_else(|| "SSH connection is unavailable".to_string())?
                            .transfers
                            .submit(op)
                            .map(|_| ())
                    })
            } else {
                Err("Wait for the remote connection and listing before downloading".into())
            };
            if let Err(error) = result {
                self.error = Some(error);
            }
        }
    }
    pub(super) fn poll(&mut self, ssh: &mut harbor_workspace::ssh::ConnectionHandle) -> bool {
        if let Some(receiver) = &self.local_picker {
            match receiver.try_recv() {
                Ok(result) => {
                    self.local_picker = None;
                    if let Some(path) = result {
                        self.local_path_text = path.display().to_string();
                        self.local_download = Some(path);
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.local_picker = None,
                _ => {}
            }
        }
        if let Some(receiver) = &self.picker {
            match receiver.try_recv() {
                Ok(result) => {
                    self.picker = None;
                    if let Some(result) = result {
                        match result.and_then(|op| ssh.transfers.submit(op).map(|_| ())) {
                            Ok(()) => self.error = None,
                            Err(e) => self.error = Some(e),
                        }
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.picker = None;
                    self.error = Some("File chooser closed unexpectedly".into());
                }
                _ => {}
            }
        }
        let mut refresh = false;
        for event in ssh.transfers.poll() {
            refresh |= matches!(
                event.outcome,
                Some(TransferOutcome::Completed | TransferOutcome::CompletedWithWarning(_))
            );
            if let Some(existing) = self.events.iter_mut().find(|e| e.id == event.id) {
                *existing = event;
            } else {
                self.events.push(event);
            }
        }
        while self.events.len() > 40 {
            if let Some(i) = self.events.iter().position(|e| e.outcome.is_some()) {
                self.events.remove(i);
            } else {
                break;
            }
        }
        refresh
    }
    fn actions(
        &mut self,
        ui: &mut Ui,
        path: &RemotePath,
        ready: bool,
        capabilities: harbor_workspace::remote_files::capabilities::RemoteCapabilities,
    ) {
        ui.horizontal_wrapped(|ui| {
            for folder in [false, true] {
                if ui
                    .add_enabled(
                        ready
                            && capabilities.upload_files
                            && (self.overwrite || capabilities.exclusive_create)
                            && self.picker.is_none()
                            && (!folder || capabilities.upload_folders),
                        Button::new(if folder {
                            "Upload folder…"
                        } else {
                            "Upload file…"
                        }),
                    )
                    .clicked()
                {
                    let (tx, rx) = std::sync::mpsc::channel();
                    self.picker = Some(rx);
                    let directory = path.clone();
                    let policy = self.policy();
                    std::thread::spawn(move || {
                        let dialog = rfd::FileDialog::new();
                        let chosen = if folder {
                            dialog.pick_folder()
                        } else {
                            dialog.pick_file()
                        };
                        let _ = tx
                            .send(chosen.map(|local| upload_operation(local, &directory, policy)));
                    });
                }
            }
            let selected_file = self.selected.as_ref().is_some_and(|e| {
                e.kind == EntryKind::File
                    || (e.kind == EntryKind::Directory && capabilities.download_folders)
            });
            if ui
                .add_enabled(
                    ready && capabilities.download_files && selected_file && self.picker.is_none(),
                    Button::new("Download…"),
                )
                .clicked()
            {
                let (tx, rx) = std::sync::mpsc::channel();
                self.picker = Some(rx);
                let selected = self.selected.as_ref().unwrap();
                let operation = self.download_operation(PathBuf::new()).unwrap();
                let name = selected.display_name.clone();
                std::thread::spawn(move || {
                    let result =
                        rfd::FileDialog::new()
                            .set_file_name(name)
                            .save_file()
                            .map(|local| {
                                let TransferOperation::Download { remote, policy, .. } = operation
                                else {
                                    unreachable!()
                                };
                                Ok(TransferOperation::Download {
                                    remote,
                                    local,
                                    policy,
                                })
                            });
                    let _ = tx.send(result);
                });
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    ready && capabilities.mutate_entries,
                    Button::new("New folder…"),
                )
                .clicked()
            {
                self.form = Some(FileActionForm::NewFolder {
                    parent: path.clone(),
                    name: String::new(),
                });
            }
            if ui
                .add_enabled(
                    ready && capabilities.mutate_entries && self.selected.is_some(),
                    Button::new("Rename…"),
                )
                .clicked()
            {
                let selected = self.selected.as_ref().unwrap();
                self.form = Some(FileActionForm::Rename {
                    source: selected.path.clone(),
                    name: String::new(),
                });
            }
            if ui
                .add_enabled(
                    ready && capabilities.mutate_entries && self.selected.is_some(),
                    Button::new("Delete…"),
                )
                .clicked()
            {
                self.form = Some(FileActionForm::Delete {
                    target: self.selected.as_ref().unwrap().path.clone(),
                    directory: self.selected.as_ref().unwrap().kind == EntryKind::Directory,
                    include_contents: false,
                });
            }
        });
        ui.checkbox(&mut self.overwrite,"Replace existing destination files").on_hover_text("Off by default. Conflicting files are preserved until you explicitly choose replacement and retry.");
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
    }
    fn dropped_files(
        &mut self,
        ui: &Ui,
        path: &RemotePath,
        ready: bool,
        ssh: Option<&mut harbor_workspace::ssh::ConnectionHandle>,
        target: &Response,
    ) {
        if !ready
            || !target.contains_pointer()
            || !self.effective_capabilities().upload_files
            || (!self.overwrite && !self.effective_capabilities().exclusive_create)
        {
            return;
        }
        let files = ui.input(|i| i.raw.dropped_files.clone());
        let Some(ssh) = ssh else {
            return;
        };
        for file in files {
            let result = upload_operation(file.path().to_path_buf(), path, self.policy())
                .and_then(|op| ssh.transfers.submit(op).map(|_| ()));
            if let Err(error) = result {
                self.error = Some(error);
            }
        }
    }
    fn dialog(&mut self, ctx: &Context, ssh: Option<&mut harbor_workspace::ssh::ConnectionHandle>) {
        let Some(mut form) = self.form.take() else {
            return;
        };
        let mut open = true;
        let mut submit = false;
        let mut cancel = false;
        let deleting = matches!(form, FileActionForm::Delete { .. });
        let title = match form {
            FileActionForm::NewFolder { .. } => "Create remote folder",
            FileActionForm::Rename { .. } => "Rename remote entry",
            FileActionForm::Delete { .. } => "Delete remote entry?",
        };
        let size = super::fluent::dialog_size(ctx, 420., 110.);
        let mut content = |ui: &mut Ui| {
            super::fluent::dialog_body(ui, size, |ui| match &mut form {
                FileActionForm::NewFolder { parent, name } => {
                    ui.label("Create a folder in:");
                    ui.monospace(parent.display());
                    ui.add(TextEdit::singleline(name).hint_text("Folder name"));
                }
                FileActionForm::Rename { source, name } => {
                    ui.label("Rename this remote entry:");
                    ui.monospace(source.display());
                    ui.add(TextEdit::singleline(name).hint_text("New name"));
                }
                FileActionForm::Delete {
                    target,
                    directory,
                    include_contents,
                } => {
                    ui.label("Permanently delete this remote entry?");
                    ui.monospace(target.display());
                    if target.identity() != target.display() {
                        ui.monospace(format!("Exact path: {}", target.identity()));
                    }
                    if *directory {
                        if !self.effective_capabilities().recursive_remove {
                            *include_contents = false;
                        }
                        ui.add_enabled(
                            self.effective_capabilities().recursive_remove,
                            egui::Checkbox::new(
                                include_contents,
                                "Also delete everything inside this folder",
                            ),
                        );
                    }
                    if *include_contents {
                        ui.label("Completed deletions cannot be undone. Cancel stops the remaining entries.");
                    } else {
                        ui.label("A nonempty directory is preserved unless contents deletion is selected.");
                    }
                }
            });
            if let Some(error) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button(if matches!(form, FileActionForm::Delete { .. }) {
                        if matches!(
                            form,
                            FileActionForm::Delete {
                                directory: true,
                                include_contents: true,
                                ..
                            }
                        ) {
                            "Delete folder and contents"
                        } else {
                            "Delete this entry"
                        }
                    } else {
                        if matches!(form, FileActionForm::Rename { .. }) {
                            "Rename"
                        } else {
                            "Create folder"
                        }
                    })
                    .clicked()
                {
                    submit = true;
                }
                let safe = ui.button("Cancel");
                if deleting {
                    super::fluent::focus_on_entry(ui, &safe, ui.id());
                }
                if safe.clicked() {
                    cancel = true;
                }
            });
        };
        let id = Id::new("remote-file-action");
        if deleting {
            let modal = super::fluent::modal(ctx, id, title, size, &mut content);
            cancel |= modal.should_close();
        } else {
            super::fluent::modeless(ctx, id, title, size)
                .open(&mut open)
                .collapsible(false)
                .show(ctx, &mut content);
        }
        if submit && !cancel {
            let result = form.operation(self.policy()).and_then(|op| {
                ssh.ok_or_else(|| "SSH connection is unavailable".to_string())?
                    .transfers
                    .submit(op)
                    .map(|_| ())
            });
            match result {
                Ok(()) => {
                    self.error = None;
                    return;
                }
                Err(e) => self.error = Some(e),
            }
        }
        if open && !cancel {
            self.form = Some(form);
        }
    }
    fn jobs(&self, ui: &mut Ui, ssh: Option<&harbor_workspace::ssh::ConnectionHandle>) {
        if self.events.is_empty() {
            return;
        }
        ui.separator();
        ui.label(RichText::new("Transfers").strong());
        ScrollArea::vertical()
            .id_salt("remote-transfer-jobs")
            .max_height(140.)
            .show(ui, |ui| {
                for event in self.events.iter().rev() {
                    ui.horizontal_wrapped(|ui| {
                        let status = match &event.outcome {
                            None => "In progress".into(),
                            Some(TransferOutcome::Completed) => "Completed".into(),
                            Some(TransferOutcome::CompletedWithWarning(message)) => {
                                format!("Completed: {message}")
                            }
                            Some(TransferOutcome::Indeterminate(message)) => {
                                format!("Outcome unknown: {message}")
                            }
                            Some(TransferOutcome::Cancelled) => "Cancelled".into(),
                            Some(TransferOutcome::Conflict) => {
                                "Conflict — destination preserved".into()
                            }
                            Some(TransferOutcome::Skipped) => "Skipped".into(),
                            Some(TransferOutcome::Failed(error)) => format!("Failed: {error}"),
                        };
                        ui.label(&event.summary);
                        if event.completed_entries > 0 {
                            ui.small(format!("{} entries completed", event.completed_entries));
                        }
                        ui.label(format!("#{} · {} bytes · {status}", event.id, event.bytes));
                        if event.committing && event.outcome.is_none() {
                            ui.small("Committing…");
                        }
                        if event.outcome.is_none()
                            && (!event.committing || event.bulk)
                            && ui
                                .small_button(if event.bulk && event.committing {
                                    "Cancel remaining"
                                } else {
                                    "Cancel"
                                })
                                .clicked()
                        {
                            if let Some(ssh) = ssh {
                                ssh.transfers.cancel(event.id);
                            }
                        }
                    });
                    if let Some(recovery) = &event.recovery {
                        let path = match recovery {
                            RecoveryPath::Local(p) => p.display().to_string(),
                            RecoveryPath::Remote(p) => p.display(),
                        };
                        ui.small(format!("Partial data: {path}"));
                    }
                }
            });
    }
}
pub(super) struct AuthForm {
    pub id: u64,
    pub title: String,
    pub prompts: Vec<harbor_workspace::ssh::Prompt>,
    pub values: Vec<Zeroizing<String>>,
    pub host_key: bool,
    pub credential: Option<harbor_workspace::ssh::ChallengeCredential>,
    pub save_in_vault: bool,
    pub vault_filled: bool,
}
impl TerminalTab {
    pub(super) fn follow_shell_directory(&mut self, path: RemotePath) {
        if !self.ssh_connected || self.ssh.is_none() {
            return;
        }
        let Some(browser) = &mut self.browser else {
            return;
        };
        if let Some(command) = browser.on_shell_directory(self.id, path) {
            self.transfer_ui.listing_changed();
            if let Err(error) = self.ssh.as_ref().unwrap().try_send(command) {
                browser.fail(self.id, browser.request, error);
            }
        }
    }
    fn shell_follow_controls(&mut self, ui: &mut Ui) {
        use harbor_workspace::{
            remote_files::BrowserFollowMode,
            ssh::shell_integration::{ShellFlavor, integration_script},
        };
        let Some(browser) = &self.browser else {
            return;
        };
        let mut follow = browser.follow_mode() == BrowserFollowMode::FollowShell;
        if ui
            .add_enabled(
                self.ssh_connected,
                Checkbox::new(&mut follow, "Follow shell directory changes"),
            )
            .changed()
        {
            let resume = self.browser.as_mut().unwrap().set_follow_mode(if follow {
                BrowserFollowMode::FollowShell
            } else {
                BrowserFollowMode::Manual
            });
            if let Some(command) = resume {
                if let Some(ssh) = &self.ssh {
                    if let Err(error) = ssh.try_send(command) {
                        let browser = self.browser.as_mut().unwrap();
                        browser.fail(self.id, browser.request, error);
                    }
                }
            }
            if follow {
                if let Some(path) = self
                    .directory_tracker
                    .as_ref()
                    .and_then(|t| t.last_directory())
                    .cloned()
                {
                    self.follow_shell_directory(path);
                }
            }
        }
        if follow {
            if let Some(reason) = self
                .directory_tracker
                .as_ref()
                .and_then(|t| t.manual_reason())
            {
                ui.small(reason);
            } else {
                ui.small("Shell directory notices available.");
            }
        } else {
            ui.small("Manual navigation");
        }
        let known = self
            .directory_tracker
            .as_ref()
            .and_then(|t| t.last_directory())
            .cloned();
        if ui
            .add_enabled(
                self.ssh_connected && known.is_some(),
                Button::new("Go to shell folder"),
            )
            .clicked()
        {
            self.navigate_remote(known.unwrap());
        }
        ui.collapsing("Shell setup",|ui|{
            ui.label("Copy setup for your remote shell, then run it there. It lasts for this shell session.");
            ui.horizontal_wrapped(|ui|{for (shell,label) in [(ShellFlavor::Bash,"Copy Bash setup"),(ShellFlavor::Zsh,"Copy Zsh setup"),(ShellFlavor::Fish,"Copy Fish setup")]{
                if ui.add_enabled(self.ssh_connected,Button::new(label)).clicked(){match integration_script(shell,&self.session.host){Ok(script)=>ui.ctx().copy_text(script),Err(error)=>self.error=error}}
            }});
            if ui.button("Copy cleanup command").clicked(){ui.ctx().copy_text("__termcraft_osc7_remove".to_owned());}
        });
        let mut go = false;
        ui.horizontal(|ui| {
            let response = ui.add_enabled(
                self.ssh_connected,
                TextEdit::singleline(&mut self.remote_path)
                    .hint_text("/remote/folder")
                    .desired_width(150.),
            );
            if response.lost_focus() && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter))
            {
                go = true;
            }
            go |= ui
                .add_enabled(
                    self.ssh_connected && !self.remote_path.is_empty(),
                    Button::new("Go"),
                )
                .clicked();
        });
        if go {
            if self.remote_path.starts_with('/') {
                match RemotePath::from_bytes(self.remote_path.as_bytes().to_vec()) {
                    Ok(path) => self.navigate_remote(path),
                    Err(error) => self.error = error,
                }
            } else {
                self.error = "Enter an absolute remote folder path.".into();
            }
        }
    }
    pub(super) fn navigate_remote(&mut self, path: RemotePath) {
        self.transfer_ui.selected = None;
        let Some(browser) = &mut self.browser else {
            return;
        };
        let request = browser.request.wrapping_add(1);
        if let Some(ssh) = &self.ssh {
            match ssh.try_send(ConnectionCommand::List { request, path }) {
                Ok(()) => browser.begin(request),
                Err(error) => browser.fail(self.id, browser.request, error),
            }
        }
    }
    pub(super) fn answer_auth(&mut self, trust: Option<TrustDecision>) {
        let Some(form) = self.auth.take() else { return };
        let save = if trust != Some(TrustDecision::Reject) && form.save_in_vault {
            form.credential
                .clone()
                .map(|credential| credentials::Request::Save {
                    destination: credentials::Destination::Terminal {
                        connection: self.id,
                        challenge: form.id,
                    },
                    credential,
                    values: form.values.clone(),
                })
        } else {
            None
        };
        let answer = ChallengeAnswer {
            id: form.id,
            values: form.values,
            trust,
        };
        let result = if let Some(gateway) = &self.gateway {
            gateway.answer(answer)
        } else if let Some(ssh) = &self.ssh {
            ssh.answer(answer)
        } else {
            Err("This authentication connection is closed".into())
        };
        match result {
            Ok(()) => self.vault_request = save,
            Err(error) => self.error = error,
        }
    }
}
impl Workspace {
    pub(super) fn remote_file_action_window(&mut self, ctx: &Context) {
        if let Some(tab) = self
            .tabs
            .iter_mut()
            .find(|tab| tab.transfer_ui.form.is_some())
        {
            tab.transfer_ui.dialog(ctx, tab.ssh.as_mut());
        }
    }

    pub(super) fn remote_files_panel(&mut self, ui: &mut Ui) {
        if self
            .tabs
            .get(self.active)
            .is_some_and(|t| self.windows.window(t.id) != 0)
        {
            return;
        }
        if self.page != Page::Terminal {
            return;
        }
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        if !tab.browser.as_ref().is_some_and(|b| b.visible) {
            return;
        }
        let id = tab.id;
        let mut edit_remote = None;
        Panel::left(Id::new(("remote-files", id)))
            .resizable(true)
            .default_size(246.)
            .min_size(200.)
            .max_size(480.)
            .frame(Frame::new().fill(mist(ui)).inner_margin(10))
            .show(ui, |ui| {
                ScrollArea::vertical()
                    .id_salt(("remote-pane-body", id))
                    .show(ui, |ui| {
                        let tab = &mut self.tabs[self.active];
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Remote files").strong());
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui
                                    .small_button("×")
                                    .on_hover_text("Hide remote files")
                                    .clicked()
                                {
                                    tab.browser.as_mut().unwrap().visible = false;
                                }
                            });
                        });
                        ui.label(
                            RichText::new(format!(
                                "{} · Embedded {}",
                                tab.session.host,
                                tab.session.remote_backend.label()
                            ))
                            .size(11.)
                            .color(muted(ui)),
                        );
                        if authentication_ui(ui, tab) {
                            return;
                        }
                        if !tab.ssh_connected {
                            ui.label(&tab.connection_status);
                        }
                        ui.small(tab.session.remote_backend.label());
                        tab.shell_follow_controls(ui);
                        let browser = tab.browser.as_ref().unwrap();
                        ui.add_space(8.);
                        ui.add(
                            Label::new(RichText::new(browser.path.display()).monospace().size(12.))
                                .truncate(),
                        );
                        let mut navigate = None;
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(tab.ssh_connected, Button::new("↑ Parent"))
                                .clicked()
                            {
                                navigate = Some(browser.path.parent());
                            }
                            if ui
                                .add_enabled(tab.ssh_connected, Button::new("Refresh"))
                                .clicked()
                            {
                                navigate = Some(browser.path.clone());
                            }
                            if browser.loading {
                                ui.spinner();
                            }
                        });
                        if let Some(error) = &browser.error {
                            ui.colored_label(ui.visuals().error_fg_color, error);
                        }
                        tab.transfer_ui.actions(
                            ui,
                            &browser.path,
                            tab.ssh_connected && !browser.loading && browser.error.is_none(),
                            tab.transfer_ui.effective_capabilities(),
                        );
                        let upload_zone = Frame::new()
                            .inner_margin(8)
                            .stroke(Stroke::new(1., line(ui)))
                            .show(ui, |ui| {
                                ui.label(
                                    if tab.transfer_ui.effective_capabilities().upload_folders {
                                        "Drop local files or folders here to upload."
                                    } else {
                                        "Drop local files here to upload. Folders require SFTP."
                                    },
                                )
                            });
                        tab.transfer_ui.dropped_files(
                            ui,
                            &browser.path,
                            tab.ssh_connected && !browser.loading && browser.error.is_none(),
                            tab.ssh.as_mut(),
                            &upload_zone.response,
                        );
                        tab.transfer_ui.local_drop_zone(
                            ui,
                            id,
                            tab.ssh_connected && !browser.loading && browser.error.is_none(),
                            tab.ssh.as_mut(),
                        );
                        tab.transfer_ui.jobs(ui, tab.ssh.as_ref());
                        ui.separator();
                        ScrollArea::vertical()
                            .id_salt(("remote-entries", id))
                            .max_height(300.)
                            .min_scrolled_height(180.)
                            .show(ui, |ui| {
                                for entry in &browser.entries {
                                    let marker = match entry.kind {
                                        EntryKind::Directory => "▸",
                                        EntryKind::Symlink => "↗",
                                        _ => "·",
                                    };
                                    let response = ui.add_enabled(
                                        !browser.loading && tab.ssh_connected,
                                        Button::new(format!("{marker}  {}", entry.display_name))
                                            .frame(false)
                                            .sense(Sense::click_and_drag()),
                                    );
                                    response.dnd_set_drag_payload(RemoteDragPayload {
                                        connection: id,
                                        path: entry.path.clone(),
                                        kind: entry.kind,
                                    });
                                    if response.clicked() {
                                        tab.transfer_ui.selected = Some(entry.clone());
                                    }
                                    if response.double_clicked()
                                        && entry.kind == EntryKind::File
                                        && tab.transfer_ui.effective_capabilities().edit
                                    {
                                        edit_remote = Some(entry.path.clone());
                                    }
                                    if response.double_clicked()
                                        && matches!(
                                            entry.kind,
                                            EntryKind::Directory | EntryKind::Symlink
                                        )
                                    {
                                        navigate = Some(entry.path.clone());
                                    }
                                    response.on_hover_text(format!(
                                        "{}\n{}",
                                        entry.path.display(),
                                        entry
                                            .size
                                            .map(|s| format!("{s} bytes"))
                                            .unwrap_or_default()
                                    ));
                                }
                                if browser.entries.is_empty()
                                    && !browser.loading
                                    && browser.error.is_none()
                                {
                                    ui.label(
                                        RichText::new("This directory is empty.").color(muted(ui)),
                                    );
                                }
                            });
                        if let Some(path) = navigate {
                            tab.navigate_remote(path);
                        }
                    });
            });
        if let Some(path) = edit_remote {
            self.open_remote_editor(id, path);
        }
    }
}

fn auth_field(
    ui: &mut Ui,
    connection: u64,
    challenge: u64,
    index: usize,
    prompt: &harbor_workspace::ssh::Prompt,
    value: &mut String,
) -> Response {
    auth_field_scoped(ui, "ssh-auth", connection, challenge, index, prompt, value)
}
pub(super) fn auth_field_scoped(
    ui: &mut Ui,
    scope: &str,
    connection: u64,
    challenge: u64,
    index: usize,
    prompt: &harbor_workspace::ssh::Prompt,
    value: &mut String,
) -> Response {
    let id = Id::new((scope, connection, challenge, index));
    let previous = egui::text_edit::TextEditState::load(ui.ctx(), id);
    let fresh = previous.is_none();
    if let Some(mut state) = previous {
        state.clear_undoer();
        state.store(ui.ctx(), id);
    }
    let response = TextEdit::singleline(value)
        .id(id)
        .password(!prompt.echo)
        .desired_width(ui.available_width())
        .show(ui)
        .response
        .response;
    if let Some(mut state) = egui::text_edit::TextEditState::load(ui.ctx(), id) {
        state.clear_undoer();
        state.store(ui.ctx(), id);
    }
    if fresh && index == 0 {
        response.request_focus();
    }
    response
}
#[cfg(test)]
mod auth_tests {
    use super::*;
    #[test]
    fn auth_field_leaves_no_secret_in_undo_history() {
        let ctx = Context::default();
        let mut value = String::new();
        let mut id = Id::NULL;
        for (time, events) in [
            (0.0, vec![]),
            (0.1, vec![Event::Text("SENTINEL_SECRET".into())]),
            (2.0, vec![]),
        ] {
            let mut output = ctx.run_ui(
                RawInput {
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ui| {
                    let response = auth_field(
                        ui,
                        7,
                        9,
                        0,
                        &harbor_workspace::ssh::Prompt {
                            label: "Password".into(),
                            echo: false,
                        },
                        &mut value,
                    );
                    id = response.id;
                    if time == 0.0 {
                        response.request_focus();
                    }
                },
            );
            output.textures_delta.clear();
        }
        assert_eq!(value, "SENTINEL_SECRET");
        let state = egui::text_edit::TextEditState::load(&ctx, id).unwrap();
        let empty = (egui::text::CCursorRange::default(), String::new());
        assert!(!state.undoer().has_undo(&empty));
    }
}
#[cfg(test)]
mod transfer_tests {
    #[test]
    fn scp_remote_directory_drop_is_rejected_before_submission() {
        use harbor_workspace::remote_files::capabilities::RemoteBackendKind;
        let mut state = TransferUi::default();
        state.backend = RemoteBackendKind::Scp;
        state.local_download = Some(PathBuf::from("chosen-local-directory"));
        let payload = RemoteDragPayload {
            connection: 42,
            path: RemotePath::from_bytes(b"/remote/folder".to_vec()).unwrap(),
            kind: EntryKind::Directory,
        };
        let error = state.download_drop_operation(&payload, 42).unwrap_err();
        assert!(error.contains("SFTP"), "{error}");
    }
    use super::*;
    use harbor_workspace::remote_files::{RemoteEntry, transfer::TransferOperation};
    #[test]
    fn old_listing_rows_cannot_be_selected_during_navigation() {
        use crate::ui::{gui_tests::shell_text_position, terminal_review_tests::owned_workspace};
        let (ctx, _root, mut workspace) = owned_workspace();
        workspace.start(Session {
            host: "127.0.0.1".into(),
            port: 1,
            ..Default::default()
        });
        let tab = &mut workspace.tabs[0];
        // This test owns the listing state. A real worker must not update it during a frame.
        drop(tab.ssh.take());
        tab.ssh_connected = true;
        let browser = tab.browser.as_mut().unwrap();
        browser.loading = true;
        browser.path = RemotePath::from_bytes(b"/old".to_vec()).unwrap();
        let path = RemotePath::from_bytes(b"/old/stale-entry".to_vec()).unwrap();
        browser.entries = vec![RemoteEntry {
            path: path.clone(),
            display_name: "stale-entry".into(),
            kind: EntryKind::File,
            size: Some(1),
        }];
        let screen = Some(Rect::from_min_size(Pos2::ZERO, vec2(1360., 1000.)));
        let draw = |workspace: &mut Workspace, time, events| {
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: screen,
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ui| workspace.render(ui),
            );
            output.textures_delta.clear();
            output
        };
        let output = draw(&mut workspace, 0., vec![]);
        let heading = shell_text_position(&output, "Remote files");
        draw(
            &mut workspace,
            0.1,
            vec![
                Event::PointerMoved(heading),
                Event::MouseWheel {
                    unit: MouseWheelUnit::Point,
                    delta: vec2(0., -320.),
                    phase: TouchPhase::Move,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        let click_row = |workspace: &mut Workspace, time| {
            let output = draw(workspace, time, vec![]);
            let position = shell_text_position(&output, "·  stale-entry");
            for (pressed, offset) in [(true, 0.1), (false, 0.2)] {
                draw(
                    workspace,
                    time + offset,
                    vec![
                        Event::PointerMoved(position),
                        Event::PointerButton {
                            pos: position,
                            button: PointerButton::Primary,
                            pressed,
                            modifiers: Modifiers::NONE,
                        },
                    ],
                );
            }
        };
        click_row(&mut workspace, 0.2);
        let tab = &mut workspace.tabs[0];
        assert!(tab.ssh_connected && tab.browser.as_ref().unwrap().loading);
        assert!(
            tab.transfer_ui.selected.is_none(),
            "A stale listing row became the action target while navigating"
        );
        tab.browser.as_mut().unwrap().loading = false;
        click_row(&mut workspace, 1.);
        assert_eq!(
            workspace.tabs[0]
                .transfer_ui
                .selected
                .as_ref()
                .map(|entry| &entry.path),
            Some(&path),
            "The same visible row must be selectable when navigation completes"
        );
    }
    #[test]
    fn remote_drop_cannot_cross_connection_or_escape_local_directory() {
        let mut state = TransferUi::default();
        state.local_download = Some(PathBuf::from("/chosen/local"));
        let payload = RemoteDragPayload {
            connection: 41,
            path: RemotePath::from_bytes(b"/remote/file".to_vec()).unwrap(),
            kind: EntryKind::File,
        };
        assert!(state.download_drop_operation(&payload, 42).is_err());
        match state.download_drop_operation(&payload, 41).unwrap() {
            TransferOperation::Download { local, remote, .. } => {
                assert_eq!(local, PathBuf::from("/chosen/local/file"));
                assert_eq!(remote, payload.path);
            }
            _ => panic!("Remote drop must only download"),
        }
        let payload = RemoteDragPayload {
            path: RemotePath::from_bytes(b"/remote/..".to_vec()).unwrap(),
            ..payload
        };
        assert!(state.download_drop_operation(&payload, 41).is_err());
    }
    #[test]
    fn folder_download_and_dropped_upload_keep_explicit_roots() {
        let root = RemotePath::from_bytes(b"/remote/chosen".to_vec()).unwrap();
        let operation = upload_operation(
            PathBuf::from("/local/source/folder"),
            &root,
            ConflictPolicy::Ask,
        )
        .unwrap();
        match operation {
            TransferOperation::Upload { remote, .. } => {
                assert_eq!(remote.as_bytes(), b"/remote/chosen/folder")
            }
            _ => panic!("A local drop must only upload"),
        }
        let mut ui = TransferUi::default();
        ui.selected = Some(RemoteEntry {
            path: root.clone(),
            display_name: "chosen".into(),
            kind: EntryKind::Directory,
            size: None,
        });
        assert!(
            matches!(ui.download_operation(PathBuf::from("/local/chosen")).unwrap(),TransferOperation::Download{remote,..}if remote==root)
        );
    }
    #[test]
    fn recursive_deletion_requires_the_explicit_contents_choice() {
        let target = RemotePath::from_bytes(b"/folder".to_vec()).unwrap();
        let mut form = FileActionForm::Delete {
            target: target.clone(),
            directory: true,
            include_contents: false,
        };
        assert!(matches!(
            form.operation(ConflictPolicy::Ask).unwrap(),
            TransferOperation::Remove { .. }
        ));
        if let FileActionForm::Delete {
            include_contents, ..
        } = &mut form
        {
            *include_contents = true;
        }
        assert!(
            matches!(form.operation(ConflictPolicy::Ask).unwrap(),TransferOperation::RemoveTree{remote,confirmed}if remote==target&&confirmed==target)
        );
    }
    #[test]
    fn action_dialog_keeps_its_confirmed_target_and_rejects_traversal() {
        let target = RemotePath::from_bytes(b"/dir/original".to_vec()).unwrap();
        let form = FileActionForm::Delete {
            target: target.clone(),
            directory: true,
            include_contents: false,
        };
        match form.operation(ConflictPolicy::Ask).unwrap() {
            TransferOperation::Remove { remote, confirmed } => {
                assert_eq!(remote, target);
                assert_eq!(confirmed, target);
            }
            _ => panic!("wrong operation"),
        }
        let form = FileActionForm::NewFolder {
            parent: target,
            name: "../outside".into(),
        };
        assert!(form.operation(ConflictPolicy::Ask).is_err());
    }
    #[test]
    fn download_uses_selected_raw_path_and_requires_a_regular_file() {
        let mut state = TransferUi::default();
        assert!(state.download_operation(PathBuf::from("out")).is_err());
        let raw = RemotePath::from_bytes(b"/dir/file\xff\n$(literal)".to_vec()).unwrap();
        state.selected = Some(RemoteEntry {
            path: raw.clone(),
            display_name: raw.display(),
            kind: EntryKind::File,
            size: None,
        });
        match state.download_operation(PathBuf::from("out")).unwrap() {
            TransferOperation::Download { remote, policy, .. } => {
                assert_eq!(remote, raw);
                assert_eq!(
                    policy,
                    harbor_workspace::remote_files::transfer::ConflictPolicy::Ask
                );
            }
            _ => panic!("wrong target"),
        }
        state.selected.as_mut().unwrap().kind = EntryKind::Symlink;
        assert!(state.download_operation(PathBuf::from("out")).is_err());
    }
}

pub(super) fn authentication_ui(ui: &mut Ui, tab: &mut TerminalTab) -> bool {
    if let Some(auth) = &mut tab.auth {
        ui.separator();
        ui.strong(&auth.title);
        for (index, prompt) in auth.prompts.iter().enumerate() {
            let label = ui.label(&prompt.label);
            auth_field(ui, tab.id, auth.id, index, prompt, &mut auth.values[index])
                .labelled_by(label.id);
        }
        if let Some(credential) = &auth.credential {
            ui.horizontal_wrapped(|ui|{
                if credential.reference.is_some()&&ui.button("Use saved credential").clicked(){tab.vault_request=Some(credentials::Request::use_terminal(tab.id,auth.id,credential.clone()));}
                ui.checkbox(&mut auth.save_in_vault,"Save entered value in vault").on_hover_text("Unlock the vault first. Saving is optional and does not wait for authentication success.");
            });
        }
        let host_key = auth.host_key;
        let mut decision = None;
        ui.horizontal_wrapped(|ui| {
            if host_key {
                if ui.button("Trust once").clicked() {
                    decision = Some(Some(TrustDecision::Once));
                }
                if ui.button("Trust and remember").clicked() {
                    decision = Some(Some(TrustDecision::Remember));
                }
            } else if primary(ui, "Continue").clicked() {
                decision = Some(None);
            }
            let safe = ui.button("Cancel");
            if host_key {
                super::fluent::focus_on_entry(ui, &safe, Id::new((tab.id, auth.id)));
            }
            if safe.clicked() {
                decision = Some(Some(TrustDecision::Reject));
            }
        });
        if let Some(trust) = decision {
            tab.answer_auth(trust);
        }
        return true;
    }

    false
}
