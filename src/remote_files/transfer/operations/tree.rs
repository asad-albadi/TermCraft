use super::*;
use std::path::{Component, Path};
#[cfg(any(windows, test))]
fn windows_basename(name: &str) -> bool {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.ends_with(['.', ' '])
        || name
            .chars()
            .any(|c| c < ' ' || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'))
    {
        return false;
    }
    let stem = name
        .split('.')
        .next()
        .unwrap()
        .trim_end_matches(' ')
        .to_uppercase();
    if matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) {
        return false;
    }
    for prefix in ["COM", "LPT"] {
        if let Some(suffix) = stem.strip_prefix(prefix) {
            if suffix.chars().count() == 1
                && suffix
                    .chars()
                    .next()
                    .is_some_and(|c| matches!(c, '1'..='9' | '¹' | '²' | '³'))
            {
                return false;
            }
        }
    }
    true
}
pub(super) fn local_name(name: &[u8]) -> Result<std::ffi::OsString, String> {
    if name.contains(&0) {
        return Err("Remote filename contains NUL".into());
    }
    #[cfg(unix)]
    let name = {
        use std::os::unix::ffi::OsStringExt;
        std::ffi::OsString::from_vec(name.to_vec())
    };
    #[cfg(not(unix))]
    let name = {
        let text = std::str::from_utf8(name)
            .map_err(|_| "Remote filename is not representable locally")?;
        if !windows_basename(text) {
            return Err("Remote filename is unsafe on this platform".into());
        }
        std::ffi::OsString::from(text)
    };
    let mut components = Path::new(&name).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err("Remote filename is not a single local path component".into());
    }
    Ok(name)
}
pub(super) fn os_bytes(name: &std::ffi::OsStr) -> Result<Vec<u8>, String> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Ok(name.as_bytes().to_vec())
    }
    #[cfg(not(unix))]
    {
        Ok(name
            .to_str()
            .ok_or("Filename is not representable by SFTP")?
            .as_bytes()
            .to_vec())
    }
}
pub(super) fn safe_link(target: &[u8], depth: usize) -> Result<(), String> {
    if target.is_empty()
        || target[0] == b'/'
        || target.contains(&0)
        || target.contains(&b'\\')
        || target.contains(&b':')
    {
        return Err("Symlink target must stay inside the selected tree".into());
    }
    let _ = depth;
    if target.split(|b| *b == b'/').any(|part| part == b"..") {
        return Err("Symlinks containing parent components are not preserved because composed links can escape the tree".into());
    }
    Ok(())
}
#[derive(Default)]
pub(super) struct Budget {
    count: usize,
    bytes: usize,
}
impl Budget {
    pub(super) fn add(
        &mut self,
        local: &Path,
        remote: &RemotePath,
        depth: usize,
    ) -> Result<(), String> {
        self.count += 1;
        self.bytes = self
            .bytes
            .saturating_add(local.as_os_str().len() + remote.as_bytes().len());
        if self.count > 10_000 || self.bytes > 4 * 1024 * 1024 || depth > 64 {
            Err("Folder transfer exceeds 10,000 entries, 64 levels or 4 MiB path budget; partial tree retained".into())
        } else {
            Ok(())
        }
    }
}
async fn symlink_remote(
    sftp: &Arc<RawSftpSession>,
    link: &RemotePath,
    target: &[u8],
    order: &mut Option<bool>,
    root: &RemotePath,
) -> Result<(), String> {
    if order.is_none() {
        // Both probe orders can only create the owned probe or fail on the existing ".".
        // OpenSSH reverses the v3 field order: openssh-portable/sftp-server.c process_symlink.
        let probe = root.child(&random_name()?)?;
        let reversed = if sftp
            .symlink(probe.as_bytes(), b".".as_slice())
            .await
            .is_ok()
        {
            false
        } else {
            sftp.symlink(b".".as_slice(), probe.as_bytes())
                .await
                .map_err(|e| format!("Cannot determine server symlink field order: {e}"))?;
            true
        };
        let result = sftp.readlink(probe.as_bytes()).await.map_err(failure)?;
        if result.files.len() != 1 || result.files[0].filename.as_bytes() != b"." {
            return Err("Server returned an unexpected symlink probe target".into());
        }
        sftp.remove(probe.as_bytes()).await.map_err(failure)?;
        *order = Some(reversed);
    }
    if order == &Some(true) {
        sftp.symlink(target, link.as_bytes())
            .await
            .map_err(failure)?;
    } else {
        sftp.symlink(link.as_bytes(), target)
            .await
            .map_err(failure)?;
    }
    Ok(())
}
pub(super) async fn no_replace_directory(source: PathBuf, target: PathBuf) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        commit::rename_no_replace(&source, &target).map_err(failure)
    })
    .await
    .map_err(failure)?
}

