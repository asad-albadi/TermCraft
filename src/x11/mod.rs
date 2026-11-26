//! Lifecycle for a local X display owned by TermCraft.

mod capabilities;
mod engine;
#[cfg(target_os = "linux")]
mod native_capabilities;
#[cfg(target_os = "macos")]
pub use capabilities::X11LogicalRect;
pub use capabilities::{OpenGlEvidence, XCapabilities, capability_probe_entry};
mod forwarding;
pub mod remote_environment;
pub use forwarding::{XClientDisplay, XDisplaySource, XForwardingLease, XTrustMode};
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod macos_clipboard;
#[cfg(any(windows, target_os = "macos", test))]
mod native_artifact;
#[cfg(target_os = "macos")]
mod native_macos;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub use macos_clipboard::MacosClipboard;
#[cfg(any(windows, target_os = "macos"))]
mod native_policy;
#[cfg(any(windows, target_os = "macos", test))]
mod native_transport;
#[cfg(windows)]
mod native_windows;
#[cfg(windows)]
mod native_windows_process;
mod platform;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod unix_connection;
pub(crate) use platform::bundled_runtime;

use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, TryRecvError},
};

pub const LINUX_XWAYLAND_ARTIFACT: &str = "xwayland-24.1.15-arch-x86_64";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum XDisplayMode {
    #[default]
    Windowed,
    Fullscreen,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XServerOptions {
    pub artifact_id: String,
    pub display_mode: XDisplayMode,
    pub monitor: Option<String>,
}

impl Default for XServerOptions {
    fn default() -> Self {
        Self {
            artifact_id: platform::DEFAULT_ARTIFACT.into(),
            display_mode: XDisplayMode::Windowed,
            monitor: None,
        }
    }
}

#[derive(Clone, Default)]
pub struct XServerManager {
    installation: Option<PathBuf>,
}

impl XServerManager {
    /// Locate packaged engines relative to this installation's resource directory.
    pub fn for_installation(root: PathBuf) -> Self {
        Self {
            installation: Some(root),
        }
    }

    /// Start on a background thread or in another blocking context.
    pub fn start(&self, options: XServerOptions) -> Result<XServerLease, String> {
        self.start_cancellable(options, &AtomicBool::new(false))
    }

    pub fn start_async(&self, options: XServerOptions) -> Result<XServerStartup, String> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let manager = self.clone();
        std::thread::Builder::new()
            .name("termcraft-xserver-start".into())
            .spawn(move || {
                let result = manager.start_cancellable(options, &worker_cancelled);
                // Dropping an undeliverable lease also stops its owned server.
                let _ = sender.send(result);
            })
            .map_err(|error| format!("Could not start the X server worker: {error}"))?;
        Ok(XServerStartup {
            receiver,
            cancelled,
        })
    }

    /// Start on a worker and poll the cancellation flag during startup.
    pub fn start_cancellable(
        &self,
        options: XServerOptions,
        cancelled: &AtomicBool,
    ) -> Result<XServerLease, String> {
        let root = match &self.installation {
            Some(root) => root.clone(),
            None => {
                crate::runtime::RuntimeCatalog::for_current_executable()?.service_runtime_root()
            }
        };
        platform::start(&root, options, cancelled)
    }
}

pub struct XServerStartup {
    receiver: Receiver<Result<XServerLease, String>>,
    cancelled: Arc<AtomicBool>,
}

impl XServerStartup {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    /// Poll once per UI update until a result arrives, then discard the startup handle.
    pub fn try_finish(&self) -> Option<Result<XServerLease, String>> {
        match self.receiver.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err("X server startup worker exited".into())),
        }
    }
}

impl Drop for XServerStartup {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub struct XServerLease {
    alive: tokio::sync::watch::Sender<bool>,
    display: String,
    authority_path: PathBuf,
    process: engine::OwnedProcess,
    #[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
    _reservation: platform::DisplayReservation,
    #[cfg(target_os = "macos")]
    _native_image: Arc<native_artifact::VerifiedSnapshot>,
    #[cfg(windows)]
    _native_image: native_artifact::VerifiedSnapshot,
    #[cfg(windows)]
    _runtime_guard: std::fs::File,
    // Stop the child before removing the files it needs.
    _runtime: tempfile::TempDir,
}

impl Drop for XServerLease {
    fn drop(&mut self) {
        self.alive.send_replace(false);
    }
}

impl XServerLease {
    pub fn client_display(&self) -> XClientDisplay {
        XClientDisplay::new(self)
    }

    pub fn display(&self) -> &str {
        &self.display
    }

    pub fn authority_path(&self) -> &Path {
        &self.authority_path
    }

    pub fn process_id(&self) -> Option<u32> {
        self.process.id()
    }

    pub fn is_running(&mut self) -> Result<bool, String> {
        let result = self.process.is_running().map_err(|error| error.to_string());
        if !matches!(result, Ok(true)) {
            self.alive.send_replace(false);
        }
        result
    }

    /// May wait briefly for the engine to release its display. Call off the UI thread.
    pub fn stop_owned(self) -> Result<(), String> {
        let mut lease = self;
        lease.alive.send_replace(false);
        lease.process.stop().map_err(|error| error.to_string())
    }
}

pub mod xdmcp;
mod xdmcp_engine;
pub mod xdmcp_host;
#[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
mod xdmcp_listener;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod xdmcp_macos;
pub use xdmcp_engine::{LINUX_XEPHYR_ARTIFACT, ManagedXdmcp, XdmcpSession};

#[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
mod xdmcp_payload;

#[cfg(target_os = "linux")]
mod xdmcp_clipboard;
