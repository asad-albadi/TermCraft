use super::{Result, invalid};
use crate::packages::install::safe_path;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const HOST_PROTOCOL: u16 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Permission {
    NativeExecution,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    pub schema: u16,
    pub id: String,
    pub version: String,
    pub host_protocol: u16,
    pub platform: String,
    pub architecture: String,
    pub archive: String,
    pub archive_sha256: String,
    pub binary_sha256: String,
    pub executable: String,
    pub entrypoints: Vec<String>,
    pub permissions: Vec<Permission>,
}

impl PluginManifest {
    pub(super) fn validate(&self) -> Result<()> {
        if self.schema != 1 || self.host_protocol != HOST_PROTOCOL {
            return Err(invalid(
                "Unsupported TermCraft plugin manifest or host protocol",
            ));
        }
        if self.platform != std::env::consts::OS
            || self.architecture != std::env::consts::ARCH
            || self.architecture != "x86_64"
            || !matches!(self.platform.as_str(), "linux" | "windows")
        {
            return Err(invalid(
                "Plugin targets an unsupported platform or architecture",
            ));
        }
        if !identifier(&self.id)
            || self.release_version().is_none()
            || self.entrypoints.is_empty()
            || self.entrypoints.len() > 12
            || self.entrypoints.iter().any(|name| !identifier(name))
            || self.entrypoints.iter().collect::<BTreeSet<_>>().len() != self.entrypoints.len()
        {
            return Err(invalid("Invalid plugin identifier, version or entrypoints"));
        }
        if self.permissions != [Permission::NativeExecution] {
            return Err(invalid(
                "The manifest must request native execution explicitly",
            ));
        }
        if !self.archive.ends_with(".tar.gz")
            || !safe_path(&format!("bin/{}", self.archive))
            || !safe_path(&format!("bin/{}", self.executable))
            || (self.platform == "windows" && !self.executable.ends_with(".exe"))
            || !is_sha256(&self.archive_sha256)
            || !is_sha256(&self.binary_sha256)
        {
            return Err(invalid("Invalid plugin filenames or SHA-256 pins"));
        }
        Ok(())
    }

    pub(super) fn release_version(&self) -> Option<[u32; 3]> {
        let mut components = self.version.split('.');
        let mut version = [0; 3];
        for number in &mut version {
            let component = components.next()?;
            *number = component.parse().ok()?;
            if number.to_string() != component {
                return None;
            }
        }
        components.next().is_none().then_some(version)
    }

    pub(super) fn digest(&self) -> Result<String> {
        let bytes = serde_json::to_vec(self).map_err(std::io::Error::other)?;
        Ok(Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect())
    }

    pub(super) fn executable_path(&self) -> String {
        format!("bin/{}", self.executable)
    }
}

fn identifier(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 48
        && id.as_bytes()[0].is_ascii_lowercase()
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

pub(super) fn is_sha256(pin: &str) -> bool {
    pin.len() == 64
        && pin
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}
