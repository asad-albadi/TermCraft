use super::*;
use crate::remote_files::editor::{
    self as document, Action, DocumentStatus, EditorRequest, MAX_DOCUMENT_BYTES, Metadata, Version,
};

/// Read literal bytes and compare handle metadata before/after. Symlinks and non-text files are refused.
async fn read(
    sftp: &Arc<RawSftpSession>,
    path: &RemotePath,
) -> Result<(Vec<u8>, FileAttributes), String> {
    let attrs = sftp.lstat(path.as_bytes()).await.map_err(failure)?.attrs;
    if !attrs.is_regular() || attrs.size.is_none_or(|n| n > MAX_DOCUMENT_BYTES as u64) {
        return Err(
            "The editor opens regular UTF-8 text files up to 2 MiB; use Download for other entries"
                .into(),
        );
    }
    let handle = sftp
        .open(path.as_bytes(), OpenFlags::READ, FileAttributes::default())
        .await
        .map_err(failure)?
        .handle;
    let guard = RemoteHandleGuard::new(sftp, handle.clone());
    let before = sftp.fstat(handle.clone()).await.map_err(failure)?.attrs;
    if !before.is_regular()
        || before.size != attrs.size
        || before.mtime != attrs.mtime
        || Metadata::from(&before) != Metadata::from(&attrs)
    {
        return Err("Remote file changed while opening; retry".into());
    }
    let mut bytes = Vec::new();
    loop {
        match sftp
            .read(handle.clone(), bytes.len() as u64, 32 * 1024)
            .await
        {
            Ok(data) => {
                if data.data.is_empty() {
                    return Err("Empty remote response before EOF".into());
                }
                if bytes.len() + data.data.len() > MAX_DOCUMENT_BYTES {
                    return Err("Remote text exceeds the 2 MiB editor limit".into());
                }
                bytes.extend(data.data);
            }
            Err(Error::Status(s)) if s.status_code == StatusCode::Eof => break,
            Err(e) => return Err(failure(e)),
        }
    }
    let after = sftp.fstat(handle).await.map_err(failure)?.attrs;
    guard.close().await?;
    if before.size != after.size
        || Metadata::from(&before) != Metadata::from(&after)
        || bytes.len() as u64 != after.size.unwrap_or(u64::MAX)
    {
        return Err("Remote file changed while reading; retry".into());
    }
    Ok((bytes, after))
}
pub(super) async fn run(
    request: &EditorRequest,
    sftp: &Arc<RawSftpSession>,
) -> Result<TransferOutcome, String> {
    let mut cancelled = request.cancelled.clone();
    if *cancelled.borrow() {
        return Ok(TransferOutcome::Cancelled);
    }
    let mut staging = None;
    let result = tokio::select! { biased;
        _ = cancelled.changed() => Ok(TransferOutcome::Cancelled),
        result = perform(request, sftp, &mut staging) => result,
    };
    if let Some(path) = staging {
        let cleanup =
            tokio::time::timeout(Duration::from_secs(2), sftp.remove(path.as_bytes())).await;
        match cleanup {
            Ok(Ok(_)) => {}
            Ok(Err(Error::Status(s))) if s.status_code == StatusCode::NoSuchFile => {}
            error => {
                return Err(format!(
                    "{result:?}. Owned remote staging could not be removed: {} ({error:?}). Local recovery is retained.",
                    path.display()
                ));
            }
        }
    }
    result
}
async fn perform(
    request: &EditorRequest,
    sftp: &Arc<RawSftpSession>,
    staging: &mut Option<RemotePath>,
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
            let (bytes, attrs) = read(sftp, &path).await?;
            let text = String::from_utf8(bytes.clone())
                .map_err(|_| "This file is not UTF-8 text; use Download")?;
            if text.contains('\0') {
                return Err("This file contains NUL bytes; use Download".into());
            }
            let root = root.clone();
            let (directory, recovery, text) = tokio::task::spawn_blocking(move || {
                let directory = document::recovery_directory(root)?;
                let recovery = document::persist(&directory, text.as_bytes())?;
                Ok::<_, String>((directory, recovery, text))
            })
            .await
            .map_err(failure)??;
            let mut s = request.state.lock().unwrap_or_else(|p| p.into_inner());
            if s.operation != request.token || *request.cancelled.borrow() {
                return Ok(TransferOutcome::Cancelled);
            }
            s.directory = Some(directory);
            s.baseline = bytes;
            s.metadata = Some(Metadata::from(&attrs));
            s.snapshot.text = text;
            s.snapshot.recovery_path = Some(recovery);
            s.snapshot.status = DocumentStatus::Ready;
        }
        Action::Save {
            bytes,
            expected,
            revision,
        } => {
            let (current, attrs) = read(sftp, &path).await?;
            let version = Version {
                bytes: current,
                metadata: Metadata::from(&attrs),
            };
            if expected.as_ref() != Some(&version) {
                return conflict(request, version);
            }
            if attrs.permissions.is_none_or(|p| p & 0o7000 != 0)
                || attrs.uid.is_none()
                || attrs.gid.is_none()
            {
                return Err("Atomic editing cannot safely preserve this file's special/unknown permission bits; no replacement was made. Use a permission-aware workflow.".into());
            }
            let partial = path.parent().child(&random_name()?)?;
            let handle = sftp
                .open(
                    partial.as_bytes(),
                    OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::EXCLUDE,
                    FileAttributes {
                        permissions: Some(0o600),
                        ..Default::default()
                    },
                )
                .await
                .map_err(failure)?
                .handle;
            *staging = Some(partial.clone());
            let guard = RemoteHandleGuard::new(sftp, handle.clone());
            for (i, chunk) in bytes.chunks(32 * 1024).enumerate() {
                sftp.write(handle.clone(), (i * 32 * 1024) as u64, chunk.to_vec())
                    .await
                    .map_err(failure)?;
            }
            let staged = sftp.fstat(handle.clone()).await.map_err(failure)?.attrs;
            // Preserve owner/group explicitly when the server created the sibling with different identity.
            if staged.uid != attrs.uid || staged.gid != attrs.gid {
                if attrs.uid.is_none() || attrs.gid.is_none() {
                    return Err("Server omitted ownership needed for a safe atomic edit".into());
                }
                sftp.fsetstat(
                    handle.clone(),
                    FileAttributes {
                        uid: attrs.uid,
                        gid: attrs.gid,
                        ..Default::default()
                    },
                )
                .await
                .map_err(|e| {
                    format!("Cannot preserve file owner/group; remote original retained: {e}")
                })?;
            }
            sftp.fsetstat(
                handle.clone(),
                FileAttributes {
                    permissions: attrs.permissions.map(|p| p & 0o777),
                    ..Default::default()
                },
            )
            .await
            .map_err(failure)?;
            let committed_attrs = sftp.fstat(handle).await.map_err(failure)?.attrs;
            if committed_attrs.uid != attrs.uid
                || committed_attrs.gid != attrs.gid
                || committed_attrs.permissions.map(|p| p & 0o7777)
                    != attrs.permissions.map(|p| p & 0o7777)
            {
                return Err(
                    "Server could not preserve security metadata; original file was not replaced"
                        .into(),
                );
            }
            guard.close().await?;
            let (latest, latest_attrs) = read(sftp, &path).await.map_err(|e| {
                format!("Final remote verification failed; original file was not replaced: {e}")
            })?;
            let latest = Version {
                bytes: latest,
                metadata: Metadata::from(&latest_attrs),
            };
            if latest != version {
                return conflict(request, latest);
            }
            // SFTP v3 cannot make the validation and rename one compare-and-swap transaction.
            if !request.enter_commit() {
                return Ok(TransferOutcome::Cancelled);
            }
            match sftp.posix_rename(partial.as_bytes(), path.as_bytes()).await {
                Ok(_) => {}
                Err(Error::Status(status)) => {
                    return Err(format!(
                        "Atomic save failed; local recovery retained: {status:?}"
                    ));
                }
                Err(error) => {
                    return Ok(TransferOutcome::Indeterminate(format!(
                        "Remote save acknowledgement was lost: {error}. Compare the remote version before retrying."
                    )));
                }
            }
            *staging = None;
            let mut s = request.state.lock().unwrap_or_else(|p| p.into_inner());
            if s.operation != request.token || *request.cancelled.borrow() {
                return Ok(TransferOutcome::Cancelled);
            }
            s.baseline = bytes.clone();
            s.metadata = Some(Metadata::from(&committed_attrs));
            s.conflict = None;
            s.snapshot.dirty = s.revision != *revision && s.snapshot.text.as_bytes() != bytes;
            s.snapshot.status = DocumentStatus::Saved;
        }
    }
    Ok(TransferOutcome::Completed)
}
fn conflict(request: &EditorRequest, version: Version) -> Result<TransferOutcome, String> {
    let mut state = request.state.lock().unwrap_or_else(|p| p.into_inner());
    if state.operation == request.token {
        state.conflict = Some(version);
    }
    Ok(TransferOutcome::Conflict)
}
