//! Persisted choices, shared with the independently built protocol engine.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClipboardPolicy {
    #[default]
    Disabled,
    ToRemote,
    FromRemote,
    Bidirectional,
}
impl ClipboardPolicy {
    pub fn to_remote(self) -> bool {
        matches!(self, Self::ToRemote | Self::Bidirectional)
    }
    pub fn from_remote(self) -> bool {
        matches!(self, Self::FromRemote | Self::Bidirectional)
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyboardRouting {
    #[default]
    Remote,
    Local,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RdpSecurity {
    #[default]
    Nla,
    Tls,
    Legacy,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriveMapping {
    pub name: String,
    pub path: std::path::PathBuf,
    pub read_only: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SerialMapping {
    pub name: String,
    pub path: std::path::PathBuf,
    pub read_only: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrinterMapping {
    pub queue: String,
    pub driver: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RdpOptions {
    pub administrative: bool,
    pub clipboard: ClipboardPolicy,
    pub keyboard: KeyboardRouting,
    pub drives: Vec<DriveMapping>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub serial_mappings: Vec<SerialMapping>,
    pub serial_ports: bool,
    pub printers: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub printer_mapping: Option<PrinterMapping>,
    pub smartcard: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smartcard_reader: Option<String>,
    pub security: RdpSecurity,
    pub domain: String,
    pub keyboard_layout: u32,
    pub certificate_authority: Option<std::path::PathBuf>,
}
#[derive(Clone, Copy, Debug)]
pub struct RdpCapabilities {
    pub clipboard: bool,
    pub drives: bool,
    pub serial_ports: bool,
    pub printers: bool,
    pub smartcard: bool,
    pub administrative: bool,
    pub tls_login: bool,
}
impl RdpCapabilities {
    pub fn embedded() -> Self {
        Self {
            clipboard: true,
            drives: true,
            serial_ports: cfg!(any(unix, windows)),
            printers: cfg!(target_os = "linux"),
            smartcard: cfg!(target_os = "linux"),
            administrative: true,
            tls_login: false,
        }
    }
}
#[derive(Clone, Debug)]
pub struct NegotiatedRdpOptions {
    pub clipboard: ClipboardPolicy,
    pub keyboard: KeyboardRouting,
    pub keyboard_layout: u32,
}
impl RdpOptions {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.domain.len() > 255 || self.domain.chars().any(char::is_control) {
            return Err("Enter a domain without control characters (maximum 255 bytes)".into());
        }
        if self.drives.len() > 16 {
            return Err("At most 16 drive mappings are supported".into());
        }
        let mut names = std::collections::BTreeSet::new();
        for drive in &self.drives {
            if drive.name.is_empty()
                || drive.name.len() > 7
                || !drive
                    .name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
                || !names.insert(drive.name.to_ascii_lowercase())
                || !drive.path.is_absolute()
            {
                return Err(
                    "Drive mappings need distinct names of 1–7 ASCII letters, digits or underscores and absolute local paths"
                        .into(),
                );
            }
        }
        if self.serial_mappings.len() > 16 {
            return Err("At most 16 serial mappings are supported".into());
        }
        let mut serial_names = std::collections::BTreeSet::new();
        for mapping in &self.serial_mappings {
            let name = mapping.name.to_ascii_uppercase();
            let valid = name
                .strip_prefix("COM")
                .and_then(|n| n.parse::<u16>().ok())
                .is_some_and(|n| (1..=256).contains(&n) && name == format!("COM{n}"));
            if !valid || !mapping.path.is_absolute() || !serial_names.insert(name) {
                return Err("Serial mappings need distinct COM1–COM256 names and absolute local device paths".into());
            }
        }
        if let Some(mapping) = &self.printer_mapping {
            if mapping.queue.is_empty()
                || mapping.queue.len() > 127
                || !mapping
                    .queue
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
                || matches!(mapping.queue.as_str(), "." | "..")
            {
                return Err("Enter a CUPS queue name using 1–127 ASCII letters, digits, hyphens, underscores or dots".into());
            }
            if mapping.driver.trim().is_empty()
                || mapping.driver.len() > 255
                || mapping.driver.chars().any(char::is_control)
            {
                return Err("Enter the Windows printer driver name (maximum 255 bytes)".into());
            }
        }
        if let Some(reader) = &self.smartcard_reader
            && (reader.trim().is_empty()
                || reader.len() > 127
                || reader.chars().any(char::is_control))
        {
            return Err("Enter the exact local PC/SC reader name (maximum 127 bytes)".into());
        }
        if self.security == RdpSecurity::Legacy {
            return Err("Legacy RDP security is unavailable; select verified TLS with NLA".into());
        }
        Ok(())
    }
    pub fn negotiate(
        &self,
        capabilities: &RdpCapabilities,
    ) -> Result<NegotiatedRdpOptions, String> {
        self.validate()?;
        if self.serial_ports && self.serial_mappings.is_empty() {
            return Err(
                "Embedded serial redirection requires an explicit local device mapping".into(),
            );
        }
        if self.printers {
            return Err("Automatic printer redirection requires the installed Windows client. Select an explicit CUPS queue for embedded redirection.".into());
        }
        if self.smartcard {
            return Err("Automatic smartcard redirection requires the installed Windows client. Select one local PC/SC reader for embedded redirection.".into());
        }
        let required = [
            (
                self.clipboard != ClipboardPolicy::Disabled,
                capabilities.clipboard,
                "clipboard",
            ),
            (
                !self.drives.is_empty(),
                capabilities.drives,
                "drive redirection",
            ),
            (
                self.serial_ports || !self.serial_mappings.is_empty(),
                capabilities.serial_ports,
                "serial-port redirection",
            ),
            (
                self.printer_mapping.is_some(),
                capabilities.printers,
                "printer redirection",
            ),
            (
                self.smartcard_reader.is_some(),
                capabilities.smartcard,
                "smartcard redirection",
            ),
            (
                self.administrative,
                capabilities.administrative,
                "administrative session",
            ),
            (
                self.security == RdpSecurity::Tls,
                capabilities.tls_login,
                "TLS graphical sign-in",
            ),
        ];
        if let Some((_, _, name)) = required
            .into_iter()
            .find(|(selected, supported, _)| *selected && !*supported)
        {
            return Err(format!(
                "The selected RDP engine has no available {name} provider. This option was not enabled."
            ));
        }
        Ok(NegotiatedRdpOptions {
            clipboard: self.clipboard,
            keyboard: self.keyboard,
            keyboard_layout: self.keyboard_layout,
        })
    }
}
