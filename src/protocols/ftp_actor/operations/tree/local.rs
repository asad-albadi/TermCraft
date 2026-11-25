use super::*;
use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt};
use cap_std::fs::{Dir, Metadata, OpenOptions};
use std::collections::{HashMap, HashSet};

type Outcome = Result<TransferOutcome, String>;

fn valid(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use cap_std::fs::MetadataExt;
        if metadata.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
        {
            return false;
        }
    }
    !metadata.is_symlink()
}

fn inspect(dir: &Dir, name: &OsStr) -> Result<Option<Metadata>, String> {
    match dir.symlink_metadata(name) {
        Ok(meta) => Ok(Some(meta)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io(error)),
    }
}

fn directory(dir: &Dir, name: &OsStr) -> Result<Dir, String> {
    let metadata = inspect(dir, name)?.ok_or("FTP local directory disappeared.")?;
    if !valid(&metadata) || !metadata.is_dir() {
        return Err("FTP cannot follow a local link or reparse point.".into());
    }
    let child = dir.open_dir_nofollow(name).map_err(io)?;
    if !valid(&child.dir_metadata().map_err(io)?) {
        return Err("FTP cannot open a local reparse point.".into());
    }
    Ok(child)
}

fn source(path: &Path) -> Result<Dir, String> {
    let file = path.file_name().ok_or("Choose a named local folder.")?;
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = Dir::open_ambient_dir(parent, cap_std::ambient_authority()).map_err(io)?;
    directory(&parent, file)
}

fn remote_name(name: &OsStr) -> Result<&[u8], String> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Ok(name.as_bytes())
    }
    #[cfg(windows)]
    {
        Ok(name
            .to_str()
            .ok_or("FTP cannot represent a local folder entry name.")?
            .as_bytes())
    }
}

async fn stage_upload(
    backend: &mut FtpBackend,
    transfer_type: FtpTransferType,
    job: &Job,
    source: Dir,
    stage: &RemotePath,
) -> Result<Option<Vec<RemoteEntry>>, String> {
    use std::collections::VecDeque;
    let mut dirs = VecDeque::from([(PathBuf::new(), stage.clone(), 0)]);
    let mut budget = Budget::default();
    let mut transferred = 0;
    let mut plan = Vec::new();
    while let Some((relative, remote, depth)) = dirs.pop_front() {
        let local = cap_directory(&source, &relative)?;
        for item in local.entries().map_err(io)? {
            if *job.cancelled.borrow() {
                return Ok(None);
            }
            let name = item.map_err(io)?.file_name();
            let name_bytes = remote_name(&name)?;
            if name_bytes.contains(&b'\\') {
                return Err("FTP cannot safely send a folder entry containing a backslash.".into());
            }
            let path = remote.child(name_bytes)?;
            budget.add(depth + 1, &path)?;
            let meta = inspect(&local, &name)?.ok_or("FTP upload source disappeared.")?;
            if !valid(&meta) {
                return Err("FTP folder uploads cannot follow links or reparse points.".into());
            }
            let kind = if meta.is_dir() {
                directory(&local, &name)?;
                backend.mkdir(&path).await?;
                dirs.push_back((relative.join(&name), path.clone(), depth + 1));
                EntryKind::Directory
            } else if meta.is_file() {
                let mut options = OpenOptions::new();
                options.read(true).follow(FollowSymlinks::No);
                #[cfg(unix)]
                {
                    use cap_std::fs::OpenOptionsExt;
                    options.custom_flags(libc::O_NONBLOCK);
                }
                let file = local.open_with(&name, &options).map_err(io)?;
                let opened = file.metadata().map_err(io)?;
                if !valid(&opened) || !opened.is_file() {
                    return Err("FTP source changed type while opening.".into());
                }
                let file = tokio::fs::File::from_std(file.into_std());
                let mut reader = Progress::new(file, job);
                reader.bytes = transferred;
                let mut cancelled = job.cancelled.clone();
                tokio::select! {
                    biased;
                    _ = cancelled.wait_for(|value| *value) => return Ok(None),
                    result = backend.write_from(&path, transfer_type, &mut reader) => { result?; }
                }
                transferred = reader.bytes;
                EntryKind::File
            } else {
                return Err("FTP folder contains an unsupported local file type.".into());
            };
            plan.push(RemoteEntry {
                path,
                display_name: String::new(),
                kind,
                size: None,
            });
        }
    }
    Ok(Some(plan))
}

