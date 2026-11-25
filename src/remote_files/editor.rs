//! Connection-bound text documents. Recovery writes and protocol operations run off the UI thread.
use super::{
    RemotePath,
    transfer::{Job, TransferManager, TransferOutcome},
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::sync::{mpsc, watch};
pub const MAX_DOCUMENT_BYTES: usize = 2 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentStatus {
    Opening,
    Ready,
    Recovering,
    Saving,
    Committing,
    Saved,
    Conflict,
    Cancelled,
    Disconnected,
    Indeterminate(String),
    Failed(String),
}
impl DocumentStatus {
    pub fn busy(&self) -> bool {
        matches!(
            self,
            Self::Opening | Self::Recovering | Self::Saving | Self::Committing
        )
    }
}
#[derive(Clone, Copy, Debug)]
pub enum SavePolicy {
    VerifyUnchanged,
    Overwrite,
}
#[derive(Clone, Copy)]
pub enum CloseDecision {
    Save,
    Discard,
    Cancel,
}
#[derive(Clone)]
pub struct DocumentSnapshot {
    pub remote_recovery: Option<RemotePath>,
    pub warning: Option<String>,
    pub connection: u64,
    pub path: RemotePath,
    pub text: String,
    pub dirty: bool,
    pub status: DocumentStatus,
    pub recovery_path: Option<PathBuf>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Endpoint {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub jump: String,
    pub gateway_user: String,
    pub authenticated: bool,
    pub target_key: String,
    pub gateway_key: String,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Metadata {
    pub permissions: Option<u32>,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    pub mtime: Option<u32>,
}
impl From<&russh_sftp::protocol::FileAttributes> for Metadata {
    fn from(a: &russh_sftp::protocol::FileAttributes) -> Self {
        Self {
            permissions: a.permissions,
            uid: a.uid,
            gid: a.gid,
            mtime: a.mtime,
        }
    }
}
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Version {
    pub bytes: Vec<u8>,
    pub metadata: Metadata,
}
pub(crate) struct State {
    pub snapshot: DocumentSnapshot,
    pub baseline: Vec<u8>,
    pub metadata: Option<Metadata>,
    pub conflict: Option<Version>,
    pub directory: Option<PathBuf>,
    pub revision: u64,
    pub operation: u64,
    pub cancel: watch::Sender<bool>,
}
pub struct RemoteDocument {
    state: Arc<Mutex<State>>,
    sender: mpsc::Sender<Job>,
    endpoint: Option<Arc<Mutex<Endpoint>>>,
}
#[derive(Clone)]
pub(crate) enum Action {
    Open {
        root: PathBuf,
    },
    Save {
        bytes: Vec<u8>,
        expected: Option<Version>,
        revision: u64,
    },
}
#[derive(Clone)]
pub struct EditorRequest {
    pub(crate) state: Arc<Mutex<State>>,
    pub(crate) action: Action,
    pub(crate) token: u64,
    pub(crate) cancelled: watch::Receiver<bool>,
}
impl std::fmt::Debug for EditorRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EditorRequest([document contents omitted])")
    }
}
impl EditorRequest {
    pub(crate) fn enter_commit(&self) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.operation != self.token || *self.cancelled.borrow() {
            return false;
        }
        state.snapshot.status = DocumentStatus::Committing;
        true
    }
    pub(crate) fn finish(&self, outcome: &TransferOutcome) {
        let mut s = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if s.operation == self.token && s.snapshot.status.busy() {
            s.snapshot.status = match outcome {
                TransferOutcome::Conflict => DocumentStatus::Conflict,
                TransferOutcome::Failed(e) => DocumentStatus::Failed(e.clone()),
                TransferOutcome::Indeterminate(e) => DocumentStatus::Indeterminate(e.clone()),
                _ if s.snapshot.status == DocumentStatus::Committing => DocumentStatus::Indeterminate("Connection ended during remote publication; compare the remote version before retrying. Local recovery is retained.".into()),
                _ => DocumentStatus::Cancelled,
            };
        }
    }
}
fn enqueue(
    sender: &mpsc::Sender<Job>,
    request: EditorRequest,
) -> Result<(), (DocumentStatus, String)> {
    let (error, rejected) = match sender.try_send(Job::for_document(request.clone())) {
        Ok(()) => return Ok(()),
        Err(mpsc::error::TrySendError::Full(job)) => ((DocumentStatus::Failed("Document queue is busy; retry Save after pending work completes. Local recovery is retained.".into()), "Document queue is busy".into()), job),
        Err(mpsc::error::TrySendError::Closed(job)) => ((DocumentStatus::Disconnected, "Document connection has ended".into()), job),
    };
    let mut s = request.state.lock().unwrap_or_else(|p| p.into_inner());
    if s.operation == request.token {
        s.snapshot.status = error.0.clone();
    }
    drop(s);
    drop(rejected);
    Err(error)
}
impl RemoteDocument {
    pub fn open(
        connection: u64,
        path: RemotePath,
        transfers: &TransferManager,
        recovery_root: PathBuf,
    ) -> Result<Self, String> {
        if transfers.connection_id() != Some(connection) {
            return Err("Document endpoint does not match the authenticated connection".into());
        }
        if !path.as_bytes().starts_with(b"/") {
            return Err("Open a file from an absolute remote directory".into());
        }
        let (cancel, cancelled) = watch::channel(false);
        let state = Arc::new(Mutex::new(State {
            snapshot: DocumentSnapshot {
                remote_recovery: None,
                warning: None,
                connection,
                path,
                text: String::new(),
                dirty: false,
                status: DocumentStatus::Opening,
                recovery_path: None,
            },
            baseline: Vec::new(),
            metadata: None,
            conflict: None,
            directory: None,
            revision: 0,
            operation: 1,
            cancel,
        }));
        let sender = transfers.document_sender();
        enqueue(
            &sender,
            EditorRequest {
                state: state.clone(),
                action: Action::Open {
                    root: recovery_root,
                },
                token: 1,
                cancelled,
            },
        )
        .map_err(|(_, e)| e)?;
        Ok(Self {
            state,
            sender,
            endpoint: transfers.editor_endpoint.clone(),
        })
    }
    pub fn snapshot(&self) -> DocumentSnapshot {
        self.state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .snapshot
            .clone()
    }
    pub fn details(&self) -> (DocumentStatus, bool, Option<PathBuf>) {
        let s = self.state.lock().unwrap_or_else(|p| p.into_inner());
        (
            s.snapshot.status.clone(),
            s.snapshot.dirty,
            s.snapshot.recovery_path.clone(),
        )
    }
    pub fn recovery_details(&self) -> (Option<RemotePath>, Option<String>) {
        let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        (
            state.snapshot.remote_recovery.clone(),
            state.snapshot.warning.clone(),
        )
    }
    pub fn set_text(&mut self, text: &str) -> Result<(), String> {
        if text.len() > MAX_DOCUMENT_BYTES || text.contains('\0') {
            return Err("The text editor supports UTF-8 text up to 2 MiB without NUL bytes; use Download for other files".into());
        }
        let mut s = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if s.directory.is_none() {
            return Err("Wait for the document to finish opening".into());
        }
        s.snapshot.text = text.to_owned();
        s.snapshot.dirty = text.as_bytes() != s.baseline;
        s.revision = s.revision.wrapping_add(1);
        Ok(())
    }
    pub fn save(&mut self, policy: SavePolicy) -> Result<(), String> {
        let (directory, bytes, expected, revision, token, cancelled) = {
            let mut s = self.state.lock().unwrap_or_else(|p| p.into_inner());
            if s.snapshot.status.busy() {
                return Err("Wait for the current document operation, or cancel it".into());
            }
            let directory = s.directory.clone().ok_or("The document has not opened")?;
            let expected = match policy {
                SavePolicy::VerifyUnchanged => s.metadata.clone().map(|metadata| Version {
                    bytes: s.baseline.clone(),
                    metadata,
                }),
                SavePolicy::Overwrite => Some(
                    s.conflict
                        .clone()
                        .ok_or("No observed conflict version is available to confirm")?,
                ),
            };
            s.operation = s.operation.wrapping_add(1);
            let (cancel, cancelled) = watch::channel(false);
            s.cancel = cancel;
            s.snapshot.status = DocumentStatus::Recovering;
            (
                directory,
                s.snapshot.text.as_bytes().to_vec(),
                expected,
                s.revision,
                s.operation,
                cancelled,
            )
        };
        let state = self.state.clone();
        let sender = self.sender.clone();
        std::thread::Builder::new()
            .name("termcraft-edit-recovery".into())
            .spawn(move || {
                let result = persist(&directory, &bytes);
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                if s.operation != token {
                    return;
                }
                match result {
                    Err(error) => {
                        s.snapshot.status = DocumentStatus::Failed(format!(
                            "Local recovery write failed; edits remain in this window: {error}"
                        ))
                    }
                    Ok(path) => {
                        s.snapshot.recovery_path = Some(path);
                        if *cancelled.borrow() {
                            s.snapshot.status = DocumentStatus::Cancelled;
                            return;
                        }
                        s.snapshot.status = DocumentStatus::Saving;
                        drop(s);
                        let _ = enqueue(
                            &sender,
                            EditorRequest {
                                state,
                                action: Action::Save {
                                    bytes,
                                    expected,
                                    revision,
                                },
                                token,
                                cancelled,
                            },
                        );
                    }
                }
            })
            .map_err(|e| {
                self.state
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .snapshot
                    .status = DocumentStatus::Failed(e.to_string());
                e.to_string()
            })?;
        Ok(())
    }
    pub fn cancel_operation(&mut self) {
        let mut s = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if s.snapshot.status == DocumentStatus::Committing {
            return;
        }
        s.cancel.send_replace(true);
        // Any in-flight final rename may already have reached the server. Cancelled never asserts remote rollback.
        if s.snapshot.status.busy() {
            s.snapshot.status = DocumentStatus::Cancelled;
        }
    }
    pub fn can_import_from(&self, source: &Self) -> bool {
        let identity = |endpoint: &Option<Arc<Mutex<Endpoint>>>| {
            endpoint
                .as_ref()
                .map(|endpoint| endpoint.lock().unwrap_or_else(|p| p.into_inner()).clone())
                .filter(|e| e.authenticated && !e.user.is_empty() && !e.target_key.is_empty())
        };
        let current = identity(&self.endpoint);
        let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        let target_ready = state.metadata.is_some()
            && state.directory.is_some()
            && !state.snapshot.status.busy()
            && !state.snapshot.dirty;
        let path = state.snapshot.path.clone();
        drop(state);
        let state = source.state.lock().unwrap_or_else(|p| p.into_inner());
        let source_ready = state.metadata.is_some()
            && state.directory.is_some()
            && !state.snapshot.status.busy()
            && state.snapshot.path == path;
        drop(state);
        current.is_some() && current == identity(&source.endpoint) && target_ready && source_ready
    }

