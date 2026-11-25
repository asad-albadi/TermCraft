//! Runtime-only binding between one X display generation and its clients.

use super::XServerLease;
use crate::ssh::{ConnectionId, x11::Forwarder};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::watch;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum XDisplaySource {
    // Keep profiles saved before the display picker on their original source.
    #[default]
    System,
    Owned,
}

impl XDisplaySource {
    pub(crate) fn is_system(&self) -> bool {
        *self == Self::System
    }
}

/// A ready, private display. A binding never follows a server restart.
#[derive(Clone)]
pub struct XClientDisplay {
    display: String,
    authority: PathBuf,
    alive: watch::Receiver<bool>,
}

impl XClientDisplay {
    pub(super) fn new(lease: &XServerLease) -> Self {
        Self {
            display: lease.display().into(),
            authority: lease.authority_path().into(),
            alive: lease.alive.subscribe(),
        }
    }

    pub fn display(&self) -> &str {
        &self.display
    }

    pub fn authority_path(&self) -> &Path {
        &self.authority
    }

    /// Pass these values to the child process's environment.
    pub fn environment(&self) -> Result<[(OsString, OsString); 2], String> {
        self.ensure_running()?;
        Ok([
            ("DISPLAY".into(), self.display.as_str().into()),
            ("XAUTHORITY".into(), self.authority.as_os_str().into()),
        ])
    }

    pub(crate) fn ensure_running(&self) -> Result<(), String> {
        if *self.alive.borrow() && self.alive.has_changed().is_ok() {
            Ok(())
        } else {
            Err("The TermCraft X server stopped. Reconnect to use its current display.".into())
        }
    }

    pub(crate) async fn stopped(&self) {
        let mut alive = self.alive.clone();
        let _ = alive.wait_for(|running| !*running).await;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XTrustMode {
    Trusted,
}

/// One embedded SSH login's forwarding credentials.
pub struct XForwardingLease {
    connection: ConnectionId,
    forwarder: Arc<Forwarder>,
}

impl Drop for XForwardingLease {
    fn drop(&mut self) {
        self.forwarder.expire();
    }
}

impl XForwardingLease {
    pub(crate) fn attach_system(connection: ConnectionId) -> Result<Self, String> {
        Ok(Self {
            connection,
            forwarder: Forwarder::from_environment()?,
        })
    }

    pub fn attach(
        connection: ConnectionId,
        server: &XServerLease,
        trust: XTrustMode,
    ) -> Result<Self, String> {
        Self::attach_display(connection, server.client_display(), trust)
    }

    pub fn connection(&self) -> ConnectionId {
        self.connection
    }

    pub(crate) fn attach_display(
        connection: ConnectionId,
        display: XClientDisplay,
        trust: XTrustMode,
    ) -> Result<Self, String> {
        let XTrustMode::Trusted = trust;
        display.ensure_running()?;
        Ok(Self {
            connection,
            forwarder: Forwarder::from_owned(display)?,
        })
    }

    pub(crate) fn forwarder(&self) -> Arc<Forwarder> {
        Arc::clone(&self.forwarder)
    }
}
