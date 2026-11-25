use super::{FtpBackend, FtpTransferType};
use crate::remote_files::{
    EntryKind, RemoteEntry, RemotePath,
    editor::{self, Action, DocumentStatus, EditorRequest, MAX_DOCUMENT_BYTES, Metadata, Version},
    transfer::{
        ConflictPolicy, Job, RecoveryPath, TransferOperation, TransferOutcome,
        local_stage::{CommitError, LocalStage},
    },
};
use std::{
    path::Path,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, ReadBuf};

mod tree;

const REMOTE_WARNING: &str = "FTP uses a separate destination check and rename. The server controls replacement and file permissions.";

pub(super) async fn run(backend: &mut FtpBackend, transfer_type: FtpTransferType, job: Job) {
    job.mark_started();
    let mut cancelled = match &job.operation {
        TransferOperation::Document(request) => request.cancelled.clone(),
        _ => job.cancelled.clone(),
    };
    let result = if matches!(
        job.operation,
        TransferOperation::Download { .. }
            | TransferOperation::Upload { .. }
            | TransferOperation::UploadFolder { .. }
            | TransferOperation::DownloadFolder { .. }
            | TransferOperation::RemoveTree { .. }
    ) {
        perform(backend, transfer_type, &job).await
    } else {
        tokio::select! {
        biased;
        _ = cancelled.wait_for(|cancelled| *cancelled) => Ok(TransferOutcome::Cancelled),
        result = perform(backend, transfer_type, &job) => result,
        }
    };
    job.finish(result.unwrap_or_else(TransferOutcome::Failed));
}

async fn perform(
    backend: &mut FtpBackend,
    transfer_type: FtpTransferType,
    job: &Job,
) -> Result<TransferOutcome, String> {
    match &job.operation {
        TransferOperation::Mkdir { remote } => {
            if !job.begin_commit() {
                return Ok(TransferOutcome::Cancelled);
            }
            let result = backend.mkdir(remote).await;
            mutation(backend, result)
        }
        TransferOperation::Remove { remote, confirmed } => {
            if remote != confirmed {
                return Err("Confirm the exact FTP path before deleting it.".into());
            }
            let entry = entry(backend, remote)
                .await?
                .ok_or("The selected FTP entry is gone.")?;
            if !job.begin_commit() {
                return Ok(TransferOutcome::Cancelled);
            }
            let result = backend.remove(remote, entry.kind).await;
            mutation(backend, result)
        }
        TransferOperation::Rename {
            source,
            destination,
            policy,
        } => {
            if source == destination {
                return Err("Choose a different destination name.".into());
            }
            if let Some(outcome) = remote_conflict(backend, destination, *policy).await? {
                return Ok(outcome);
            }
            publish(backend, source, destination, job).await
        }
        TransferOperation::Upload {
            local,
            remote,
            policy,
        } => {
            if *job.cancelled.borrow() {
                return Ok(TransferOutcome::Cancelled);
            }
            let mut options = tokio::fs::OpenOptions::new();
            options.read(true);
            #[cfg(unix)]
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
            #[cfg(windows)]
            options.custom_flags(
                windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT,
            );
            let file = options.open(local).await.map_err(io)?;
            let metadata = file.metadata().await.map_err(io)?;
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
                if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                    return Err(
                        "Choose a local file directly. FTP uploads cannot follow reparse points."
                            .into(),
                    );
                }
            }
            if !metadata.is_file() {
                return Err("Choose a regular local file to upload.".into());
            }
            if let Some(outcome) = remote_conflict(backend, remote, *policy).await? {
                return Ok(outcome);
            }
            let (directory, stage) = remote_stage(backend, remote, job).await?;
            let mut cancelled = job.cancelled.clone();
            let mut reader = Progress::new(file, job);
            tokio::select! {
                biased;
                _ = cancelled.wait_for(|value| *value) => return Ok(TransferOutcome::Cancelled),
                result = backend.write_from(&stage, transfer_type, &mut reader) => { result?; }
            }

            if let Some(outcome) = remote_conflict(backend, remote, *policy).await? {
                return Ok(outcome);
            }
            let outcome = publish(backend, &stage, remote, job).await?;
            Ok(cleanup_published(backend, directory, job, outcome).await)
        }
        TransferOperation::Download {
            remote,
            local,
            policy,
        } => {
            let mut cancelled = job.cancelled.clone();
            if *cancelled.borrow() {
                return Ok(TransferOutcome::Cancelled);
            }
            let selected = entry(backend, remote)
                .await?
                .ok_or("The selected FTP entry is gone.")?;
            if selected.kind != EntryKind::File {
                return Err(
                    "The selected FTP file changed type. Select the current entry before retrying."
                        .into(),
                );
            }
            if let Some(outcome) = local_conflict(local, *policy).await? {
                return Ok(outcome);
            }
            let expected_size = selected.size;
            let (stage, writer) = LocalStage::create(local.clone()).await?;
            job.recovery(Some(RecoveryPath::Local(stage.initial_path().to_owned())));
            let mut writer = Progress::new(writer, job);
            let read = tokio::select! {
                biased;
                _ = cancelled.wait_for(|cancelled| *cancelled) => None,
                result = backend.read_to(remote, transfer_type, &mut writer, u64::MAX) => Some(result),
            };
            job.recovery(stage.recovery_path().ok().map(RecoveryPath::Local));
            let Some(read) = read else {
                return Ok(TransferOutcome::Cancelled);
            };
            let received = read?;
            writer.inner.flush().await.map_err(io)?;
            writer.inner.sync_all().await.map_err(io)?;
            drop(writer);
            if transfer_type == FtpTransferType::Binary
                && let Some(expected) = expected_size
                && expected != received
            {
                return Err(format!(
                    "FTP listed {expected} bytes but sent {received}. The staged download was retained."
                ));
            }
            if !job.begin_commit() {
                return Ok(TransferOutcome::Cancelled);
            }
            match stage.commit(*policy == ConflictPolicy::Overwrite).await {
                Ok(true) => {
                    job.recovery(None);
                    Ok(TransferOutcome::Completed)
                }
                Ok(false) => Ok(conflict(*policy)),
                Err(CommitError::Failed(message)) => Err(message),
                Err(CommitError::Indeterminate(message)) => {
                    Ok(TransferOutcome::Indeterminate(message))
                }
            }
        }
        TransferOperation::UploadFolder {
            local,
            remote,
            policy,
        } => {
            if *job.cancelled.borrow() {
                return Ok(TransferOutcome::Cancelled);
            }
            tree::upload(backend, transfer_type, job, local, remote, *policy).await
        }
        TransferOperation::DownloadFolder {
            remote,
            local,
            policy,
        } => {
            if *job.cancelled.borrow() {
                return Ok(TransferOutcome::Cancelled);
            }
            if entry(backend, remote)
                .await?
                .is_none_or(|selected| selected.kind != EntryKind::Directory)
            {
                return Err("The selected FTP folder disappeared or changed type. Select the current entry before retrying.".into());
            }
            tree::download(backend, transfer_type, job, remote, local, *policy).await
        }
        TransferOperation::Document(request) => document(backend, request, job).await,
        TransferOperation::RemoveTree { remote, confirmed } => {
            tree::remove(backend, remote, confirmed, job).await
        }
    }
}

