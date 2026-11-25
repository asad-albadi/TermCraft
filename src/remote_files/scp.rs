//! Explicit SCP compatibility: byte-framed enumeration and legacy SCP on an existing SSH connection.
use super::{
    DirectorySnapshot, EntryKind, RemoteEntry, RemotePath,
    transfer::{ConflictPolicy, Job, TransferOutcome},
};
use crate::remote_files::editor::{Metadata, Version};
use russh::{ChannelMsg, ChannelStream, client};
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
mod editor;
mod source;
use editor::UploadResult;
use source::UploadSource;
const HELPER: &str = include_str!("scp/helper.py");
type Stream = ChannelStream<client::Msg>;
fn error(e: impl std::fmt::Display) -> String {
    format!("SCP helper: {e}")
}
pub(crate) struct PreparedDownload {
    pub(crate) folder: bool,
    stream: Stream,
    mode: u32,
    size: u64,
    metadata: Option<Metadata>,
}
struct RawChannelGuard {
    channel: Option<russh::Channel<client::Msg>>,
    _slot: tokio::sync::OwnedSemaphorePermit,
}
impl RawChannelGuard {
    fn into_stream(mut self) -> Stream {
        self.channel.take().unwrap().into_stream()
    }
}
impl Drop for RawChannelGuard {
    fn drop(&mut self) {
        if let Some(channel) = self.channel.take() {
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(async move {
                    let _ = tokio::time::timeout(Duration::from_secs(2), channel.close()).await;
                });
            }
        }
    }
}
pub struct ScpBackend<H: client::Handler> {
    handle: Arc<client::Handle<H>>,
    startups: Arc<tokio::sync::Semaphore>,
    capabilities: Arc<std::sync::Mutex<Option<super::capabilities::RemoteCapabilities>>>,
}
impl<H: client::Handler> Clone for ScpBackend<H> {
    fn clone(&self) -> Self {
        Self {
            handle: self.handle.clone(),
            startups: self.startups.clone(),
            capabilities: self.capabilities.clone(),
        }
    }
}
impl<H: client::Handler + 'static> ScpBackend<H> {
    pub(crate) fn new(handle: Arc<client::Handle<H>>) -> Self {
        Self {
            handle,
            startups: Arc::new(tokio::sync::Semaphore::new(2)),
            capabilities: Arc::new(std::sync::Mutex::new(None)),
        }
    }
    pub(crate) fn capabilities(&self) -> Option<super::capabilities::RemoteCapabilities> {
        *self.capabilities.lock().unwrap_or_else(|p| p.into_inner())
    }
    async fn stream(
        &self,
        operation: &str,
        path: &RemotePath,
        policy: ConflictPolicy,
    ) -> Result<Stream, String> {
        self.stream_version(operation, path, policy, None).await
    }
    async fn stream_version(
        &self,
        operation: &str,
        path: &RemotePath,
        policy: ConflictPolicy,
        expected: Option<&Version>,
    ) -> Result<Stream, String> {
        let extra = expected
            .map(|expected| {
                let digest = Sha256::digest(&expected.bytes)
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>();
                let data = serde_json::to_vec(
                    &serde_json::json!({"sha256": digest, "metadata": expected.metadata}),
                )
                .map_err(error)?;
                Ok::<_, String>(data.iter().map(|b| format!("{b:02x}")).collect::<String>())
            })
            .transpose()?;
        self.stream_args(operation, path, policy, extra.as_deref())
            .await
    }
    async fn stream_args(
        &self,
        operation: &str,
        path: &RemotePath,
        policy: ConflictPolicy,
        extra: Option<&str>,
    ) -> Result<Stream, String> {
        if path.as_bytes().len() > 32768 {
            return Err("SCP helper path exceeds 32 KiB".into());
        }
        let hex = path
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let script_hex = HELPER
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        // POSIX shells and Fish disagree about backslashes inside single quotes. Hex is data in both.
        let script = format!("'exec(bytes.fromhex(\"{script_hex}\"))'");
        let policy = if policy == ConflictPolicy::Overwrite {
            "overwrite"
        } else {
            "ask"
        };
        let mut command = format!("python3 -c {script} {operation} {hex} {policy}");
        if let Some(extra) = extra {
            if extra.len() > 65536 || !extra.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("Invalid SCP helper argument".into());
            }
            command.push(' ');
            command.push_str(extra);
        }
        if command.len() > 120 * 1024 {
            return Err(
                "SCP helper launch exceeds the remote exec argument budget; choose a shorter path"
                    .into(),
            );
        }
        let slot = self.startups.clone().try_acquire_owned().map_err(
            |_| "SCP helper channel startup is busy; wait for pending opens or reconnect",
        )?;
        let handle = self.handle.clone();
        // Keep ownership until open confirmation even when the caller cancels. An unclaimed result closes itself.
        let opening = tokio::spawn(async move {
            let channel = handle.channel_open_session().await.map_err(error)?;
            Ok::<_, String>(RawChannelGuard {
                channel: Some(channel),
                _slot: slot,
            })
        });
        let mut guard = tokio::time::timeout(Duration::from_secs(10), opening)
            .await
            .map_err(|_| "SCP channel open timed out; pending opens remain bounded")?
            .map_err(error)??;
        let channel = guard.channel.as_mut().unwrap();
        tokio::time::timeout(
            Duration::from_secs(10),
            channel.exec(true, command.as_bytes()),
        )
        .await
        .map_err(|_| "SCP helper exec submission timed out")?
        .map_err(error)?;
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                match channel.wait().await {
                    Some(ChannelMsg::Success) => return Ok(()),
                    Some(ChannelMsg::Failure | ChannelMsg::Close) | None => {
                        return Err(
                            "SCP helper execution is unavailable on this endpoint".to_owned()
                        );
                    }
                    _ => {}
                }
            }
        })
        .await
        .map_err(|_| "SCP helper exec timed out")??;
        Ok(guard.into_stream())
    }
    pub(crate) async fn control(
        &self,
        job: &Job,
        operation: &str,
        path: &RemotePath,
        other: Option<&RemotePath>,
        policy: ConflictPolicy,
    ) -> Result<TransferOutcome, String> {
        let extra = other.map(|path| {
            path.as_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        });
        let mut stream = self
            .stream_args(operation, path, policy, extra.as_deref())
            .await?;
        let ready = application_line(&mut stream).await?;
        if ready == b"TC_CONFLICT" {
            return Ok(conflict(policy));
        }
        if ready != b"TC_READY" {
            return Err(format!(
                "SCP operation preparation failed: {}",
                safe(&ready)
            ));
        }
        if !job.begin_commit() {
            return Ok(TransferOutcome::Cancelled);
        }
        let result = async {
            write_progress(&mut stream, b"TC_COMMIT\n").await?;
            application_line(&mut stream).await
        }
        .await;
        match result {
            Ok(reply) if reply == b"TCOK" => Ok(TransferOutcome::Completed),
            Ok(reply) if reply == b"TC_CONFLICT" => Ok(conflict(policy)),
            Ok(reply) if reply.starts_with(b"TC_FAILED:") => {
                Err(remote_failure(Some(job), &reply)?)
            }
            error => Ok(TransferOutcome::Indeterminate(format!(
                "Remote mutation result was not confirmed: {error:?}. Refresh before retrying."
            ))),
        }
    }
    pub(crate) async fn remove_tree(
        &self,
        job: &Job,
        path: &RemotePath,
    ) -> Result<TransferOutcome, String> {
        let mut stream = self
            .stream("remove_tree", path, ConflictPolicy::Ask)
            .await?;
        self.bulk_steps(job, &mut stream).await
    }
    async fn bulk_steps(&self, job: &Job, stream: &mut Stream) -> Result<TransferOutcome, String> {
        loop {
            let ready = preparation_line(stream).await?;
            let last = match ready.as_slice() {
                b"TC_STEP:0" => false,
                b"TC_STEP:1" => true,
                _ => {
                    return Err(format!(
                        "Bulk operation preparation failed: {}",
                        safe(&ready)
                    ));
                }
            };
            if !job.begin_step() {
                return Ok(TransferOutcome::Cancelled);
            }
            let reply = async {
                write_progress(stream, b"TC_COMMIT\n").await?;
                application_line(stream).await
            }
            .await;
            match reply {
                Ok(reply) if reply == b"TC_DONE" => {
                    if !job.end_step(last) {
                        return Ok(TransferOutcome::Cancelled);
                    }
                    if last {
                        return Ok(TransferOutcome::Completed);
                    }
                }
                Ok(reply) if last && reply.starts_with(b"TC_DONE_WARN:") => {
                    job.end_step(true);
                    return Ok(TransferOutcome::CompletedWithWarning(safe(&decode_hex(
                        &reply[13..],
                    )?)));
                }
                Ok(reply) if reply.starts_with(b"TC_FAILED:") => {
                    return Err(remote_failure(Some(job), &reply)?);
                }
                other => {
                    return Ok(TransferOutcome::Indeterminate(format!(
                        "Current entry result was not confirmed: {other:?}. Refresh before retrying."
                    )));
                }
            }
        }
    }
    pub async fn list(
        &self,
        connection: u64,
        request: u64,
        path: RemotePath,
    ) -> Result<DirectorySnapshot, String> {
        tokio::time::timeout(Duration::from_secs(10), async {
            let mut stream = self.stream("list", &path, ConflictPolicy::Ask).await?;
            let first = stream.read_u8().await.map_err(|_| "SCP directory helper unavailable; requires remote POSIX Python 3. Manual terminal remains available.".to_owned())?;
            if first == 2 { return Err(format!("SCP directory helper: {}", safe(&line(&mut stream).await?))); }
            let mut magic = [0; 6]; magic[0] = first;
            stream.read_exact(&mut magic[1..]).await.map_err(error)?;
            if &magic != b"TCFS2\0" { return Err("SCP directory helper returned an unsupported response; no listing was accepted".into()); }
            let flags = stream.read_u8().await.map_err(error)?;
            let capabilities = super::capabilities::RemoteCapabilities::scp(flags);
            if !capabilities.browse { return Err("SCP endpoint did not advertise directory enumeration".into()); }
            *self.capabilities.lock().unwrap_or_else(|p| p.into_inner()) = Some(capabilities);
            let length = stream.read_u32().await.map_err(error)? as usize;
            if length == 0 || length > 32768 { return Err("SCP helper directory path exceeds budget".into()); }
            let mut bytes = vec![0; length]; stream.read_exact(&mut bytes).await.map_err(error)?;
            let path = RemotePath::from_bytes(bytes)?;
            if !path.as_bytes().starts_with(b"/") { return Err("SCP helper returned a non-absolute directory".into()); }
            let count = stream.read_u32().await.map_err(error)? as usize;
            if count > 10000 { return Err("SCP directory exceeds 10000 entries".into()); }
            let mut budget = length; let mut entries = Vec::with_capacity(count);
            for _ in 0..count {
                let n = stream.read_u32().await.map_err(error)? as usize;
                let kind = stream.read_u8().await.map_err(error)?;
                let size = stream.read_u64().await.map_err(error)?;
                budget = budget.checked_add(n).and_then(|n| n.checked_add(length + 16)).ok_or("SCP directory budget overflow")?;
                if n == 0 || n > 32768 || budget > 4 * 1024 * 1024 { return Err("SCP directory exceeds 4 MiB path budget".into()); }
                let mut name = vec![0; n]; stream.read_exact(&mut name).await.map_err(error)?;
                let child = path.child(&name)?;
                entries.push(RemoteEntry { path: child, display_name: RemotePath::from_bytes(name)?.display(), kind: match kind {1 => EntryKind::Directory, 2 => EntryKind::File, 3 => EntryKind::Symlink, _ => EntryKind::Other}, size: Some(size) });
            }
            Ok(DirectorySnapshot { connection, request, path, entries })
        }).await.map_err(|_| "SCP directory helper timed out".to_owned())?
    }
    pub(crate) async fn upload(
        &self,
        job: &Job,
        file: tokio::fs::File,
        remote: RemotePath,
        mode: u32,
        policy: ConflictPolicy,
    ) -> Result<TransferOutcome, String> {
        let before = file.metadata().await.map_err(error)?;
        if !before.is_file() {
            return Err("SCP compatibility requires a regular source file".into());
        }
        Ok(self
            .upload_source(
                job,
                UploadSource::File { file, before },
                remote,
                mode,
                policy,
                None,
                false,
            )
            .await?
            .outcome)
    }
    #[cfg(any(unix, windows))]
    pub(crate) async fn upload_tree(
        &self,
        job: &Job,
        file: tokio::fs::File,
        remote: RemotePath,
        policy: ConflictPolicy,
    ) -> Result<TransferOutcome, String> {
        let before = file.metadata().await.map_err(error)?;
        Ok(self
            .upload_source(
                job,
                UploadSource::File { file, before },
                remote,
                0o600,
                policy,
                None,
                true,
            )
            .await?
            .outcome)
    }
    async fn upload_editor(
        &self,
        job: &Job,
        bytes: Vec<u8>,
        remote: RemotePath,
        expected: Version,
    ) -> Result<UploadResult, String> {
        let mode = expected
            .metadata
            .permissions
            .ok_or("Remote editor omitted file permissions")?
            & 0o777;
        self.upload_source(
            job,
            UploadSource::Memory(std::io::Cursor::new(bytes)),
            remote,
            mode,
            ConflictPolicy::Overwrite,
            Some(expected),
            false,
        )
        .await
    }
    async fn upload_source(
        &self,
        job: &Job,
        mut source: UploadSource,
        remote: RemotePath,
        mode: u32,
        policy: ConflictPolicy,
        expected: Option<Version>,
        folder: bool,
    ) -> Result<UploadResult, String> {
        let size = source.len();
        let mut stream = self
            .stream_version(
                if folder {
                    "put_tree"
                } else if expected.is_some() {
                    "put_editor"
                } else {
                    "put"
                },
                &remote,
                policy,
                expected.as_ref(),
            )
            .await?;
        if !ack_job(&mut stream, Some(job)).await? {
            return Ok(UploadResult {
                outcome: conflict(policy),
                metadata: None,
            });
        }
        let metadata = application_line(&mut stream).await?;
        stage_metadata(job, &metadata, b"TCSTAGING:")?;
        stream
            .write_all(format!("C{:04o} {size} payload\n", mode & 0o777).as_bytes())
            .await
            .map_err(error)?;
        if !ack_job(&mut stream, Some(job)).await? {
            return Ok(UploadResult {
                outcome: conflict(policy),
                metadata: None,
            });
        }
        let mut bytes = 0;
        let mut buffer = vec![0; 32 * 1024];
        while bytes < size {
            let n = (size - bytes).min(buffer.len() as u64) as usize;
            source.read_exact(&mut buffer[..n]).await?;
            write_progress(&mut stream, &buffer[..n]).await?;
            bytes += n as u64;
            job.advance(bytes);
        }
        if source.read_extra().await? != 0 {
            return Err(
                "Readable data exceeds advertised SCP source size; destination was not committed"
                    .into(),
            );
        }
        if !source.unchanged().await? {
            return Err("SCP source changed during upload; destination was not committed".into());
        }
        stream.write_all(&[0]).await.map_err(error)?;
        if !ack_job(&mut stream, Some(job)).await? {
            return Ok(UploadResult {
                outcome: conflict(policy),
                metadata: None,
            });
        }
        let staged = application_line(&mut stream).await?;
        stage_metadata(job, &staged, b"TCSTAGED:")?;
        if folder {
            let kind = application_line(&mut stream).await?;
            if kind == b"TC_BULK" {
                let outcome = self.bulk_steps(job, &mut stream).await?;
                if outcome == TransferOutcome::Completed {
                    job.recovery(None);
                }
                return Ok(UploadResult {
                    outcome,
                    metadata: None,
                });
            }
            if kind != b"TC_SINGLE" {
                return Err("Invalid folder publication mode".into());
            }
        }
        if !job.begin_commit() {
            return Ok(UploadResult {
                outcome: TransferOutcome::Cancelled,
                metadata: None,
            });
        }
        let reply = async {
            write_progress(&mut stream, b"TC_COMMIT\n").await?;
            application_line(&mut stream).await
        }
        .await;
        let outcome = match reply {
            Err(message) => TransferOutcome::Indeterminate(format!("Remote commit acknowledgement was lost: {message}. Compare the destination before retrying.")),
            Ok(message) if message == b"TCOK" => { job.recovery(None); TransferOutcome::Completed }
            Ok(message) if message.starts_with(b"TC_EDITOR_SAVED:") => {
                let saved = decode_hex(&message[16..]).and_then(|bytes| serde_json::from_slice::<editor::EditorSaved>(&bytes).map_err(error));
                let saved = match saved {
                    Ok(saved) => saved,
                    Err(message) => return Ok(UploadResult { outcome: TransferOutcome::Indeterminate(format!("Remote editor commit reply was malformed: {message}. Compare the destination before retrying.")), metadata: None }),
                };
                job.recovery(None);
                return Ok(UploadResult { outcome: saved.warning.map(TransferOutcome::CompletedWithWarning).unwrap_or(TransferOutcome::Completed), metadata: Some(saved.metadata) });
            }
            Ok(message) if message.starts_with(b"TC_CONFLICT:") => { stage_metadata(job, &message, b"TC_CONFLICT:")?; conflict(policy) }
            Ok(message) if message.starts_with(b"TC_FAILED:") => return Err(remote_failure(Some(job), &message)?),
            Ok(message) if message.starts_with(b"TCWARN:") => { job.recovery(None); TransferOutcome::CompletedWithWarning(safe(&decode_hex(&message[7..])?)) }
            Ok(_) => TransferOutcome::Indeterminate("Remote commit returned an invalid acknowledgement; compare the destination before retrying".into()),
        };
        Ok(UploadResult {
            outcome,
            metadata: None,
        })
    }
    pub(crate) async fn prepare_download(
        &self,
        remote: RemotePath,
    ) -> Result<PreparedDownload, String> {
        self.prepare_download_inner(remote, false).await
    }
    async fn prepare_download_inner(
        &self,
        remote: RemotePath,
        editing: bool,
    ) -> Result<PreparedDownload, String> {
        self.prepare_download_operation(remote, editing, if editing { "get_editor" } else { "get" })
            .await
    }
    async fn prepare_download_operation(
        &self,
        remote: RemotePath,
        editing: bool,
        operation: &str,
    ) -> Result<PreparedDownload, String> {
        let mut stream = self.stream(operation, &remote, ConflictPolicy::Ask).await?;
        stream.write_all(&[0]).await.map_err(error)?;
        let header = preparation_line(&mut stream).await?;
        let folder = header.first() == Some(&b'D');
        if header.first() != Some(&b'C') && !(folder && !editing) {
            return Err(format!("SCP helper rejected download: {}", safe(&header)));
        }
        let fields = header[1..].split(|b| *b == b' ').collect::<Vec<_>>();
        if fields.len() != 3 || fields[0].len() != 4 || fields[2] != b"payload" {
            return Err("SCP helper returned an unsafe file header".into());
        }
        let mode = u32::from_str_radix(std::str::from_utf8(fields[0]).map_err(error)?, 8)
            .map_err(error)?;
        let size = std::str::from_utf8(fields[1])
            .map_err(error)?
            .parse::<u64>()
            .map_err(error)?;
        let metadata = if editing {
            let message = application_line(&mut stream).await?;
            let bytes = message
                .strip_prefix(b"TCMETA:")
                .ok_or("Remote editor omitted security metadata")?;
            Some(serde_json::from_slice::<Metadata>(&decode_hex(bytes)?).map_err(error)?)
        } else {
            None
        };
        Ok(PreparedDownload {
            folder,
            stream,
            mode,
            size,
            metadata,
        })
    }
    pub(crate) async fn download<W: tokio::io::AsyncWrite + Unpin>(
        &self,
        job: &Job,
        prepared: PreparedDownload,
        file: &mut W,
    ) -> Result<u32, String> {
        let PreparedDownload {
            mut stream,
            mode,
            size,
            ..
        } = prepared;
        stream.write_all(&[0]).await.map_err(error)?;
        let mut bytes = 0;
        let mut buffer = vec![0; 32 * 1024];
        while bytes < size {
            let n = (size - bytes).min(buffer.len() as u64) as usize;
            let n = tokio::time::timeout(Duration::from_secs(15), stream.read(&mut buffer[..n]))
                .await
                .map_err(|_| "SCP download stalled")?
                .map_err(error)?;
            if n == 0 {
                return Err("SCP source ended before its advertised size".into());
            }
            file.write_all(&buffer[..n]).await.map_err(error)?;
            bytes += n as u64;
            job.advance(bytes);
        }
        if !ack(&mut stream).await? {
            return Err("SCP helper reported a content conflict".into());
        }
        stream.write_all(&[0]).await.map_err(error)?;
        stream.shutdown().await.map_err(error)?;
        completion(&mut stream).await?;
        Ok(mode & 0o777)
    }
}
fn safe(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .flat_map(char::escape_default)
        .collect()
}
async fn line(stream: &mut Stream) -> Result<Vec<u8>, String> {
    line_limit(stream, 4096).await
}
async fn line_limit(stream: &mut Stream, limit: usize) -> Result<Vec<u8>, String> {
    tokio::time::timeout(Duration::from_secs(15), async {
        let mut bytes = Vec::new();
        loop {
            let byte = stream.read_u8().await.map_err(error)?;
            if byte == b'\n' {
                return Ok(bytes);
            }
            if bytes.len() >= limit {
                return Err("SCP helper response exceeds 4 KiB".into());
            }
            bytes.push(byte);
        }
    })
    .await
    .map_err(|_| "SCP helper response timed out".to_owned())?
}
async fn ack(stream: &mut Stream) -> Result<bool, String> {
    ack_job(stream, None).await
}
async fn ack_job(stream: &mut Stream, job: Option<&Job>) -> Result<bool, String> {
    let code = loop {
        let code = tokio::time::timeout(Duration::from_secs(15), stream.read_u8())
            .await
            .map_err(|_| "SCP acknowledgement timed out")?
            .map_err(error)?;
        if code == b'T' {
            if application_line(stream).await? != b"C_PREPARING" {
                return Err("Invalid folder preparation frame".into());
            }
        } else {
            break code;
        }
    };
    if code == 0 {
        return Ok(true);
    }
    let message = application_line(stream).await?;
    if code == 1 && message == b"TC_CONFLICT" {
        return Ok(false);
    }
    if message.starts_with(b"TC_FAILED:") {
        return Err(remote_failure(job, &message)?);
    }
    Err(format!("SCP helper failed: {}", safe(&message)))
}
fn conflict(policy: ConflictPolicy) -> TransferOutcome {
    if policy == ConflictPolicy::Skip {
        TransferOutcome::Skipped
    } else {
        TransferOutcome::Conflict
    }
}
async fn application_line(stream: &mut Stream) -> Result<Vec<u8>, String> {
    line_limit(stream, 128 * 1024).await
}
async fn write_progress(stream: &mut Stream, bytes: &[u8]) -> Result<(), String> {
    let mut written = 0;
    while written < bytes.len() {
        let n = tokio::time::timeout(Duration::from_secs(15), stream.write(&bytes[written..]))
            .await
            .map_err(|_| "SCP upload stalled")?
            .map_err(error)?;
        if n == 0 {
            return Err("SCP channel stopped accepting data".into());
        }
        written += n;
    }
    Ok(())
}
fn decode_hex(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.len() % 2 != 0 || bytes.len() > 128 * 1024 {
        return Err("Invalid SCP helper hexadecimal frame".into());
    }
    bytes
        .chunks(2)
        .map(|pair| {
            u8::from_str_radix(std::str::from_utf8(pair).map_err(error)?, 16).map_err(error)
        })
        .collect()
}
fn stage_metadata(job: &Job, message: &[u8], prefix: &[u8]) -> Result<(), String> {
    let bytes = message
        .strip_prefix(prefix)
        .ok_or("SCP helper returned invalid staging metadata")?;
    let path = decode_hex(bytes)?;
    job.recovery(if path.is_empty() {
        None
    } else {
        Some(super::transfer::RecoveryPath::Remote(
            RemotePath::from_bytes(path)?,
        ))
    });
    Ok(())
}
fn remote_failure(job: Option<&Job>, message: &[u8]) -> Result<String, String> {
    let (path, reason) = message
        .strip_prefix(b"TC_FAILED:")
        .and_then(|m| {
            m.iter()
                .position(|b| *b == b':')
                .map(|i| (&m[..i], &m[i + 1..]))
        })
        .ok_or("Invalid SCP failure frame")?;
    if let Some(job) = job {
        stage_metadata(
            job,
            &[b"TC_FAILED:".as_slice(), path].concat(),
            b"TC_FAILED:",
        )?;
    }
    Ok(format!(
        "SCP publication failed; staged data retained when available: {}",
        safe(&decode_hex(reason)?)
    ))
}
async fn completion(stream: &mut Stream) -> Result<(), String> {
    let message = line(stream).await?;
    if message == b"TCOK" {
        Ok(())
    } else {
        Err(format!("SCP commit failed: {}", safe(&message)))
    }
}

async fn preparation_line(stream: &mut Stream) -> Result<Vec<u8>, String> {
    loop {
        let frame = line(stream).await?;
        if frame != b"TC_PREPARING" {
            return Ok(frame);
        }
    }
}
