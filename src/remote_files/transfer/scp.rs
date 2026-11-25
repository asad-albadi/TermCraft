use super::*;
use tokio::io::AsyncWriteExt;
pub(crate) async fn run<H: russh::client::Handler + 'static>(
    job: Job,
    backend: super::super::scp::ScpBackend<H>,
) {
    {
        job.state.lock().unwrap_or_else(|p| p.into_inner()).started = true;
    }
    if let TransferOperation::Document(request) = &job.operation {
        let result = backend.editor(&job, request).await;
        job.finish(result.unwrap_or_else(TransferOutcome::Failed));
        return;
    }
    if *job.cancelled.borrow() {
        job.finish(TransferOutcome::Cancelled);
        return;
    }
    let mut cancelled = job.cancelled.clone();
    let result = tokio::select! { biased;
        _ = cancelled.changed() => Ok(TransferOutcome::Cancelled),
        result = copy(&job, &backend) => result,
    };
    job.finish(result.unwrap_or_else(TransferOutcome::Failed));
}
async fn copy<H: russh::client::Handler + 'static>(
    job: &Job,
    backend: &super::super::scp::ScpBackend<H>,
) -> Result<TransferOutcome, String> {
    match &job.operation {
        TransferOperation::RemoveTree { remote, confirmed } => {
            operations::scp_confirmed_target(remote, confirmed)?;
            backend.remove_tree(job, remote).await
        }
        TransferOperation::Mkdir { remote } => backend.control(job, "mkdir", remote, None, ConflictPolicy::Ask).await,
        TransferOperation::Rename { source, destination, policy } => {
            if source == destination { return Ok(TransferOutcome::Skipped); }
            backend.control(job, "rename", source, Some(destination), *policy).await
        }
        TransferOperation::Remove { remote, confirmed } => {
            operations::scp_confirmed_target(remote, confirmed)?;
            backend.control(job, "remove", remote, None, ConflictPolicy::Ask).await
        }
        TransferOperation::Upload { local, remote, policy } => {
            #[cfg(any(unix, windows))]
            if tokio::fs::symlink_metadata(local).await.map_err(|e| e.to_string())?.is_dir() {
                let archive = super::folder_archive::build(local.clone(), job.cancelled.clone()).await?;
                return backend.upload_tree(job, archive, remote.clone(), *policy).await;
            }
            let (file, mode) = operations::scp_source(local.clone()).await?;
            backend.upload(job, file, remote.clone(), mode, *policy).await
        }
        TransferOperation::Download { local, remote, policy } => {
            let existing = match tokio::fs::symlink_metadata(local).await {
                Ok(meta) => Some(meta),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => return Err(e.to_string()),
            };
            if existing.is_some() && *policy != ConflictPolicy::Overwrite {
                return Ok(if *policy == ConflictPolicy::Skip { TransferOutcome::Skipped } else { TransferOutcome::Conflict });
            }
            let prepared = backend.prepare_download(remote.clone()).await?;
            if prepared.folder {
                #[cfg(any(unix, windows))] { return download_tree(job, backend, prepared, local.clone(), *policy, existing).await; }
                #[cfg(not(any(unix, windows)))] { return Err("Anchored SCP folder downloads are not available on this local platform".into()); }
            }
            if existing.as_ref().is_some_and(|m| m.is_dir()) { return Err("SCP file download cannot replace a directory".into()); }
            let (staging, mut file) = super::local_stage::LocalStage::create(local.clone()).await?;
            job.report(0, Some(RecoveryPath::Local(staging.initial_path().to_owned())), None);
            let mode = backend.download(job, prepared, &mut file).await?;
            #[cfg(unix)] { use std::os::unix::fs::PermissionsExt;
                let mode = existing.as_ref().filter(|m| m.is_file()).map(|m| m.permissions().mode() & 0o777).unwrap_or(mode);
                file.set_permissions(std::fs::Permissions::from_mode(mode)).await.map_err(|e| e.to_string())?;
            }
            #[cfg(not(unix))] let _ = mode;
            file.flush().await.map_err(|e| e.to_string())?;
            file.sync_all().await.map_err(|e| e.to_string())?;
            let file = file.into_std().await;
            tokio::task::spawn_blocking(move || operations::close_recovery_file(file).map_err(|e| e.to_string())).await.map_err(|e| e.to_string())??;
            if !job.begin_commit() { return Ok(TransferOutcome::Cancelled); }
            let result = staging.commit(*policy == ConflictPolicy::Overwrite).await;
            let copy = staging.clone();
            let recovery = tokio::task::spawn_blocking(move || copy.recovery_path()).await.map_err(|e| e.to_string())?;
            match result {
                Ok(true) => { job.recovery(None); Ok(TransferOutcome::Completed) }
                Ok(false) => { job.recovery(recovery.ok().map(RecoveryPath::Local)); Ok(if *policy == ConflictPolicy::Skip { TransferOutcome::Skipped } else { TransferOutcome::Conflict }) }
                Err(super::local_stage::CommitError::Failed(message)) => { job.recovery(recovery.ok().map(RecoveryPath::Local)); Err(message) }
                Err(super::local_stage::CommitError::Indeterminate(message)) => { job.recovery(recovery.ok().map(RecoveryPath::Local)); Ok(TransferOutcome::Indeterminate(message)) }
            }
        }
        _ => Err("This SCP compatibility backend currently supports directory enumeration and regular file transfers. Folder copies, entry mutations and remote editing require SFTP.".into()),
    }
}