fn io(error: std::io::Error) -> String {
    error.to_string()
}

async fn entry(backend: &mut FtpBackend, path: &RemotePath) -> Result<Option<RemoteEntry>, String> {
    if !path.as_bytes().starts_with(b"/") || path.parent() == *path {
        return Err("Choose an absolute FTP file or directory path below the root.".into());
    }
    Ok(backend
        .list(&path.parent())
        .await?
        .into_iter()
        .find(|entry| entry.path == *path))
}

async fn regular_entry(backend: &mut FtpBackend, path: &RemotePath) -> Result<RemoteEntry, String> {
    entry(backend, path)
        .await?
        .filter(|entry| entry.kind == EntryKind::File)
        .ok_or_else(|| "Choose a regular FTP file from the directory listing.".into())
}

fn conflict(policy: ConflictPolicy) -> TransferOutcome {
    if policy == ConflictPolicy::Skip {
        TransferOutcome::Skipped
    } else {
        TransferOutcome::Conflict
    }
}

async fn remote_conflict(
    backend: &mut FtpBackend,
    path: &RemotePath,
    policy: ConflictPolicy,
) -> Result<Option<TransferOutcome>, String> {
    Ok(entry(backend, path).await?.and_then(|existing| {
        (policy != ConflictPolicy::Overwrite || existing.kind != EntryKind::File)
            .then(|| conflict(policy))
    }))
}

async fn local_conflict(
    path: &Path,
    policy: ConflictPolicy,
) -> Result<Option<TransferOutcome>, String> {
    match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) if policy == ConflictPolicy::Overwrite && metadata.is_file() => Ok(None),
        Ok(_) => Ok(Some(conflict(policy))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io(error)),
    }
}

async fn remote_stage(
    backend: &mut FtpBackend,
    destination: &RemotePath,
    job: &Job,
) -> Result<(RemotePath, RemotePath), String> {
    let mut entropy = [0u8; 16];
    getrandom::fill(&mut entropy).map_err(|error| error.to_string())?;
    let suffix: String = entropy.iter().map(|byte| format!("{byte:02x}")).collect();
    let directory = destination
        .parent()
        .child(format!(".termcraft-{suffix}").as_bytes())?;
    let stage = directory.child(b"payload.part")?;
    // MKD reserves the staging name before STOR opens its payload.
    job.recovery(Some(RecoveryPath::Remote(directory.clone())));
    if let Err(error) = backend.mkdir(&directory).await {
        if backend.is_connected() {
            job.recovery(None);
        }
        return Err(error);
    }
    Ok((directory, stage))
}

fn mutation(backend: &FtpBackend, result: Result<(), String>) -> Result<TransferOutcome, String> {
    match result {
        Ok(()) => Ok(TransferOutcome::Completed),
        Err(message) if !backend.is_connected() => Ok(TransferOutcome::Indeterminate(format!(
            "{message} The server response was lost. Check the remote directory before retrying."
        ))),
        Err(message) => Err(message),
    }
}

