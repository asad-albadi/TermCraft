//! Active checks run in a short-lived child: Xlib/GL driver state and credentials
//! never enter the UI process, and a wedged graphics driver can be terminated.
use super::XClientDisplay;
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OpenGlEvidence {
    pub vendor: String,
    pub renderer: String,
    pub version: String,
    pub direct: bool,
    pub software: bool,
    pub matched_pixels: u32,
    pub sampled_pixels: u32,
    /// PNG encoded from the actual glReadPixels buffer (OpenGL bottom-up rows).
    pub scene_png_base64: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct XCapabilities {
    pub glx_version: Option<String>,
    pub opengl: Option<OpenGlEvidence>,
    pub composite_version: Option<String>,
    pub composite_pixels: bool,
    pub randr_version: Option<String>,
    pub dimensions: [u32; 2],
    /// An actual observed size transition, never inferred from advertised modes.
    pub randr_change: Option<[u32; 2]>,
    #[serde(default)]
    pub initial_mode_restored: bool,
    pub errors: Vec<String>,
}
impl XCapabilities {
    pub fn rendering_verified(&self) -> bool {
        self.opengl
            .as_ref()
            .is_some_and(|gl| gl.sampled_pixels > 0 && gl.matched_pixels == gl.sampled_pixels)
    }
    /// Blocking active qualification; invoke before clients attach to a fresh owned
    /// display. Temporarily changes an advertised screen mode and restores it.
    /// On a child crash/timeout, the owner must discard that fresh server because
    /// an interrupted native driver cannot guarantee its initial mode was restored.
    pub fn probe(display: &XClientDisplay, cancelled: &AtomicBool) -> Result<Self, String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        Self::probe_with_executable(display, &exe, cancelled)
    }
    pub fn probe_with_executable(
        display: &XClientDisplay,
        exe: &Path,
        cancelled: &AtomicBool,
    ) -> Result<Self, String> {
        display.ensure_running()?;
        let output = tempfile::tempfile().map_err(|e| e.to_string())?;
        let mut command = Command::new(exe);
        command
            .arg("--termcraft-x11-capability-probe")
            .envs(display.environment()?)
            .env_remove("WAYLAND_SOCKET")
            .stdin(Stdio::null())
            .stdout(output.try_clone().map_err(|e| e.to_string())?)
            .stderr(Stdio::null());
        let mut child = command
            .spawn()
            .map_err(|e| format!("Could not start X capability probe: {e}"))?;
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            if cancelled.load(Ordering::Relaxed)
                || display.ensure_running().is_err()
                || Instant::now() >= deadline
            {
                let _ = child.kill();
                let _ = child.wait();
                return Err(
                    "X capability probe cancelled, display stopped, or graphics driver timed out"
                        .into(),
                );
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    if !status.success() {
                        return Err(format!(
                            "X capability probe failed ({status}); native graphics libraries or context may be unavailable"
                        ));
                    }
                    use std::io::{Read, Seek, SeekFrom};
                    let mut output = output;
                    output.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
                    let mut data = Vec::new();
                    output
                        .take(65537)
                        .read_to_end(&mut data)
                        .map_err(|e| e.to_string())?;
                    if data.len() > 65536 {
                        return Err("X capability response exceeded limit".into());
                    }
                    return serde_json::from_slice(&data)
                        .map_err(|e| format!("Invalid X capability response: {e}"));
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(20)),
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(e.to_string());
                }
            }
        }
    }
}
/// Internal subprocess entry; no UI, persistence, or global environment mutation.
pub fn capability_probe_entry() -> Option<i32> {
    if std::env::args().nth(1).as_deref() != Some("--termcraft-x11-capability-probe") {
        return None;
    }
    #[cfg(target_os = "linux")]
    let result = unsafe { super::native_capabilities::probe() };
    #[cfg(all(windows, target_arch = "x86_64"))]
    let result = super::native_capabilities_windows::probe();
    #[cfg(not(any(target_os = "linux", all(windows, target_arch = "x86_64"))))]
    let result: Result<XCapabilities, String> =
        Err("Native graphics probing is unavailable on this platform".into());
    match result {
        Ok(report) => {
            println!("{}", serde_json::to_string(&report).unwrap());
            Some(0)
        }
        Err(error) => {
            eprintln!("{error}");
            Some(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extension_advertisement_is_not_render_verification() {
        let report = XCapabilities {
            glx_version: Some("1.4".into()),
            ..Default::default()
        };
        assert!(!report.rendering_verified());
    }
}
