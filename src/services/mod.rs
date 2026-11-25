//! Listeners and protocol workers owned by the current workspace.
pub mod cron;
mod files;
mod iperf;
mod nfs;
mod sshd;
pub use iperf::{IperfServiceConfig, IperfVersion};
pub use sshd::{SshAuthorizedUser, SshServiceConfig};
mod ftp;
mod supervisor;
mod vnc;
pub use vnc::{VncServiceConfig, VncSource};
mod telnet;
mod tftp;

pub use supervisor::{ServiceId, ServiceState, ServiceStatus, ServiceSupervisor};

use crate::credentials::Secret;
use sha2::{Digest, Sha256};
use std::{net::SocketAddr, path::PathBuf, time::Duration};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceProtocol {
    Ftp,
    Tftp,
    Nfs,
    Telnet,
    Ssh,
    Iperf,
    Vnc,
    Cron,
}

pub enum ServiceAuth {
    Password { username: String, password: Secret },
    Anonymous,
    Unauthenticated,
}

pub enum ServiceRuntime {
    Embedded,
    Cron(cron::CronServiceConfig),
    Ssh(SshServiceConfig),
    Iperf(IperfServiceConfig),
    Vnc(VncServiceConfig),
    Shell { executable: PathBuf },
}

pub struct ServiceConfig {
    pub protocol: ServiceProtocol,
    pub bind: SocketAddr,
    pub allow_non_loopback: bool,
    pub root: Option<PathBuf>,
    pub auth: ServiceAuth,
    pub allow_write: bool,
    pub runtime: ServiceRuntime,
    pub lifetime: Option<Duration>,
}

impl ServiceConfig {
    pub fn ftp(root: PathBuf, username: String, password: Secret) -> Self {
        Self {
            protocol: ServiceProtocol::Ftp,
            root: Some(root),
            auth: ServiceAuth::Password { username, password },
            ..Self::local(ServiceProtocol::Ftp)
        }
    }

    /// NFSv3 and MOUNTv3 over one TCP port, /share export; AUTH_SYS/anonymous
    /// clients share this account's access. Read-only and loopback by default.
    pub fn nfs(root: PathBuf) -> Self {
        Self {
            root: Some(root),
            ..Self::local(ServiceProtocol::Nfs)
        }
    }

    pub fn tftp(root: PathBuf) -> Self {
        Self {
            root: Some(root),
            ..Self::local(ServiceProtocol::Tftp)
        }
    }

    pub fn telnet(shell: PathBuf, username: String, password: Secret) -> Self {
        Self {
            auth: ServiceAuth::Password { username, password },
            runtime: ServiceRuntime::Shell { executable: shell },
            ..Self::local(ServiceProtocol::Telnet)
        }
    }

    fn local(protocol: ServiceProtocol) -> Self {
        Self {
            protocol,
            bind: ([127, 0, 0, 1], 0).into(),
            allow_non_loopback: false,
            root: None,
            auth: ServiceAuth::Unauthenticated,
            allow_write: false,
            runtime: ServiceRuntime::Embedded,
            lifetime: None,
        }
    }
}

pub(super) enum Auth {
    Password {
        user: String,
        digest: Zeroizing<[u8; 32]>,
    },
    Anonymous,
    None,
}

impl Auth {
    fn prepare(config: ServiceAuth) -> Result<Self, String> {
        match config {
            ServiceAuth::Anonymous => Ok(Self::Anonymous),
            ServiceAuth::Unauthenticated => Ok(Self::None),
            ServiceAuth::Password { username, password } => {
                if username.is_empty()
                    || username.len() > 255
                    || username.bytes().any(|byte| byte.is_ascii_control())
                    || password.expose().is_empty()
                    || password.expose().len() > 1024
                    || password
                        .expose()
                        .bytes()
                        .any(|byte| byte.is_ascii_control())
                {
                    return Err("Enter a username and password without control characters.".into());
                }
                let digest = Zeroizing::new(Sha256::digest(password.expose().as_bytes()).into());
                Ok(Self::Password {
                    user: username,
                    digest,
                })
            }
        }
    }

    fn allows(&self, user: &[u8], password: &[u8]) -> bool {
        match self {
            Self::Password {
                user: expected,
                digest,
            } => {
                let supplied: Zeroizing<[u8; 32]> = Zeroizing::new(Sha256::digest(password).into());
                bool::from(digest.ct_eq(&*supplied)) && user == expected.as_bytes()
            }
            Self::Anonymous => user.eq_ignore_ascii_case(b"anonymous"),
            Self::None => false,
        }
    }
}

#[cfg(test)]
mod nfs_lifecycle_tests {
    use super::*;
    use std::{net::TcpStream, thread, time::Instant};
    #[test]
    fn nfs_supervisor_owns_lifecycle_and_rejects_unapproved_network_exposure() {
        let root = tempfile::tempdir().unwrap();
        let supervisor = ServiceSupervisor::default();
        let mut denied = ServiceConfig::nfs(root.path().to_owned());
        denied.bind = "0.0.0.0:0".parse().unwrap();
        assert!(supervisor.start(denied).is_err());
        let mut denied = ServiceConfig::nfs(root.path().to_owned());
        denied.auth = ServiceAuth::Anonymous;
        assert!(supervisor.start(denied).is_err());
        assert!(
            supervisor
                .start(ServiceConfig::nfs(PathBuf::from("relative")))
                .is_err()
        );
        let config = ServiceConfig::nfs(root.path().to_owned());
        assert!(!config.allow_write);
        assert!(!config.allow_non_loopback);
        let id = supervisor.start(config).unwrap();
        let status = supervisor.status(id).unwrap();
        assert_eq!(status.state, ServiceState::Running);
        let address = status.address.unwrap();
        assert!(TcpStream::connect(address).is_ok());
        supervisor.stop(id).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while supervisor.status(id).unwrap().state != ServiceState::Stopped {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        assert!(TcpStream::connect(address).is_err());
        let id = supervisor
            .start(ServiceConfig::nfs(root.path().to_owned()))
            .unwrap();
        assert_eq!(supervisor.status(id).unwrap().state, ServiceState::Running);
        let address = supervisor.status(id).unwrap().address.unwrap();
        drop(supervisor);
        assert!(TcpStream::connect(address).is_err());
    }
}

#[cfg(test)]
mod vnc_service_tests;
