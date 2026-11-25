//! Bounded per-connection file jobs. Progress cannot crowd out final outcomes.
use super::RemotePath;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::sync::{mpsc, watch};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ConflictPolicy {
    #[default]
    Ask,
    Skip,
    Overwrite,
}
#[derive(Clone, Debug)]
pub enum TransferOperation {
    Document(super::editor::EditorRequest),
    RemoveTree {
        remote: RemotePath,
        confirmed: RemotePath,
    },
    Mkdir {
        remote: RemotePath,
    },
    Rename {
        source: RemotePath,
        destination: RemotePath,
        policy: ConflictPolicy,
    },
    Remove {
        remote: RemotePath,
        confirmed: RemotePath,
    },
    Upload {
        local: PathBuf,
        remote: RemotePath,
        policy: ConflictPolicy,
    },
    Download {
        remote: RemotePath,
        local: PathBuf,
        policy: ConflictPolicy,
    },
    /// An explicitly selected FTP directory source.
    UploadFolder {
        local: PathBuf,
        remote: RemotePath,
        policy: ConflictPolicy,
    },
    /// An explicitly selected FTP directory source.
    DownloadFolder {
        remote: RemotePath,
        local: PathBuf,
        policy: ConflictPolicy,
    },
}
impl TransferOperation {
    fn summary(&self) -> String {
        match self {
            Self::Document(_) => "Remote document operation".into(),
            Self::Upload { local, remote, .. } => {
                format!("Upload {local:?} → {}", remote.display())
            }
            Self::Download { remote, local, .. } => {
                format!("Download {} → {local:?}", remote.display())
            }
            Self::UploadFolder { local, remote, .. } => {
                format!("Upload folder {local:?} → {}", remote.display())
            }
            Self::DownloadFolder { remote, local, .. } => {
                format!("Download folder {} → {local:?}", remote.display())
            }
            Self::Mkdir { remote } => format!("Create folder {}", remote.display()),
            Self::Rename {
                source,
                destination,
                ..
            } => format!("Rename {} → {}", source.display(), destination.display()),
            Self::Remove { remote, .. } => format!("Delete {}", remote.display()),
            Self::RemoveTree { remote, .. } => {
                format!("Delete folder and contents {}", remote.display())
            }
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransferOutcome {
    Completed,
    CompletedWithWarning(String),
    Indeterminate(String),
    Skipped,
    Conflict,
    Cancelled,
    Failed(String),
}
#[derive(Clone, Debug)]
pub enum RecoveryPath {
    Local(PathBuf),
    Remote(RemotePath),
}
#[derive(Clone, Debug)]
pub struct TransferEvent {
    pub committing: bool,
    pub bulk: bool,
    pub summary: String,
    pub id: u64,
    pub bytes: u64,
    pub completed_entries: u64,
    pub outcome: Option<TransferOutcome>,
    pub recovery: Option<RecoveryPath>,
}
struct State {
    started: bool,
    tree_recovery: Option<RecoveryPath>,
    cancel_after_commit: bool,
    event: TransferEvent,
    dirty: bool,
}
#[derive(Clone)]
pub(crate) struct Job {
    pub operation: TransferOperation,
    pub cancelled: watch::Receiver<bool>,
    state: Arc<Mutex<State>>,
}
impl Job {
    pub(crate) fn mark_started(&self) {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .started = true;
    }
    pub(crate) fn begin_commit(&self) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.event.outcome.is_some() || state.cancel_after_commit || *self.cancelled.borrow() {
            return false;
        }
        if let TransferOperation::Document(request) = &self.operation {
            if !request.enter_commit() {
                return false;
            }
        }
        state.event.committing = true;
        state.dirty = true;
        true
    }

    pub(crate) fn begin_step(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .event
            .bulk = true;
        self.begin_commit()
    }
    pub(crate) fn end_step(&self, last: bool) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        state.event.committing = false;
        state.event.completed_entries += 1;
        state.dirty = true;
        last || (!state.cancel_after_commit && !*self.cancelled.borrow())
    }
    pub(crate) fn recovery(&self, recovery: Option<RecoveryPath>) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.event.outcome.is_none() {
            state.event.recovery = state.tree_recovery.clone().or(recovery);
            state.dirty = true;
        }
    }
    pub(crate) fn recovery_path(&self) -> Option<RecoveryPath> {
        self.state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .event
            .recovery
            .clone()
    }
    pub(crate) fn advance(&self, bytes: u64) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.event.outcome.is_some() {
            return;
        }
        state.event.bytes = bytes;
        state.dirty = true;
    }
    pub(crate) fn for_document(request: super::editor::EditorRequest) -> Self {
        let (_, cancelled) = watch::channel(false);
        Self {
            operation: TransferOperation::Document(request),
            cancelled,
            state: Arc::new(Mutex::new(State {
                started: false,
                tree_recovery: None,
                cancel_after_commit: false,
                dirty: false,
                event: TransferEvent {
                    committing: false,
                    bulk: false,
                    summary: "Remote document operation".into(),
                    id: 0,
                    bytes: 0,
                    completed_entries: 0,
                    outcome: None,
                    recovery: None,
                },
            })),
        }
    }
    pub(crate) fn entry_completed(&self) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        state.event.completed_entries += 1;
        state.dirty = true;
    }
    fn sync_document_recovery(&self) {
        if let TransferOperation::Document(request) = &self.operation {
            let recovery = self.recovery_path();
            if let Some(RecoveryPath::Remote(path)) = recovery {
                let mut state = request.state.lock().unwrap_or_else(|p| p.into_inner());
                if state.operation == request.token {
                    state.snapshot.remote_recovery = Some(path);
                }
            }
        }
    }
    pub(crate) fn finish(&self, outcome: TransferOutcome) {
        self.sync_document_recovery();
        if let TransferOperation::Document(request) = &self.operation {
            request.finish(&outcome);
        }
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.event.outcome.is_none() {
            state.event.outcome = Some(outcome);
            state.dirty = true;
        }
    }
    pub fn report(
        &self,
        bytes: u64,
        recovery: Option<RecoveryPath>,
        outcome: Option<TransferOutcome>,
    ) {
        let mut s = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if s.event.outcome.is_some() {
            return;
        }
        s.event.bytes = bytes;
        s.event.recovery = s.tree_recovery.clone().or(recovery);
        s.event.outcome = outcome;
        s.dirty = true;
    }
}
pub struct TransferManager {
    pub(crate) editor_endpoint: Option<Arc<Mutex<super::editor::Endpoint>>>,
    pub(crate) connection: Option<u64>,
    sender: mpsc::Sender<Job>,
    jobs: BTreeMap<u64, (watch::Sender<bool>, Arc<Mutex<State>>)>,
    closed: bool,
}
impl TransferManager {
    pub(crate) fn channel() -> (Self, mpsc::Receiver<Job>) {
        let (sender, receiver) = mpsc::channel(16);
        (
            Self {
                editor_endpoint: None,
                connection: None,
                sender,
                jobs: BTreeMap::new(),
                closed: false,
            },
            receiver,
        )
    }
    pub fn submit(&mut self, operation: TransferOperation) -> Result<u64, String> {
        if self.closed || self.jobs.len() >= 32 {
            return Err("File queue is full or disconnected; wait and retry".into());
        }
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let (cancel, cancelled) = watch::channel(false);
        let state = Arc::new(Mutex::new(State {
            started: false,
            tree_recovery: None,
            cancel_after_commit: false,
            event: TransferEvent {
                committing: false,
                bulk: false,
                id,
                summary: operation.summary(),
                bytes: 0,
                completed_entries: 0,
                outcome: None,
                recovery: None,
            },
            dirty: true,
        }));
        self.sender
            .try_send(Job {
                operation,
                cancelled,
                state: state.clone(),
            })
            .map_err(|_| "File queue is busy or disconnected")?;
        self.jobs.insert(id, (cancel, state));
        Ok(id)
    }
    pub fn cancel(&self, id: u64) {
        if let Some((cancel, state)) = self.jobs.get(&id) {
            let mut state = state.lock().unwrap_or_else(|p| p.into_inner());
            if state.event.committing {
                if state.event.bulk {
                    state.cancel_after_commit = true;
                    state.dirty = true;
                }
                return;
            }
            cancel.send_replace(true);
            if !state.started && state.event.outcome.is_none() {
                state.event.outcome = Some(TransferOutcome::Cancelled);
                state.dirty = true;
            }
        }
    }
    pub fn poll(&mut self) -> Vec<TransferEvent> {
        let mut events = Vec::new();
        self.jobs.retain(|_, (_, state)| {
            let mut state = state.lock().unwrap_or_else(|p| p.into_inner());
            if state.dirty {
                events.push(state.event.clone());
                state.dirty = false;
            }
            state.event.outcome.is_none()
        });
        events
    }
    pub(crate) fn disconnect(&mut self) {
        self.closed = true;
        for (cancel, state) in self.jobs.values() {
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            cancel.send_replace(true);
            if s.event.outcome.is_none() {
                s.event.outcome = Some(if s.event.committing {
                    TransferOutcome::Indeterminate("Connection ended during commit; compare the destination before retrying. Recovery data may remain.".into())
                } else {
                    TransferOutcome::Failed(
                        "SSH connection ended; partial data may be recovered".into(),
                    )
                });
                s.dirty = true;
            }
        }
    }
}
impl Drop for TransferManager {
    fn drop(&mut self) {
        self.disconnect();
    }
}
impl TransferManager {
    pub fn connection_id(&self) -> Option<u64> {
        self.connection
    }
    pub(crate) fn document_sender(&self) -> mpsc::Sender<Job> {
        self.sender.clone()
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.sync_document_recovery();
        if let TransferOperation::Document(request) = &self.operation {
            request.finish(&TransferOutcome::Cancelled);
        }
    }
}
pub(crate) use operations::close_recovery_file;
mod operations;
pub(crate) use operations::run;
pub(crate) mod local_stage;
mod scp;
pub(crate) use scp::run as run_scp;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queued_cancellation_is_observable_before_worker_start() {
        let (mut manager, _receiver) = TransferManager::channel();
        let id = manager
            .submit(TransferOperation::Download {
                remote: RemotePath::from_bytes(b"/file".to_vec()).unwrap(),
                local: PathBuf::from("unused"),
                policy: ConflictPolicy::Ask,
            })
            .unwrap();
        manager.cancel(id);
        assert_eq!(manager.poll()[0].outcome, Some(TransferOutcome::Cancelled));
    }
}