async fn walk(
    job: &mut Job,
    sftp: &Arc<RawSftpSession>,
    upload: bool,
    local: PathBuf,
    remote: RemotePath,
) -> Result<TransferOutcome, String> {
    if upload {
        return walk_upload(job, sftp, local, remote).await;
    }
    let mut budget = Budget::default();
    let mut stack = vec![(local, remote, 0usize)];
    let mut directory_modes = Vec::new();
    while let Some((local, remote, depth)) = stack.pop() {
        if *job.cancelled.borrow() {
            return Ok(TransferOutcome::Cancelled);
        }
        let attrs = sftp.lstat(remote.as_bytes()).await.map_err(failure)?.attrs;
        directory_modes.push((local.clone(), attrs.permissions.unwrap_or(0o755) & 0o777));
        let handle = sftp
            .opendir(remote.as_bytes())
            .await
            .map_err(failure)?
            .handle;
        let guard = RemoteHandleGuard::new(sftp, handle.clone());
        let result=async {
                loop {match sftp.readdir(handle.clone()).await {
                    Ok(batch)=>for entry in batch.files {
                        if *job.cancelled.borrow(){return Ok(TransferOutcome::Cancelled);}
                        let name=entry.filename.as_bytes();if name==b"." || name==b".." {continue;}
                        let source=remote.child(name)?;let destination=local.join(local_name(name)?);budget.add(&destination,&source,depth+1)?;
                        // lstat instead of trusting a stale readdir type; symlinks never become traversal roots.
                        let attrs=sftp.lstat(source.as_bytes()).await.map_err(failure)?.attrs;
                        if attrs.is_dir(){let mut builder=tokio::fs::DirBuilder::new();#[cfg(unix)] builder.mode(0o700);builder.create(&destination).await.map_err(failure)?;stack.push((destination,source,depth+1));}
                        else if attrs.is_symlink(){
                            let link=sftp.readlink(source.as_bytes()).await.map_err(failure)?;if link.files.len()!=1{return Err("Server returned an invalid symlink target".into());}
                            let target=link.files[0].filename.as_bytes();safe_link(target,depth)?;
                            #[cfg(unix)] {use std::os::unix::ffi::OsStrExt;tokio::fs::symlink(std::ffi::OsStr::from_bytes(target),&destination).await.map_err(failure)?;}
                            #[cfg(windows)] {
                                let target=std::str::from_utf8(target).map_err(|_|"Symlink target cannot be represented on Windows")?;
                                let kind=sftp.stat(source.as_bytes()).await.map_err(|e|format!("Cannot determine the Windows symlink target type: {e}"))?.attrs;
                                let result=if kind.is_dir(){tokio::fs::symlink_dir(target,&destination).await}else{tokio::fs::symlink_file(target,&destination).await};
                                result.map_err(|e|format!("Windows could not preserve this symlink; symlink permission or Developer Mode may be required: {e}"))?;
                            }
                            #[cfg(not(any(unix,windows)))] {return Err("Preserving downloaded symlinks is not supported on this platform yet".into());}
                        }
                        else if attrs.is_regular(){job.operation=TransferOperation::Download{remote:source,local:destination,policy:ConflictPolicy::Ask};let outcome=copy_entry(job,sftp).await?;if outcome!=TransferOutcome::Completed{return Ok(outcome);}}
                        else{return Err("Folder contains an unsupported remote entry".into());}
                    },
                    Err(Error::Status(s))if s.status_code==StatusCode::Eof=>return Ok(TransferOutcome::Completed),
                    Err(e)=>return Err(failure(e)),
                }}
            }.await;
        let close = guard.close().await;
        let outcome = result?;
        close?;
        if outcome != TransferOutcome::Completed {
            return Ok(outcome);
        }
    }
    #[cfg(unix)]
    for (path, mode) in directory_modes.into_iter().rev() {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
            .await
            .map_err(failure)?;
    }
    Ok(TransferOutcome::Completed)
}
async fn walk_upload(
    job: &mut Job,
    sftp: &Arc<RawSftpSession>,
    local: PathBuf,
    remote: RemotePath,
) -> Result<TransferOutcome, String> {
    use source::{SourceDir, SourceNode};
    let root = SourceDir::open(local.clone()).await?;
    let root_mode = root.mode().await?;
    let entries = root.entries().await?;
    let remote_root = remote.clone();
    let mut order = None;
    let mut budget = Budget::default();
    let mut stack = vec![(root, entries.into_iter(), remote, local, 0usize, root_mode)];
    while let Some((directory, entries, remote, local, depth, _)) = stack.last_mut() {
        if *job.cancelled.borrow() {
            return Ok(TransferOutcome::Cancelled);
        }
        if let Some(entry) = entries.next() {
            let destination = remote.child(&os_bytes(&entry.name)?)?;
            let source_path = local.join(&entry.name);
            let next_depth = *depth + 1;
            budget.add(&source_path, &destination, next_depth)?;
            match directory.open_child(entry).await? {
                SourceNode::Directory(child) => {
                    let mode = child.mode().await?;
                    let entries = child.entries().await?;
                    sftp.mkdir(
                        destination.as_bytes(),
                        FileAttributes {
                            permissions: Some(0o700),
                            ..Default::default()
                        },
                    )
                    .await
                    .map_err(failure)?;
                    stack.push((
                        child,
                        entries.into_iter(),
                        destination,
                        source_path,
                        next_depth,
                        mode,
                    ));
                }
                SourceNode::File { file, mode } => {
                    let outcome =
                        upload_opened(job, sftp, file, destination, ConflictPolicy::Ask, mode)
                            .await?;
                    if outcome != TransferOutcome::Completed {
                        return Ok(outcome);
                    }
                }
                SourceNode::Symlink(target) => {
                    safe_link(&target, *depth)?;
                    symlink_remote(sftp, &destination, &target, &mut order, &remote_root).await?;
                }
            }
        } else {
            let (_, _, remote, _, _, mode) = stack.pop().unwrap();
            sftp.setstat(
                remote.as_bytes(),
                FileAttributes {
                    permissions: Some(mode),
                    ..Default::default()
                },
            )
            .await
            .map_err(failure)?;
        }
    }
    Ok(TransferOutcome::Completed)
}
pub(super) async fn copy_tree(
    job: &mut Job,
    sftp: &Arc<RawSftpSession>,
) -> Result<TransferOutcome, String> {
    let operation = job.operation.clone();
    let mut cancelled = job.cancelled.clone();
    let (upload, local, remote, policy) = match operation {
        TransferOperation::Upload {
            local,
            remote,
            policy,
        } => (true, local, remote, policy),
        TransferOperation::Download {
            local,
            remote,
            policy,
        } => (false, local, remote, policy),
        _ => unreachable!(),
    };
    let exists = if upload {
        remote_exists(sftp, &remote).await?
    } else {
        match tokio::fs::symlink_metadata(&local).await {
            Ok(_) => true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(e) => return Err(failure(e)),
        }
    };
    if exists {
        if let Some(outcome) = conflict(policy) {
            return Ok(outcome);
        }
    }
    let (local_stage, remote_stage, recovery) = if upload {
        let stage = remote.parent().child(&random_name()?)?;
        sftp.mkdir(
            stage.as_bytes(),
            FileAttributes {
                permissions: Some(0o700),
                ..Default::default()
            },
        )
        .await
        .map_err(failure)?;
        (local.clone(), stage.clone(), RecoveryPath::Remote(stage))
    } else {
        let parent = local
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let stage = parent.join(String::from_utf8(random_name()?).unwrap());
        let mut builder = tokio::fs::DirBuilder::new();
        #[cfg(unix)]
        builder.mode(0o700);
        builder.create(&stage).await.map_err(failure)?;
        (stage.clone(), remote.clone(), RecoveryPath::Local(stage))
    };
    {
        let mut state = job.state.lock().unwrap_or_else(|p| p.into_inner());
        state.tree_recovery = Some(recovery.clone());
        state.event.recovery = Some(recovery);
        state.dirty = true;
    }
    let outcome = tokio::select! {biased;_=cancelled.changed()=>Ok(TransferOutcome::Cancelled),result=walk(job,sftp,upload,local_stage.clone(),remote_stage.clone())=>result}?;
    if outcome != TransferOutcome::Completed {
        return Ok(outcome);
    }
    if *job.cancelled.borrow() {
        return Ok(TransferOutcome::Cancelled);
    }
    if exists && policy == ConflictPolicy::Overwrite {
        let outcome = if upload {
            super::merge::remote(job, sftp, remote_stage.clone(), remote.clone()).await?
        } else {
            super::merge::local(job, local_stage.clone(), local.clone()).await?
        };
        if outcome != TransferOutcome::Completed {
            return Ok(outcome);
        }
        if upload {
            sftp.rmdir(remote_stage.as_bytes()).await.map_err(failure)?;
        } else {
            tokio::fs::remove_dir(local_stage.clone())
                .await
                .map_err(failure)?;
        }
    } else if upload {
        if policy == ConflictPolicy::Overwrite {
            sftp.posix_rename(remote_stage.as_bytes(), remote.as_bytes())
                .await
                .map_err(failure)?;
        } else {
            sftp.rename(remote_stage.as_bytes(), remote.as_bytes())
                .await
                .map_err(failure)?;
        }
    } else if policy == ConflictPolicy::Overwrite {
        tokio::fs::rename(local_stage, local)
            .await
            .map_err(failure)?;
    } else {
        no_replace_directory(local_stage, local).await?;
    }
    let mut state = job.state.lock().unwrap_or_else(|p| p.into_inner());
    state.tree_recovery = None;
    state.event.recovery = None;
    state.dirty = true;
    Ok(TransferOutcome::Completed)
}
#[cfg(test)]
mod windows_name_tests {
    use super::*;
    #[test]
    fn windows_download_names_reject_device_aliases_and_path_syntax() {
        for name in [
            "CON",
            "con.txt",
            "NUL.tar.gz",
            "COM¹",
            "LPT².log",
            "CONIN$",
            "file:stream",
            "folder\\child",
            "bad?name",
            "name.",
            "name ",
            "control\u{1}",
            ".",
            "..",
            "",
        ] {
            assert!(!windows_basename(name), "{name:?}");
        }
        for name in ["normal.txt", "é.txt", "COM10", ".hidden", "literal ' quote"] {
            assert!(windows_basename(name), "{name:?}");
        }
    }
}