fn relative_remote(
    stage: &RemotePath,
    path: &RemotePath,
    destination: &RemotePath,
) -> Result<RemotePath, String> {
    let suffix = path
        .as_bytes()
        .strip_prefix(stage.as_bytes())
        .and_then(|suffix| suffix.strip_prefix(b"/"))
        .ok_or("Invalid FTP staged path.")?;
    suffix
        .split(|byte| *byte == b'/')
        .try_fold(destination.clone(), |parent, name| parent.child(name))
}

fn blocked(path: &RemotePath, roots: &HashSet<Vec<u8>>) -> bool {
    let bytes = path.as_bytes();
    bytes
        .iter()
        .enumerate()
        .any(|(i, byte)| *byte == b'/' && roots.contains(&bytes[..i]))
}

pub(in crate::protocols::ftp_actor::operations) async fn upload(
    backend: &mut FtpBackend,
    transfer_type: FtpTransferType,
    job: &Job,
    input: &Path,
    remote: &RemotePath,
    policy: ConflictPolicy,
) -> Outcome {
    if !canonical_tree_path(remote) {
        return Err("Choose an unambiguous absolute FTP folder path.".into());
    }
    let input = source(input)?;
    if entry(backend, remote)
        .await?
        .is_some_and(|entry| entry.kind != EntryKind::Directory)
    {
        return Ok(conflict(policy));
    }
    let (container, staging) = remote_stage(backend, remote, job).await?;
    backend.mkdir(&staging).await?;
    let Some(plan) = stage_upload(backend, transfer_type, job, input, &staging).await? else {
        return Ok(TransferOutcome::Cancelled);
    };
    match entry(backend, remote).await? {
        None => {
            let outcome = publish(backend, &staging, remote, job).await?;
            return Ok(cleanup_published(backend, container, job, outcome).await);
        }
        Some(entry) if entry.kind != EntryKind::Directory => return Ok(conflict(policy)),
        _ => {}
    }

    let mut blocked_roots = HashSet::new();
    let mut skipped = false;
    let mut dirs = vec![staging.clone()];
    for staged in plan {
        let target = relative_remote(&staging, &staged.path, remote)?;
        if blocked(&target, &blocked_roots) {
            continue;
        }
        // Recheck the parent before touching a descendant. FTP has no directory handle.
        if entry(backend, &target.parent())
            .await?
            .is_none_or(|parent| parent.kind != EntryKind::Directory)
        {
            return Err("FTP destination folder changed during merge.".into());
        }
        let existing = entry(backend, &target).await?;
        if staged.kind == EntryKind::Directory {
            dirs.push(staged.path.clone());
            if existing
                .as_ref()
                .is_some_and(|entry| entry.kind == EntryKind::Directory)
            {
                continue;
            }
        }
        if existing.is_some_and(|entry| {
            staged.kind != EntryKind::File
                || entry.kind != EntryKind::File
                || policy != ConflictPolicy::Overwrite
        }) {
            if policy == ConflictPolicy::Skip {
                blocked_roots.insert(target.as_bytes().to_vec());
                skipped = true;
                continue;
            }
            return Ok(TransferOutcome::Conflict);
        }
        if !job.begin_step() {
            return Ok(TransferOutcome::Cancelled);
        }
        let result = if staged.kind == EntryKind::Directory {
            backend.mkdir(&target).await
        } else {
            backend.rename(&staged.path, &target).await
        };
        let outcome = mutation(backend, result)?;
        if outcome != TransferOutcome::Completed {
            return Ok(outcome);
        }
        if !job.end_step(false) {
            return Ok(TransferOutcome::Cancelled);
        }
    }
    if skipped {
        return Ok(TransferOutcome::CompletedWithWarning(
            "Folder merged. Conflicting entries remain in FTP staging.".into(),
        ));
    }
    for dir in dirs.into_iter().rev() {
        if *job.cancelled.borrow() {
            return Ok(TransferOutcome::Cancelled);
        }
        if let Err(error) = backend.remove(&dir, EntryKind::Directory).await {
            return Ok(TransferOutcome::CompletedWithWarning(format!(
                "Folder merged. FTP staging remains: {error}"
            )));
        }
    }
    Ok(cleanup_published(
        backend,
        container,
        job,
        TransferOutcome::CompletedWithWarning(REMOTE_WARNING.into()),
    )
    .await)
}

