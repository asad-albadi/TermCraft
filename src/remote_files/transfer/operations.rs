use super::*;
use russh_sftp::{
    client::{RawSftpSession, error::Error},
    protocol::{FileAttributes, OpenFlags, StatusCode},
};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
struct RemoteHandleGuard {
    sftp: Arc<RawSftpSession>,
    handle: Option<russh_sftp::protocol::SftpHandle>,
}
impl RemoteHandleGuard {
    fn new(sftp: &Arc<RawSftpSession>, handle: russh_sftp::protocol::SftpHandle) -> Self {
        Self {
            sftp: sftp.clone(),
            handle: Some(handle),
        }
    }
    async fn close(mut self) -> Result<(), String> {
        let handle = self.handle.take().unwrap();
        tokio::time::timeout(Duration::from_secs(2), self.sftp.close(handle))
            .await
            .map_err(|_| "Remote handle close timed out")?
            .map_err(failure)?;
        Ok(())
    }
}
impl Drop for RemoteHandleGuard {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let sftp = self.sftp.clone();
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(async move {
                    let _ = tokio::time::timeout(Duration::from_secs(2), sftp.close(handle)).await;
                });
            }
        }
    }
}
fn failure(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn random_name() -> Result<Vec<u8>, String> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(failure)?;
    Ok(format!(
        ".termcraft-{}.part",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    )
    .into_bytes())
}
async fn remote_exists(sftp: &Arc<RawSftpSession>, path: &RemotePath) -> Result<bool, String> {
    match sftp.lstat(path.as_bytes()).await {
        Ok(_) => Ok(true),
        Err(Error::Status(s)) if s.status_code == StatusCode::NoSuchFile => Ok(false),
        Err(e) => Err(failure(e)),
    }
}
fn conflict(policy: ConflictPolicy) -> Option<TransferOutcome> {
    match policy {
        ConflictPolicy::Ask => Some(TransferOutcome::Conflict),
        ConflictPolicy::Skip => Some(TransferOutcome::Skipped),
        ConflictPolicy::Overwrite => None,
    }
}
async fn copy_entry(job: &mut Job, sftp: &Arc<RawSftpSession>) -> Result<TransferOutcome, String> {
    if *job.cancelled.borrow() {
        return Ok(TransferOutcome::Cancelled);
    }
    let base = job
        .state
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .event
        .bytes;
    let mut cancelled = job.cancelled.clone();
    match &job.operation.clone() {
        TransferOperation::Document(request) => editor::run(request, sftp).await,
        TransferOperation::UploadFolder { .. } | TransferOperation::DownloadFolder { .. } => {
            Err("This explicit folder operation is only supported by the FTP workspace.".into())
        }
        TransferOperation::Mkdir { remote } => {
            sftp.mkdir(
                remote.as_bytes(),
                FileAttributes {
                    permissions: Some(0o755),
                    ..Default::default()
                },
            )
            .await
            .map_err(failure)?;
            Ok(TransferOutcome::Completed)
        }
        TransferOperation::Rename {
            source,
            destination,
            policy,
        } => {
            if source == destination {
                return Ok(TransferOutcome::Skipped);
            }
            if remote_exists(sftp, destination).await? {
                if let Some(outcome) = conflict(*policy) {
                    return Ok(outcome);
                }
            }
            if *policy == ConflictPolicy::Overwrite {
                sftp.posix_rename(source.as_bytes(), destination.as_bytes())
                    .await
                    .map_err(failure)?;
            } else {
                sftp.rename(source.as_bytes(), destination.as_bytes())
                    .await
                    .map_err(failure)?;
            }
            Ok(TransferOutcome::Completed)
        }
        TransferOperation::RemoveTree { remote, confirmed } => {
            delete::confirmed_target(remote, confirmed)?;
            delete::run(job, sftp, remote.clone()).await
        }
        TransferOperation::Remove { remote, confirmed } => {
            delete::confirmed_target(remote, confirmed)?;
            if remote != confirmed {
                return Err(
                    "The remote target differs from the deletion confirmation; nothing was removed"
                        .into(),
                );
            }
            if remote.as_bytes() == b"/" || remote.as_bytes() == b"." || remote.as_bytes() == b".."
            {
                return Err("Refusing to remove a filesystem root or navigation entry".into());
            }
            let attrs = sftp.lstat(remote.as_bytes()).await.map_err(failure)?.attrs;
            if attrs.is_dir() {
                sftp.rmdir(remote.as_bytes()).await.map_err(failure)?;
            } else {
                sftp.remove(remote.as_bytes()).await.map_err(failure)?;
            }
            Ok(TransferOutcome::Completed)
        }

        TransferOperation::Download {
            remote,
            local,
            policy,
        } => {
            let existing = match tokio::fs::symlink_metadata(local).await {
                Ok(meta) => Some(meta),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => return Err(failure(e)),
            };
            let exists = existing.is_some();
            if exists {
                if let Some(outcome) = conflict(*policy) {
                    return Ok(outcome);
                }
            }
            let attrs = sftp.lstat(remote.as_bytes()).await.map_err(failure)?.attrs;
            if !attrs.is_regular() {
                return Err("Select a regular file; recursive transfers are not available in this increment".into());
            }
            #[cfg(unix)]
            let final_mode = {
                use std::os::unix::fs::PermissionsExt;
                existing
                    .as_ref()
                    .filter(|m| m.is_file())
                    .map(|m| m.permissions().mode())
                    .or(attrs.permissions)
                    .unwrap_or(0o600)
                    & 0o777
            };
            let parent = local
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| std::path::Path::new("."));
            let name = String::from_utf8(random_name()?).unwrap();
            let staging = parent.join(name);
            let mut options = tokio::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            options.mode(0o600);
            let mut file = options.open(&staging).await.map_err(failure)?;
            let recovery = Some(RecoveryPath::Local(staging.clone()));
            job.report(base, recovery.clone(), None);
            let handle = sftp
                .open(
                    remote.as_bytes(),
                    OpenFlags::READ,
                    FileAttributes::default(),
                )
                .await
                .map_err(failure)?
                .handle;
            let guard = RemoteHandleGuard::new(sftp, handle.clone());
            let mut bytes = 0;
            let result: Result<TransferOutcome, String> = tokio::select! {biased;
                _=cancelled.changed()=>Ok(TransferOutcome::Cancelled),
                result=async {
                    loop {match sftp.read(handle.clone(),bytes,32*1024).await {
                        Ok(data)=>{if data.data.is_empty(){return Err("Server returned empty data before EOF".into());}file.write_all(&data.data).await.map_err(failure)?;bytes+=data.data.len() as u64;job.report(base+bytes,recovery.clone(),None);}
                        Err(Error::Status(s))if s.status_code==StatusCode::Eof=>break,
                        Err(e)=>return Err(failure(e)),
                    }}
                    #[cfg(unix)] {use std::os::unix::fs::PermissionsExt;file.set_permissions(std::fs::Permissions::from_mode(final_mode)).await.map_err(failure)?;}
                    file.sync_all().await.map_err(failure)?;Ok(TransferOutcome::Completed)
                }=>result,
            };
            let close = guard.close().await;
            let local_file = file.into_std().await;
            let local_close = tokio::task::spawn_blocking(move || {
                commit::close_file(local_file).map_err(failure)
            })
            .await
            .map_err(failure)?;
            let outcome = result?;
            if outcome != TransferOutcome::Completed {
                return Ok(outcome);
            }
            close?;
            local_close?;
            if *job.cancelled.borrow() {
                return Ok(TransferOutcome::Cancelled);
            }
            if *policy == ConflictPolicy::Overwrite {
                tokio::fs::rename(&staging, local).await.map_err(failure)?;
            } else {
                tree::no_replace_directory(staging.clone(), local.clone()).await?;
            }
            job.report(base + bytes, None, None);
            Ok(TransferOutcome::Completed)
        }
        TransferOperation::Upload {
            local,
            remote,
            policy,
        } => {
            let (file, mode) = source::open_file(local.clone()).await?;
            upload_opened(job, sftp, file, remote.clone(), *policy, mode).await
        }
    }
}
async fn upload_opened(
    job: &mut Job,
    sftp: &Arc<RawSftpSession>,
    mut file: tokio::fs::File,
    remote: RemotePath,
    policy: ConflictPolicy,
    source_mode: u32,
) -> Result<TransferOutcome, String> {
    let existing = match sftp.lstat(remote.as_bytes()).await {
        Ok(value) => Some(value.attrs),
        Err(Error::Status(s)) if s.status_code == StatusCode::NoSuchFile => None,
        Err(e) => return Err(failure(e)),
    };
    if existing.is_some() {
        if let Some(outcome) = conflict(policy) {
            return Ok(outcome);
        }
    }
    let final_mode = existing
        .filter(|a| a.is_regular())
        .and_then(|a| a.permissions)
        .unwrap_or(source_mode)
        & 0o777;
    let base = job
        .state
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .event
        .bytes;
    let mut cancelled = job.cancelled.clone();
    if *cancelled.borrow() {
        return Ok(TransferOutcome::Cancelled);
    }
    let staging = remote.parent().child(&random_name()?)?;
    let handle = sftp
        .open(
            staging.as_bytes(),
            OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::EXCLUDE,
            FileAttributes {
                permissions: Some(0o600),
                ..Default::default()
            },
        )
        .await
        .map_err(failure)?
        .handle;
    let guard = RemoteHandleGuard::new(sftp, handle.clone());
    let recovery = Some(RecoveryPath::Remote(staging.clone()));
    job.report(base, recovery.clone(), None);
    let mut bytes = 0;
    let result: Result<TransferOutcome, String> = tokio::select! {biased;
        _=cancelled.changed()=>Ok(TransferOutcome::Cancelled),
        result=async {let mut buffer=vec![0;32*1024];loop{let n=file.read(&mut buffer).await.map_err(failure)?;if n==0{break;}sftp.write(handle.clone(),bytes,buffer[..n].to_vec()).await.map_err(failure)?;bytes+=n as u64;job.report(base+bytes,recovery.clone(),None);}sftp.fsetstat(handle.clone(),FileAttributes{permissions:Some(final_mode),..Default::default()}).await.map_err(failure)?;Ok(TransferOutcome::Completed)}=>result,
    };
    let close = guard.close().await;
    let outcome = result?;
    if outcome != TransferOutcome::Completed {
        return Ok(outcome);
    }
    close?;
    if *job.cancelled.borrow() {
        return Ok(TransferOutcome::Cancelled);
    }
    if policy == ConflictPolicy::Overwrite {
        sftp.posix_rename(staging.as_bytes(), remote.as_bytes())
            .await
            .map_err(|e| format!("Atomic remote replace failed (partial retained): {e}"))?;
    } else {
        sftp.rename(staging.as_bytes(), remote.as_bytes())
            .await
            .map_err(failure)?;
    }
    job.report(base + bytes, None, None);
    Ok(TransferOutcome::Completed)
}
mod commit;
mod editor;
pub(crate) fn close_recovery_file(file: std::fs::File) -> std::io::Result<()> {
    commit::close_file(file)
}
pub(super) async fn scp_source(path: PathBuf) -> Result<(tokio::fs::File, u32), String> {
    source::open_file(path).await
}

