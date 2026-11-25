#![allow(dead_code)] // Component wrapper has no UI caller; compile real native APIs.
#[path = "../../../src/x11/native_artifact.rs"]
pub mod native_artifact;
#[path = "../../../src/x11/native_transport.rs"]
pub mod native_transport;
#[path = "../../../src/x11/native_policy.rs"]
mod native_policy;
#[cfg(windows)]
#[path = "../../../src/x11/native_windows_process.rs"]
mod native_windows_process;
#[cfg(windows)]
#[path = "../../../src/x11/native_windows.rs"]
mod native_windows;
#[path = "../../../src/x11/engine.rs"]
mod engine;
#[derive(PartialEq)]
pub enum XDisplayMode { Windowed, Fullscreen }
pub struct XServerOptions { pub artifact_id: String, pub monitor: Option<String>, pub display_mode: XDisplayMode }
pub struct XServerLease {
    alive: tokio::sync::watch::Sender<bool>, display: String, authority_path: std::path::PathBuf, process: engine::OwnedProcess,
    #[cfg(any(windows, target_os="macos"))]
    _native_image: native_artifact::VerifiedSnapshot,
    #[cfg(windows)]
    _runtime_guard: std::fs::File,
    _runtime: tempfile::TempDir,
}

#[cfg(target_os="macos")]
#[path = "../../../src/x11/native_macos.rs"]
mod native_macos;