fn local_name(path: &RemotePath) -> Result<PathBuf, String> {
    local_download_path(Path::new(""), path)
}

fn cap_directory(root: &Dir, path: &Path) -> Result<Dir, String> {
    let mut dir = root.try_clone().map_err(io)?;
    for part in path.components() {
        dir = directory(&dir, part.as_os_str())?;
    }
    Ok(dir)
}

pub(in crate::protocols::ftp_actor::operations) async fn download(
    backend: &mut FtpBackend,
    transfer_type: FtpTransferType,
    job: &Job,
    remote: &RemotePath,
    local: &Path,
    policy: ConflictPolicy,
) -> Outcome {
    if !canonical_tree_path(remote) {
        return Err("Choose an unambiguous absolute FTP folder path.".into());
    }
    let existing = match tokio::fs::symlink_metadata(local).await {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Some(metadata),
        Ok(_) => return Ok(conflict(policy)),
        Err(error) if error.kind() == ErrorKind::NotFound => None,
        Err(error) => return Err(io(error)),
    };
    let Some(plan) = remote_plan(backend, remote, job, false).await? else {
        return Ok(TransferOutcome::Cancelled);
    };
    let (mut stage, root, archive) = LocalStage::create_tree(local.to_owned()).await?;
    drop(archive);
    job.recovery(Some(RecoveryPath::Local(stage.initial_path().to_owned())));
    let _recovery = TreeRecovery {
        stage: stage.clone(),
        job,
    };
    let target = if let Some(existing) = existing {
        Some(Dir::from_std_file(
            stage.attach_merge_target(existing).await?,
        ))
    } else {
        None
    };
    let root = Dir::from_std_file(root);
    let mut paths = HashMap::from([(remote.as_bytes().to_vec(), PathBuf::new())]);
    let mut local_plan = Vec::new();
    let mut transferred = 0;
    for item in plan {
        if *job.cancelled.borrow() {
            return Ok(TransferOutcome::Cancelled);
        }
        let parent = paths
            .get(item.path.parent().as_bytes())
            .ok_or("FTP parent entry is missing.")?;
        let name = local_name(&item.path)?;
        let dir = cap_directory(&root, parent)?;
        let relative = parent.join(&name);
        if item.kind == EntryKind::Directory {
            dir.create_dir(&name).map_err(io)?;
            paths.insert(item.path.as_bytes().to_vec(), relative.clone());
        } else {
            let mut options = OpenOptions::new();
            options
                .write(true)
                .create_new(true)
                .follow(FollowSymlinks::No);
            #[cfg(unix)]
            {
                use cap_std::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut writer = Progress::new(
                tokio::fs::File::from_std(dir.open_with(&name, &options).map_err(io)?.into_std()),
                job,
            );
            writer.bytes = transferred;
            let mut cancelled = job.cancelled.clone();
            let count = tokio::select! {
                biased;
                _ = cancelled.wait_for(|value| *value) => None,
                result = backend.read_to(&item.path, transfer_type, &mut writer, u64::MAX) => Some(result),
            };
            writer.inner.flush().await.map_err(io)?;
            writer.inner.sync_all().await.map_err(io)?;
            let Some(count) = count else {
                return Ok(TransferOutcome::Cancelled);
            };
            let count = count?;
            if transfer_type == FtpTransferType::Binary
                && item.size.is_some_and(|size| size != count)
            {
                return Err(
                    "The FTP server sent a different file size. Folder staging was retained."
                        .into(),
                );
            }
            transferred = writer.bytes;
        }
        local_plan.push((relative, item.kind));
    }
    if let Some(target) = target {
        let merge = merge_local(job, &stage, &root, &target, local_plan, policy).await?;
        if merge != TransferOutcome::Completed {
            return Ok(merge);
        }
        drop(target);
        drop(root);
        return Ok(match stage.clean_merged_tree().await {
            Ok(()) => {
                job.recovery(None);
                TransferOutcome::Completed
            }
            Err(error) => TransferOutcome::CompletedWithWarning(format!(
                "Folder merged. Private staging remains: {error}"
            )),
        });
    }
    drop(root);
    if !job.begin_commit() {
        return Ok(TransferOutcome::Cancelled);
    }
    match stage.commit(false).await {
        Ok(true) => {
            job.recovery(None);
            match stage.clean_tree_container().await {
                Ok(()) => Ok(TransferOutcome::Completed),
                Err(error) => Ok(TransferOutcome::CompletedWithWarning(format!(
                    "Folder published. Private staging remains: {error}"
                ))),
            }
        }
        Ok(false) => Ok(conflict(policy)),
        Err(CommitError::Failed(error)) => Err(error),
        Err(CommitError::Indeterminate(error)) => Ok(TransferOutcome::Indeterminate(error)),
    }
}

struct TreeRecovery<'a> {
    stage: LocalStage,
    job: &'a Job,
}

