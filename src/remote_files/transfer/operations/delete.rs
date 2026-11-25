use super::*;
pub(super) fn confirmed_target(remote: &RemotePath, confirmed: &RemotePath) -> Result<(), String> {
    if remote != confirmed {
        return Err(
            "The remote target differs from the deletion confirmation; nothing was removed".into(),
        );
    }
    let components = remote
        .as_bytes()
        .split(|b| *b == b'/')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>();
    if components.is_empty() || components.iter().any(|p| *p == b"." || *p == b"..") {
        return Err("Refusing to remove a filesystem root or navigation entry".into());
    }
    Ok(())
}
async fn canonical_directory(sftp: &Arc<RawSftpSession>, path: &RemotePath) -> Result<(), String> {
    let result = sftp.realpath(path.as_bytes()).await.map_err(failure)?;
    if result.files.len() != 1 || result.files[0].filename.as_bytes() != path.as_bytes() {
        return Err("Directory path changed or is not canonical. Refresh the remote pane and confirm deletion again.".into());
    }
    Ok(())
}
struct Frame {
    path: RemotePath,
    entries: std::vec::IntoIter<RemotePath>,
    depth: usize,
}
pub(super) async fn run(
    job: &mut Job,
    sftp: &Arc<RawSftpSession>,
    root: RemotePath,
) -> Result<TransferOutcome, String> {
    if *job.cancelled.borrow() {
        return Ok(TransferOutcome::Cancelled);
    }
    let attrs = sftp.lstat(root.as_bytes()).await.map_err(failure)?.attrs;
    if !attrs.is_dir() {
        sftp.remove(root.as_bytes()).await.map_err(failure)?;
        job.entry_completed();
        return Ok(TransferOutcome::Completed);
    }
    canonical_directory(sftp, &root).await?;
    let mut budget = tree::Budget::default();
    let entries = merge::entries(sftp, &root, &root, &mut budget, 0).await?;
    let mut stack = vec![Frame {
        path: root,
        entries: entries.into_iter(),
        depth: 0,
    }];
    while let Some(frame) = stack.last_mut() {
        if *job.cancelled.borrow() {
            return Ok(TransferOutcome::Cancelled);
        }
        if let Some(path) = frame.entries.next() {
            let attrs = sftp.lstat(path.as_bytes()).await.map_err(failure)?.attrs;
            if attrs.is_dir() {
                canonical_directory(sftp, &path).await?;
                let depth = frame.depth + 1;
                let entries = merge::entries(sftp, &path, &path, &mut budget, depth).await?;
                stack.push(Frame {
                    path,
                    entries: entries.into_iter(),
                    depth,
                });
            } else {
                sftp.remove(path.as_bytes()).await.map_err(failure)?;
                job.entry_completed();
            }
        } else {
            let frame = stack.pop().unwrap();
            canonical_directory(sftp, &frame.path).await?;
            sftp.rmdir(frame.path.as_bytes()).await.map_err(failure)?;
            job.entry_completed();
        }
    }
    Ok(TransferOutcome::Completed)
}
