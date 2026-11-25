//! Fully copied files merge atomically one at a time; unrelated entries are preserved.
use super::*;
use source::{SourceDir, SourceEntry, SourceNode};
use tree::{Budget, os_bytes, safe_link};
struct LocalFrame {
    source: SourceDir,
    target: SourceDir,
    entries: std::vec::IntoIter<SourceEntry>,
    local: PathBuf,
    remote: RemotePath,
    depth: usize,
    mode: Option<u32>,
    name: Option<std::ffi::OsString>,
}
pub(super) async fn local(
    job: &mut Job,
    stage: PathBuf,
    target: PathBuf,
) -> Result<TransferOutcome, String> {
    let from = SourceDir::open(stage.clone()).await?;
    let to = SourceDir::open(target.clone()).await?;
    local_handles(job, target, from, to, None, None).await
}
#[cfg(any(unix, windows))]
pub(in crate::remote_files::transfer) async fn scp_local(
    job: &Job,
    target: PathBuf,
    from: std::fs::File,
    to: std::fs::File,
    stage: super::super::local_stage::LocalStage,
    modes: super::super::folder_archive::FolderModes,
) -> Result<TransferOutcome, String> {
    local_handles(
        job,
        target,
        SourceDir::from_file(from),
        SourceDir::from_file(to),
        Some(stage),
        Some(modes.directories),
    )
    .await
}
async fn local_handles(
    job: &Job,
    target: PathBuf,
    from: SourceDir,
    to: SourceDir,
    guard: Option<super::super::local_stage::LocalStage>,
    modes: Option<std::collections::HashMap<Vec<u8>, u32>>,
) -> Result<TransferOutcome, String> {
    from.set_mode(from.mode().await? | 0o700).await?;
    let entries = from.entries().await?;
    let root = RemotePath::from_bytes(b"/".to_vec()).unwrap();
    let mut budget = Budget::default();
    let mut stack = vec![LocalFrame {
        source: from,
        target: to,
        entries: entries.into_iter(),
        local: target,
        remote: root,
        depth: 0,
        mode: None,
        name: None,
    }];
    while let Some(frame) = stack.last_mut() {
        if *job.cancelled.borrow() {
            return Ok(TransferOutcome::Cancelled);
        }
        if let Some(entry) = frame.entries.next() {
            if let Some(stage) = &guard {
                stage.validate_merge_root().await?;
            }
            let source_path = frame.remote.child(&os_bytes(&entry.name)?)?;
            let local_path = frame.local.join(&entry.name);
            budget.add(&local_path, &source_path, frame.depth + 1)?;
            match frame.source.open_child(entry.clone()).await? {
                SourceNode::Directory(child) => {
                    let mode = modes
                        .as_ref()
                        .and_then(|m| {
                            m.get(
                                source_path
                                    .as_bytes()
                                    .strip_prefix(b"/")
                                    .unwrap_or(source_path.as_bytes()),
                            )
                        })
                        .copied()
                        .unwrap_or(child.mode().await?);
                    child.set_mode(mode | 0o700).await?;
                    let entries = child.entries().await?;
                    if guard.is_some() && !job.begin_step() {
                        return Ok(TransferOutcome::Cancelled);
                    }
                    let (target, created) =
                        frame.target.directory_child(entry.name.clone()).await?;
                    if guard.is_some() && !job.end_step(false) {
                        return Ok(TransferOutcome::Cancelled);
                    }
                    let depth = frame.depth + 1;
                    stack.push(LocalFrame {
                        source: child,
                        target,
                        entries: entries.into_iter(),
                        local: local_path,
                        remote: source_path,
                        depth,
                        mode: created.then_some(mode),
                        name: Some(entry.name),
                    });
                }
                SourceNode::File { file, .. } => {
                    #[cfg(unix)]
                    if let Some(mode) = frame.target.file_mode(entry.name.clone()).await? {
                        use std::os::unix::fs::PermissionsExt;
                        file.set_permissions(std::fs::Permissions::from_mode(mode))
                            .await
                            .map_err(failure)?;
                        file.sync_all().await.map_err(failure)?;
                    }
                    let file = file.into_std().await;
                    tokio::task::spawn_blocking(move || commit::close_file(file).map_err(failure))
                        .await
                        .map_err(failure)??;
                    if guard.is_some() && !job.begin_step() {
                        return Ok(TransferOutcome::Cancelled);
                    }
                    frame.target.rename_from(&frame.source, entry.name).await?;
                    if guard.is_some() {
                        if !job.end_step(false) {
                            return Ok(TransferOutcome::Cancelled);
                        }
                    } else {
                        job.entry_completed();
                    }
                }
                SourceNode::Symlink(link) => {
                    safe_link(&link, frame.depth)?;
                    frame.target.validate_link(link).await?;
                    if guard.is_some() && !job.begin_step() {
                        return Ok(TransferOutcome::Cancelled);
                    }
                    frame.target.rename_from(&frame.source, entry.name).await?;
                    if guard.is_some() {
                        if !job.end_step(false) {
                            return Ok(TransferOutcome::Cancelled);
                        }
                    } else {
                        job.entry_completed();
                    }
                }
            }
        } else {
            let frame = stack.pop().unwrap();
            if guard.is_some() && !job.begin_step() {
                return Ok(TransferOutcome::Cancelled);
            }
            if let Some(stage) = &guard {
                stage.validate_merge_root().await?;
            }
            if let Some(mode) = frame.mode {
                frame.target.set_mode(mode).await?;
            }
            let name = frame.name;
            drop(frame.source);
            if let Some(name) = name {
                stack.last().unwrap().source.remove_directory(name).await?;
            }
            if guard.is_some() && !job.end_step(false) {
                return Ok(TransferOutcome::Cancelled);
            }
        }
    }
    Ok(TransferOutcome::Completed)
}
async fn remote_directory(sftp: &Arc<RawSftpSession>, path: &RemotePath) -> Result<bool, String> {
    match sftp.lstat(path.as_bytes()).await{Ok(attrs)if attrs.attrs.is_dir()=>Ok(false),Ok(_)=>Err("A destination directory is a symlink or non-directory; merge stopped without following it".into()),Err(Error::Status(s))if s.status_code==StatusCode::NoSuchFile=>{sftp.mkdir(path.as_bytes(),FileAttributes{permissions:Some(0o700),..Default::default()}).await.map_err(failure)?;Ok(true)},Err(e)=>Err(failure(e))}
}
pub(super) async fn entries(
    sftp: &Arc<RawSftpSession>,
    source: &RemotePath,
    target: &RemotePath,
    budget: &mut Budget,
    depth: usize,
) -> Result<Vec<RemotePath>, String> {
    let handle = sftp
        .opendir(source.as_bytes())
        .await
        .map_err(failure)?
        .handle;
    let guard = RemoteHandleGuard::new(sftp, handle.clone());
    let result = async {
        let mut entries = Vec::new();
        loop {
            match sftp.readdir(handle.clone()).await {
                Ok(batch) => {
                    for entry in batch.files {
                        let name = entry.filename.as_bytes();
                        if name == b"." || name == b".." {
                            continue;
                        }
                        let from = source.child(name)?;
                        let to = target.child(name)?;
                        budget.add(std::path::Path::new(&to.display()), &from, depth)?;
                        entries.push(from);
                    }
                }
                Err(Error::Status(s)) if s.status_code == StatusCode::Eof => return Ok(entries),
                Err(e) => return Err(failure(e)),
            }
        }
    }
    .await;
    let close = guard.close().await;
    let result = result?;
    close?;
    Ok(result)
}
async fn validate_remote_link(
    sftp: &Arc<RawSftpSession>,
    parent: &RemotePath,
    link: &[u8],
    depth: usize,
) -> Result<(), String> {
    safe_link(link, depth)?;
    let mut path = parent.clone();
    for name in link
        .split(|b| *b == b'/')
        .filter(|p| !p.is_empty() && *p != b".")
    {
        path = path.child(name)?;
        match sftp.lstat(path.as_bytes()).await {
            Ok(attrs) if attrs.attrs.is_symlink() => {
                return Err("Cannot merge a link through another destination symlink".into());
            }
            Ok(_) => {}
            Err(Error::Status(s)) if s.status_code == StatusCode::NoSuchFile => return Ok(()),
            Err(e) => return Err(failure(e)),
        }
    }
    Ok(())
}
struct RemoteFrame {
    source: RemotePath,
    target: RemotePath,
    entries: std::vec::IntoIter<RemotePath>,
    depth: usize,
    mode: Option<u32>,
}
pub(super) async fn remote(
    job: &mut Job,
    sftp: &Arc<RawSftpSession>,
    stage: RemotePath,
    target: RemotePath,
) -> Result<TransferOutcome, String> {
    remote_directory(sftp, &target).await?;
    let mut budget = Budget::default();
    let stage_mode = sftp
        .lstat(stage.as_bytes())
        .await
        .map_err(failure)?
        .attrs
        .permissions
        .unwrap_or(0o700)
        & 0o777;
    sftp.setstat(
        stage.as_bytes(),
        FileAttributes {
            permissions: Some(stage_mode | 0o700),
            ..Default::default()
        },
    )
    .await
    .map_err(failure)?;
    let list = entries(sftp, &stage, &target, &mut budget, 0).await?;
    let mut stack = vec![RemoteFrame {
        source: stage,
        target,
        entries: list.into_iter(),
        depth: 0,
        mode: None,
    }];
    while let Some(frame) = stack.last_mut() {
        if *job.cancelled.borrow() {
            return Ok(TransferOutcome::Cancelled);
        }
        if let Some(source) = frame.entries.next() {
            let name = source.as_bytes().rsplit(|b| *b == b'/').next().unwrap();
            let target = frame.target.child(name)?;
            let attrs = sftp.lstat(source.as_bytes()).await.map_err(failure)?.attrs;
            if attrs.is_dir() {
                let created = remote_directory(sftp, &target).await?;
                let depth = frame.depth + 1;
                let original_mode = attrs.permissions.unwrap_or(0o755) & 0o777;
                sftp.setstat(
                    source.as_bytes(),
                    FileAttributes {
                        permissions: Some(original_mode | 0o700),
                        ..Default::default()
                    },
                )
                .await
                .map_err(failure)?;
                let list = entries(sftp, &source, &target, &mut budget, depth).await?;
                stack.push(RemoteFrame {
                    source,
                    target,
                    entries: list.into_iter(),
                    depth,
                    mode: created.then_some(attrs.permissions.unwrap_or(0o755) & 0o777),
                });
            } else {
                if attrs.is_symlink() {
                    let link = sftp.readlink(source.as_bytes()).await.map_err(failure)?;
                    if link.files.len() != 1 {
                        return Err("Invalid staged symlink".into());
                    }
                    validate_remote_link(
                        sftp,
                        &frame.target,
                        link.files[0].filename.as_bytes(),
                        frame.depth,
                    )
                    .await?;
                } else if attrs.is_regular() {
                    match sftp.lstat(target.as_bytes()).await {
                        Ok(previous) if previous.attrs.is_regular() => {
                            if let Some(mode) = previous.attrs.permissions {
                                sftp.setstat(
                                    source.as_bytes(),
                                    FileAttributes {
                                        permissions: Some(mode & 0o777),
                                        ..Default::default()
                                    },
                                )
                                .await
                                .map_err(failure)?;
                            }
                        }
                        Ok(_) => {}
                        Err(Error::Status(s)) if s.status_code == StatusCode::NoSuchFile => {}
                        Err(e) => return Err(failure(e)),
                    }
                } else {
                    return Err("Unsupported staged entry in folder merge".into());
                }
                sftp.posix_rename(source.as_bytes(), target.as_bytes())
                    .await
                    .map_err(|e| {
                        format!(
                            "Merge commit not acknowledged; refresh destination to confirm: {e}"
                        )
                    })?;
                job.entry_completed();
            }
        } else {
            let frame = stack.pop().unwrap();
            if let Some(mode) = frame.mode {
                sftp.setstat(
                    frame.target.as_bytes(),
                    FileAttributes {
                        permissions: Some(mode),
                        ..Default::default()
                    },
                )
                .await
                .map_err(failure)?;
            }
            if !stack.is_empty() {
                sftp.rmdir(frame.source.as_bytes()).await.map_err(failure)?;
            }
        }
    }
    Ok(TransferOutcome::Completed)
}
