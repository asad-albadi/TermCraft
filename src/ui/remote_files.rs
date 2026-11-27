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

fn navigation_button(ui: &mut Ui, enabled: bool, kind: &str, label: &str) -> Response {
    let response = ui.add_enabled(
        enabled,
        Button::new("").min_size(Vec2::splat(fluent::CONTROL_HEIGHT)),
    );
    let center = response.rect.center();
    let stroke = Stroke::new(
        1.6,
        if response.enabled() {
            ink(ui)
        } else {
            muted(ui)
        },
    );
    let line = |a: Vec2, b: Vec2| {
        ui.painter().line_segment([center + a, center + b], stroke);
    };
    match kind {
        "parent" => {
            line(vec2(0., 6.), vec2(0., -6.));
            line(vec2(-4., -2.), vec2(0., -6.));
            line(vec2(0., -6.), vec2(4., -2.));
        }
        "go" => {
            line(vec2(-6., 0.), vec2(6., 0.));
            line(vec2(2., -4.), vec2(6., 0.));
            line(vec2(6., 0.), vec2(2., 4.));
        }
        _ => {
            let points = (0..24)
                .map(|n| {
                    let angle =
                        -std::f32::consts::FRAC_PI_2 + n as f32 / 23. * std::f32::consts::PI * 1.5;
                    center + vec2(angle.cos(), angle.sin()) * 6.
                })
                .collect();
            ui.painter()
                .add(egui::epaint::PathShape::line(points, stroke));
            line(vec2(-9., 4.), vec2(-6., 0.));
            line(vec2(-6., 0.), vec2(-3., 4.));
        }
    }
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, response.enabled(), label));
    response.fluent_hint(label)
}

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
    address_path: Option<RemotePath>,
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
    fn local_download_settings(&mut self, ui: &mut Ui) {
        ui.label("Local download folder");
        ui.horizontal_wrapped(|ui| {
            if ui
                .add(
                    crate::ui::fluent::singleline(&mut self.local_path_text)
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
    }
    fn local_drop_zone(
        &mut self,
        ui: &mut Ui,
        connection: u64,
        ready: bool,
        ssh: Option<&mut harbor_workspace::ssh::ConnectionHandle>,
    ) {
        ui.separator();
        let (_, payload) =
            ui.dnd_drop_zone::<RemoteDragPayload, _>(Frame::new().inner_margin(2), |ui| {
                ui.horizontal(|ui| {
                    ui.menu_button("Downloads", |ui| self.local_download_settings(ui))
                        .response
                        .fluent_hint("Choose the local folder used for drag-and-drop downloads");
                    let destination = self.local_download.as_ref().map_or_else(
                        || "Choose a download folder first".to_owned(),
                        |path| {
                            format!(
                                "Drop a remote file here to download into {}",
                                path.display()
                            )
                        },
                    );
                    ui.label(RichText::new("Drop here").color(muted(ui)))
                        .fluent_hint(destination);
                });
            });
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
        ui.menu_button("Upload", |ui| {
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
        });
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
                let result = rfd::FileDialog::new()
                    .set_file_name(name)
                    .save_file()
                    .map(|local| {
                        let TransferOperation::Download { remote, policy, .. } = operation else {
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
    }
    fn entry_actions(
        &mut self,
        ui: &mut Ui,
        path: &RemotePath,
        ready: bool,
        capabilities: harbor_workspace::remote_files::capabilities::RemoteCapabilities,
    ) {
        if ui
            .add_enabled(
                ready && capabilities.mutate_entries,
                Button::new("New folder…"),
            )
            .clicked()
        {
            ui.close();
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
            ui.close();
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
            ui.close();
            self.form = Some(FileActionForm::Delete {
                target: self.selected.as_ref().unwrap().path.clone(),
                directory: self.selected.as_ref().unwrap().kind == EntryKind::Directory,
                include_contents: false,
            });
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
                    ui.add(crate::ui::fluent::singleline(name).hint_text("Folder name"));
                }
                FileActionForm::Rename { source, name } => {
                    ui.label("Rename this remote entry:");
                    ui.monospace(source.display());
                    ui.add(crate::ui::fluent::singleline(name).hint_text("New name"));
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
                let tab = &mut self.tabs[self.active];
                ui.horizontal(|ui| {
                    ui.strong("Remote files").fluent_hint(format!("{} · Embedded {}", tab.session.host, tab.session.remote_backend.label()));
                    ui.small(tab.session.remote_backend.label());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("×").fluent_hint("Hide remote files").clicked() {
                            tab.browser.as_mut().unwrap().visible = false;
                        }
                    });
                });
                if tab.auth.is_some() {
                    ScrollArea::vertical().id_salt(("remote-auth",id))
                        .max_height(ui.available_height())
                        .show(ui, |ui| authentication_ui(ui,tab));
                    return;
                }
                if !tab.ssh_connected { ui.label(&tab.connection_status); }
                let browser = tab.browser.as_ref().unwrap();
                let ready = tab.ssh_connected && !browser.loading && browser.error.is_none();
                let path = browser.path.clone();
                if !browser.loading && tab.transfer_ui.address_path.as_ref() != Some(&path) {
                    tab.remote_path = String::from_utf8(path.as_bytes().to_vec()).unwrap_or_default();
                    tab.transfer_ui.address_path = Some(path.clone());
                }
                let mut navigate = None;
                let mut go = false;
                ui.horizontal(|ui| {
                    if navigation_button(ui,tab.ssh_connected,"parent","Parent folder").clicked() { navigate = Some(path.parent()); }
                    let response = ui.add_enabled(tab.ssh_connected,
                        crate::ui::fluent::singleline(&mut tab.remote_path)
                            .id(Id::new(("remote-address",id)))
                            .hint_text(path.display()).desired_width((ui.available_width()-40.).max(40.)));
                    let response = response.fluent_hint(format!("Current folder: {}\nEnter an absolute path and press Enter",path.display()));
                    go = response.lost_focus() && ui.input_mut(|i| i.consume_key(Modifiers::NONE,Key::Enter));
                    let edited = !tab.remote_path.is_empty() && tab.remote_path.as_bytes() != path.as_bytes();
                    if navigation_button(ui,tab.ssh_connected,if edited { "go" } else { "refresh" },
                        if edited { "Open folder" } else { "Refresh folder" }).clicked() {
                        if edited { go = true; } else { navigate = Some(path.clone()); }
                    }
                });
                if go {
                    if tab.remote_path.starts_with('/') {
                        match RemotePath::from_bytes(tab.remote_path.as_bytes().to_vec()) {
                            Ok(path) => navigate = Some(path), Err(error) => tab.error = error,
                        }
                    } else { tab.error = "Enter an absolute remote folder path.".into(); }
                }
                let capabilities = tab.transfer_ui.effective_capabilities();
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().button_padding.x = 8.;
                    tab.transfer_ui.actions(ui,&path,ready,capabilities);
                    ui.menu_button("More", |ui| {
                        tab.transfer_ui.entry_actions(ui,&path,ready,capabilities);
                        ui.separator();
                        ui.menu_button("Shell integration", |ui| tab.shell_follow_controls(ui));
                        ui.menu_button("Transfer settings", |ui| {
                            ui.checkbox(&mut tab.transfer_ui.overwrite,"Replace existing destination files")
                                .fluent_hint("Off by default. Conflicting files are preserved until you explicitly choose replacement and retry.");
                            tab.transfer_ui.local_download_settings(ui);
                        });
                        ui.menu_button("Help", |ui| {
                            ui.set_max_width(260.);
                            ui.label("Double-click a folder to open it, or a file to edit it. Right-click an entry for file actions.");
                            ui.label("Drop local files onto the file list to upload. Drop a remote entry onto Downloads to save it in the chosen local folder.");
                            ui.label("Use Shell integration to follow the terminal folder or copy setup for your remote shell.");
                        });
                    });
                });
                if tab.transfer_ui.overwrite {
                    ui.checkbox(&mut tab.transfer_ui.overwrite,"Replace existing files")
                        .fluent_hint("Replacement is enabled for transfers. Uncheck to preserve conflicting destination files.");
                }
                if let Some(error) = &tab.transfer_ui.error {
                    ui.colored_label(ui.visuals().error_fg_color,error);
                }
                let browser = tab.browser.as_ref().unwrap();
                if browser.loading { ui.horizontal(|ui| { ui.spinner(); ui.label("Loading files…"); }); }
                if let Some(error) = &browser.error { ui.colored_label(ui.visuals().error_fg_color,error); }
                if !tab.transfer_ui.events.is_empty() {
                    let active = tab.transfer_ui.events.iter().filter(|e|e.outcome.is_none()).count();
                    let attention = tab.transfer_ui.events.iter().any(|e|!matches!(e.outcome,Some(TransferOutcome::Completed)));
                    let issues = tab.transfer_ui.events.iter().filter(|e| e.outcome.is_some() && !matches!(e.outcome,Some(TransferOutcome::Completed))).count();
                    egui::CollapsingHeader::new(format!("Transfers · {active} active · {issues} notices"))
                        .id_salt(("remote-transfers",id)).default_open(attention)
                        .open((active > 0).then_some(true))
                        .show(ui, |ui| tab.transfer_ui.jobs(ui,tab.ssh.as_ref()));
                }
                ui.separator();
                let listing = ScrollArea::vertical()
                            .id_salt(("remote-entries", id))
                            .auto_shrink([false,false])
                            .max_height((ui.available_height()-56.).max(32.))
                            .min_scrolled_height(32.)
                            .show(ui, |ui| {
                                for entry in &browser.entries {
                                    let row_id =
                                        ui.id().with(("remote-entry", id, entry.path.as_bytes()));
                                    let selected = tab
                                        .transfer_ui
                                        .selected
                                        .as_ref()
                                        .is_some_and(|row| row.path == entry.path);
                                    let response = ui
                                        .add_enabled_ui(
                                            !browser.loading && tab.ssh_connected,
                                            |ui| {
                                                explorer::ExplorerRow::remote(entry).show(
                                                    ui,
                                                    row_id,
                                                    selected,
                                                    ui.spacing().interact_size.y,
                                                    Sense::click_and_drag(),
                                                )
                                            },
                                        )
                                        .inner;
                                    response.dnd_set_drag_payload(RemoteDragPayload {
                                        connection: id,
                                        path: entry.path.clone(),
                                        kind: entry.kind,
                                    });
                                    if response.clicked() {
                                        tab.transfer_ui.selected = Some(entry.clone());
                                    }
                                    if ready {
                                        response.context_menu(|ui| {
                                            tab.transfer_ui.selected = Some(entry.clone());
                                            tab.transfer_ui.entry_actions(ui,&path,ready,capabilities);
                                        });
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
                let drop_target = ui.interact(listing.inner_rect,ui.id().with("upload-list"),Sense::hover());
                tab.transfer_ui.dropped_files(ui,&path,ready,tab.ssh.as_mut(),&drop_target);
                tab.transfer_ui.local_drop_zone(ui,id,ready,tab.ssh.as_mut());
                if let Some(path) = navigate { tab.navigate_remote(path); }
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
    let response = crate::ui::fluent::singleline(value)
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
    fn narrow_terminal_moves_commands_into_reachable_overflow() {
        use crate::ui::{gui_tests::shell_text_position, terminal_review_tests::owned_workspace};
        let (ctx, _root, mut workspace) = owned_workspace();
        workspace.start(Session {
            host: "127.0.0.1".into(),
            port: 1,
            ..Default::default()
        });
        drop(workspace.tabs[0].ssh.take());
        let draw = |workspace: &mut Workspace, events| {
            super::super::gui_tests::shell_frame(&ctx, vec2(260., 640.), 1., events, |ui| {
                workspace.terminals(ui)
            })
        };
        fn labels(output: &egui::FullOutput) -> String {
            fn add(shape: &egui::epaint::Shape, text: &mut String) {
                match shape {
                    egui::epaint::Shape::Text(t) => {
                        text.push_str(t.galley.text());
                        text.push('\n');
                    }
                    egui::epaint::Shape::Vec(v) => {
                        for s in v {
                            add(s, text);
                        }
                    }
                    _ => {}
                }
            }
            let mut text = String::new();
            for s in &output.shapes {
                add(&s.shape, &mut text);
            }
            text
        }
        draw(&mut workspace, vec![]);
        let output = draw(&mut workspace, vec![]);
        let text = labels(&output);
        assert!(
            !text
                .lines()
                .any(|label| label == "Find" || label == "Reconnect"),
            "Commands crowd a narrow terminal: {text}"
        );
        let more = shell_text_position(&output, "More");
        for pressed in [true, false] {
            draw(
                &mut workspace,
                vec![
                    Event::PointerMoved(more),
                    Event::PointerButton {
                        pos: more,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
        let output = draw(&mut workspace, vec![]);
        let text = labels(&output);
        assert!(text.contains("Reconnect"));
        let find = shell_text_position(&output, "Find");
        for pressed in [true, false] {
            draw(
                &mut workspace,
                vec![
                    Event::PointerMoved(find),
                    Event::PointerButton {
                        pos: find,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
        assert!(
            workspace.tabs[0].search_open,
            "Overflow Find must open terminal search"
        );
    }
    #[test]
    fn remote_path_mouse_go_preserves_draft_and_validates_before_navigation() {
        use crate::ui::terminal_review_tests::owned_workspace;
        let (ctx, _root, mut workspace) = owned_workspace();
        workspace.start(Session {
            host: "127.0.0.1".into(),
            port: 1,
            ..Default::default()
        });
        let tab = &mut workspace.tabs[0];
        drop(tab.ssh.take());
        tab.ssh_connected = true;
        let browser = tab.browser.as_mut().unwrap();
        browser.loading = false;
        browser.error = None;
        tab.transfer_ui.address_path = Some(browser.path.clone());
        tab.remote_path = "relative-draft".into();
        let field_id = Id::new(("remote-address", tab.id));
        let draw = |workspace: &mut Workspace, events| {
            super::super::gui_tests::shell_frame(&ctx, vec2(1360., 760.), 1., events, |ui| {
                workspace.render(ui)
            })
        };
        draw(&mut workspace, vec![]);
        draw(&mut workspace, vec![]);
        let field = ctx
            .read_response(field_id)
            .expect("Address field has a stable identity");
        let button = field.rect.right_center() + vec2(24., 0.);
        for pressed in [true, false] {
            draw(
                &mut workspace,
                vec![
                    Event::PointerMoved(button),
                    Event::PointerButton {
                        pos: button,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
        assert_eq!(workspace.tabs[0].remote_path, "relative-draft");
        assert_eq!(
            workspace.tabs[0].error,
            "Enter an absolute remote folder path."
        );
        assert!(!workspace.tabs[0].browser.as_ref().unwrap().loading);
    }
    #[test]
    fn remote_row_menu_uses_its_own_target_and_keeps_mutations_confirmed() {
        use crate::ui::{gui_tests::shell_text_position, terminal_review_tests::owned_workspace};
        let (ctx, _root, mut workspace) = owned_workspace();
        workspace.start(Session {
            host: "127.0.0.1".into(),
            port: 1,
            ..Default::default()
        });
        let tab = &mut workspace.tabs[0];
        drop(tab.ssh.take());
        tab.ssh_connected = true;
        let browser = tab.browser.as_mut().unwrap();
        browser.loading = false;
        browser.error = None;
        let entries: Vec<_> = ["previous-target", "clicked-target"]
            .into_iter()
            .map(|name| RemoteEntry {
                path: RemotePath::from_bytes(format!("/{name}").into_bytes()).unwrap(),
                display_name: name.into(),
                kind: EntryKind::File,
                size: Some(1),
            })
            .collect();
        tab.transfer_ui.selected = Some(entries[0].clone());
        browser.entries = entries.clone();
        let draw = |workspace: &mut Workspace, events| {
            super::super::gui_tests::shell_frame(&ctx, vec2(1360., 760.), 1., events, |ui| {
                workspace.render(ui)
            })
        };
        draw(&mut workspace, vec![]);
        let output = draw(&mut workspace, vec![]);
        let target = shell_text_position(&output, "clicked-target");
        for pressed in [true, false] {
            draw(
                &mut workspace,
                vec![
                    Event::PointerMoved(target),
                    Event::PointerButton {
                        pos: target,
                        button: PointerButton::Secondary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
        let output = draw(&mut workspace, vec![]);
        let delete = shell_text_position(&output, "Delete…");
        for pressed in [true, false] {
            draw(
                &mut workspace,
                vec![
                    Event::PointerMoved(delete),
                    Event::PointerButton {
                        pos: delete,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
        let form = workspace.tabs[0]
            .transfer_ui
            .form
            .as_ref()
            .expect("Deletion must open a confirmation");
        assert!(
            matches!(form,FileActionForm::Delete { target, include_contents: false, .. } if target == &entries[1].path)
        );
        assert!(!workspace.tabs[0].transfer_ui.overwrite);
    }
    #[test]
    fn remote_pane_prioritizes_listing_and_keeps_options_on_demand() {
        use crate::ui::{gui_tests::shell_text_position, terminal_review_tests::owned_workspace};
        for height in [640., 1000.] {
            let (ctx, _root, mut workspace) = owned_workspace();
            workspace.start(Session {
                host: "127.0.0.1".into(),
                port: 1,
                ..Default::default()
            });
            let tab = &mut workspace.tabs[0];
            drop(tab.ssh.take());
            tab.ssh_connected = true;
            let browser = tab.browser.as_mut().unwrap();
            browser.loading = false;
            browser.error = None;
            browser.entries = (0..40)
                .map(|n| RemoteEntry {
                    path: RemotePath::from_bytes(format!("/entry-{n:02}").into_bytes()).unwrap(),
                    display_name: format!("entry-{n:02}"),
                    kind: EntryKind::Directory,
                    size: None,
                })
                .collect();
            let mut output = None;
            for _ in 0..3 {
                let mut frame = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1360., height))),
                        ..Default::default()
                    },
                    |ui| workspace.render(ui),
                );
                frame.textures_delta.clear();
                output = Some(frame);
            }
            let output = output.unwrap();
            let panel_id = Id::new(("remote-files", workspace.tabs[0].id));
            let initial_width = egui::containers::panel::PanelState::load(&ctx, panel_id)
                .unwrap()
                .size()
                .x;
            for _ in 0..20 {
                let mut frame = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1360., height))),
                        ..Default::default()
                    },
                    |ui| workspace.render(ui),
                );
                frame.textures_delta.clear();
            }
            let idle_width = egui::containers::panel::PanelState::load(&ctx, panel_id)
                .unwrap()
                .size()
                .x;
            assert!(
                (idle_width - initial_width).abs() < 1.,
                "Idle pane expanded from {initial_width} to {idle_width}"
            );
            let first = shell_text_position(&output, "entry-00");
            assert!(first.y < 330., "File listing starts too low: {first:?}");
            let row = if height == 640. {
                "entry-05"
            } else {
                "entry-13"
            };
            let last = shell_text_position(&output, row);
            assert!(
                last.y < height - 50.,
                "Available listing space was not used: {last:?}"
            );
            fn collect(shape: &egui::epaint::Shape, text: &mut String) {
                match shape {
                    egui::epaint::Shape::Text(t) => {
                        text.push_str(t.galley.text());
                        text.push('\n');
                    }
                    egui::epaint::Shape::Vec(v) => {
                        for s in v {
                            collect(s, text);
                        }
                    }
                    _ => {}
                }
            }
            let mut text = String::new();
            for shape in &output.shapes {
                collect(&shape.shape, &mut text);
            }
            for hidden in [
                "Manual navigation",
                "Follow shell directory changes",
                "Replace existing destination files",
                "Drop local files or folders here to upload.",
            ] {
                assert!(
                    !text.contains(hidden),
                    "Advanced guidance consumes listing space: {hidden}"
                );
            }
        }
    }
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
            let position = shell_text_position(&output, "stale-entry");
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
        let first_id = ctx
            .memory(|m| m.focused())
            .expect("clicked remote row should own keyboard focus");
        let tab = &mut workspace.tabs[0];
        tab.browser.as_mut().unwrap().entries.insert(
            0,
            RemoteEntry {
                path: RemotePath::from_bytes(b"/old/inserted".to_vec()).unwrap(),
                display_name: "inserted".into(),
                kind: EntryKind::File,
                size: None,
            },
        );
        tab.transfer_ui.selected = None;
        draw(&mut workspace, 1.5, vec![]);
        assert!(ctx.read_response(first_id).is_some_and(|r| r.has_focus()));
        draw(
            &mut workspace,
            1.6,
            vec![Event::Key {
                key: Key::Space,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
        assert_eq!(
            workspace.tabs[0]
                .transfer_ui
                .selected
                .as_ref()
                .map(|e| &e.path),
            Some(&path),
            "inserting a different row moved keyboard focus to another file"
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
                ui.checkbox(&mut auth.save_in_vault,"Save entered value in vault").fluent_hint("Unlock the vault first. Saving is optional and does not wait for authentication success.");
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
