use super::*;
use harbor_workspace::{
    credentials::{CredentialBinding, CredentialRole, Secret, vault::Vault},
    ssh::ChallengeCredential,
};
use std::collections::VecDeque;
use zeroize::{Zeroize, Zeroizing};
#[derive(Clone, PartialEq, Eq)]
pub(super) enum Destination {
    Desktop {
        connection: u64,
        challenge: u64,
    },
    Terminal {
        connection: u64,
        challenge: u64,
    },
    Shared {
        source: String,
        generation: u64,
        owner: u64,
        answer: u64,
    },
}
pub(super) enum Request {
    Use {
        destination: Destination,
        credential: ChallengeCredential,
    },
    Save {
        destination: Destination,
        credential: ChallengeCredential,
        values: Vec<Zeroizing<String>>,
    },
}
impl Request {
    pub(super) fn use_terminal(
        connection: u64,
        challenge: u64,
        credential: ChallengeCredential,
    ) -> Self {
        Self::Use {
            destination: Destination::Terminal {
                connection,
                challenge,
            },
            credential,
        }
    }
}
#[derive(Clone)]
enum Link {
    Session(String),
    Source(String),
}
enum Effect {
    Opened,
    Changed,
    Deleted(String),
    Stored {
        id: String,
        binding: CredentialBinding,
        link: Link,
    },
}
struct Outcome {
    vault: Option<Vault>,
    result: Result<Effect, String>,
}
struct Job {
    rx: Receiver<Outcome>,
}
#[derive(Default)]
pub(super) struct State {
    vault: Option<Vault>,
    job: Option<Job>,
    finished: Option<Outcome>,
    queue: VecDeque<Request>,
    uses: VecDeque<(Destination, ChallengeCredential)>,
    pub(super) open: bool,
    editor: Option<MasterEditor>,
    error: String,
    lock_requested: bool,
    close_after_job: bool,
}
#[derive(Clone, Copy, PartialEq)]
enum MasterMode {
    Create,
    Unlock,
    Change,
}
struct MasterEditor {
    mode: MasterMode,
    master: Zeroizing<String>,
    repeat: Zeroizing<String>,
}
impl MasterEditor {
    fn new(mode: MasterMode) -> Self {
        Self {
            mode,
            master: Zeroizing::new(String::new()),
            repeat: Zeroizing::new(String::new()),
        }
    }
}
enum Operation {
    Create(Secret),
    Unlock(Secret),
    Put {
        vault: Vault,
        id: String,
        binding: CredentialBinding,
        secret: Secret,
        link: Link,
    },
    Delete {
        vault: Vault,
        id: String,
    },
    Change {
        vault: Vault,
        master: Secret,
    },
}
impl Workspace {
    pub(super) fn vault_path(&self) -> PathBuf {
        self.store_path.parent().unwrap().join("credentials.vault")
    }
    pub(super) fn vault_unlocked(&self) -> bool {
        self.credentials
            .vault
            .as_ref()
            .is_some_and(|v| v.is_unlocked())
            && !self.credentials.lock_requested
    }
    pub(super) fn vault_busy(&self) -> bool {
        self.credentials.job.is_some()
            || self.credentials.finished.is_some()
            || !self.credentials.queue.is_empty()
    }
    pub(super) fn vault_modal(&self) -> bool {
        self.credentials.open
    }
    pub(super) fn create_vault(&mut self, master: Secret, ctx: &Context) {
        self.start_vault(Operation::Create(master), ctx);
    }
    pub(super) fn unlock_vault(&mut self, master: Secret, ctx: &Context) {
        self.start_vault(Operation::Unlock(master), ctx);
    }
    pub(super) fn delete_vault_credential(&mut self, id: String, ctx: &Context) {
        if let Some(vault) = self.credentials.vault.take() {
            self.start_vault(Operation::Delete { vault, id }, ctx);
        }
    }
    fn start_vault(&mut self, operation: Operation, ctx: &Context) {
        if self.credentials.job.is_some() || self.credentials.finished.is_some() {
            self.credentials.error = "A vault operation is already running.".into();
            return;
        }
        let path = self.vault_path();
        let (tx, rx) = mpsc::sync_channel(1);
        let ctx = ctx.clone();
        let result = std::thread::Builder::new()
            .name("credential-vault".into())
            .spawn(move || {
                let outcome = match operation {
                    Operation::Create(master) => match Vault::create(&path, master) {
                        Ok(v) => Outcome {
                            vault: Some(v),
                            result: Ok(Effect::Opened),
                        },
                        Err(e) => Outcome {
                            vault: None,
                            result: Err(e),
                        },
                    },
                    Operation::Unlock(master) => match Vault::unlock(&path, master) {
                        Ok(v) => Outcome {
                            vault: Some(v),
                            result: Ok(Effect::Opened),
                        },
                        Err(e) => Outcome {
                            vault: None,
                            result: Err(e),
                        },
                    },
                    Operation::Put {
                        mut vault,
                        id,
                        binding,
                        secret,
                        link,
                    } => {
                        let result = vault
                            .put_bound(id.clone(), secret, binding.clone())
                            .map(|_| Effect::Stored { id, binding, link });
                        Outcome {
                            vault: Some(vault),
                            result,
                        }
                    }
                    Operation::Delete { mut vault, id } => {
                        let result = vault.delete(&id).map(|_| Effect::Deleted(id));
                        Outcome {
                            vault: Some(vault),
                            result,
                        }
                    }
                    Operation::Change { mut vault, master } => {
                        let result = vault.change_master(master).map(|_| Effect::Changed);
                        Outcome {
                            vault: Some(vault),
                            result,
                        }
                    }
                };
                let _ = tx.send(outcome);
                ctx.request_repaint();
            });
        match result {
            Ok(_) => {
                self.credentials.job = Some(Job { rx });
                self.credentials.error.clear();
            }
            Err(_) => {
                self.credentials.error =
                    "Could not start vault worker; unlock the vault again.".into();
            }
        }
    }
    pub(super) fn lock_vault(&mut self) {
        if let Some(mut vault) = self.credentials.vault.take() {
            vault.lock();
        }
        self.credentials.lock_requested =
            self.credentials.job.is_some() || self.credentials.finished.is_some();
        self.credentials.queue.clear();
        self.credentials.uses.clear();
        self.credentials.editor = None;
        for tab in &mut self.tabs {
            if let Some(desktop) = &mut tab.desktop {
                desktop.clear_vault_password();
            }
            if let Some(auth) = &mut tab.auth {
                if auth.vault_filled {
                    for value in &mut auth.values {
                        value.zeroize();
                    }
                    auth.vault_filled = false;
                }
            }
        }
        self.clear_shared_vault_values();
    }
    fn forget_credential_ui(&mut self, id: &str) {
        fn references(session: &mut Session, id: &str) {
            if session.credential_ref.as_deref() == Some(id) {
                session.credential_ref = None;
            }
            session
                .gateway_credential_refs
                .retain(|_, value| value != id);
            if let Some(proxy) = &mut session.ssh_options.proxy {
                if proxy.credential_ref.as_deref() == Some(id) {
                    proxy.credential_ref = None;
                }
            }
            if session.gateway_credential_ref.as_deref() == Some(id) {
                session.gateway_credential_ref = None;
            }
        }
        if let Some(form) = &mut self.session_form {
            references(form, id);
        }
        for tab in &mut self.tabs {
            if tab.session.credential_ref.as_deref() == Some(id) {
                if let Some(desktop) = &mut tab.desktop {
                    desktop.clear_vault_password();
                }
            }
            references(&mut tab.session, id);
            if let Some(auth) = &mut tab.auth {
                if auth
                    .credential
                    .as_ref()
                    .is_some_and(|c| c.reference.as_deref() == Some(id))
                {
                    if auth.vault_filled {
                        for value in &mut auth.values {
                            value.zeroize();
                        }
                    }
                    auth.vault_filled = false;
                    auth.save_in_vault = false;
                    if let Some(c) = &mut auth.credential {
                        c.reference = None;
                    }
                }
            }
        }
        self.credentials
            .uses
            .retain(|(_, c)| c.reference.as_deref() != Some(id));
        self.credentials.queue.retain(|request| match request {
            Request::Use { credential, .. } | Request::Save { credential, .. } => {
                credential.reference.as_deref() != Some(id)
            }
        });
        self.forget_shared_credential_ui(id);
    }
    pub(super) fn request_vault(&mut self, request: Request, ctx: &Context) {
        if self.credentials.queue.len() >= 32 {
            self.error_in(
                errors::Domain::Vault,
                "Vault request queue is full; this credential was not saved or used.",
            );
            return;
        }
        self.credentials.queue.push_back(request);
        self.poll_vault(ctx);
    }
    fn vault_link(&self, destination: &Destination, binding: &CredentialBinding) -> Option<Link> {
        match binding {
            CredentialBinding::Rdp { profile, .. } => {
                let Destination::Desktop { connection, .. } = destination else {
                    return None;
                };
                let tab = self.tabs.iter().find(|t| t.id == *connection)?;
                if harbor_workspace::desktop::rdp_options::auth_profile_binding(&tab.session)
                    != *profile
                {
                    return None;
                }
                self.store
                    .sessions
                    .iter()
                    .find(|s| {
                        s.id == tab.session.id
                            && harbor_workspace::desktop::rdp_options::auth_profile_binding(s)
                                == *profile
                    })
                    .map(|s| Link::Session(s.id.clone()))
            }
            CredentialBinding::HttpsBasic { source_id, url } => {
                if !matches!(destination,Destination::Shared{source,..} if source==source_id) {
                    return None;
                }
                self.store.shared_sources.iter().find(|s|&s.id==source_id&&matches!(&s.location,harbor_workspace::sessions::shared::SharedLocation::Https(uri)if uri==url)).map(|_|Link::Source(source_id.clone()))
            }
            CredentialBinding::Ssh { profile, .. } | CredentialBinding::Proxy { profile, .. } => {
                let session_id = match destination {
                    Destination::Terminal { connection, .. } => self
                        .tabs
                        .iter()
                        .find(|t| t.id == *connection)?
                        .session
                        .id
                        .clone(),
                    Destination::Desktop { .. } => return None,
                    Destination::Shared { source, .. } => match &self
                        .store
                        .shared_sources
                        .iter()
                        .find(|s| &s.id == source)?
                        .location
                    {
                        harbor_workspace::sessions::shared::SharedLocation::Sftp {
                            session_id,
                            ..
                        } => session_id.clone(),
                        _ => return None,
                    },
                };
                self.store
                    .sessions
                    .iter()
                    .find(|s| {
                        s.id == session_id
                            && harbor_workspace::ssh::auth_profile_binding(s) == *profile
                    })
                    .map(|_| Link::Session(session_id))
            }
        }
    }
    fn publish_vault_link(&mut self, id: String, binding: &CredentialBinding, link: Link) {
        let mut updated = false;
        match(link,binding){
   (Link::Session(session_id),CredentialBinding::Rdp{profile,..})=>if let Some(s)=self.store.sessions.iter_mut().find(|s|s.id==session_id&&harbor_workspace::desktop::rdp_options::auth_profile_binding(s)==*profile){s.credential_ref=Some(id);updated=true;},
   (Link::Session(session_id),CredentialBinding::Ssh{profile,role,host,port,user,..})=>if let Some(s)=self.store.sessions.iter_mut().find(|s|s.id==session_id&&harbor_workspace::ssh::auth_profile_binding(s)==*profile){match role{CredentialRole::Target=>s.credential_ref=Some(id),CredentialRole::Gateway=>{s.gateway_credential_refs.insert(harbor_workspace::ssh::gateway_reference_key(host,*port,user),id);}}updated=true;},
   (Link::Session(session_id),CredentialBinding::Proxy{profile,..})=>if let Some(s)=self.store.sessions.iter_mut().find(|s|s.id==session_id&&harbor_workspace::ssh::auth_profile_binding(s)==*profile){if let Some(proxy)=&mut s.ssh_options.proxy{proxy.credential_ref=Some(id);updated=true;}},
   (Link::Source(source_id),CredentialBinding::HttpsBasic{source_id:bound,url})=>if let Some(s)=self.store.shared_sources.iter_mut().find(|s|s.id==source_id&&&s.id==bound&&matches!(&s.location,harbor_workspace::sessions::shared::SharedLocation::Https(uri)if uri==url)){s.credential_ref=Some(id);updated=true;},_=>{}
  }
        if updated {
            self.persist();
        } else {
            self.message("Credential saved in the vault; its source/session changed, so no new reference was linked.");
        }
    }
    pub(super) fn poll_vault(&mut self, ctx: &Context) {
        for tab in &mut self.tabs {
            if let Some(request) = tab.vault_request.take() {
                if self.credentials.queue.len() < 32 {
                    self.credentials.queue.push_back(request)
                }
            }
        }
        if let Some(job) = &self.credentials.job {
            match job.rx.try_recv() {
                Ok(outcome) => {
                    self.credentials.finished = Some(outcome);
                    self.credentials.job = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.credentials.job = None;
                    self.credentials.error = "Vault worker stopped; unlock it again.".into();
                }
                Err(_) => {}
            }
        }
        if self.import_applying() {
            return;
        }
        if let Some(outcome) = self.credentials.finished.take() {
            self.credentials.vault = outcome.vault;
            match outcome.result {
                Ok(Effect::Stored { id, binding, link }) => {
                    self.publish_vault_link(id, &binding, link)
                }
                Ok(Effect::Opened) => {
                    self.credentials.editor = None;
                    self.credentials.error.clear();
                }
                Ok(Effect::Changed) => {
                    self.credentials.editor = None;
                    self.message("Vault master password changed.");
                }
                Ok(Effect::Deleted(id)) => {
                    for s in &mut self.store.sessions {
                        if s.credential_ref.as_ref() == Some(&id) {
                            s.credential_ref = None;
                        }
                        s.gateway_credential_refs.retain(|_, value| value != &id);
                        if let Some(proxy) = &mut s.ssh_options.proxy {
                            if proxy.credential_ref.as_ref() == Some(&id) {
                                proxy.credential_ref = None;
                            }
                        }
                        if s.gateway_credential_ref.as_ref() == Some(&id) {
                            s.gateway_credential_ref = None;
                        }
                    }
                    for s in &mut self.store.shared_sources {
                        if s.credential_ref.as_ref() == Some(&id) {
                            s.credential_ref = None;
                        }
                    }
                    self.forget_credential_ui(&id);
                    self.persist();
                    self.message("Credential deleted from the vault.");
                }
                Err(error) => {
                    self.credentials.error = error;
                    self.credentials.open = true;
                }
            }
            if self.credentials.lock_requested {
                self.lock_vault();
                self.credentials.lock_requested = false;
            }
        }
        if self.credentials.job.is_none() {
            for _ in 0..32 {
                let Some(request) = self.credentials.queue.pop_front() else {
                    break;
                };
                match request {
                    Request::Use {
                        destination,
                        credential,
                    } => {
                        if self.vault_unlocked() {
                            self.fill_vault_credential(destination, credential);
                        } else {
                            if self.credentials.uses.len() < 32
                                && !self.credentials.uses.iter().any(|(d, _)| d == &destination)
                            {
                                self.credentials.uses.push_back((destination, credential));
                            }
                            self.credentials.open = true;
                            if self.credentials.editor.is_none() {
                                self.credentials.editor =
                                    Some(MasterEditor::new(MasterMode::Unlock));
                            }
                        }
                    }
                    Request::Save {
                        destination,
                        credential,
                        values,
                    } => {
                        let Some(link) = self.vault_link(&destination, &credential.binding) else {
                            self.message("Save the session before remembering this credential; shared definitions cannot select local secrets.");
                            continue;
                        };
                        if !self.vault_unlocked() {
                            self.error_in(errors::Domain::Vault,"Credential was not saved because the vault is locked. Unlock it before choosing Save in vault.");
                            self.credentials.open = true;
                            continue;
                        }
                        let expected = match credential.binding {
                            CredentialBinding::Ssh { .. } | CredentialBinding::Rdp { .. } => 1,
                            CredentialBinding::HttpsBasic { .. }
                            | CredentialBinding::Proxy { .. } => 2,
                        };
                        if values.len() != expected {
                            self.error_in(
                                errors::Domain::Vault,
                                "Credential field count does not match its authentication method.",
                            );
                            continue;
                        }
                        let values = values.iter().map(|v| v.as_str()).collect::<Vec<_>>();
                        let packed = match serde_json::to_string(&values) {
                            Ok(s) => Secret::new(s),
                            Err(_) => {
                                self.error_in(
                                    errors::Domain::Vault,
                                    "Credential could not be encoded.",
                                );
                                continue;
                            }
                        };
                        let vault = self.credentials.vault.take().unwrap();
                        let id = format!("credential-{}", new_session_id());
                        self.start_vault(
                            Operation::Put {
                                vault,
                                id,
                                binding: credential.binding,
                                secret: packed,
                                link,
                            },
                            ctx,
                        );
                        break;
                    }
                }
            }
        }
        if self.vault_unlocked() {
            for (destination, credential) in std::mem::take(&mut self.credentials.uses) {
                self.fill_vault_credential(destination, credential);
            }
        }
        if self.credentials.close_after_job && !self.vault_busy() {
            self.credentials.close_after_job = false;
            self.request_window_session_close(ctx);
        }
    }
    fn fill_vault_credential(&mut self, destination: Destination, credential: ChallengeCredential) {
        let active = match &destination {
            Destination::Desktop {
                connection,
                challenge,
            } => self
                .tabs
                .iter()
                .find(|t| t.id == *connection)
                .is_some_and(|t| {
                    t.session.credential_ref == credential.reference
                        && t.desktop.as_ref().is_some_and(|d| {
                            d.vault_matches(&t.session, *challenge, &credential.binding)
                        })
                }),
            Destination::Terminal {
                connection,
                challenge,
            } => self
                .tabs
                .iter()
                .find(|t| t.id == *connection)
                .and_then(|t| t.auth.as_ref())
                .is_some_and(|a| {
                    a.id == *challenge
                        && a.credential.as_ref().is_some_and(|c| {
                            c.binding == credential.binding && c.reference == credential.reference
                        })
                }),
            Destination::Shared { .. } => {
                self.shared_vault_destination_current(&destination, &credential)
            }
        };
        if !active {
            return;
        }
        let result = (|| -> Result<Vec<Zeroizing<String>>, String> {
            let reference = credential
                .reference
                .as_ref()
                .ok_or("No credential is linked to this session/source.")?;
            let vault = self.credentials.vault.as_ref().ok_or("Vault is locked")?;
            let secret = vault.get_bound(reference, &credential.binding)?;
            let values: Vec<String> = serde_json::from_str(secret.expose())
                .map_err(|_| "Saved credential has an invalid format")?;
            Ok(values.into_iter().map(Zeroizing::new).collect())
        })();
        match result {
            Ok(mut values) => match destination {
                Destination::Desktop {
                    connection,
                    challenge,
                } => {
                    if values.len() == 1 {
                        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == connection) {
                            if let Some(desktop) = &mut tab.desktop {
                                if desktop.vault_matches(
                                    &tab.session,
                                    challenge,
                                    &credential.binding,
                                ) {
                                    desktop.fill_vault_password(values.remove(0));
                                }
                            }
                        }
                    }
                }
                Destination::Terminal {
                    connection,
                    challenge,
                } => {
                    if let Some(auth) = self
                        .tabs
                        .iter_mut()
                        .find(|t| t.id == connection)
                        .and_then(|t| t.auth.as_mut())
                        .filter(|a| {
                            a.id == challenge
                                && a.credential.as_ref().is_some_and(|c| {
                                    c.binding == credential.binding
                                        && c.reference == credential.reference
                                })
                                && a.values.len() == values.len()
                        })
                    {
                        auth.values = values;
                        auth.vault_filled = true;
                    }
                }
                destination @ Destination::Shared { .. } => {
                    self.fill_shared_vault_values(&destination, &credential, values);
                }
            },
            Err(error) => {
                self.credentials.error = error;
                self.credentials.open = true;
            }
        }
    }
    pub(super) fn defer_vault_close(&mut self, ctx: &Context) -> bool {
        if self.vault_busy() && ctx.input(|i| i.viewport().close_requested()) {
            self.credentials.close_after_job = true;
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.allow_remote_exit = false;
            self.allow_session_exit = false;
            self.message("The vault operation is finishing before the app closes.");
            true
        } else {
            false
        }
    }
    pub(super) fn credential_windows(&mut self, ctx: &Context) {
        if !self.credentials.open {
            return;
        }
        let mut close = false;
        let mut unlock = None;
        let mut delete = None;
        let mut change = false;
        let mut lock = false;
        let mut leave_master_editor = false;
        let size = fluent::dialog_size(ctx, 520., 110.);
        let response = fluent::modal(
            ctx,
            Id::new("credential-manager"),
            "Credential vault",
            size,
            |ui| {
                fluent::dialog_body(ui, size, |ui| {
                    if self.credentials.lock_requested {
                        ui.label("Locking after the current operation finishes…");
                    } else if self.credentials.job.is_some() {
                        ui.label("Working…");
                    } else if self.vault_unlocked() {
                        ui.label("Unlocked");
                    } else {
                        ui.label("Locked. Passwords are saved only when you choose Save in vault.");
                    }
                    if !self.credentials.error.is_empty() {
                        crate::ui::errors::show(
                            ui,
                            crate::ui::errors::Domain::Vault,
                            &self.credentials.error,
                        );
                    }
                    if self.credentials.job.is_none() {
                        if let Some(editor) = &mut self.credentials.editor {
                            let title = match editor.mode {
                                MasterMode::Create => "Create vault",
                                MasterMode::Unlock => "Unlock vault",
                                MasterMode::Change => "Change master password",
                            };
                            ui.strong(title);
                            let prompt = harbor_workspace::ssh::Prompt {
                                label: "Master password".into(),
                                echo: false,
                            };
                            let label = ui.label(&prompt.label);
                            remote_files::auth_field_scoped(
                                ui,
                                "vault-master",
                                0,
                                0,
                                0,
                                &prompt,
                                &mut editor.master,
                            )
                            .labelled_by(label.id);
                            if editor.mode != MasterMode::Unlock {
                                let label = ui.label("Repeat master password");
                                remote_files::auth_field_scoped(
                                    ui,
                                    "vault-master",
                                    0,
                                    0,
                                    1,
                                    &prompt,
                                    &mut editor.repeat,
                                )
                                .labelled_by(label.id);
                            }
                            if ui.button(title).clicked() {
                                if editor.master.is_empty() || editor.master.len() > 1024 {
                                    self.credentials.error =
                                        "Enter a master password of 1–1024 bytes.".into();
                                } else if editor.mode != MasterMode::Unlock
                                    && editor.master.as_str() != editor.repeat.as_str()
                                {
                                    self.credentials.error =
                                        "The master passwords do not match.".into();
                                } else {
                                    unlock = Some((
                                        editor.mode,
                                        Secret::new(std::mem::take(&mut *editor.master)),
                                    ));
                                }
                            }
                        } else if self.vault_unlocked() {
                            ui.horizontal_wrapped(|ui| {
                                lock = ui.button("Lock vault").clicked();
                                change = ui.button("Change master password…").clicked();
                            });
                            let entries = self
                                .credentials
                                .vault
                                .as_ref()
                                .and_then(|v| v.list().ok())
                                .unwrap_or_default();
                            ScrollArea::vertical().max_height(280.).show(ui, |ui| {
                                for entry in entries {
                                    ui.push_id(&entry.id, |ui| {
                                        ui.horizontal_wrapped(|ui| {
                                            let label = match &entry.binding {
                                                Some(CredentialBinding::Rdp {
                                                    host,
                                                    port,
                                                    user,
                                                    domain,
                                                    ..
                                                }) => {
                                                    format!("RDP {host}:{port} · {domain}\\{user}")
                                                }
                                                Some(CredentialBinding::Ssh {
                                                    host,
                                                    port,
                                                    user,
                                                    role,
                                                    ..
                                                }) => format!("{role:?} · {user}@{host}:{port}"),
                                                Some(CredentialBinding::HttpsBasic {
                                                    url, ..
                                                }) => url.clone(),
                                                Some(CredentialBinding::Proxy {
                                                    kind,
                                                    host,
                                                    port,
                                                    ..
                                                }) => format!("{kind:?} proxy · {host}:{port}"),
                                                None => "Unlinked credential".into(),
                                            };
                                            ui.label(label);
                                            if ui.button("Delete").clicked() {
                                                delete = Some(entry.id.clone());
                                            }
                                        });
                                    });
                                }
                            });
                        } else {
                            ui.horizontal_wrapped(|ui| {
                                if ui.button("Unlock vault…").clicked() {
                                    self.credentials.editor =
                                        Some(MasterEditor::new(MasterMode::Unlock));
                                }
                                if ui.button("Create vault…").clicked() {
                                    self.credentials.editor =
                                        Some(MasterEditor::new(MasterMode::Create));
                                }
                            });
                        }
                    } else if ui.button("Lock when finished").clicked() {
                        lock = true;
                    }
                });
                ui.separator();
                if self.credentials.editor.is_some() && self.credentials.job.is_none() {
                    let [back, done] = fluent::dialog_actions(ui, ["Back to vault", "Close"], None);
                    leave_master_editor = back.clicked();
                    close = done.clicked();
                } else {
                    close = fluent::dialog_actions(ui, ["Close"], None)[0].clicked();
                }
            },
        );
        close |= response.should_close();
        if leave_master_editor {
            self.credentials.editor = None;
            self.credentials.error.clear();
        }
        if close {
            self.credentials.open = false;
            self.credentials.editor = None;
            self.credentials.uses.clear();
        }
        if lock {
            self.lock_vault();
        }
        if change {
            self.credentials.editor = Some(MasterEditor::new(MasterMode::Change));
        }
        if let Some(id) = delete {
            self.delete_vault_credential(id, ctx);
        }
        if let Some((mode, master)) = unlock {
            match mode {
                MasterMode::Create => self.create_vault(master, ctx),
                MasterMode::Unlock => self.unlock_vault(master, ctx),
                MasterMode::Change => {
                    if let Some(vault) = self.credentials.vault.take() {
                        self.start_vault(Operation::Change { vault, master }, ctx);
                    }
                }
            }
        }
    }
}
