use super::*;
use crate::remote_files::editor::{
    self as document, Action, DocumentStatus, EditorRequest, MAX_DOCUMENT_BYTES, Metadata, Version,
};
#[derive(serde::Deserialize)]
pub(super) struct EditorSaved {
    pub metadata: Metadata,
    pub warning: Option<String>,
}
pub(super) struct UploadResult {
    pub outcome: TransferOutcome,
    pub metadata: Option<Metadata>,
}
impl<H: client::Handler + 'static> ScpBackend<H> {
    async fn read_editor(&self, job: &Job, path: RemotePath) -> Result<Version, String> {
        let prepared = self.prepare_download_inner(path, true).await?;
        if prepared.size > MAX_DOCUMENT_BYTES as u64 {
            return Err("Remote text exceeds the 2 MiB editor limit".into());
        }
        let metadata = prepared
            .metadata
            .clone()
            .ok_or("SCP editor response omitted security metadata")?;
        let mut bytes = Vec::with_capacity(prepared.size as usize);
        self.download(job, prepared, &mut bytes).await?;
        Ok(Version { bytes, metadata })
    }
    pub(crate) async fn editor(
        &self,
        job: &Job,
        request: &EditorRequest,
    ) -> Result<TransferOutcome, String> {
        if *request.cancelled.borrow() {
            return Ok(TransferOutcome::Cancelled);
        }
        let mut cancelled = request.cancelled.clone();
        tokio::select! { biased;
            _ = cancelled.changed() => Ok(TransferOutcome::Cancelled),
            result = self.editor_operation(job, request) => result,
        }
    }
    async fn editor_operation(
        &self,
        job: &Job,
        request: &EditorRequest,
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
                let version = self.read_editor(job, path).await?;
                document::loaded(request, root.clone(), version.bytes, version.metadata).await
            }
            Action::Save {
                bytes,
                expected,
                revision,
            } => {
                let current = self.read_editor(job, path.clone()).await?;
                if expected.as_ref() != Some(&current) {
                    return conflict(request, current);
                }
                let expected = expected.clone().ok_or("No baseline version is available")?;
                let result = self
                    .upload_editor(job, bytes.clone(), path.clone(), expected)
                    .await;
                if let Some(super::super::transfer::RecoveryPath::Remote(path)) =
                    job.recovery_path()
                {
                    let mut state = request.state.lock().unwrap_or_else(|p| p.into_inner());
                    if state.operation == request.token {
                        state.snapshot.remote_recovery = Some(path);
                    }
                }
                let result = result?;
                match result.outcome {
                    TransferOutcome::Conflict => {
                        {
                            let mut state = request.state.lock().unwrap_or_else(|p| p.into_inner());
                            if state.operation == request.token {
                                state.snapshot.status = DocumentStatus::Saving;
                            }
                        }
                        let current = self.read_editor(job, path).await?;
                        conflict(request, current)
                    }
                    TransferOutcome::Completed | TransferOutcome::CompletedWithWarning(_) => {
                        let Some(metadata) = result.metadata else {
                            return Ok(TransferOutcome::Indeterminate("Remote save completed without version metadata; compare the remote file before retrying".into()));
                        };
                        let warning = if let TransferOutcome::CompletedWithWarning(warning) =
                            result.outcome
                        {
                            Some(warning)
                        } else {
                            None
                        };
                        let mut state = request.state.lock().unwrap_or_else(|p| p.into_inner());
                        if state.operation != request.token {
                            return Ok(TransferOutcome::Cancelled);
                        }
                        state.baseline = bytes.clone();
                        state.metadata = Some(metadata);
                        state.conflict = None;
                        state.snapshot.dirty =
                            state.revision != *revision && state.snapshot.text.as_bytes() != bytes;
                        state.snapshot.remote_recovery = None;
                        state.snapshot.warning = warning;
                        state.snapshot.status = DocumentStatus::Saved;
                        Ok(TransferOutcome::Completed)
                    }
                    outcome => Ok(outcome),
                }
            }
        }
    }
}
fn conflict(request: &EditorRequest, version: Version) -> Result<TransferOutcome, String> {
    let mut state = request.state.lock().unwrap_or_else(|p| p.into_inner());
    if state.operation == request.token {
        state.conflict = Some(version);
    }
    Ok(TransferOutcome::Conflict)
}