pub(super) fn scp_confirmed_target(
    remote: &RemotePath,
    confirmed: &RemotePath,
) -> Result<(), String> {
    delete::confirmed_target(remote, confirmed)
}

mod delete;
pub(super) mod merge;
pub(super) mod source;
mod tree;
async fn copy(job: &mut Job, sftp: &Arc<RawSftpSession>) -> Result<TransferOutcome, String> {
    if *job.cancelled.borrow() {
        return Ok(TransferOutcome::Cancelled);
    }
    let directory = match &job.operation {
        TransferOperation::Upload { local, .. } => tokio::fs::symlink_metadata(local)
            .await
            .map_err(failure)?
            .is_dir(),
        TransferOperation::Download { remote, .. } => sftp
            .lstat(remote.as_bytes())
            .await
            .map_err(failure)?
            .attrs
            .is_dir(),
        _ => false,
    };
    if directory {
        tree::copy_tree(job, sftp).await
    } else {
        copy_entry(job, sftp).await
    }
}
pub(crate) async fn run(mut job: Job, sftp: Arc<RawSftpSession>) {
    {
        let mut state = job.state.lock().unwrap_or_else(|p| p.into_inner());
        state.started = true;
    }
    let result = copy(&mut job, &sftp).await;
    let outcome = result.unwrap_or_else(TransferOutcome::Failed);
    job.finish(outcome);
}

pub(super) fn download_child(
    directory: &std::path::Path,
    remote: &RemotePath,
) -> Result<PathBuf, String> {
    let name = remote
        .as_bytes()
        .rsplit(|b| *b == b'/')
        .next()
        .ok_or("Remote entry has no filename")?;
    Ok(directory.join(tree::local_name(name)?))
}