/// Map one remote basename beneath a chosen local directory without interpreting it as a path.
pub fn local_download_path(
    directory: &std::path::Path,
    remote: &RemotePath,
) -> Result<PathBuf, String> {
    operations::download_child(directory, remote)
}
#[cfg(test)]
mod review_tests {
    use super::*;
    #[test]
    fn submitted_commits_have_authoritative_or_indeterminate_outcomes() {
        let (mut manager, mut receiver) = TransferManager::channel();
        let id = manager
            .submit(TransferOperation::Mkdir {
                remote: RemotePath::from_bytes(b"/commit-target".to_vec()).unwrap(),
            })
            .unwrap();
        let job = receiver.try_recv().unwrap();
        assert!(job.begin_commit());
        manager.cancel(id);
        assert!(
            !*job.cancelled.borrow(),
            "An in-flight commit cannot be reported as cancelled"
        );
        assert!(manager.poll()[0].committing);
        manager.disconnect();
        assert!(matches!(
            manager.poll()[0].outcome,
            Some(TransferOutcome::Indeterminate(_))
        ));
        let (mut manager, mut receiver) = TransferManager::channel();
        let id = manager
            .submit(TransferOperation::Mkdir {
                remote: RemotePath::from_bytes(b"/cancelled-target".to_vec()).unwrap(),
            })
            .unwrap();
        let job = receiver.try_recv().unwrap();
        manager.cancel(id);
        assert!(
            !job.begin_commit(),
            "Cancellation before commit still prevents publication"
        );
    }
    #[test]
    fn bulk_cancellation_waits_for_an_authoritative_entry_and_stops_the_next() {
        let (mut manager, mut receiver) = TransferManager::channel();
        let id = manager
            .submit(TransferOperation::Mkdir {
                remote: RemotePath::from_bytes(b"/bulk".to_vec()).unwrap(),
            })
            .unwrap();
        let job = receiver.try_recv().unwrap();
        assert!(job.begin_step());
        manager.cancel(id);
        assert!(!*job.cancelled.borrow());
        assert!(!job.end_step(false));
        assert!(!job.begin_step());
        assert_eq!(manager.poll()[0].completed_entries, 1);
    }
    #[test]
    fn equal_sized_jobs_keep_distinct_immutable_operation_summaries() {
        let (mut manager, mut receiver) = TransferManager::channel();
        for name in ["source-a", "source-b"] {
            manager
                .submit(TransferOperation::Upload {
                    local: PathBuf::from(name),
                    remote: RemotePath::from_bytes(format!("/remote/{name}").into_bytes()).unwrap(),
                    policy: ConflictPolicy::Ask,
                })
                .unwrap();
        }
        let mut job = receiver.try_recv().unwrap();
        job.operation = TransferOperation::Mkdir {
            remote: RemotePath::from_bytes(b"/temporary-child".to_vec()).unwrap(),
        };
        job.report(0, None, None);
        let events = manager.poll();
        assert!(events[0].summary.contains("source-a"));
        assert!(events[1].summary.contains("source-b"));
        assert!(events[0].summary.contains("Upload"));
        assert_ne!(events[0].summary, events[1].summary);
    }
}

#[cfg(any(unix, windows))]
mod folder_archive;
