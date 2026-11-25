//! Source-scoped, read-only snapshots. A refresh never receives mutable local Store access.
use crate::model::{Session, SessionId, new_session_id};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum SharedFormat {
    #[default]
    Workspace,
    Moba,
    Putty,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SharedLocation {
    File(PathBuf),
    Https(String),
    Sftp { session_id: SessionId, path: String },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SharedEntry {
    pub id: String,
    pub session: Session,
    pub requires_review: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SharedSnapshot {
    pub source_id: String,
    pub revision: String,
    pub entries: Arc<Vec<SharedEntry>>,
    pub stale: bool,
    pub last_error: Option<String>,
    pub warnings: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SharedSourceConfig {
    pub id: String,
    pub name: String,
    pub location: SharedLocation,
    pub format: SharedFormat,
    #[serde(default)]
    pub credential_ref: Option<String>,
    #[serde(default)]
    pub cached: Option<SharedSnapshot>,
}
impl SharedSourceConfig {
    pub fn new(name: &str, location: SharedLocation, format: SharedFormat) -> Result<Self, String> {
        let config = Self {
            id: format!("shared-{}", new_session_id()),
            name: name.trim().into(),
            location,
            format,
            credential_ref: None,
            cached: None,
        };
        config.validate()?;
        Ok(config)
    }
    pub fn from_url(name: &str, uri: &str, format: SharedFormat) -> Result<Self, String> {
        let url = url::Url::parse(uri).map_err(|_| "Invalid shared-source URL")?;
        if url.scheme() != "https" {
            return Err(
                "Use a shared file, HTTPS URL, or an SFTP source linked to a saved SSH session"
                    .into(),
            );
        }
        Self::new(name, SharedLocation::Https(url.into()), format)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty()
            || self.id.len() > 256
            || self.name.is_empty()
            || self.name.len() > 512
            || self.name.chars().any(char::is_control)
        {
            return Err("Shared source needs a bounded name and identity".into());
        }
        if self
            .credential_ref
            .as_ref()
            .is_some_and(|r| r.is_empty() || r.len() > 256 || r.chars().any(char::is_control))
        {
            return Err("Invalid credential reference".into());
        }
        match &self.location {
            SharedLocation::File(path) => {
                if path.as_os_str().is_empty()
                    || path
                        .to_str()
                        .is_none_or(|s| s.chars().any(char::is_control))
                {
                    return Err("Choose a shared file".into());
                }
            }
            SharedLocation::Https(uri) => {
                let url = url::Url::parse(uri).map_err(|_| "Invalid HTTPS source")?;
                if url.scheme() != "https"
                    || url.host_str().is_none()
                    || !url.username().is_empty()
                    || url.password().is_some()
                    || url.fragment().is_some()
                    || url.query().is_some()
                {
                    return Err("HTTPS source must have a host and no URL credentials, query or fragment; use a credential reference for authentication".into());
                }
            }
            SharedLocation::Sftp { session_id, path } => {
                if session_id.is_empty()
                    || path.is_empty()
                    || path.len() > 4096
                    || path.chars().any(char::is_control)
                {
                    return Err("Choose a saved SSH session and a remote file path".into());
                }
            }
        }
        Ok(())
    }
    pub fn inherit_metadata(&mut self, previous: &Self) {
        self.id = previous.id.clone();
        self.cached = None;
        self.credential_ref = previous.credential_ref.clone();
        if self.binding().ok() != previous.binding().ok() {
            self.credential_ref = None;
        }
    }
    pub fn metadata_only(&self) -> Self {
        let mut config = self.clone();
        config.cached = None;
        config
    }
    pub fn binding(&self) -> Result<String, String> {
        let mut config = self.metadata_only();
        config.name.clear();
        Ok(hash(&serde_json::to_vec(&config).map_err(
            |_| "Source configuration could not be encoded",
        )?))
    }
}
pub trait SharedReader: Send + Sync {
    fn fetch(&self, config: &SharedSourceConfig, cancel: &AtomicBool) -> Result<Vec<u8>, String>;
}
pub struct FileReader;
impl SharedReader for FileReader {
    fn fetch(&self, config: &SharedSourceConfig, cancel: &AtomicBool) -> Result<Vec<u8>, String> {
        use std::io::Read;
        let SharedLocation::File(path) = &config.location else {
            return Err(
                "This reader requires a shared file; configure a network reader for HTTPS/SFTP"
                    .into(),
            );
        };
        if cancel.load(Ordering::Acquire) {
            return Err("Refresh cancelled".into());
        }
        let meta = std::fs::metadata(path).map_err(|_| "Shared file is unavailable")?;
        if !meta.is_file() || meta.len() > super::MAX_INPUT as u64 {
            return Err("Shared source must be a regular file no larger than 8 MiB".into());
        }
        let file = std::fs::File::open(path).map_err(|_| "Could not open shared file")?;
        let mut bytes = Vec::new();
        file.take(super::MAX_INPUT as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "Could not read shared file")?;
        if bytes.len() > super::MAX_INPUT {
            return Err("Shared source exceeds 8 MiB".into());
        }
        if cancel.load(Ordering::Acquire) {
            return Err("Refresh cancelled".into());
        }
        Ok(bytes)
    }
}
pub struct SharedSource {
    config: SharedSourceConfig,
    reader: Arc<dyn SharedReader>,
    cancel: Arc<AtomicBool>,
}
impl SharedSource {
    pub fn new(config: SharedSourceConfig) -> Self {
        Self::with_reader(config, Arc::new(FileReader))
    }
    pub fn with_reader(mut config: SharedSourceConfig, reader: Arc<dyn SharedReader>) -> Self {
        if let Some(snapshot) = &mut config.cached {
            for entry in Arc::make_mut(&mut snapshot.entries) {
                entry.session.clear_credential_refs();
            }
        }
        Self {
            config,
            reader,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
    pub fn with_cancellation(mut self, cancel: Arc<AtomicBool>) -> Self {
        self.cancel = cancel;
        self
    }
    pub fn config(&self) -> &SharedSourceConfig {
        &self.config
    }
    pub fn snapshot(&self) -> Option<&SharedSnapshot> {
        self.config.cached.as_ref()
    }
    pub fn cancellation(&self) -> Arc<AtomicBool> {
        self.cancel.clone()
    }
    pub fn refresh(&mut self) -> Result<SharedSnapshot, String> {
        let result = self
            .config
            .validate()
            .and_then(|()| self.reader.fetch(&self.config, &self.cancel))
            .and_then(|bytes| self.parse(&bytes));
        match result {
            Ok(snapshot) => {
                self.config.cached = Some(snapshot.clone());
                Ok(snapshot)
            }
            Err(error) => {
                if let Some(cache) = &mut self.config.cached {
                    cache.stale = true;
                    cache.last_error = Some(error.chars().take(1024).collect());
                }
                Err(error)
            }
        }
    }
    pub fn copy_to_local(&self, id: &str) -> Result<Session, String> {
        let entry = self
            .snapshot()
            .and_then(|s| s.entries.iter().find(|e| e.id == id))
            .ok_or("Shared definition no longer exists")?;
        let mut session = entry.session.clone();
        session.id = new_session_id();
        session.clear_credential_refs();
        Ok(session)
    }
    fn parse(&self, bytes: &[u8]) -> Result<SharedSnapshot, String> {
        if bytes.len() > super::MAX_INPUT {
            return Err("Shared source exceeds 8 MiB".into());
        }
        let mut entries = Vec::new();
        let mut warnings = Vec::new();
        match self.config.format {
            SharedFormat::Workspace => {
                let value: serde_json::Value = serde_json::from_slice(bytes)
                    .map_err(|_| "Shared workspace JSON could not be read")?;
                let rows = value
                    .get("sessions")
                    .and_then(serde_json::Value::as_array)
                    .ok_or("Shared workspace has no sessions array")?;
                if rows.len() > super::MAX_SESSIONS {
                    return Err("Shared source exceeds 10,000 definitions".into());
                }
                let mut used = std::collections::HashSet::new();
                for row in rows {
                    let id=row.get("id").and_then(serde_json::Value::as_str).filter(|id|!id.is_empty()&&id.len()<=256).ok_or("Shared workspace sessions need stable IDs; re-save the source with a current TermCraft version")?;
                    if !used.insert(id) {
                        return Err("Shared source contains duplicate session IDs".into());
                    }
                    let session: Session = serde_json::from_value(row.clone())
                        .map_err(|_| "Invalid shared session")?;
                    entries.push(self.entry(id, session)?);
                }
            }
            format => {
                let report = match format {
                    SharedFormat::Moba => super::import_moba(bytes),
                    SharedFormat::Putty => super::import_putty(bytes),
                    _ => unreachable!(),
                }?;
                for candidate in report.candidates {
                    let key = format!(
                        "{}:{}:{}",
                        candidate.location.section.len(),
                        candidate.location.section,
                        candidate.location.entry
                    );
                    entries.push(self.entry(&key, candidate.session)?);
                }
                warnings.extend(
                    report
                        .unsupported
                        .into_iter()
                        .take(100)
                        .map(|f| format!("Line {}: {} — {}", f.location.line, f.field, f.message)),
                );
                warnings.push("This format has no stable upstream IDs; a rename replaces the shared node, without changing local copies.".into());
            }
        }
        if self.cancel.load(Ordering::Acquire) {
            return Err("Refresh cancelled".into());
        }
        let revision = hash(
            &serde_json::to_vec(&entries).map_err(|_| "Shared definitions could not be encoded")?,
        );
        Ok(SharedSnapshot {
            source_id: self.config.id.clone(),
            revision,
            entries: Arc::new(entries),
            stale: false,
            last_error: None,
            warnings,
        })
    }
    fn entry(&self, key: &str, mut session: Session) -> Result<SharedEntry, String> {
        if session.name.len() > 4096
            || session.command.len() > 65536
            || session.folder.len() > 4096
            || session.host.len() > 4096
            || session.key.len() > 4096
            || session.user.len() > 4096
        {
            return Err("Shared definition contains oversized fields".into());
        }
        if !crate::model::PROTOCOLS.contains(&session.protocol.as_str()) {
            return Err("Shared definition contains an unsupported protocol".into());
        }
        let id = format!(
            "shared-entry-{}",
            hash(format!("{}:{}:{key}", self.config.id.len(), self.config.id).as_bytes())
        );
        session.id = id.clone();
        // Source data cannot select credentials from this machine.
        session.clear_credential_refs();
        let requires_review = !session.command.is_empty()
            || !session.key.is_empty()
            || !session.ssh_options.is_default()
            || matches!(session.protocol.as_str(), "Local" | "Serial" | "Browser");
        Ok(SharedEntry {
            id,
            session,
            requires_review,
        })
    }
}
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
