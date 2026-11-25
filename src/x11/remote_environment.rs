//! User-owned SSH session startup. Separate from ordinary forwarded applications.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteEnvironment {
    #[default]
    None,
    Preset(DesktopPreset),
    /// Explicit user command; interpreted by the remote login shell, never locally.
    Custom(String),
}
impl RemoteEnvironment {
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopPreset {
    Xfce,
    Mate,
    Lxqt,
    Gnome,
}
impl DesktopPreset {
    pub const ALL: [Self; 4] = [Self::Xfce, Self::Mate, Self::Lxqt, Self::Gnome];
    pub fn label(self) -> &'static str {
        match self {
            Self::Xfce => "Xfce",
            Self::Mate => "MATE",
            Self::Lxqt => "LXQt",
            Self::Gnome => "GNOME (X11)",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteCommand {
    pub program: String,
    pub arguments: Vec<String>,
}
impl RemoteCommand {
    /// SSH exec transports a command string. Quote every argument, including custom scripts.
    pub fn ssh_exec(&self) -> String {
        std::iter::once(&self.program)
            .chain(self.arguments.iter())
            .map(|s| format!("'{}'", s.replace('\'', "'\\''")))
            .collect::<Vec<_>>()
            .join(" ")
    }
}
pub fn resolve_environment(env: &RemoteEnvironment) -> Result<Option<RemoteCommand>, String> {
    let (program, arguments) = match env {
        RemoteEnvironment::None => return Ok(None),
        RemoteEnvironment::Preset(preset) => (
            "dbus-run-session",
            vec![
                "--".into(),
                match preset {
                    DesktopPreset::Xfce => "xfce4-session",
                    DesktopPreset::Mate => "mate-session",
                    DesktopPreset::Lxqt => "startlxqt",
                    DesktopPreset::Gnome => "gnome-session",
                }
                .into(),
            ],
        ),
        RemoteEnvironment::Custom(command) => {
            if command.trim().is_empty() || command.len() > 8192 || command.contains('\0') {
                return Err(
                    "Enter a remote desktop command of 1–8192 bytes without NUL characters.".into(),
                );
            }
            ("sh", vec!["-lc".into(), command.clone()])
        }
    };
    Ok(Some(RemoteCommand {
        program: program.into(),
        arguments,
    }))
}
