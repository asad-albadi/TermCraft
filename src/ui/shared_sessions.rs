use super::*;
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::ui::terminal_review_tests::owned_workspace;
    use harbor_workspace::sessions::shared::{SharedFormat, SharedLocation, SharedSourceConfig};
    fn wait(ctx: &Context, w: &mut Workspace) {
        let end = Instant::now() + Duration::from_secs(4);
        loop {
            w.poll_shared_sources(ctx);
            if !w.shared_sources_busy() {
                break;
            }
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn readonly_source_cache_survives_restart_and_never_overwrites_local_names() {
        let (ctx, root, mut w) = owned_workspace();
        let local = Session {
            name: "Office".into(),
            host: "local.test".into(),
            ..Default::default()
        };
        w.store.sessions.push(local.clone());
        let path = root.path().join("team.json");
        let mut upstream = Store::default();
        upstream.sessions.push(Session {
            name: "Office".into(),
            host: "shared.test".into(),
            ..Default::default()
        });
        upstream.save(&path).unwrap();
        let config = SharedSourceConfig::new(
            "Team",
            SharedLocation::File(path.clone()),
            SharedFormat::Workspace,
        )
        .unwrap();
        let id = config.id.clone();
        w.add_shared_source(config, &ctx);
        wait(&ctx, &mut w);
        assert_eq!(w.store.sessions[0].id, local.id);
        let entry = w.shared_snapshot(&id).unwrap().entries[0].id.clone();
        assert!(w.shared_cache_path(&id).is_file());
        assert!(
            Store::load(&w.store_path).unwrap().shared_sources[0]
                .cached
                .is_none()
        );
        w.copy_shared_to_local(&id, &entry);
        assert_eq!(w.store.sessions.len(), 2);
        assert_ne!(w.store.sessions[1].id, entry);
        assert_eq!(w.store.sessions[0].host, "local.test");
        fs::remove_file(path).unwrap();
        w.refresh_shared_source(&id, &ctx);
        wait(&ctx, &mut w);
        assert!(w.shared_snapshot(&id).unwrap().stale);
        let second_ctx = Context::default();
        let mut second = Workspace::new_context(&second_ctx, None, root.path().to_owned());
        wait(&second_ctx, &mut second);
        assert!(second.shared_snapshot(&id).unwrap().stale);
        assert_eq!(second.shared_snapshot(&id).unwrap().entries[0].id, entry);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(second.shared_cache_path(&id))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o077,
                0
            );
        }
    }
    #[test]
    fn source_definition_rows_are_painted_with_readonly_and_stale_text() {
        let (ctx, root, mut w) = owned_workspace();
        let path = root.path().join("source.json");
        let mut store = Store::default();
        store.sessions.push(Session {
            name: "Shared office".into(),
            host: "source.test".into(),
            ..Default::default()
        });
        store.save(&path).unwrap();
        let cfg = SharedSourceConfig::new(
            "Team",
            SharedLocation::File(path.clone()),
            SharedFormat::Workspace,
        )
        .unwrap();
        let id = cfg.id.clone();
        w.add_shared_source(cfg, &ctx);
        wait(&ctx, &mut w);
        fs::remove_file(path).unwrap();
        w.refresh_shared_source(&id, &ctx);
        wait(&ctx, &mut w);
        fn text(shape: &egui::epaint::Shape, out: &mut String) {
            match shape {
                egui::epaint::Shape::Text(t) => out.push_str(t.galley.text()),
                egui::epaint::Shape::Vec(v) => {
                    for s in v {
                        text(s, out)
                    }
                }
                _ => {}
            }
        }
        let mut labels = String::new();
        for _ in 0..2 {
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800., 900.))),
                    ..Default::default()
                },
                |ui| w.shared_session_tree(ui),
            );
            output.textures_delta.clear();
            labels.clear();
            for shape in output.shapes {
                text(&shape.shape, &mut labels);
            }
        }
        assert!(
            labels.contains("Read-only") && labels.contains("stale"),
            "{labels}"
        );
    }
    struct PromptReader {
        auth: std::sync::Arc<dyn harbor_workspace::shared_network::SourceAuth>,
        owner: u64,
        received: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }
    impl harbor_workspace::sessions::shared::SharedReader for PromptReader {
        fn fetch(
            &self,
            cfg: &SharedSourceConfig,
            cancel: &std::sync::atomic::AtomicBool,
        ) -> Result<Vec<u8>, String> {
            let answer = self.auth.answer(
                harbor_workspace::shared_network::SourceChallenge {
                    source_id: cfg.id.clone(),
                    owner: self.owner,
                    answer_id: 1,
                    endpoint: None,
                    title: "Owned source sign-in".into(),
                    prompts: vec![harbor_workspace::ssh::Prompt {
                        label: "Password".into(),
                        echo: false,
                    }],
                    host_key: false,
                    credential_ref: None,
                    credential: None,
                },
                cancel,
            )?;
            assert_eq!(answer.values[0].as_str(), "owned secret");
            self.received
                .store(true, std::sync::atomic::Ordering::Release);
            harbor_workspace::sessions::shared::SharedReader::fetch(
                &harbor_workspace::sessions::shared::FileReader,
                cfg,
                cancel,
            )
        }
    }
    #[test]
    fn cancelled_source_prompt_cannot_send_values_to_a_later_refresh() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        let (ctx, root, mut w) = owned_workspace();
        let path = root.path().join("source.json");
        let mut data = Store::default();
        data.sessions.push(Session {
            name: "Shared".into(),
            host: "source.test".into(),
            ..Default::default()
        });
        data.save(&path).unwrap();
        let cfg = SharedSourceConfig::new(
            "Private",
            SharedLocation::File(path),
            SharedFormat::Workspace,
        )
        .unwrap();
        let id = cfg.id.clone();
        w.add_shared_source(cfg, &ctx);
        wait(&ctx, &mut w);
        let received = Arc::new(AtomicBool::new(false));
        let capture = received.clone();
        let old = (1u64 << 63) + 401;
        w.refresh_shared_source_with(&id, &ctx, move |auth| {
            Arc::new(PromptReader {
                auth,
                owner: old,
                received: capture,
            })
        });
        let end = Instant::now() + Duration::from_secs(3);
        while !w.has_shared_prompt() {
            w.poll_shared_sources(&ctx);
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(5));
        }
        w.cancel_shared_source(&id);
        assert!(!w.submit_shared_auth(&id, old, vec!["owned secret".to_owned().into()], None));
        assert!(!received.load(Ordering::Acquire));
        let capture = received.clone();
        w.refresh_shared_source_with(&id, &ctx, move |auth| {
            Arc::new(PromptReader {
                auth,
                owner: old + 1,
                received: capture,
            })
        });
        let end = Instant::now() + Duration::from_secs(3);
        while !w.has_shared_prompt() {
            w.poll_shared_sources(&ctx);
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!w.submit_shared_auth(&id, old, vec!["owned secret".to_owned().into()], None));
        assert!(w.submit_shared_auth(&id, old + 1, vec!["owned secret".to_owned().into()], None));
        wait(&ctx, &mut w);
        assert!(received.load(Ordering::Acquire));
        assert!(
            !fs::read_to_string(w.shared_cache_path(&id))
                .unwrap()
                .contains("owned secret")
        );
    }
    struct VaultPromptReader {
        auth: Arc<dyn SourceAuth>,
        owner: u64,
    }
    impl SharedReader for VaultPromptReader {
        fn fetch(&self, cfg: &SharedSourceConfig, cancel: &AtomicBool) -> Result<Vec<u8>, String> {
            let SharedLocation::Https(url) = &cfg.location else {
                panic!()
            };
            let answer = self.auth.answer(
                SourceChallenge {
                    source_id: cfg.id.clone(),
                    owner: self.owner,
                    answer_id: 1,
                    endpoint: Some(url.clone()),
                    title: "Owned Basic challenge".into(),
                    prompts: vec![
                        harbor_workspace::ssh::Prompt {
                            label: "Username".into(),
                            echo: true,
                        },
                        harbor_workspace::ssh::Prompt {
                            label: "Password".into(),
                            echo: false,
                        },
                    ],
                    host_key: false,
                    credential_ref: cfg.credential_ref.clone(),
                    credential: Some(harbor_workspace::ssh::ChallengeCredential {
                        reference: cfg.credential_ref.clone(),
                        binding: harbor_workspace::credentials::CredentialBinding::HttpsBasic {
                            source_id: cfg.id.clone(),
                            url: url.clone(),
                        },
                    }),
                },
                cancel,
            )?;
            assert_eq!(answer.values[1].as_str(), "owned source secret");
            Err("Owned fixture stops after sign-in".into())
        }
    }
    #[test]
    fn saved_source_credential_never_fills_a_canceled_refresh_generation() {
        use harbor_workspace::credentials::{CredentialBinding, Secret, vault::Vault};
        let (ctx, root, mut w) = owned_workspace();
        let uri = "https://owned.example.test/sessions";
        let mut cfg = SharedSourceConfig::new(
            "Team",
            SharedLocation::Https(uri.into()),
            SharedFormat::Workspace,
        )
        .unwrap();
        cfg.credential_ref = Some("owned-source-ref".into());
        let id = cfg.id.clone();
        let binding = CredentialBinding::HttpsBasic {
            source_id: id.clone(),
            url: uri.into(),
        };
        let mut vault = Vault::create(
            &root.path().join("credentials.vault"),
            Secret::new("owned master password"),
        )
        .unwrap();
        vault
            .put_bound(
                "owned-source-ref".into(),
                Secret::new("[\"owned user\",\"owned source secret\"]"),
                binding,
            )
            .unwrap();
        drop(vault);
        w.unlock_vault(Secret::new("owned master password"), &ctx);
        let end = Instant::now() + Duration::from_secs(20);
        while !w.vault_unlocked() {
            w.poll_vault(&ctx);
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(5));
        }
        w.add_shared_source(cfg, &ctx);
        w.refresh_shared_source_with(&id, &ctx, |auth| {
            Arc::new(VaultPromptReader { auth, owner: 60001 })
        });
        let end = Instant::now() + Duration::from_secs(3);
        while !w.has_shared_prompt() {
            w.poll_shared_sources(&ctx);
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(5));
        }
        // A nested vault dismissal must return to the unanswered challenge.
        let draw = |w: &mut Workspace, events| {
            super::super::gui_tests::shell_frame(&ctx, vec2(950., 660.), 1., events, |ui| {
                w.shared_sessions_windows(ui.ctx());
                w.credential_windows(ui.ctx());
            })
        };
        draw(&mut w, vec![]);
        draw(&mut w, vec![]);
        w.credentials.open = true;
        draw(&mut w, vec![]);
        draw(&mut w, vec![]);
        draw(
            &mut w,
            vec![Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
        assert!(!w.credentials.open);
        assert!(w.has_shared_prompt());
        assert_eq!(
            w.shared_sources
                .forms
                .front()
                .unwrap()
                .request
                .challenge
                .owner,
            60001
        );
        let form = w.shared_sources.forms.front().unwrap();
        let old = super::super::credentials::Destination::Shared {
            source: id.clone(),
            generation: form.generation,
            owner: 60001,
            answer: 1,
        };
        let credential = form.request.challenge.credential.clone().unwrap();
        w.cancel_shared_source(&id);
        w.refresh_shared_source_with(&id, &ctx, |auth| {
            Arc::new(VaultPromptReader { auth, owner: 60002 })
        });
        let end = Instant::now() + Duration::from_secs(3);
        while !w.has_shared_prompt() {
            w.poll_shared_sources(&ctx);
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(5));
        }
        w.request_vault(
            super::super::credentials::Request::Use {
                destination: old,
                credential: credential.clone(),
            },
            &ctx,
        );
        assert!(
            w.shared_sources
                .forms
                .front()
                .unwrap()
                .values
                .iter()
                .all(|v| v.is_empty())
        );
        let form = w.shared_sources.forms.front().unwrap();
        let current = super::super::credentials::Destination::Shared {
            source: id.clone(),
            generation: form.generation,
            owner: 60002,
            answer: 1,
        };
        w.request_vault(
            super::super::credentials::Request::Use {
                destination: current,
                credential,
            },
            &ctx,
        );
        assert_eq!(
            w.shared_sources.forms.front().unwrap().values[1].as_str(),
            "owned source secret"
        );
        w.lock_vault();
        assert!(
            w.shared_sources
                .forms
                .front()
                .unwrap()
                .values
                .iter()
                .all(|v| v.is_empty())
        );
        w.cancel_shared_source(&id);
    }
}
use harbor_workspace::{
    sessions::shared::{
        SharedFormat, SharedLocation, SharedReader, SharedSnapshot, SharedSource,
        SharedSourceConfig,
    },
    shared_network::{NetworkReader, SourceAuth, SourceChallenge},
    ssh::{ChallengeAnswer, TrustDecision},
};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use zeroize::Zeroizing;
type Factory = Box<dyn FnOnce(Arc<dyn SourceAuth>) -> Arc<dyn SharedReader> + Send>;
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Load,
    Fetch,
    Save,
}
enum Outcome {
    Loaded(Option<SharedSnapshot>),
    Fetched {
        snapshot: Option<SharedSnapshot>,
        error: Option<String>,
    },
    Saved,
}
struct PromptRequest {
    challenge: SourceChallenge,
    reply: std::sync::mpsc::SyncSender<ChallengeAnswer>,
}
struct Bridge {
    source: String,
    tx: std::sync::mpsc::SyncSender<PromptRequest>,
    ctx: Context,
}
impl SourceAuth for Bridge {
    fn answer(&self, c: SourceChallenge, cancel: &AtomicBool) -> Result<ChallengeAnswer, String> {
        if c.source_id != self.source
            || c.prompts.len() > 32
            || c.title.len() > 4096
            || c.prompts.iter().any(|p| p.label.len() > 1024)
        {
            return Err("Invalid source sign-in request".into());
        }
        let id = c.answer_id;
        let (tx, rx) = mpsc::sync_channel(1);
        self.tx
            .try_send(PromptRequest {
                challenge: c,
                reply: tx,
            })
            .map_err(|_| "Source sign-in queue closed or busy")?;
        self.ctx.request_repaint();
        loop {
            if cancel.load(Ordering::Acquire) {
                return Err("Source sign-in cancelled".into());
            }
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(answer) => {
                    if cancel.load(Ordering::Acquire) || answer.id != id {
                        return Err("Stale source sign-in answer".into());
                    }
                    return Ok(answer);
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("Source sign-in closed".into());
                }
                Err(_) => {}
            }
        }
    }
}
struct Job {
    kind: Kind,
    generation: u64,
    cancel: Arc<AtomicBool>,
    rx: Receiver<Result<Outcome, String>>,
    prompts: Receiver<PromptRequest>,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}