#[cfg(any(unix, windows))]
async fn download_tree<H: russh::client::Handler + 'static>(
    job: &Job,
    backend: &super::super::scp::ScpBackend<H>,
    prepared: super::super::scp::PreparedDownload,
    local: std::path::PathBuf,
    policy: ConflictPolicy,
    existing: Option<std::fs::Metadata>,
) -> Result<TransferOutcome, String> {
    if existing.as_ref().is_some_and(|m| !m.is_dir()) {
        return Err("A folder cannot overwrite a non-directory or a link".into());
    }
    let (mut stage, root, mut archive) =
        super::local_stage::LocalStage::create_tree(local.clone()).await?;
    let target = if let Some(existing) = existing {
        Some(stage.attach_merge_target(existing).await?)
    } else {
        None
    };
    let _recovery = TreeRecovery {
        stage: stage.clone(),
        job: job.clone(),
    };
    job.recovery(Some(RecoveryPath::Local(
        stage.initial_path().parent().unwrap().to_owned(),
    )));
    backend.download(job, prepared, &mut archive).await?;
    archive.flush().await.map_err(|e| e.to_string())?;
    archive.sync_all().await.map_err(|e| e.to_string())?;
    let file = archive.into_std().await;
    let modes = super::folder_archive::extract(
        file,
        root.try_clone().map_err(|e| e.to_string())?,
        job.cancelled.clone(),
    )
    .await?;
    if let Some(target) = target {
        let outcome =
            operations::merge::scp_local(job, local, root, target, stage.clone(), modes).await?;
        if outcome != TransferOutcome::Completed {
            return Ok(outcome);
        }
        if !job.begin_step() {
            return Ok(TransferOutcome::Cancelled);
        }
        let cleanup = stage.clean_merged_tree().await;
        job.end_step(true);
        return match cleanup {
            Ok(()) => {
                job.recovery(None);
                Ok(TransferOutcome::Completed)
            }
            Err(message) => Ok(TransferOutcome::CompletedWithWarning(format!(
                "Folder merge completed; private staging retained: {message}"
            ))),
        };
    }
    super::folder_archive::finalize(root, modes).await?;
    if !job.begin_commit() {
        return Ok(TransferOutcome::Cancelled);
    }
    match stage.commit(policy == ConflictPolicy::Overwrite).await {
        Ok(true) => {
            job.recovery(None);
            match stage.clean_tree_container().await {
                Ok(()) => Ok(TransferOutcome::Completed),
                Err(message) => Ok(TransferOutcome::CompletedWithWarning(format!(
                    "Folder published; private container cleanup failed: {message}"
                ))),
            }
        }
        Ok(false) => Ok(if policy == ConflictPolicy::Skip {
            TransferOutcome::Skipped
        } else {
            TransferOutcome::Conflict
        }),
        Err(super::local_stage::CommitError::Failed(message)) => Err(message),
        Err(super::local_stage::CommitError::Indeterminate(message)) => {
            Ok(TransferOutcome::Indeterminate(message))
        }
    }
}

#[cfg(any(unix, windows))]
struct TreeRecovery {
    stage: super::local_stage::LocalStage,
    job: Job,
}
#[cfg(any(unix, windows))]
impl Drop for TreeRecovery {
    fn drop(&mut self) {
        if self.job.recovery_path().is_some() {
            self.job
                .recovery(self.stage.recovery_path().ok().map(RecoveryPath::Local));
        }
    }
}
