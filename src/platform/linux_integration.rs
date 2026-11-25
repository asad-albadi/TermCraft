//! Per-user package integration. Python installer owns hashed files and never changes defaults.
use std::path::{Path, PathBuf};
pub struct DesktopIntegration {
    script: PathBuf,
}
impl DesktopIntegration {
    pub fn from_package(root: &Path) -> Result<Self, String> {
        let script = root.join("desktop-integration.py");
        if !script.is_file() {
            return Err(
                "Desktop integration is available from an unpacked Linux release package.".into(),
            );
        }
        Ok(Self { script })
    }
    fn run(&self, action: &str) -> Result<String, String> {
        let output = std::process::Command::new("python3")
            .arg(&self.script)
            .arg(action)
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().into());
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().into())
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