struct Slot(Arc<AtomicUsize>, Context);
impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
        self.1.request_repaint();
    }
}
struct Runtime {
    binding: String,
    snapshot: Option<SharedSnapshot>,
    job: Option<Job>,
    need_cache: bool,
    need_fetch: bool,
    factory: Option<Factory>,
    status: String,
    error: String,
    filter: String,
    revision: String,
    rows: Vec<usize>,
}
impl Runtime {
    fn new(binding: String) -> Self {
        Self {
            binding,
            snapshot: None,
            job: None,
            need_cache: true,
            need_fetch: false,
            factory: None,
            status: "Loading cache".into(),
            error: String::new(),
            filter: String::new(),
            revision: String::new(),
            rows: Vec::new(),
        }
    }
}
struct Form {
    source: String,
    generation: u64,
    request: PromptRequest,
    values: Vec<Zeroizing<String>>,
    save_in_vault: bool,
    vault_filled: bool,
}
pub(super) struct State {
    runtime: BTreeMap<String, Runtime>,
    forms: VecDeque<Form>,
    busy: Arc<AtomicUsize>,
    next: u64,
    pub(super) dirty: bool,
    pub(super) manager: bool,
    draft: Option<Draft>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            runtime: BTreeMap::new(),
            forms: VecDeque::new(),
            busy: Arc::new(AtomicUsize::new(0)),
            next: 1,
            dirty: true,
            manager: false,
            draft: None,
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum SourceKind {
    File,
    Https,
    Sftp,
}
struct Draft {
    id: Option<String>,
    name: String,
    kind: SourceKind,
    text: String,
    session: String,
    format: SharedFormat,
    error: String,
}
#[derive(serde::Serialize, serde::Deserialize)]
struct Cache {
    binding: String,
    snapshot: SharedSnapshot,
}
const MAX_CACHE: usize = 32 * 1024 * 1024;
fn read_cache(
    path: &std::path::Path,
    binding: &str,
    source: &str,
) -> Result<Option<SharedSnapshot>, String> {
    use std::io::Read;
    if !path.exists() {
        return Ok(None);
    }
    let file = fs::File::open(path).map_err(|_| "Shared cache could not be opened")?;
    let mut bytes = Vec::new();
    file.take(MAX_CACHE as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Shared cache could not be read")?;
    if bytes.len() > MAX_CACHE {
        return Err("Shared cache exceeds its size limit".into());
    }
    let cache: Cache = serde_json::from_slice(&bytes)
        .map_err(|_| "Shared cache is damaged; refresh the source")?;
    if cache.binding != binding || cache.snapshot.source_id != source {
        return Ok(None);
    }
    if cache.snapshot.entries.len() > sessions_max()
        || cache
            .snapshot
            .entries
            .iter()
            .any(|e| e.id != e.session.id || !e.id.starts_with("shared-entry-"))
    {
        return Err("Shared cache identity/entry limit is invalid".into());
    }
    let mut snapshot = cache.snapshot;
    for entry in Arc::make_mut(&mut snapshot.entries) {
        entry.session.clear_credential_refs();
    }
    snapshot.stale = true;
    snapshot.last_error = Some("Cached definitions have not been refreshed in this run".into());
    Ok(Some(snapshot))
}
fn sessions_max() -> usize {
    harbor_workspace::sessions::MAX_SESSIONS
}
fn save_cache(
    path: &std::path::Path,
    binding: String,
    snapshot: SharedSnapshot,
) -> Result<(), String> {
    use std::io::Write;
    let parent = path.parent().ok_or("Cache directory is unavailable")?;
    fs::create_dir_all(parent).map_err(|_| "Could not create shared cache directory")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
            .map_err(|_| "Could not protect shared cache directory")?;
    }
    let bytes = serde_json::to_vec(&Cache { binding, snapshot })
        .map_err(|_| "Shared cache could not be encoded")?;
    if bytes.len() > MAX_CACHE {
        return Err("Shared cache exceeds its size limit".into());
    }
    let mut temp =
        tempfile::NamedTempFile::new_in(parent).map_err(|_| "Could not stage shared cache")?;
    temp.write_all(&bytes)
        .map_err(|_| "Could not write shared cache")?;
    temp.as_file()
        .sync_all()
        .map_err(|_| "Could not sync shared cache")?;
    temp.persist(path)
        .map_err(|_| "Could not publish shared cache; previous cache was kept")?;
    Ok(())
}
impl Workspace {
    fn source_reader(&self, cfg: &SharedSourceConfig) -> NetworkReader {
        let sessions: Vec<Session> = if let SharedLocation::Sftp { session_id, .. } = &cfg.location
        {
            self.store
                .sessions
                .iter()
                .filter(|s| &s.id == session_id)
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
        let agent = sessions
            .first()
            .and_then(|s| self.session_agent_endpoint(s));
        NetworkReader::new(
            self.store_path
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .to_owned(),
            sessions,
        )
        .with_agent_endpoint(agent)
        .with_defaults(self.store.preferences.ssh_defaults.clone())
    }
    pub(super) fn shared_cache_path(&self, id: &str) -> PathBuf {
        use sha2::{Digest, Sha256};
        let name: String = Sha256::digest(id.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        self.store_path
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join("shared-cache")
            .join(format!("{name}.json"))
    }
    pub(super) fn shared_snapshot(&self, id: &str) -> Option<&SharedSnapshot> {
        self.shared_sources
            .runtime
            .get(id)
            .and_then(|r| r.snapshot.as_ref())
    }
    #[cfg(test)]
    pub(super) fn shared_source_progress(&self, id: &str) -> Option<(&str, &str)> {
        self.shared_sources
            .runtime
            .get(id)
            .map(|runtime| (runtime.status.as_str(), runtime.error.as_str()))
    }
    pub(super) fn shared_sources_busy(&self) -> bool {
        self.shared_sources.busy.load(Ordering::Acquire) > 0
            || self
                .shared_sources
                .runtime
                .values()
                .any(|r| r.job.is_some() || r.need_cache || r.need_fetch)
    }
    pub(super) fn has_shared_prompt(&self) -> bool {
        !self.shared_sources.forms.is_empty()
    }
    pub(super) fn shared_modal(&self) -> bool {
        self.shared_sources.draft.is_some() || self.has_shared_prompt()
    }
    fn spawn_shared(
        &mut self,
        kind: Kind,
        ctx: &Context,
        work: impl FnOnce(
            Arc<AtomicBool>,
            std::sync::mpsc::SyncSender<PromptRequest>,
        ) -> Result<Outcome, String>
        + Send
        + 'static,
    ) -> Option<Job> {
        if self
            .shared_sources
            .busy
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < 4).then_some(n + 1)
            })
            .is_err()
        {
            return None;
        }
        let counter = self.shared_sources.busy.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancelled = cancel.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        let (ptx, prompts) = mpsc::sync_channel(1);
        let ctx = ctx.clone();
        let generation = self.shared_sources.next;
        self.shared_sources.next += 1;
        let slot = Slot(counter, ctx.clone());
        if std::thread::Builder::new()
            .name("termcraft-shared-source".into())
            .spawn(move || {
                let result = work(cancelled.clone(), ptx);
                drop(slot);
                if !cancelled.load(Ordering::Acquire) {
                    let _ = tx.try_send(result);
                }
                ctx.request_repaint();
            })
            .is_err()
        {
            return None;
        }
        Some(Job {
            kind,
            generation,
            cancel,
            rx,
            prompts,
        })
    }
    fn sync_shared_bindings(&mut self) {
        if !self.shared_sources.dirty {
            return;
        }
        let configs: Vec<_> = self.store.shared_sources.iter().take(32).cloned().collect();
        self.shared_sources
            .runtime
            .retain(|id, _| configs.iter().any(|c| &c.id == id));
        for cfg in configs {
            let binding = self.source_reader(&cfg).dependency(&cfg);
            match binding {
                Ok(binding) => {
                    if self
                        .shared_sources
                        .runtime
                        .get(&cfg.id)
                        .is_none_or(|r| r.binding != binding)
                    {
                        let old = self.shared_sources.runtime.remove(&cfg.id);
                        let mut next = Runtime::new(binding);
                        if let Some(mut old) = old {
                            if old.job.as_ref().is_some_and(|j| j.kind == Kind::Save) {
                                next.job = old.job.take();
                                next.status = "Finishing previous cache write".into();
                            }
                        }
                        self.shared_sources.runtime.insert(cfg.id.clone(), next);
                        self.shared_sources.forms.retain(|f| f.source != cfg.id);
                    }
                }
                Err(error) => {
                    let old = self.shared_sources.runtime.remove(&cfg.id);
                    let mut r = Runtime::new(String::new());
                    if let Some(mut old) = old {
                        if old.job.as_ref().is_some_and(|j| j.kind == Kind::Save) {
                            r.job = old.job.take();
                        }
                    }
                    r.need_cache = false;
                    r.error = error;
                    r.status = "Fix source configuration".into();
                    self.shared_sources.runtime.insert(cfg.id.clone(), r);
                    self.shared_sources.forms.retain(|f| f.source != cfg.id);
                }
            }
        }
        self.shared_sources.dirty = false;
    }
    pub(super) fn poll_shared_sources(&mut self, ctx: &Context) {
        self.sync_shared_bindings();
        let ids: Vec<_> = self.shared_sources.runtime.keys().cloned().collect();
        let mut results = Vec::new();
        let mut prompts = Vec::new();
        for id in &ids {
            if let Some(job) = self
                .shared_sources
                .runtime
                .get(id)
                .and_then(|r| r.job.as_ref())
            {
                match job.rx.try_recv() {
                    Ok(result) => results.push((id.clone(), job.generation, result)),
                    Err(mpsc::TryRecvError::Disconnected) => results.push((
                        id.clone(),
                        job.generation,
                        Err("Shared source worker ended without a result".into()),
                    )),
                    Err(_) => {}
                }
                while let Ok(request) = job.prompts.try_recv() {
                    prompts.push(Form {
                        source: id.clone(),
                        generation: job.generation,
                        values: request
                            .challenge
                            .prompts
                            .iter()
                            .map(|_| Zeroizing::new(String::new()))
                            .collect(),
                        request,
                        save_in_vault: false,
                        vault_filled: false,
                    });
                }
            }
        }
        for form in prompts {
            if self.shared_sources.forms.len() < 4 {
                self.shared_sources.forms.push_back(form);
            }
        }
        let mut saves = Vec::new();
        for (id, generation, result) in results {
            let Some(r) = self.shared_sources.runtime.get_mut(&id) else {
                continue;
            };
            if !r.job.as_ref().is_some_and(|j| j.generation == generation) {
                continue;
            }
            r.job = None;
            self.shared_sources
                .forms
                .retain(|f| f.source != id || f.generation != generation);
            match result {
                Ok(Outcome::Loaded(snapshot)) => {
                    r.snapshot = snapshot;
                    r.status = if r.snapshot.is_some() {
                        "Cached · stale"
                    } else {
                        "Not refreshed"
                    }
                    .into();
                }
                Ok(Outcome::Fetched { snapshot, error }) => {
                    r.snapshot = snapshot;
                    r.error = error.unwrap_or_default();
                    if r.error.is_empty() {
                        r.status = "Fresh · saving cache".into();
                        if let Some(snapshot) = &r.snapshot {
                            saves.push((id.clone(), r.binding.clone(), snapshot.clone()));
                        }
                    } else {
                        r.status = "Cached · stale / offline".into();
                    }
                }
                Ok(Outcome::Saved) => {
                    if r.snapshot.is_some() {
                        r.status = "Fresh · cached".into();
                        r.error.clear();
                    } else if r.binding.is_empty() {
                        r.status = "Fix source configuration".into();
                    } else {
                        r.status = "Cache write finished; validating source".into();
                    }
                }
                Err(error) => {
                    r.error = error;
                    r.status = "Source/cache error".into();
                }
            }
        }
        for (id, binding, snapshot) in saves {
            let path = self.shared_cache_path(&id);
            if let Some(job) = self.spawn_shared(Kind::Save, ctx, move |_, _| {
                save_cache(&path, binding, snapshot)?;
                Ok(Outcome::Saved)
            }) {
                self.shared_sources.runtime.get_mut(&id).unwrap().job = Some(job);
            } else {
                let r = self.shared_sources.runtime.get_mut(&id).unwrap();
                r.error="Fresh definitions are in memory; cache writer is busy. Refresh again to save them.".into();
            }
        }
        for id in ids {
            if self.shared_sources.busy.load(Ordering::Acquire) >= 4 {
                break;
            }
            let Some(r) = self.shared_sources.runtime.get(&id) else {
                continue;
            };
            if r.job.is_some() {
                continue;
            }
            let Some(cfg) = self
                .store
                .shared_sources
                .iter()
                .find(|c| c.id == id)
                .cloned()
            else {
                continue;
            };
            if r.need_cache {
                let binding = r.binding.clone();
                let path = self.shared_cache_path(&id);
                let source = id.clone();
                if let Some(job) = self.spawn_shared(Kind::Load, ctx, move |_, _| {
                    Ok(Outcome::Loaded(read_cache(&path, &binding, &source)?))
                }) {
                    let r = self.shared_sources.runtime.get_mut(&id).unwrap();
                    r.need_cache = false;
                    r.job = Some(job);
                }
            } else if r.need_fetch {
                let reader = self.source_reader(&cfg);
                let r = self.shared_sources.runtime.get_mut(&id).unwrap();
                let previous = r.snapshot.clone();
                let factory = r.factory.take();
                let ctx2 = ctx.clone();
                if let Some(job) = self.spawn_shared(Kind::Fetch, ctx, move |cancel, tx| {
                    let source = cfg.id.clone();
                    let auth: Arc<dyn SourceAuth> = Arc::new(Bridge {
                        source,
                        tx,
                        ctx: ctx2,
                    });
                    let reader: Arc<dyn SharedReader> = if let Some(factory) = factory {
                        factory(auth)
                    } else {
                        Arc::new(reader.with_auth(auth))
                    };
                    let mut cfg = cfg.metadata_only();
                    cfg.cached = previous;
                    let mut shared =
                        SharedSource::with_reader(cfg, reader).with_cancellation(cancel);
                    let error = shared.refresh().err();
                    Ok(Outcome::Fetched {
                        snapshot: shared.snapshot().cloned(),
                        error,
                    })
                }) {
                    let r = self.shared_sources.runtime.get_mut(&id).unwrap();
                    r.need_fetch = false;
                    r.status = "Refreshing".into();
                    r.job = Some(job);
                }
            }
        }
        self.shared_sources.forms.retain(|f| {
            self.shared_sources
                .runtime
                .get(&f.source)
                .and_then(|r| r.job.as_ref())
                .is_some_and(|j| j.generation == f.generation && j.kind == Kind::Fetch)
        });
    }
    pub(super) fn add_shared_source(&mut self, config: SharedSourceConfig, ctx: &Context) {
        if self.import_applying() || !self.store_ok {
            self.message("Finish the current settings operation before adding a source");
            return;
        }
        if let Err(error) = config.validate() {
            self.message(error);
            return;
        }
        if self.store.shared_sources.len() >= 32 {
            self.message("At most 32 shared sources can be active");
            return;
        }
        let id = config.id.clone();
        self.store.shared_sources.push(config.metadata_only());
        self.shared_sources.dirty = true;
        self.persist();
        self.sync_shared_bindings();
        self.refresh_shared_source(&id, ctx);
    }
    pub(super) fn refresh_shared_source(&mut self, id: &str, ctx: &Context) {
        self.request_shared_refresh(id, ctx, None);
    }
    pub(super) fn refresh_shared_source_with(
        &mut self,
        id: &str,
        ctx: &Context,
        factory: impl FnOnce(Arc<dyn SourceAuth>) -> Arc<dyn SharedReader> + Send + 'static,
    ) {
        self.request_shared_refresh(id, ctx, Some(Box::new(factory)));
    }
    fn request_shared_refresh(&mut self, id: &str, ctx: &Context, factory: Option<Factory>) {
        self.sync_shared_bindings();
        if let Some(r) = self.shared_sources.runtime.get_mut(id) {
            if r.job.as_ref().is_some_and(|j| j.kind == Kind::Save) {
                return;
            }
            r.factory = factory;
            r.need_fetch = true;
            r.error.clear();
        }
        ctx.request_repaint();
    }
    pub(super) fn cancel_shared_source(&mut self, id: &str) {
        if let Some(r) = self.shared_sources.runtime.get_mut(id) {
            if r.job.as_ref().is_some_and(|j| j.kind == Kind::Save) {
                return;
            }
            r.job = None;
            r.need_fetch = false;
            r.need_cache = false;
            r.factory = None;
            r.status = "Cancelled · cached data is stale".into();
            if let Some(s) = &mut r.snapshot {
                s.stale = true;
                s.last_error = Some("Refresh cancelled".into());
            }
        }
        self.shared_sources.forms.retain(|f| f.source != id);
    }
    pub(super) fn submit_shared_auth(
        &mut self,
        id: &str,
        owner: u64,
        values: Vec<Zeroizing<String>>,
        trust: Option<TrustDecision>,
    ) -> bool {
        let index = self.shared_sources.forms.iter().position(|f| {
            f.source == id
                && f.request.challenge.owner == owner
                && self
                    .shared_sources
                    .runtime
                    .get(id)
                    .and_then(|r| r.job.as_ref())
                    .is_some_and(|j| j.generation == f.generation && j.kind == Kind::Fetch)
        });
        let Some(index) = index else {
            return false;
        };
        let form = self.shared_sources.forms.remove(index).unwrap();
        form.request
            .reply
            .try_send(ChallengeAnswer {
                id: form.request.challenge.answer_id,
                values,
                trust,
            })
            .is_ok()
    }
    pub(super) fn copy_shared_to_local(&mut self, id: &str, entry: &str) {
        if self.import_applying() {
            return;
        }
        let Some(cfg) = self
            .store
            .shared_sources
            .iter()
            .find(|c| c.id == id)
            .cloned()
        else {
            return;
        };
        let Some(snapshot) = self.shared_snapshot(id).cloned() else {
            return;
        };
        let requires_review = snapshot
            .entries
            .iter()
            .find(|e| e.id == entry)
            .is_some_and(|e| e.requires_review);
        let mut cfg = cfg;
        cfg.cached = Some(snapshot);
        match SharedSource::new(cfg).copy_to_local(entry) {
            Ok(mut session) => {
                let base = session.title();
                session.name = format!("{base} copy");
                let mut n = 2;
                while self
                    .store
                    .sessions
                    .iter()
                    .any(|s| s.folder == session.folder && s.name == session.name)
                {
                    session.name = format!("{base} copy {n}");
                    n += 1;
                }
                if requires_review {
                    self.open_session_form(session);
                    self.editing = None;
                    self.save_session = true;
                    self.form_error.clear();
                    self.message("Review the shared command/resource settings, then save the independent local copy.");
                } else {
                    self.store.sessions.push(session);
                    self.persist();
                }
            }
            Err(error) => self.message(error),
        }
    }
}
impl Workspace {
    pub(super) fn shared_session_tree(&mut self, ui: &mut Ui) {
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.label(crate::ui::fluent::emphasized(
                ui.ctx(),
                RichText::new("Read-only shared sources"),
            ));
            if ui.small_button("Manage…").clicked() {
                self.shared_sources.manager = true;
            }
        });
        let configs: Vec<_> = self.store.shared_sources.iter().take(32).cloned().collect();
        let query = self.session_search.to_lowercase();
        let mut actions = Vec::new();
        for cfg in configs {
            let Some(r) = self.shared_sources.runtime.get_mut(&cfg.id) else {
                continue;
            };
            let header = format!("{} · {}", cfg.name, r.status);
            CollapsingHeader::new(header)
                .id_salt(("shared-source", &cfg.id))
                .default_open(true)
                .show(ui, |ui| {
                    if !r.error.is_empty() {
                        ui.add(Label::new(&r.error).truncate())
                            .fluent_hint(&r.error);
                    }
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .add_enabled(r.job.is_none(), Button::new("Refresh"))
                            .clicked()
                        {
                            actions.push((cfg.id.clone(), String::new(), 0));
                        }
                        if r.job.as_ref().is_some_and(|j| j.kind != Kind::Save)
                            && ui.small_button("Cancel refresh").clicked()
                        {
                            actions.push((cfg.id.clone(), String::new(), 1));
                        }
                    });
                    if let Some(snapshot) = &r.snapshot {
                        if snapshot.stale {
                            ui.small(
                                "Cached / stale definitions · refresh when the source is available",
                            );
                        }
                        if r.filter != query || r.revision != snapshot.revision {
                            r.rows = snapshot
                                .entries
                                .iter()
                                .enumerate()
                                .filter(|(_, e)| {
                                    format!(
                                        "{} {} {}",
                                        e.session.title(),
                                        e.session.host,
                                        e.session.folder
                                    )
                                    .to_lowercase()
                                    .contains(&query)
                                })
                                .map(|(i, _)| i)
                                .collect();
                            r.filter = query.clone();
                            r.revision = snapshot.revision.clone();
                        }
                        ScrollArea::vertical()
                            .id_salt(("shared-rows", &cfg.id))
                            .max_height(240.)
                            .show_rows(ui, fluent::CONTROL_HEIGHT, r.rows.len(), |ui, range| {
                                for row in range {
                                    let entry = &snapshot.entries[r.rows[row]];
                                    ui.push_id((&cfg.id, &entry.id), |ui| {
                                        let detail = format!(
                                            "Read-only · {} · {}:{} · {}",
                                            entry.session.protocol,
                                            entry.session.host,
                                            entry.session.port,
                                            entry.session.folder,
                                        );
                                        let (row, activate) = super::saved_session_row(
                                            ui,
                                            &entry.id,
                                            &entry.session.title(),
                                            &detail,
                                        );
                                        if activate {
                                            actions.push((cfg.id.clone(), entry.id.clone(), 2));
                                        }
                                        super::context_actions(&row, |ui, focus_first| {
                                            ui.label("Read-only source definition");
                                            let connect = if entry.requires_review {
                                                "Review and connect…"
                                            } else {
                                                "Connect"
                                            };
                                            if super::context_item(ui, connect, focus_first) {
                                                actions.push((cfg.id.clone(), entry.id.clone(), 2));
                                                ui.close();
                                            }
                                            if ui.button("Copy to local").clicked() {
                                                actions.push((cfg.id.clone(), entry.id.clone(), 3));
                                                ui.close();
                                            }
                                        });
                                    });
                                }
                            });
                    } else {
                        ui.small("No verified cached definitions");
                    }
                });
        }
        for (source, entry, kind) in actions {
            match kind {
                0 => self.refresh_shared_source(&source, ui.ctx()),
                1 => self.cancel_shared_source(&source),
                2 => {
                    let target = self
                        .shared_snapshot(&source)
                        .and_then(|s| s.entries.iter().find(|e| e.id == entry))
                        .cloned();
                    if let Some(target) = target {
                        if target.requires_review {
                            self.open_session_form(target.session);
                            self.editing = None;
                            self.save_session = false;
                            self.form_error.clear();
                            self.message(
                                "Review the shared command/resource settings before connecting.",
                            );
                        } else {
                            self.start(target.session);
                        }
                    }
                }
                3 => self.copy_shared_to_local(&source, &entry),
                _ => {}
            }
        }
    }
    pub(super) fn shared_sessions_windows(&mut self, ctx: &Context) {
        let mut edit = None;
        let mut refresh = None;
        let mut remove = None;
        let mut add = false;
        let mut manager = self.shared_sources.manager;
        let size = fluent::dialog_size(ctx, 680., 145.);
        let mut close_manager = false;
        fluent::modeless(ctx, Id::new("Shared session sources"), "Shared session sources", size).open(&mut manager).show(ctx,|ui|{
   ui.label("Shared definitions remain separate and read-only. Copying creates an independent local session.");
   ui.add_enabled_ui(!self.import_applying(),|ui|{
    fluent::dialog_body(ui, size, |ui| {
    for cfg in &self.store.shared_sources {
     let saving=self.shared_sources.runtime.get(&cfg.id).and_then(|r|r.job.as_ref()).is_some_and(|j|j.kind==Kind::Save);
     ui.push_id(&cfg.id,|ui|{ui.horizontal_wrapped(|ui|{
      ui.label(&cfg.name);if ui.add_enabled(!saving,Button::new("Refresh")).clicked(){refresh=Some(cfg.id.clone());}
      if ui.add_enabled(!saving,Button::new("Edit…")).clicked(){edit=Some(cfg.clone());}
      if ui.add_enabled(!saving,Button::new("Remove source")).clicked(){remove=Some(cfg.id.clone());}
     });if let Some(r)=self.shared_sources.runtime.get(&cfg.id){ui.label(&r.status);if !r.error.is_empty(){ui.colored_label(ui.visuals().error_fg_color,&r.error);}}});
    }
   });
   add = ui.button("Add shared source…").clicked();
   });
   close_manager = ui.button("Close").clicked();
  });
        self.shared_sources.manager = manager && !close_manager;
        if add {
            self.shared_sources.draft = Some(Draft {
                id: None,
                name: String::new(),
                kind: SourceKind::File,
                text: String::new(),
                session: String::new(),
                format: SharedFormat::Workspace,
                error: String::new(),
            });
        }
        if let Some(cfg) = edit {
            let (kind, text, session) = match cfg.location {
                SharedLocation::File(path) => (
                    SourceKind::File,
                    path.to_string_lossy().into_owned(),
                    String::new(),
                ),
                SharedLocation::Https(uri) => (SourceKind::Https, uri, String::new()),
                SharedLocation::Sftp { session_id, path } => (SourceKind::Sftp, path, session_id),
            };
            self.shared_sources.draft = Some(Draft {
                id: Some(cfg.id),
                name: cfg.name,
                kind,
                text,
                session,
                format: cfg.format,
                error: String::new(),
            });
        }
        if let Some(id) = refresh {
            self.refresh_shared_source(&id, ctx);
        }
        if let Some(id) = remove {
            self.remove_shared_source(&id, ctx);
        }
        self.shared_source_draft(ctx);
        self.shared_auth_window(ctx);
    }
    fn remove_shared_source(&mut self, id: &str, ctx: &Context) {
        if self.import_applying()
            || self
                .shared_sources
                .runtime
                .get(id)
                .and_then(|r| r.job.as_ref())
                .is_some_and(|j| j.kind == Kind::Save)
        {
            return;
        }
        if self.shared_sources.busy.load(Ordering::Acquire) >= 4 {
            self.message("Wait for a shared-source operation to finish before removing its cache.");
            return;
        }
        self.cancel_shared_source(id);
        self.shared_sources.runtime.remove(id);
        self.store.shared_sources.retain(|s| s.id != id);
        self.persist();
        let path = self.shared_cache_path(id);
        if let Some(cleanup) = self.spawn_shared(Kind::Save, ctx, move |_, _| {
            if path.exists() {
                fs::remove_file(path)
                    .map_err(|_| "Source metadata removed; cache cleanup failed")?;
            }
            Ok(Outcome::Saved)
        }) {
            drop(cleanup);
        } else {
            self.message("Source removed; its private cache cleanup could not start.");
        }
    }
    fn shared_source_draft(&mut self, ctx: &Context) {
        let Some(mut draft) = self.shared_sources.draft.take() else {
            return;
        };
        let mut save = false;
        let mut cancel = false;
        let size = fluent::dialog_size(ctx, 580., 120.);
        let response = fluent::modal(
            ctx,
            Id::new("shared-source-setup"),
            if draft.id.is_some() {
                "Edit shared source"
            } else {
                "Add shared source"
            },
            size,
            |ui| {
                fluent::dialog_body(ui, size, |ui| {
                    ui.add_enabled_ui(!self.import_applying(),|ui|{
    let name = field(ui,"Name",&mut draft.name,"Team sessions"); fluent::focus_on_entry(ui, &name, ui.id());
    ui.horizontal_wrapped(|ui|{ui.selectable_value(&mut draft.kind,SourceKind::File,"Shared file");ui.selectable_value(&mut draft.kind,SourceKind::Https,"HTTPS");ui.selectable_value(&mut draft.kind,SourceKind::Sftp,"SFTP");});
    match draft.kind {
     SourceKind::File=>{ui.horizontal_wrapped(|ui|{ui.add(TextEdit::singleline(&mut draft.text).hint_text("Shared file path").desired_width((size.width-110.).max(1.)));if ui.button("Browse…").clicked(){if let Some(path)=rfd::FileDialog::new().pick_file(){if let Some(text)=path.to_str(){draft.text=text.to_owned();}else{draft.error="This source path cannot be stored as portable text; choose a UTF-8 path.".into();}}}});},
     SourceKind::Https=>{field(ui,"HTTPS URL",&mut draft.text,"https://example.com/sessions.json");ui.small("Use a URL without embedded credentials, query or fragment. Sign-in prompts are private; certificates are verified and redirects are refused.");},
     SourceKind::Sftp=>{
      ComboBox::from_id_salt("shared-ssh-profile").selected_text(self.store.sessions.iter().find(|s|s.id==draft.session).map(Session::title).unwrap_or_else(||"Choose saved embedded SSH profile".into())).show_ui(ui,|ui|{
       for session in self.store.sessions.iter().filter(|s|s.protocol=="SSH"&&s.ssh_engine==SshEngine::Embedded){ui.selectable_value(&mut draft.session,session.id.clone(),format!("{} · {}:{}",session.title(),session.host,session.port));}
      });field(ui,"Remote file",&mut draft.text,"/path/to/sessions.json");ui.small("Uses the embedded SSH trust/sign-in engine and SFTP only; no shell or login command is started.");
     }
    }
    ui.horizontal_wrapped(|ui|{ui.label("Format");ui.selectable_value(&mut draft.format,SharedFormat::Workspace,"TermCraft JSON");ui.selectable_value(&mut draft.format,SharedFormat::Moba,"Moba SSH");ui.selectable_value(&mut draft.format,SharedFormat::Putty,"PuTTY SSH .reg");});
    if !draft.error.is_empty(){ui.colored_label(ui.visuals().error_fg_color,&draft.error);}
   });
                });
                ui.horizontal_wrapped(|ui| {
                    save = ui
                        .add_enabled(!self.import_applying(), Button::new("Save and refresh"))
                        .clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            },
        );
        if response.should_close() {
            cancel = true;
        }
        if cancel {
            return;
        }
        if save {
            let location = match draft.kind {
                SourceKind::File => SharedLocation::File(PathBuf::from(&draft.text)),
                SourceKind::Https => SharedLocation::Https(draft.text.clone()),
                SourceKind::Sftp => SharedLocation::Sftp {
                    session_id: draft.session.clone(),
                    path: draft.text.clone(),
                },
            };
            match SharedSourceConfig::new(&draft.name, location, draft.format) {
                Ok(mut cfg) => {
                    if let Some(id) = &draft.id {
                        if let Some(index) =
                            self.store.shared_sources.iter().position(|s| &s.id == id)
                        {
                            let old = &self.store.shared_sources[index];
                            cfg.inherit_metadata(old);
                            self.cancel_shared_source(id);
                            self.store.shared_sources[index] = cfg.metadata_only();
                            self.persist();
                            self.sync_shared_bindings();
                            self.refresh_shared_source(id, ctx);
                        }
                    } else {
                        self.add_shared_source(cfg, ctx);
                    }
                    return;
                }
                Err(error) => draft.error = error,
            }
        }
        self.shared_sources.draft = Some(draft);
    }
    fn shared_auth_window(&mut self, ctx: &Context) {
        let Some(mut form) = self.shared_sources.forms.pop_front() else {
            return;
        };
        let mut answer = None;
        let mut cancel = false;
        let mut use_saved = false;
        let size = fluent::dialog_size(ctx, 520., 180.);
        let response = fluent::modal(
            ctx,
            Id::new("shared-source-sign-in"),
            "Shared source sign-in",
            size,
            |ui| {
                let c = &form.request.challenge;
                ScrollArea::vertical()
                    .id_salt("shared-auth-fields")
                    .max_height(size.body_height)
                    .show(ui, |ui| {
                        ui.label(&c.title);
                        if let Some(endpoint) = &c.endpoint {
                            ui.label(endpoint);
                        }
                        for (index, prompt) in c.prompts.iter().enumerate() {
                            let label = ui
                                .add(Label::new(&prompt.label).wrap())
                                .fluent_hint(&prompt.label);
                            super::remote_files::auth_field_scoped(
                                ui,
                                "shared-source-auth",
                                c.owner,
                                c.answer_id,
                                index,
                                prompt,
                                &mut form.values[index],
                            )
                            .labelled_by(label.id);
                        }
                    });
                if let Some(credential) = &c.credential {
                    ui.horizontal_wrapped(|ui|{
                if credential.reference.is_some(){use_saved=ui.button("Use saved credential").clicked();}
                ui.checkbox(&mut form.save_in_vault,"Save entered value in vault").fluent_hint("Unlock the vault first. Saving does not wait for authentication success.");
            });
                }
                ui.horizontal_wrapped(|ui| {
                    if c.host_key {
                        if ui.button("Trust once").clicked() {
                            answer = Some(Some(TrustDecision::Once));
                        }
                        if ui.button("Trust and remember").clicked() {
                            answer = Some(Some(TrustDecision::Remember));
                        }
                    } else if ui.button("Continue").clicked() {
                        answer = Some(None);
                    }
                    let safe = ui.button("Cancel refresh");
                    if c.host_key {
                        fluent::focus_on_entry(ui, &safe, Id::new((c.owner, c.answer_id)));
                    }
                    cancel = safe.clicked();
                });
            },
        );
        cancel |= response.should_close();
        let source = form.source.clone();
        let owner = form.request.challenge.owner;
        if use_saved {
            if let Some(credential) = form.request.challenge.credential.clone() {
                let destination = super::credentials::Destination::Shared {
                    source: form.source.clone(),
                    generation: form.generation,
                    owner,
                    answer: form.request.challenge.answer_id,
                };
                self.shared_sources.forms.push_front(form);
                self.request_vault(
                    super::credentials::Request::Use {
                        destination,
                        credential,
                    },
                    ctx,
                );
                return;
            }
        }
        if cancel {
            self.shared_sources.forms.push_front(form);
            self.cancel_shared_source(&source);
        } else if let Some(trust) = answer {
            let save = if form.save_in_vault && trust != Some(TrustDecision::Reject) {
                form.request.challenge.credential.clone().map(|credential| {
                    super::credentials::Request::Save {
                        destination: super::credentials::Destination::Shared {
                            source: form.source.clone(),
                            generation: form.generation,
                            owner,
                            answer: form.request.challenge.answer_id,
                        },
                        credential,
                        values: form.values.clone(),
                    }
                })
            } else {
                None
            };
            let values = std::mem::take(&mut form.values);
            self.shared_sources.forms.push_front(form);
            let accepted = self.submit_shared_auth(&source, owner, values, trust);
            if accepted {
                if let Some(request) = save {
                    self.request_vault(request, ctx);
                }
            }
        } else {
            self.shared_sources.forms.push_front(form);
        }
    }
}

fn field(ui: &mut Ui, label: &str, value: &mut String, hint: &str) -> Response {
    fluent::field(ui, label, value, hint, 460.)
}

impl Workspace {
    pub(super) fn clear_shared_vault_values(&mut self) {
        for form in &mut self.shared_sources.forms {
            if form.vault_filled {
                for value in &mut form.values {
                    zeroize::Zeroize::zeroize(value);
                }
                form.vault_filled = false;
            }
        }
    }
    pub(super) fn fill_shared_vault_values(
        &mut self,
        destination: &super::credentials::Destination,
        credential: &harbor_workspace::ssh::ChallengeCredential,
        values: Vec<Zeroizing<String>>,
    ) -> bool {
        let super::credentials::Destination::Shared {
            source,
            generation,
            owner,
            answer,
        } = destination
        else {
            return false;
        };
        if !self
            .shared_sources
            .runtime
            .get(source)
            .and_then(|r| r.job.as_ref())
            .is_some_and(|j| j.kind == Kind::Fetch && j.generation == *generation)
        {
            return false;
        }
        if let Some(form) = self.shared_sources.forms.iter_mut().find(|f| {
            f.source == *source
                && f.generation == *generation
                && f.request.challenge.owner == *owner
                && f.request.challenge.answer_id == *answer
                && f.request.challenge.credential.as_ref().is_some_and(|c| {
                    c.binding == credential.binding && c.reference == credential.reference
                })
                && f.values.len() == values.len()
        }) {
            form.values = values;
            form.vault_filled = true;
            true
        } else {
            false
        }
    }
}

impl Workspace {
    pub(super) fn shared_vault_destination_current(
        &self,
        destination: &super::credentials::Destination,
        credential: &harbor_workspace::ssh::ChallengeCredential,
    ) -> bool {
        let super::credentials::Destination::Shared {
            source,
            generation,
            owner,
            answer,
        } = destination
        else {
            return false;
        };
        self.shared_sources
            .runtime
            .get(source)
            .and_then(|r| r.job.as_ref())
            .is_some_and(|j| j.kind == Kind::Fetch && j.generation == *generation)
            && self.shared_sources.forms.iter().any(|f| {
                f.source == *source
                    && f.generation == *generation
                    && f.request.challenge.owner == *owner
                    && f.request.challenge.answer_id == *answer
                    && f.request.challenge.credential.as_ref().is_some_and(|c| {
                        c.binding == credential.binding && c.reference == credential.reference
                    })
            })
    }
}

impl Workspace {
    pub(super) fn forget_shared_credential_ui(&mut self, id: &str) {
        for form in &mut self.shared_sources.forms {
            if form
                .request
                .challenge
                .credential
                .as_ref()
                .is_some_and(|c| c.reference.as_deref() == Some(id))
            {
                if form.vault_filled {
                    for value in &mut form.values {
                        zeroize::Zeroize::zeroize(value);
                    }
                }
                form.vault_filled = false;
                form.save_in_vault = false;
                if let Some(c) = &mut form.request.challenge.credential {
                    c.reference = None;
                }
                form.request.challenge.credential_ref = None;
            }
        }
    }
}