async fn publish(
    backend: &mut FtpBackend,
    source: &RemotePath,
    destination: &RemotePath,
    job: &Job,
) -> Result<TransferOutcome, String> {
    if !job.begin_commit() {
        return Ok(TransferOutcome::Cancelled);
    }
    let result = backend.rename(source, destination).await;
    match mutation(backend, result)? {
        TransferOutcome::Completed => {
            Ok(TransferOutcome::CompletedWithWarning(REMOTE_WARNING.into()))
        }
        outcome => Ok(outcome),
    }
}

async fn cleanup_published(
    backend: &mut FtpBackend,
    directory: RemotePath,
    job: &Job,
    outcome: TransferOutcome,
) -> TransferOutcome {
    if !matches!(outcome, TransferOutcome::CompletedWithWarning(_)) {
        return outcome;
    }
    match backend.remove(&directory, EntryKind::Directory).await {
        Ok(()) => {
            job.recovery(None);
            outcome
        }
        Err(error) => TransferOutcome::CompletedWithWarning(format!(
            "{REMOTE_WARNING} The staging folder could not be removed: {error}"
        )),
    }
}

async fn version(backend: &mut FtpBackend, path: &RemotePath) -> Result<Version, String> {
    let before = regular_entry(backend, path).await?;
    if before
        .size
        .is_some_and(|size| size > MAX_DOCUMENT_BYTES as u64)
    {
        return Err(
            "The FTP editor opens UTF-8 files up to 2 MiB. Use Download for larger files.".into(),
        );
    }
    let bytes = backend
        .read(path, FtpTransferType::Binary, MAX_DOCUMENT_BYTES)
        .await?;
    let after = regular_entry(backend, path).await?;
    if before.size != after.size || after.size.is_some_and(|size| size != bytes.len() as u64) {
        return Err(
            "The remote file changed while reading. Compare the latest file before retrying."
                .into(),
        );
    }
    Ok(Version {
        bytes,
        metadata: Metadata {
            permissions: None,
            uid: None,
            gid: None,
            mtime: None,
        },
    })
}

async fn document(
    backend: &mut FtpBackend,
    request: &EditorRequest,
    job: &Job,
) -> Result<TransferOutcome, String> {
    let path = request
        .state
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .snapshot
        .path
        .clone();
    match &request.action {
        Action::Open { root } => {
            let current = version(backend, &path).await?;
            editor::loaded(request, root.clone(), current.bytes, current.metadata).await
        }
        Action::Save {
            bytes,
            expected,
            revision,
        } => {
            let current = version(backend, &path).await?;
            if expected.as_ref() != Some(&current) {
                return Ok(document_conflict(request, current));
            }
            let (directory, stage) = remote_stage(backend, &path, job).await?;
            backend
                .write(&stage, FtpTransferType::Binary, bytes)
                .await?;
            let latest = version(backend, &path).await?;
            if latest != current {
                return Ok(document_conflict(request, latest));
            }
            let outcome = publish(backend, &stage, &path, job).await?;
            let outcome = cleanup_published(backend, directory, job, outcome).await;
            if let TransferOutcome::CompletedWithWarning(warning) = &outcome {
                let mut state = request.state.lock().unwrap_or_else(|p| p.into_inner());
                if state.operation == request.token {
                    state.baseline.clone_from(bytes);
                    state.metadata = Some(latest.metadata);
                    state.conflict = None;
                    state.snapshot.dirty =
                        state.revision != *revision && state.snapshot.text.as_bytes() != bytes;
                    state.snapshot.status = DocumentStatus::Saved;
                    state.snapshot.warning = Some(warning.clone());
                    state.snapshot.remote_recovery = None;
                }
            }
            Ok(outcome)
        }
    }
}

fn document_conflict(request: &EditorRequest, version: Version) -> TransferOutcome {
    let mut state = request.state.lock().unwrap_or_else(|p| p.into_inner());
    if state.operation == request.token {
        state.conflict = Some(version);
    }
    TransferOutcome::Conflict
}

struct Progress<'a, T> {
    inner: T,
    bytes: u64,
    job: &'a Job,
}
impl<'a, T> Progress<'a, T> {
    fn new(inner: T, job: &'a Job) -> Self {
        Self {
            inner,
            bytes: 0,
            job,
        }
    }
    fn transferred(&mut self, count: usize) {
        self.bytes += count as u64;
        self.job.advance(self.bytes);
    }
}
impl<T: AsyncRead + Unpin> AsyncRead for Progress<'_, T> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buffer.filled().len();
        let result = Pin::new(&mut self.inner).poll_read(cx, buffer);
        if let Poll::Ready(Ok(())) = result {
            self.transferred(buffer.filled().len() - before);
        }
        result
    }
}
impl<T: AsyncWrite + Unpin> AsyncWrite for Progress<'_, T> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let result = Pin::new(&mut self.inner).poll_write(cx, bytes);
        if let Poll::Ready(Ok(count)) = result {
            self.transferred(count);
        }
        result
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}
