//! FTP directory transfers stage the complete incoming tree before publishing.
use super::*;
use crate::remote_files::transfer::local_download_path;
use std::{collections::HashMap, ffi::OsStr, io::ErrorKind, path::PathBuf};

const MAX_ENTRIES: usize = 10_000;
const MAX_DEPTH: usize = 64;
const MAX_PATH_BYTES: usize = 4 * 1024 * 1024;

#[derive(Default)]
struct Budget {
    entries: usize,
    paths: usize,
}

impl Budget {
    fn add(&mut self, depth: usize, path: &RemotePath) -> Result<(), String> {
        self.entries += 1;
        self.paths += path.as_bytes().len();
        if depth > MAX_DEPTH || self.entries > MAX_ENTRIES || self.paths > MAX_PATH_BYTES {
            return Err("FTP folder exceeds the depth, entry count, or path size limit.".into());
        }
        Ok(())
    }
}

async fn listing(
    backend: &mut FtpBackend,
    path: &RemotePath,
    job: &Job,
) -> Result<Option<Vec<RemoteEntry>>, String> {
    let mut cancelled = job.cancelled.clone();
    tokio::select! {
        biased;
        _ = cancelled.wait_for(|value| *value) => Ok(None),
        result = backend.list(path) => result.map(Some),
    }
}

async fn remote_plan(
    backend: &mut FtpBackend,
    root: &RemotePath,
    job: &Job,
    deleting: bool,
) -> Result<Option<Vec<RemoteEntry>>, String> {
    let mut plan = Vec::new();
    let mut dirs = vec![(root.clone(), 0)];
    let mut budget = Budget::default();
    while let Some((path, depth)) = dirs.pop() {
        let Some(entries) = listing(backend, &path, job).await? else {
            return Ok(None);
        };
        for entry in entries {
            let basename = entry
                .path
                .as_bytes()
                .rsplit(|byte| *byte == b'/')
                .next()
                .unwrap_or_default();
            if basename.contains(&b'\\') || entry.path.parent() != path {
                return Err("FTP listed a folder entry with an ambiguous path.".into());
            }
            budget.add(depth + 1, &entry.path)?;
            match entry.kind {
                EntryKind::Directory => dirs.push((entry.path.clone(), depth + 1)),
                EntryKind::File => {}
                EntryKind::Symlink if deleting => {}
                _ => return Err("FTP folder contains a link or unsupported entry.".into()),
            }
            plan.push(entry);
        }
    }
    // A parent enters the plan before any of its children.
    Ok(Some(plan))
}

fn canonical_tree_path(root: &RemotePath) -> bool {
    root.as_bytes().strip_prefix(b"/").is_some_and(|path| {
        path.split(|byte| *byte == b'/')
            .all(|name| !name.is_empty() && !matches!(name, b"." | b"..") && !name.contains(&b'\\'))
    })
}

pub(super) async fn remove(
    backend: &mut FtpBackend,
    root: &RemotePath,
    confirmed: &RemotePath,
    job: &Job,
) -> Result<TransferOutcome, String> {
    if !canonical_tree_path(root)
        || root != confirmed
        || entry(backend, root)
            .await?
            .is_none_or(|e| e.kind != EntryKind::Directory)
    {
        return Err("Confirm the exact directory path before deleting its contents.".into());
    }
    let Some(plan) = remote_plan(backend, root, job, true).await? else {
        return Ok(TransferOutcome::Cancelled);
    };
    let mut previous_parent = None;
    let mut current_entries = HashMap::new();
    for expected in plan.into_iter().rev() {
        if *job.cancelled.borrow() {
            return Ok(TransferOutcome::Cancelled);
        }
        let parent = expected.path.parent();
        if previous_parent.as_ref() != Some(&parent) {
            let Some(entries) = listing(backend, &parent, job).await? else {
                return Ok(TransferOutcome::Cancelled);
            };
            current_entries = entries
                .into_iter()
                .map(|entry| (entry.path.as_bytes().to_vec(), entry.kind))
                .collect();
            previous_parent = Some(parent);
        }
        if current_entries.remove(expected.path.as_bytes()) != Some(expected.kind) {
            return Err("An FTP entry changed type during deletion. Inspect the remaining folder before retrying.".into());
        }
        if !job.begin_step() {
            return Ok(TransferOutcome::Cancelled);
        }
        let result = backend.remove(&expected.path, expected.kind).await;
        let outcome = mutation(backend, result)?;
        if outcome != TransferOutcome::Completed {
            return Ok(outcome);
        }
        if !job.end_step(false) {
            return Ok(TransferOutcome::Cancelled);
        }
    }
    if !job.begin_step() {
        return Ok(TransferOutcome::Cancelled);
    }
    let result = backend.remove(root, EntryKind::Directory).await;
    let outcome = mutation(backend, result)?;
    if outcome == TransferOutcome::Completed {
        job.end_step(true);
    }
    Ok(outcome)
}

#[cfg(any(unix, windows))]
mod local;
#[cfg(any(unix, windows))]
pub(super) use local::{download, upload};

#[cfg(not(any(unix, windows)))]
pub(super) async fn upload(
    _: &mut FtpBackend,
    _: FtpTransferType,
    _: &Job,
    _: &Path,
    _: &RemotePath,
    _: ConflictPolicy,
) -> Result<TransferOutcome, String> {
    Err("Anchored folder uploads are unavailable on this platform.".into())
}

#[cfg(not(any(unix, windows)))]
pub(super) async fn download(
    _: &mut FtpBackend,
    _: FtpTransferType,
    _: &Job,
    _: &RemotePath,
    _: &Path,
    _: ConflictPolicy,
) -> Result<TransferOutcome, String> {
    Err("Anchored folder downloads are unavailable on this platform.".into())
}