    pub fn import_recovery_from(&mut self, source: &Self) -> Result<(), String> {
        if !self.can_import_from(source) {
            return Err("Recovery requires successfully opened matching authenticated documents and a destination without unsaved edits".into());
        }
        if self
            .state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .directory
            .is_none()
        {
            return Err("Wait for the fresh remote document to finish opening".into());
        }
        self.set_text(&source.snapshot().text)
    }
    pub fn request_close(&mut self, decision: CloseDecision) -> Result<bool, String> {
        let s = self.snapshot();
        if s.status.busy() {
            return Ok(false);
        }
        if !s.dirty {
            return Ok(true);
        }
        match decision {
            CloseDecision::Cancel => Ok(false),
            CloseDecision::Discard => Ok(true),
            CloseDecision::Save => {
                self.save(SavePolicy::VerifyUnchanged)?;
                Ok(false)
            }
        }
    }
}
fn random_name() -> Result<String, String> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
pub(crate) fn recovery_directory(root: PathBuf) -> Result<PathBuf, String> {
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    if !std::fs::symlink_metadata(&root)
        .map_err(|e| e.to_string())?
        .is_dir()
    {
        return Err("Recovery root must be a directory, not a symbolic link".into());
    }
    let directory = root.join(random_name()?);
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&directory).map_err(|e| e.to_string())?;
    Ok(directory)
}
pub(crate) fn persist(directory: &std::path::Path, bytes: &[u8]) -> Result<PathBuf, String> {
    use std::io::Write;
    let path = directory.join(format!("{}.txt", random_name()?));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&path).map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    super::transfer::close_recovery_file(file).map_err(|e| e.to_string())?;
    Ok(path)
}
#[cfg(test)]
mod review_tests {
    use super::*;
    use crate::remote_files::transfer::TransferOperation;
    fn opened_state() -> (
        RemoteDocument,
        TransferManager,
        mpsc::Receiver<Job>,
        Job,
        PathBuf,
    ) {
        let root =
            std::env::temp_dir().join(format!("termcraft-editor-state-{}", random_name().unwrap()));
        std::fs::create_dir(&root).unwrap();
        let (mut manager, mut receiver) = TransferManager::channel();
        manager.connection = Some(92);
        let doc = RemoteDocument::open(
            92,
            RemotePath::from_bytes(b"/file".to_vec()).unwrap(),
            &manager,
            root.clone(),
        )
        .unwrap();
        let old = receiver.try_recv().unwrap();
        {
            let mut s = doc.state.lock().unwrap();
            s.snapshot.status = DocumentStatus::Ready;
            s.directory = Some(root.clone());
        }
        (doc, manager, receiver, old, root)
    }
    #[test]
    fn earlier_job_completion_cannot_finish_a_new_save() {
        let (mut doc, _manager, mut receiver, old, root) = opened_state();
        doc.set_text("mine").unwrap();
        doc.save(SavePolicy::VerifyUnchanged).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        let new_job = loop {
            if let Ok(job) = receiver.try_recv() {
                break job;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        };
        old.finish(TransferOutcome::Failed("earlier failure".into()));
        drop(old);
        assert_eq!(doc.snapshot().status, DocumentStatus::Saving);
        drop(new_job);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn full_document_queue_is_retryable_and_keeps_local_recovery() {
        let (mut doc, mut manager, _receiver, old, root) = opened_state();
        for n in 0..16 {
            manager
                .submit(TransferOperation::Mkdir {
                    remote: RemotePath::from_bytes(format!("/queued-{n}").into_bytes()).unwrap(),
                })
                .unwrap();
        }
        doc.set_text("mine").unwrap();
        doc.save(SavePolicy::VerifyUnchanged).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while doc.snapshot().status.busy() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(
            matches!(doc.snapshot().status, DocumentStatus::Failed(ref e) if e.contains("busy")),
            "{:?}",
            doc.snapshot().status
        );
        assert_eq!(
            std::fs::read(doc.snapshot().recovery_path.unwrap()).unwrap(),
            b"mine"
        );
        drop(old);
        std::fs::remove_dir_all(root).unwrap();
    }
}

pub(crate) async fn loaded(
    request: &EditorRequest,
    root: PathBuf,
    bytes: Vec<u8>,
    metadata: Metadata,
) -> Result<TransferOutcome, String> {
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err("Remote text exceeds the 2 MiB editor limit".into());
    }
    let text = String::from_utf8(bytes.clone())
        .map_err(|_| "This file is not UTF-8 text; use Download")?;
    if text.contains('\0') {
        return Err("This file contains NUL bytes; use Download".into());
    }
    let (directory, recovery, text) = tokio::task::spawn_blocking(move || {
        let directory = recovery_directory(root)?;
        let path = persist(&directory, text.as_bytes())?;
        Ok::<_, String>((directory, path, text))
    })
    .await
    .map_err(|e| e.to_string())??;
    let mut state = request.state.lock().unwrap_or_else(|p| p.into_inner());
    if state.operation != request.token || *request.cancelled.borrow() {
        return Ok(TransferOutcome::Cancelled);
    }
    state.directory = Some(directory);
    state.baseline = bytes;
    state.metadata = Some(metadata);
    state.snapshot.text = text;
    state.snapshot.recovery_path = Some(recovery);
    state.snapshot.remote_recovery = None;
    state.snapshot.warning = None;
    state.snapshot.status = DocumentStatus::Ready;
    Ok(TransferOutcome::Completed)
}
