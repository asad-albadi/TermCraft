//! Owned bundle registration delegates to the packaged native LaunchServices tool.
use std::path::{Path, PathBuf};
pub struct DesktopIntegration {
    script: PathBuf,
}
impl DesktopIntegration {
    pub fn from_bundle(bundle: &Path) -> Result<Self, String> {
        let script = bundle.join("Contents/Resources/desktop-integration.py");
        if !script.is_file() {
            return Err("Choose a packaged TermCraft.app bundle.".into());
        }
        Ok(Self { script })
    }
    fn run(&self, action: &str) -> Result<String, String> {
        if !cfg!(target_os = "macos") {
            return Err("LaunchServices registration requires native macOS.".into());
        }
        let result = std::process::Command::new("python3")
            .arg(&self.script)
            .arg(action)
            .output()
            .map_err(|e| e.to_string())?;
        if !result.status.success() {
            return Err(String::from_utf8_lossy(&result.stderr).trim().into());
        }
        Ok(String::from_utf8_lossy(&result.stdout).trim().into())
    }
    pub fn install_user(&self) -> Result<(), String> {
        self.run("install").map(|_| ())
    }
    pub fn remove_user(&self) -> Result<(), String> {
        self.run("uninstall").map(|_| ())
    }
    pub fn status(&self) -> Result<bool, String> {
        Ok(self.run("status")? == "installed")
    }
}