impl Drop for TreeRecovery<'_> {
    fn drop(&mut self) {
        if self.job.recovery_path().is_some() {
            self.job
                .recovery(self.stage.recovery_path().ok().map(|container| {
                    let tree = container.join("tree");
                    RecoveryPath::Local(if tree.is_dir() { tree } else { container })
                }));
        }
    }
}

async fn merge_local(
    job: &Job,
    stage: &LocalStage,
    source: &Dir,
    target: &Dir,
    plan: Vec<(PathBuf, EntryKind)>,
    policy: ConflictPolicy,
) -> Outcome {
    let mut blocked = Vec::<PathBuf>::new();
    let mut directories = Vec::new();
    for (path, kind) in plan {
        if blocked.iter().any(|root| path.starts_with(root)) {
            continue;
        }
        stage.validate_merge_root().await?;
        let parent = path.parent().unwrap();
        let name = path.file_name().unwrap();
        let to = cap_directory(target, parent)?;
        let existing = inspect(&to, name)?;
        if kind == EntryKind::Directory {
            directories.push(path.clone());
            if existing
                .as_ref()
                .is_some_and(|meta| valid(meta) && meta.is_dir())
            {
                continue;
            }
        }
        if existing.as_ref().is_some_and(|meta| {
            kind != EntryKind::File
                || !valid(meta)
                || !meta.is_file()
                || policy != ConflictPolicy::Overwrite
        }) {
            if policy == ConflictPolicy::Skip {
                blocked.push(path);
                continue;
            }
            return Ok(TransferOutcome::Conflict);
        }
        if !job.begin_step() {
            return Ok(TransferOutcome::Cancelled);
        }
        if kind == EntryKind::Directory {
            to.create_dir(name).map_err(io)?;
        } else {
            let from = cap_directory(source, parent)?;
            if policy == ConflictPolicy::Overwrite {
                from.rename(name, &to, name).map_err(io)?;
            } else {
                match from.hard_link(name, &to, name) {
                    Ok(()) => from.remove_file(name).map_err(io)?,
                    Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                        return Ok(conflict(policy));
                    }
                    Err(error) => return Err(io(error)),
                }
            }
        }
        if !job.end_step(false) {
            return Ok(TransferOutcome::Cancelled);
        }
    }
    if !blocked.is_empty() {
        return Ok(TransferOutcome::CompletedWithWarning(
            "Folder merged. Conflicting entries remain in local staging.".into(),
        ));
    }
    for dir in directories.into_iter().rev() {
        cap_directory(source, dir.parent().unwrap())?
            .remove_dir(dir.file_name().unwrap())
            .map_err(io)?;
    }
    Ok(TransferOutcome::Completed)
}
