//! Secrets are deliberately neither Debug nor serializable to ordinary workspace JSON.
use zeroize::Zeroizing;
mod format;
pub mod vault;
pub type CredentialId = String;
#[derive(Clone)]
pub struct Secret(Zeroizing<String>);
impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(Zeroizing::new(value.into()))
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum CredentialRole {
    Target,
    Gateway,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum SshSecretKind {
    Password,
    KeyPassphrase { key_digest: String },
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub enum CredentialBinding {
    Rdp {
        host: String,
        port: u16,
        user: String,
        domain: String,
        certificate: String,
        profile: String,
    },
    Ssh {
        role: CredentialRole,
        host: String,
        port: u16,
        user: String,
        host_key: String,
        profile: String,
        kind: SshSecretKind,
    },
    HttpsBasic {
        source_id: String,
        url: String,
    },
    Proxy {
        kind: crate::model::ProxyKind,
        host: String,
        port: u16,
        profile: String,
    },
}
impl CredentialBinding {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Rdp {
                host,
                port,
                user,
                domain,
                certificate,
                profile,
            } => {
                crate::model::validate_host(host)?;
                if *port == 0
                    || host.len() > 4096
                    || user.is_empty()
                    || user.len() > 4096
                    || user.chars().any(char::is_control)
                    || domain.len() > 255
                    || domain.chars().any(char::is_control)
                    || !digest(certificate)
                    || !digest(profile)
                {
                    return Err("Invalid RDP credential binding.".into());
                }
            }
            Self::Ssh {
                host,
                port,
                user,
                host_key,
                profile,
                kind,
                ..
            } => {
                crate::model::validate_host(host)?;
                crate::model::validate_user(user)?;
                if *port == 0
                    || host.len() > 4096
                    || user.len() > 4096
                    || user.is_empty()
                    || host_key.is_empty()
                    || host_key.len() > 1024
                    || host_key.chars().any(char::is_control)
                    || !digest(profile)
                {
                    return Err("Invalid SSH credential binding.".into());
                }
                if let SshSecretKind::KeyPassphrase { key_digest } = kind {
                    if !digest(key_digest) {
                        return Err("Invalid key credential binding.".into());
                    }
                }
            }
            Self::Proxy {
                host,
                port,
                profile,
                ..
            } => {
                crate::model::validate_host(host)?;
                if host.len() > 255 || *port == 0 || !digest(profile) {
                    return Err("Invalid proxy credential binding.".into());
                }
            }
            Self::HttpsBasic { source_id, url } => {
                if !valid_id(source_id) {
                    return Err("Invalid source credential binding.".into());
                }
                let parsed =
                    url::Url::parse(url).map_err(|_| "Invalid HTTPS credential binding")?;
                if parsed.scheme() != "https"
                    || parsed.host_str().is_none()
                    || !parsed.username().is_empty()
                    || parsed.password().is_some()
                    || parsed.query().is_some()
                    || parsed.fragment().is_some()
                    || url.len() > 8192
                {
                    return Err("Invalid HTTPS credential binding.".into());
                }
            }
        }
        Ok(())
    }
}
fn digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|c| c.is_ascii_hexdigit())
}
fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}

pub mod agent;
pub mod keys;
