//! Network readers use verified TLS or the existing embedded SSH engine, never installed clients.
use crate::{
    model::Session,
    remote_files::RemotePath,
    sessions::{
        MAX_INPUT,
        shared::{FileReader, SharedLocation, SharedReader, SharedSourceConfig},
    },
    ssh::{
        ChallengeAnswer, ConnectionConfig, ConnectionEvent, ConnectionHandle, Prompt, TrustDecision,
    },
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use zeroize::Zeroizing;
static NEXT_OWNER: AtomicU64 = AtomicU64::new(1u64 << 63);
pub struct SourceChallenge {
    pub source_id: String,
    pub owner: u64,
    pub answer_id: u64,
    pub endpoint: Option<String>,
    pub title: String,
    pub prompts: Vec<Prompt>,
    pub host_key: bool,
    pub credential_ref: Option<String>,
    pub credential: Option<crate::ssh::ChallengeCredential>,
}
pub trait SourceAuth: Send + Sync {
    fn answer(
        &self,
        challenge: SourceChallenge,
        cancel: &AtomicBool,
    ) -> Result<ChallengeAnswer, String>;
}
struct NoInteractiveAuth;
impl SourceAuth for NoInteractiveAuth {
    fn answer(&self, _: SourceChallenge, _: &AtomicBool) -> Result<ChallengeAnswer, String> {
        Err("Shared source requires a private sign-in or host verification".into())
    }
}
pub struct NetworkReader {
    defaults: crate::model::SshDefaults,
    agent: Option<crate::credentials::agent::AgentEndpoint>,
    data_dir: PathBuf,
    sessions: Vec<Session>,
    auth: Arc<dyn SourceAuth>,
    tls: ureq::tls::TlsConfig,
    user_known_hosts: Vec<PathBuf>,
}
impl NetworkReader {
    pub fn with_defaults(mut self, defaults: crate::model::SshDefaults) -> Self {
        self.defaults = defaults;
        self
    }
    pub fn with_agent_endpoint(
        mut self,
        endpoint: Option<crate::credentials::agent::AgentEndpoint>,
    ) -> Self {
        self.agent = endpoint;
        self
    }
    pub fn new(data_dir: PathBuf, sessions: Vec<Session>) -> Self {
        Self {
            agent: None,
            defaults: Default::default(),
            data_dir,
            sessions,
            auth: Arc::new(NoInteractiveAuth),
            user_known_hosts: vec![crate::model::home_dir().join(".ssh/known_hosts")],
            tls: ureq::tls::TlsConfig::builder()
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        }
    }
    pub fn dependency(&self, config: &SharedSourceConfig) -> Result<String, String> {
        use sha2::{Digest, Sha256};
        config.validate()?;
        let mut bytes = config.binding()?.into_bytes();
        if let SharedLocation::Sftp { session_id, .. } = &config.location {
            let mut profile = self
                .sessions
                .iter()
                .find(|s| &s.id == session_id)
                .ok_or("Saved SSH profile no longer exists")?
                .clone();
            profile.name.clear();
            profile.folder.clear();
            profile.directory.clear();
            profile.command.clear();
            profile.x11 = false;
            profile.terminal = Default::default();
            profile.highlight_enabled = None;
            profile.fullscreen = false;
            profile.view_only = false;
            profile.width = 0;
            profile.height = 0;
            bytes.extend(
                serde_json::to_vec(&profile).map_err(|_| "Could not bind SSH source profile")?,
            );
        }
        Ok(Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect())
    }
    pub fn with_user_known_hosts(mut self, paths: Vec<PathBuf>) -> Self {
        self.user_known_hosts = paths;
        self
    }
    pub fn with_auth(mut self, auth: Arc<dyn SourceAuth>) -> Self {
        self.auth = auth;
        self
    }
    pub fn with_trusted_certificate(mut self, pem: &[u8]) -> Result<Self, String> {
        let cert = ureq::tls::Certificate::from_pem(pem)
            .map_err(|_| "Could not read trusted certificate")?;
        self.tls = ureq::tls::TlsConfig::builder()
            .root_certs(ureq::tls::RootCerts::new_with_certs(&[cert]))
            .build();
        Ok(self)
    }
    fn https(
        &self,
        config: &SharedSourceConfig,
        uri: &str,
        cancel: &AtomicBool,
    ) -> Result<Vec<u8>, String> {
        use base64::Engine;
        use std::io::Read;
        let url = url::Url::parse(uri).map_err(|_| "Invalid HTTPS source")?;
        let endpoint = url.origin().ascii_serialization();
        let owner = NEXT_OWNER.fetch_add(1, Ordering::Relaxed);
        let agent = ureq::Agent::config_builder()
            .https_only(true)
            .max_redirects(0)
            .timeout_global(Some(Duration::from_secs(20)))
            .timeout_per_read(Some(Duration::from_secs(3)))
            .tls_config(self.tls.clone())
            .http_status_as_error(false)
            .build()
            .new_agent();
        let mut authorization = None;
        for attempt in 0..2 {
            if cancel.load(Ordering::Acquire) {
                return Err("Refresh cancelled".into());
            }
            let mut request = agent.get(uri);
            if let Some(header) = &authorization {
                request = request.header("authorization", header);
            }
            let mut response = request
                .call()
                .map_err(|_| "HTTPS connection, certificate verification or request failed")?;
            let status = response.status().as_u16();
            if status == 401 && attempt == 0 {
                let basic = response
                    .headers()
                    .get("www-authenticate")
                    .and_then(|v| v.to_str().ok())
                    .is_some_and(|v| v.to_ascii_lowercase().starts_with("basic"));
                if !basic {
                    return Err("HTTPS source requires an unsupported authentication method".into());
                }
                let answer = self.auth.answer(
                    SourceChallenge {
                        source_id: config.id.clone(),
                        owner,
                        answer_id: 1,
                        endpoint: Some(endpoint.clone()),
                        title: format!("{} · HTTPS sign-in to {endpoint}", config.name),
                        prompts: vec![
                            Prompt {
                                label: "Username".into(),
                                echo: true,
                            },
                            Prompt {
                                label: "Password".into(),
                                echo: false,
                            },
                        ],
                        host_key: false,
                        credential_ref: config.credential_ref.clone(),
                        credential: Some(crate::ssh::ChallengeCredential {
                            reference: config.credential_ref.clone(),
                            binding: crate::credentials::CredentialBinding::HttpsBasic {
                                source_id: config.id.clone(),
                                url: uri.to_string(),
                            },
                        }),
                    },
                    cancel,
                )?;
                if answer.id != 1
                    || answer.trust == Some(TrustDecision::Reject)
                    || answer.values.len() != 2
                    || answer.values[0].contains(':')
                {
                    return Err("Source sign-in was cancelled or invalid".into());
                }
                let credentials = Zeroizing::new(format!(
                    "{}:{}",
                    answer.values[0].as_str(),
                    answer.values[1].as_str()
                ));
                let encoded = Zeroizing::new(format!(
                    "Basic {}",
                    base64::engine::general_purpose::STANDARD.encode(credentials.as_bytes())
                ));
                let mut header = encoded
                    .parse::<ureq::http::HeaderValue>()
                    .map_err(|_| "Could not encode source sign-in")?;
                header.set_sensitive(true);
                authorization = Some(header);
                continue;
            }
            if status != 200 {
                return Err(format!(
                    "HTTPS source returned status {status}; redirects are not followed"
                ));
            }
            if response
                .headers()
                .get("content-length")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .is_some_and(|n| n > MAX_INPUT as u64)
            {
                return Err("HTTPS source exceeds 8 MiB".into());
            }
            let mut bytes = Vec::new();
            let mut buffer = [0u8; 32768];
            let mut reader = response.body_mut().as_reader();
            loop {
                if cancel.load(Ordering::Acquire) {
                    return Err("Refresh cancelled".into());
                }
                let n = reader
                    .read(&mut buffer)
                    .map_err(|_| "HTTPS source body could not be read within its limits")?;
                if n == 0 {
                    break;
                }
                if bytes.len() + n > MAX_INPUT {
                    return Err("HTTPS source exceeds 8 MiB".into());
                }
                bytes.extend_from_slice(&buffer[..n]);
            }
            return Ok(bytes);
        }
        Err("HTTPS sign-in was rejected".into())
    }
    fn sftp(
        &self,
        config: &SharedSourceConfig,
        session_id: &str,
        path: &str,
        cancel: &AtomicBool,
    ) -> Result<Vec<u8>, String> {
        let session = self
            .sessions
            .iter()
            .find(|s| s.id == session_id)
            .ok_or("The saved SSH session for this source no longer exists")?
            .clone();
        if session.protocol != "SSH" || session.ssh_engine != crate::model::SshEngine::Embedded {
            return Err("SFTP sources require a saved embedded SSH profile; external OpenSSH-only options cannot be applied".into());
        }
        let owner = NEXT_OWNER.fetch_add(1, Ordering::Relaxed);
        let request = owner;
        let mut handle = ConnectionHandle::read_file(
            ConnectionConfig {
                id: owner,
                agent: self.agent.clone(),
                defaults: self.defaults.clone(),
                clock: None,
                session,
                app_known_hosts: self.data_dir.join("ssh/known_hosts"),
                user_known_hosts: self.user_known_hosts.clone(),
            },
            request,
            RemotePath::from_bytes(path.as_bytes().to_vec())?,
            MAX_INPUT,
        )?;
        let mut deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if cancel.load(Ordering::Acquire) {
                handle.cancel();
                return Err("Refresh cancelled".into());
            }
            if Instant::now() > deadline {
                handle.cancel();
                return Err("SFTP source timed out".into());
            }
            for event in handle.poll() {
                match event {
                    ConnectionEvent::FileContents {
                        connection,
                        request: r,
                        bytes,
                    } if connection == owner && r == request => return Ok(bytes),
                    ConnectionEvent::Challenge {
                        connection,
                        id,
                        title,
                        prompts,
                        host_key,
                        credential,
                    } if connection == owner => {
                        let answer = self.auth.answer(
                            SourceChallenge {
                                source_id: config.id.clone(),
                                owner,
                                answer_id: id,
                                endpoint: None,
                                title: format!("{} · {title}", config.name),
                                prompts,
                                host_key,
                                credential_ref: credential
                                    .as_ref()
                                    .and_then(|c| c.reference.clone()),
                                credential,
                            },
                            cancel,
                        )?;
                        if answer.id != id {
                            return Err("Stale shared-source authentication answer".into());
                        }
                        handle.answer(answer)?;
                        deadline = Instant::now() + Duration::from_secs(60);
                    }
                    ConnectionEvent::Exited {
                        connection,
                        message,
                    } if connection == owner => return Err(message),
                    _ => {}
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
impl SharedReader for NetworkReader {
    fn fetch(&self, config: &SharedSourceConfig, cancel: &AtomicBool) -> Result<Vec<u8>, String> {
        config.validate()?;
        match &config.location {
            SharedLocation::File(_) => FileReader.fetch(config, cancel),
            SharedLocation::Https(uri) => self.https(config, uri, cancel),
            SharedLocation::Sftp { session_id, path } => {
                self.sftp(config, session_id, path, cancel)
            }
        }
    }
}
