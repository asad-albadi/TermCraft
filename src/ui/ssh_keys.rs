use super::*;
use harbor_workspace::credentials::{
    Secret,
    agent::{AgentEndpoint, AgentManager, AgentTarget, Listing},
    keys::{KeyAlgorithm, KeyInfo, KeyManager, KeyPairPaths, KeySettings},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use zeroize::{Zeroize, Zeroizing};
pub(super) struct State {
    manager: Option<AgentManager>,
    owned: Option<AgentEndpoint>,
    job: Option<Job>,
    pub(super) open: bool,
    target: AgentTarget,
    listing: Option<(AgentTarget, Listing)>,
    selected: Option<PathBuf>,
    info: Option<KeyInfo>,
    generated: Option<KeyPairPaths>,
    parent: String,
    name: String,
    algorithm: KeyAlgorithm,
    comment: String,
    passphrase: Zeroizing<String>,
    repeat: Zeroizing<String>,
    expires: bool,
    lifetime: u32,
    error: String,
    status: String,
}
impl Default for State {
    fn default() -> Self {
        Self {
            manager: Some(AgentManager::from_system()),
            owned: None,
            job: None,
            open: false,
            target: AgentTarget::System,
            listing: None,
            selected: None,
            info: None,
            generated: None,
            parent: home_dir().to_string_lossy().into(),
            name: "termcraft-key".into(),
            algorithm: KeyAlgorithm::Ed25519,
            comment: String::new(),
            passphrase: Zeroizing::new(String::new()),
            repeat: Zeroizing::new(String::new()),
            expires: false,
            lifetime: 3600,
            error: String::new(),
            status: String::new(),
        }
    }
}
struct Job {
    rx: Receiver<Outcome>,
    cancel: Arc<AtomicBool>,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}
struct Outcome {
    manager: AgentManager,
    result: Result<Data, String>,
}
enum Data {
    Started,
    Stopped,
    List(AgentTarget, Listing),
    Refresh,
    Pair(KeyPairPaths, KeyInfo),
    Inspected(PathBuf, KeyInfo),
    Picked(PathBuf, Result<KeyInfo, String>),
    Parent(PathBuf),
    Nothing,
}
enum Operation {
    Start,
    Stop,
    List(AgentTarget),
    Add {
        target: AgentTarget,
        path: PathBuf,
        passphrase: Option<Secret>,
        lifetime: Option<Duration>,
    },
    Remove {
        target: AgentTarget,
        public: String,
    },
    Generate {
        destination: PathBuf,
        settings: KeySettings,
    },
    Import {
        source: PathBuf,
        destination: PathBuf,
        passphrase: Option<Secret>,
    },
    Inspect {
        path: PathBuf,
        passphrase: Option<Secret>,
    },
    PickKey,
    PickParent,
}
impl Workspace {
    pub(super) fn owned_agent_endpoint(&self) -> Option<AgentEndpoint> {
        self.ssh_keys.owned.clone()
    }
    pub(super) fn session_agent_endpoint(&self, session: &Session) -> Option<AgentEndpoint> {
        if session.agent_mode == AgentMode::Owned {
            self.owned_agent_endpoint()
        } else {
            None
        }
    }
    pub(super) fn ssh_keys_busy(&self) -> bool {
        self.ssh_keys.job.is_some()
    }
    pub(super) fn generated_key_paths(&self) -> Option<&KeyPairPaths> {
        self.ssh_keys.generated.as_ref()
    }
    pub(super) fn start_owned_agent(&mut self, ctx: &Context) {
        self.key_operation(Operation::Start, ctx);
    }
    pub(super) fn add_agent_identity(
        &mut self,
        target: AgentTarget,
        path: PathBuf,
        passphrase: Option<Secret>,
        lifetime: Option<Duration>,
        ctx: &Context,
    ) {
        self.ssh_keys.target = target;
        self.key_operation(
            Operation::Add {
                target,
                path,
                passphrase,
                lifetime,
            },
            ctx,
        );
    }
    pub(super) fn generate_key_bundle(
        &mut self,
        destination: PathBuf,
        settings: KeySettings,
        ctx: &Context,
    ) {
        self.key_operation(
            Operation::Generate {
                destination,
                settings,
            },
            ctx,
        );
    }
    fn key_operation(&mut self, operation: Operation, ctx: &Context) {
        if self.ssh_keys.job.is_some() {
            self.ssh_keys.error = "A key/agent operation is already running.".into();
            return;
        }
        let mut manager = self
            .ssh_keys
            .manager
            .take()
            .unwrap_or_else(AgentManager::from_system);
        let (tx, rx) = mpsc::sync_channel(1);
        let cancel = Arc::new(AtomicBool::new(false));
        let canceled = cancel.clone();
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new()
            .name("ssh-keys-and-agent".into())
            .spawn(move || {
                let result = (|| -> Result<Data, String> {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|_| "Could not start key/agent worker")?;
                    match operation {
                        Operation::Start => {
                            manager.start_owned()?;
                            Ok(Data::Started)
                        }
                        Operation::Stop => {
                            manager.stop_owned()?;
                            Ok(Data::Stopped)
                        }
                        Operation::List(target) => runtime
                            .block_on(manager.list(target))
                            .map(|listing| Data::List(target, listing)),
                        Operation::Add {
                            target,
                            path,
                            passphrase,
                            lifetime,
                        } => {
                            runtime.block_on(manager.add(target, &path, passphrase, lifetime))?;
                            Ok(Data::Refresh)
                        }
                        Operation::Remove { target, public } => {
                            runtime.block_on(manager.remove(target, &public))?;
                            Ok(Data::Refresh)
                        }
                        Operation::Generate {
                            destination,
                            settings,
                        } => {
                            let pair =
                                KeyManager::generate_cancelable(&destination, settings, &canceled)?;
                            let info = KeyManager::inspect(&pair.private, None)?;
                            Ok(Data::Pair(pair, info))
                        }
                        Operation::Import {
                            source,
                            destination,
                            passphrase,
                        } => {
                            let pair =
                                KeyManager::import(&source, &destination, passphrase, &canceled)?;
                            let info = KeyManager::inspect(&pair.private, None)?;
                            Ok(Data::Pair(pair, info))
                        }
                        Operation::Inspect { path, passphrase } => {
                            let info = KeyManager::inspect(&path, passphrase.as_ref())?;
                            Ok(Data::Inspected(path, info))
                        }
                        Operation::PickKey => match rfd::FileDialog::new()
                            .set_title("Inspect or import SSH key")
                            .pick_file()
                        {
                            Some(path) => {
                                let info = KeyManager::inspect(&path, None);
                                Ok(Data::Picked(path, info))
                            }
                            None => Ok(Data::Nothing),
                        },
                        Operation::PickParent => Ok(rfd::FileDialog::new()
                            .set_title("Choose key-bundle parent folder")
                            .pick_folder()
                            .map(Data::Parent)
                            .unwrap_or(Data::Nothing)),
                    }
                })();
                let _ = tx.send(Outcome { manager, result });
                ctx.request_repaint();
            });
        match spawned {
            Ok(_) => {
                self.ssh_keys.job = Some(Job { rx, cancel });
                self.ssh_keys.error.clear();
                self.ssh_keys.status = "Working…".into();
            }
            Err(_) => {
                self.ssh_keys.manager = Some(AgentManager::from_system());
                self.ssh_keys.owned = None;
                self.ssh_keys.error = "Could not start key/agent operation.".into();
            }
        }
    }
    pub(super) fn poll_ssh_keys(&mut self, ctx: &Context) {
        let outcome = self
            .ssh_keys
            .job
            .as_ref()
            .and_then(|j| match j.rx.try_recv() {
                Ok(result) => Some(Ok(result)),
                Err(mpsc::TryRecvError::Disconnected) => Some(Err(())),
                Err(_) => None,
            });
        let Some(outcome) = outcome else { return };
        self.ssh_keys.job = None;
        let outcome = match outcome {
            Ok(o) => o,
            Err(()) => {
                self.ssh_keys.manager = Some(AgentManager::from_system());
                self.ssh_keys.owned = None;
                self.ssh_keys.error = "Key/agent worker stopped unexpectedly.".into();
                return;
            }
        };
        self.ssh_keys.owned = outcome.manager.owned_endpoint();
        self.ssh_keys.manager = Some(outcome.manager);
        let mut refresh = false;
        match outcome.result {
            Ok(Data::Started) => {
                self.ssh_keys.target = AgentTarget::Owned;
                self.ssh_keys.status = "TermCraft-owned agent started.".into();
                refresh = true
            }
            Ok(Data::Stopped) => {
                self.ssh_keys.listing = None;
                self.ssh_keys.status = "Owned agent stopped. System agents are unchanged.".into();
            }
            Ok(Data::List(target, listing)) => {
                self.ssh_keys.status =
                    format!("{target:?}: {} identities loaded", listing.identities.len());
                self.ssh_keys.listing = Some((target, listing));
            }
            Ok(Data::Refresh) => {
                self.ssh_keys.listing = None;
                self.ssh_keys.status = "Agent update accepted.".into();
                refresh = true
            }
            Ok(Data::Pair(pair, info)) => {
                self.message(format!("Key bundle created: {}", pair.private.display()));
                self.ssh_keys.selected = Some(pair.private.clone());
                self.ssh_keys.generated = Some(pair);
                self.ssh_keys.info = Some(info);
                self.ssh_keys.status =
                    "Key bundle created without replacing existing files.".into();
            }
            Ok(Data::Picked(path, info)) => {
                self.ssh_keys.selected = Some(path);
                self.ssh_keys.info = None;
                match info {
                    Ok(info) => self.ssh_keys.info = Some(info),
                    Err(error) => self.ssh_keys.error = error,
                }
                self.ssh_keys.status = "Key file selected; enter its passphrase if needed.".into();
            }
            Ok(Data::Inspected(path, info)) => {
                self.ssh_keys.selected = Some(path);
                self.ssh_keys.info = Some(info);
                self.ssh_keys.status = "Key inspected; source file is unchanged.".into();
            }
            Ok(Data::Parent(path)) => match path.to_str() {
                Some(value) => self.ssh_keys.parent = value.into(),
                None => self.ssh_keys.error = "Choose a folder with a valid Unicode path.".into(),
            },
            Ok(Data::Nothing) => {
                self.ssh_keys.status = "Selection canceled.".into();
            }
            Err(error) => {
                self.ssh_keys.error = error;
                self.ssh_keys.status = "Operation did not complete.".into();
            }
        }
        if refresh {
            self.key_operation(Operation::List(self.ssh_keys.target), ctx);
        }
    }
    fn key_destination(&self) -> Result<PathBuf, String> {
        let parent = PathBuf::from(&self.ssh_keys.parent);
        if !parent.is_absolute() {
            return Err("Choose an absolute parent-folder path.".into());
        }
        let name = PathBuf::from(&self.ssh_keys.name);
        if name.components().count() != 1
            || !matches!(
                name.components().next(),
                Some(std::path::Component::Normal(_))
            )
            || self.ssh_keys.name.chars().any(char::is_control)
        {
            return Err("Enter a single new bundle folder name.".into());
        }
        Ok(parent.join(name))
    }
    pub(super) fn ssh_key_windows(&mut self, ctx: &Context) {
        if !self.ssh_keys.open {
            return;
        }
        let mut operation = None;
        let mut close = false;
        let mut use_key = false;
        let busy = self.ssh_keys_busy();
        let size = fluent::dialog_size(ctx, 640., 110.);
        let response = fluent::modal(
            ctx,
            Id::new("ssh-key-manager"),
            "SSH keys and agents",
            size,
            |ui| {
                fluent::dialog_body(ui, size, |ui| {
                    if !self.ssh_keys.error.is_empty() {
                        crate::ui::errors::show(
                            ui,
                            crate::ui::errors::Domain::Keys,
                            &self.ssh_keys.error,
                        );
                    }
                    ui.label(&self.ssh_keys.status);
                    if busy {
                        ui.label("Running in the background. Canceling key generation stops publication after the current computation finishes.");
                        if ui.button("Cancel operation").clicked() {
                            if let Some(job) = &self.ssh_keys.job {
                                job.cancel.store(true, Ordering::Release);
                            }
                            self.ssh_keys.status="Cancellation requested; an already accepted agent update may still complete.".into();
                        }
                    }
                    ui.add_enabled_ui(!busy,|ui|{

     ui.horizontal_wrapped(|ui|{ui.label("Agent");ui.selectable_value(&mut self.ssh_keys.target,AgentTarget::System,"System");ui.selectable_value(&mut self.ssh_keys.target,AgentTarget::Owned,"TermCraft-owned");if cfg!(windows){ui.selectable_value(&mut self.ssh_keys.target,AgentTarget::Pageant,"Pageant");}if ui.button("Refresh identities").clicked(){operation=Some(Operation::List(self.ssh_keys.target));}});
     ui.horizontal_wrapped(|ui|{if ui.add_enabled(self.ssh_keys.owned.is_none(),Button::new("Start owned agent")).clicked(){operation=Some(Operation::Start);}if ui.add_enabled(self.ssh_keys.owned.is_some(),Button::new("Stop owned agent")).clicked(){operation=Some(Operation::Stop);}ui.menu_button("Agent help",|ui|{ui.set_max_width(320.);ui.label("System agents are never stopped by TermCraft. Owned/Pageant session selection is available with embedded SSH.");});});
     if let Some((listed_target,listing))=&self.ssh_keys.listing{if *listed_target==self.ssh_keys.target{if listing.omitted>0{ui.label(format!("{} additional identities omitted; showing the first 256.",listing.omitted));}for identity in &listing.identities{ui.push_id(&identity.fingerprint,|ui|{ui.horizontal_wrapped(|ui|{ui.label(format!("{} · {} {}",identity.algorithm,identity.fingerprint,identity.comment));if ui.button("Remove from agent").clicked(){operation=Some(Operation::Remove{target:*listed_target,public:identity.public_key.clone()});}});});}}}
     ui.separator();ui.strong("Inspect / import a key");if ui.button("Choose key file…").clicked(){operation=Some(Operation::PickKey);}if let Some(path)=&self.ssh_keys.selected{ui.label(path.display().to_string());}
     if let Some(info)=&self.ssh_keys.info{ui.label(format!("{} · {}",info.algorithm,info.fingerprint));ui.label(if info.encrypted{"Private key is passphrase-protected."}else if info.private{"Unencrypted private key."}else{"Public key only."});if ui.button("Copy public key").clicked(){ui.ctx().copy_text(info.public_key.clone());}if ui.add_enabled(info.private,Button::new("Use private key in session…")).clicked(){use_key=true;}}
     let prompt=harbor_workspace::ssh::Prompt{label:"Key passphrase (optional)".into(),echo:false};let label=ui.label(&prompt.label);remote_files::auth_field_scoped(ui,"key-manager",0,0,0,&prompt,&mut self.ssh_keys.passphrase).labelled_by(label.id);
     ui.horizontal_wrapped(|ui|{ui.checkbox(&mut self.ssh_keys.expires,"Expire added identity");if self.ssh_keys.expires{ui.add(DragValue::new(&mut self.ssh_keys.lifetime).range(1..=u32::MAX).suffix(" seconds"));}});
     if let Some(path)=self.ssh_keys.selected.clone(){ui.horizontal_wrapped(|ui|{if ui.button("Unlock / inspect selected file").clicked(){let pass=if self.ssh_keys.passphrase.is_empty(){None}else{Some(Secret::new(std::mem::take(&mut *self.ssh_keys.passphrase)))};operation=Some(Operation::Inspect{path:path.clone(),passphrase:pass});}if ui.add_enabled(self.ssh_keys.info.as_ref().is_none_or(|i|i.private),Button::new("Add selected key to agent")).clicked(){let pass=if self.ssh_keys.passphrase.is_empty(){None}else{Some(Secret::new(std::mem::take(&mut *self.ssh_keys.passphrase)))};operation=Some(Operation::Add{target:self.ssh_keys.target,path,passphrase:pass,lifetime:if self.ssh_keys.expires{Some(Duration::from_secs(self.ssh_keys.lifetime.into()))}else{None}});}});}
     ui.separator();ui.strong("Create a new key bundle");ui.label("Parent folder");ui.horizontal_wrapped(|ui|{ui.text_edit_singleline(&mut self.ssh_keys.parent);if ui.button("Choose folder…").clicked(){operation=Some(Operation::PickParent);}});ui.label("New bundle folder name");ui.text_edit_singleline(&mut self.ssh_keys.name);
     ui.label("Algorithm");ui.horizontal_wrapped(|ui|for(algorithm,label)in[(KeyAlgorithm::Ed25519,"Ed25519"),(KeyAlgorithm::EcdsaP256,"ECDSA P-256"),(KeyAlgorithm::EcdsaP384,"ECDSA P-384"),(KeyAlgorithm::EcdsaP521,"ECDSA P-521"),(KeyAlgorithm::Rsa3072,"RSA 3072"),(KeyAlgorithm::Rsa4096,"RSA 4096")]{ui.selectable_value(&mut self.ssh_keys.algorithm,algorithm,label);});ui.label("Comment");ui.text_edit_singleline(&mut self.ssh_keys.comment);
     ui.menu_button("Passphrase help",|ui|{ui.set_max_width(320.);ui.label("The passphrase field above protects a generated key. Leave it blank for an unencrypted key.");});let label=ui.label("Repeat passphrase for generation");remote_files::auth_field_scoped(ui,"key-manager",0,0,1,&prompt,&mut self.ssh_keys.repeat).labelled_by(label.id);
     ui.horizontal_wrapped(|ui|{if primary(ui,"Generate key bundle").clicked(){match self.key_destination(){Ok(destination)=>{if self.ssh_keys.passphrase.as_str()!=self.ssh_keys.repeat.as_str(){self.ssh_keys.error="Key passphrases do not match.".into();}else{let passphrase=if self.ssh_keys.passphrase.is_empty(){None}else{Some(Secret::new(std::mem::take(&mut *self.ssh_keys.passphrase)))};self.ssh_keys.repeat.zeroize();operation=Some(Operation::Generate{destination,settings:KeySettings{algorithm:self.ssh_keys.algorithm,comment:self.ssh_keys.comment.clone(),passphrase}});}},Err(error)=>self.ssh_keys.error=error}}if ui.button("Import selected key into new bundle").clicked(){if let Some(source)=self.ssh_keys.selected.clone(){match self.key_destination(){Ok(destination)=>{let passphrase=if self.ssh_keys.passphrase.is_empty(){None}else{Some(Secret::new(std::mem::take(&mut *self.ssh_keys.passphrase)))};operation=Some(Operation::Import{source,destination,passphrase});},Err(error)=>self.ssh_keys.error=error}}else{self.ssh_keys.error="Choose a key file first.".into();}}});
     ui.menu_button("Bundle details",|ui|{ui.set_max_width(320.);ui.label("A bundle contains private_key and public_key.pub. Existing destinations are never replaced.");});
    });
                });
                ui.separator();
                close = fluent::dialog_actions(ui, ["Close"], None)[0].clicked();
            },
        );
        close |= response.should_close();
        if close {
            self.ssh_keys.open = false;
            self.ssh_keys.passphrase.zeroize();
            self.ssh_keys.repeat.zeroize();
        }
        if use_key {
            if let Some(path) = self.ssh_keys.selected.clone() {
                if let Some(path) = path.to_str() {
                    if self.session_form.is_none() {
                        self.new_session("SSH");
                    }
                    if let Some(session) = &mut self.session_form {
                        session.key = path.into();
                    }
                    self.ssh_keys.open = false;
                    self.ssh_keys.passphrase.zeroize();
                    self.ssh_keys.repeat.zeroize();
                } else {
                    self.ssh_keys.error = "Session key paths require valid Unicode.".into();
                }
            }
        }
        if let Some(operation) = operation {
            self.key_operation(operation, ctx);
        }
    }
}
