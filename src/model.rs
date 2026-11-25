use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

pub const PROTOCOLS: &[&str] = &[
    "SSH", "Telnet", "Rsh", "Rlogin", "RDP", "VNC", "FTP", "SFTP", "Serial", "Local", "Mosh",
    "Browser", "XDMCP",
];
pub const SFTP_PIPELINE: usize = 32;
pub const DOWNLOAD_BLOCK: usize = 32 * 1024;
pub const UPLOAD_BLOCK: usize = 64 * 1024;
pub const SFTP_BUFFER: usize = 256 * 1024;
pub fn protocol_port(p: &str) -> u16 {
    match p {
        "SSH" | "SFTP" => 22,
        "Telnet" => 23,
        "Rsh" => 514,
        "Rlogin" => 513,
        "RDP" => 3389,
        "VNC" => 5900,
        "FTP" => 21,
        "Mosh" => 60000,
        "Browser" => 443,
        "XDMCP" => 177,
        _ => 0,
    }
}
#[path = "model/ssh_options.rs"]
pub mod ssh_options;
pub use ssh_options::{ProxyKind, ProxySettings, SshDefaults, SshOptions};
pub fn validate_host(host: &str) -> Result<(), String> {
    if host.is_empty()
        || host.starts_with('-')
        || host
            .chars()
            .any(|c| !(c.is_ascii_alphanumeric() || ".-_:[]%".contains(c)))
    {
        return Err("Enter a hostname or IP address, without spaces or options.".into());
    }
    Ok(())
}
pub fn validate_user(user: &str) -> Result<(), String> {
    if user.starts_with('-')
        || user
            .chars()
            .any(|c| !(c.is_ascii_alphanumeric() || "._-\\@".contains(c)))
    {
        return Err("The username contains unsupported characters.".into());
    }
    Ok(())
}
pub fn ssh_arguments(
    host: &str,
    user: &str,
    port: u16,
    key: &str,
    jump: &str,
    x11: bool,
    compression: bool,
) -> Result<Vec<String>, String> {
    validate_host(host)?;
    validate_user(user)?;
    if port == 0 {
        return Err("Choose a port from 1 to 65535.".into());
    }
    let mut a = vec![
        "-p".into(),
        port.to_string(),
        "-o".into(),
        "ServerAliveInterval=60".into(),
    ];
    if !key.is_empty() {
        a.extend(["-i".into(), key.into()]);
    }
    if !jump.is_empty() {
        if jump.starts_with('-')
            || jump
                .chars()
                .any(|c| !(c.is_ascii_alphanumeric() || "._-@:,[]%".contains(c)))
        {
            return Err("Enter the jump host as user@host:port.".into());
        }
        a.extend(["-J".into(), jump.into()]);
    }
    if x11 {
        a.push("-Y".into());
    } else {
        a.extend([
            "-o".into(),
            "ForwardX11=no".into(),
            "-o".into(),
            "ForwardX11Trusted=no".into(),
        ]);
    }
    if compression {
        a.extend(["-C".into(), "-o".into(), "Compression=yes".into()]);
    } else {
        a.extend(["-o".into(), "Compression=no".into()]);
    }
    a.push(if user.is_empty() {
        host.into()
    } else {
        format!("{user}@{host}")
    });
    Ok(a)
}
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq, Default)]
pub enum SshEngine {
    #[default]
    Embedded,
    OpenSsh,
}
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq, Default)]
pub enum DesktopEngine {
    #[default]
    Embedded,
    External,
}
impl DesktopEngine {
    pub fn is_embedded(&self) -> bool {
        *self == Self::Embedded
    }
}
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq, Default)]
pub enum AgentMode {
    #[default]
    System,
    Owned,
    Disabled,
    Pageant,
}
fn system_agent_mode(mode: &AgentMode) -> bool {
    *mode == AgentMode::System
}
fn legacy_ssh_engine() -> SshEngine {
    SshEngine::OpenSsh
}
pub type SessionId = String;
pub fn new_session_id() -> SessionId {
    let mut bytes = [0u8; 16];
    if getrandom::fill(&mut bytes).is_ok() {
        format!(
            "session-{}",
            bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
        )
    } else {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        format!(
            "session-fallback-{time:x}-{:x}-{:x}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        )
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MoshProvider {
    #[default]
    System,
    Bundled,
}

impl MoshProvider {
    fn is_system(&self) -> bool {
        *self == Self::System
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct Session {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub startup_macro: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "crate::protocols::ftp_actor::SessionOptions::is_legacy"
    )]
    pub ftp_options: crate::protocols::ftp_actor::SessionOptions,
    #[serde(skip_serializing_if = "crate::browser::Options::is_legacy")]
    pub browser_options: crate::browser::Options,
    #[serde(skip_serializing_if = "vnc_default")]
    pub vnc_options: crate::desktop::vnc::VncOptions,
    #[serde(skip_serializing_if = "crate::desktop::rdp_options::RdpOptions::is_default")]
    pub rdp_options: crate::desktop::rdp_options::RdpOptions,
    #[serde(skip_serializing_if = "DesktopEngine::is_embedded")]
    pub desktop_engine: DesktopEngine,
    #[serde(skip_serializing_if = "crate::x11::xdmcp::XdmcpConfig::is_default")]
    pub xdmcp_options: crate::x11::xdmcp::XdmcpConfig,
    pub id: SessionId,
    pub credential_ref: Option<String>,
    pub gateway_credential_ref: Option<String>,
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub gateway_credential_refs: std::collections::BTreeMap<String, String>,
    #[serde(skip_serializing_if = "SshOptions::is_default")]
    pub ssh_options: SshOptions,
    pub name: String,
    #[serde(default = "legacy_ssh_engine")]
    pub ssh_engine: SshEngine,
    #[serde(skip_serializing_if = "system_agent_mode")]
    pub agent_mode: AgentMode,
    pub remote_backend: crate::remote_files::capabilities::RemoteBackendKind,
    pub folder: String,
    pub protocol: String,
    pub host: String,
    pub user: String,
    pub port: u16,
    pub key: String,
    pub jump: String,
    #[serde(skip_serializing_if = "crate::x11::remote_environment::RemoteEnvironment::is_none")]
    pub remote_environment: crate::x11::remote_environment::RemoteEnvironment,
    pub x11: bool,
    #[serde(skip_serializing_if = "crate::x11::XDisplaySource::is_system")]
    pub x11_source: crate::x11::XDisplaySource,
    pub compression: bool,
    pub command: String,
    pub directory: String,
    pub mosh_ssh_port: u16,
    #[serde(skip_serializing_if = "MoshProvider::is_system")]
    pub mosh_provider: MoshProvider,
    pub baud: u32,
    pub serial_device: String,
    #[serde(skip_serializing_if = "crate::serial::SerialLine::is_default")]
    pub serial_line: crate::serial::SerialLine,
    pub fullscreen: bool,
    pub view_only: bool,
    pub width: u32,
    pub height: u32,
    pub highlight_enabled: Option<bool>,
    pub terminal: crate::terminal::settings::TerminalOverrides,
}
impl Default for Session {
    fn default() -> Self {
        Self {
            startup_macro: None,
            ftp_options: Default::default(),
            browser_options: Default::default(),
            vnc_options: Default::default(),
            rdp_options: Default::default(),
            desktop_engine: Default::default(),
            id: new_session_id(),
            credential_ref: None,
            gateway_credential_ref: None,
            gateway_credential_refs: Default::default(),
            ssh_options: Default::default(),
            highlight_enabled: None,
            terminal: crate::terminal::settings::TerminalOverrides::default(),
            name: String::new(),
            ssh_engine: SshEngine::Embedded,
            agent_mode: AgentMode::System,
            remote_backend: Default::default(),
            folder: "My sessions".into(),
            protocol: "SSH".into(),
            host: String::new(),
            user: String::new(),
            port: 22,
            key: String::new(),
            jump: String::new(),
            remote_environment: Default::default(),
            xdmcp_options: Default::default(),
            x11: false,
            x11_source: Default::default(),
            compression: true,
            command: String::new(),
            directory: String::new(),
            mosh_ssh_port: 22,
            mosh_provider: MoshProvider::System,
            baud: 9600,
            serial_device: String::new(),
            serial_line: Default::default(),
            fullscreen: false,
            view_only: false,
            width: 1280,
            height: 800,
        }
    }
}
impl Session {
    pub fn clear_credential_refs(&mut self) {
        self.credential_ref = None;
        self.gateway_credential_ref = None;
        self.gateway_credential_refs.clear();
        self.ssh_options.clear_credentials();
    }

    pub fn title(&self) -> String {
        if self.name.trim().is_empty() {
            if self.protocol == "Local" {
                "Local terminal".into()
            } else {
                self.host.clone()
            }
        } else {
            self.name.clone()
        }
    }
    pub fn sftp_session(&self) -> Result<Self, String> {
        if !matches!(self.protocol.as_str(), "SSH" | "SFTP") {
            return Err("Open file transfer from an SSH or SFTP session.".into());
        }
        let mut session = self.clone();
        session.name = format!("{} · SFTP", self.title());
        session.protocol = "SFTP".into();
        session.command.clear();
        session.x11 = false;
        Ok(session)
    }
    pub fn ssh_args(&self) -> Result<Vec<String>, String> {
        ssh_arguments(
            &self.host,
            &self.user,
            self.port,
            &self.key,
            &self.jump,
            self.x11,
            self.compression,
        )
    }
}
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct Tunnel {
    pub name: String,
    pub host: String,
    pub user: String,
    pub ssh_port: u16,
    pub key: String,
    pub mode: String,
    pub bind: String,
    pub local_port: u16,
    pub destination: String,
    pub remote_port: u16,
}
impl Default for Tunnel {
    fn default() -> Self {
        Self {
            name: "New tunnel".into(),
            host: String::new(),
            user: String::new(),
            ssh_port: 22,
            key: String::new(),
            mode: "Local".into(),
            bind: "127.0.0.1".into(),
            local_port: 8080,
            destination: "localhost".into(),
            remote_port: 80,
        }
    }
}
impl Tunnel {
    pub fn args(&self) -> Result<Vec<String>, String> {
        if !matches!(self.mode.as_str(), "Local" | "Remote" | "Dynamic") {
            return Err("Choose a Local, Remote or Dynamic tunnel.".into());
        }
        validate_host(&self.bind)?;
        let address = |host: &str| {
            if host.contains(':') && !host.starts_with('[') {
                format!("[{host}]")
            } else {
                host.to_string()
            }
        };
        if self.local_port == 0 || self.remote_port == 0 {
            return Err("Ports must be between 1 and 65535.".into());
        }
        let mut a = ssh_arguments(
            &self.host,
            &self.user,
            self.ssh_port,
            &self.key,
            "",
            false,
            false,
        )?;
        let target = a.pop().unwrap();
        a.extend(["-N".into(), "-o".into(), "ExitOnForwardFailure=yes".into()]);
        if self.mode == "Dynamic" {
            a.extend([
                "-D".into(),
                format!("{}:{}", address(&self.bind), self.local_port),
            ]);
        } else {
            validate_host(&self.destination)?;
            a.extend([
                if self.mode == "Remote" { "-R" } else { "-L" }.into(),
                format!(
                    "{}:{}:{}:{}",
                    address(&self.bind),
                    self.local_port,
                    address(&self.destination),
                    self.remote_port
                ),
            ]);
        }
        a.push(target);
        Ok(a)
    }
}
#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct Macro {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<crate::macros::Script>,
    pub name: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub append_enter: bool,
}
#[derive(Clone, Copy, Serialize, Deserialize, Debug, Default, PartialEq, Eq)]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}
#[derive(Clone, Copy, Serialize, Deserialize, Debug, Default, PartialEq, Eq)]
pub enum HighlightMatch {
    #[default]
    Literal,
    Regex,
}
#[derive(Clone, Copy, Serialize, Deserialize, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct HighlightStyle {
    pub foreground: Option<[u8; 3]>,
    pub background: Option<[u8; 3]>,
}
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(default)]
pub struct HighlightRule {
    pub pattern: String,
    pub kind: HighlightMatch,
    pub precedence: i32,
    pub enabled: bool,
    pub case_sensitive: bool,
    pub light: HighlightStyle,
    pub dark: HighlightStyle,
}
impl Default for HighlightRule {
    fn default() -> Self {
        Self {
            pattern: "ERROR".into(),
            kind: HighlightMatch::Literal,
            precedence: 0,
            enabled: true,
            case_sensitive: true,
            light: HighlightStyle {
                foreground: Some([135, 24, 38]),
                background: Some([255, 224, 226]),
            },
            dark: HighlightStyle {
                foreground: Some([255, 218, 222]),
                background: Some([91, 28, 39]),
            },
        }
    }
}
#[derive(Clone, Copy, Serialize, Deserialize, Debug, Default, PartialEq, Eq)]
pub enum RightClickBehavior {
    #[default]
    ContextMenu,
    Paste,
}
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(default)]
pub struct ClipboardSettings {
    pub copy_on_select: bool,
    pub right_click: RightClickBehavior,
    pub word_delimiters: String,
}
impl Default for ClipboardSettings {
    fn default() -> Self {
        Self {
            copy_on_select: false,
            right_click: RightClickBehavior::ContextMenu,
            word_delimiters: " \t./\\()[]{}<>:;,\"'`|".into(),
        }
    }
}
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct Preferences {
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub rdp_certificates: std::collections::BTreeMap<String, String>,
    #[serde(skip_serializing_if = "SshDefaults::is_default")]
    pub ssh_defaults: SshDefaults,
    pub font_size: f32,
    pub terminal: Option<crate::terminal::settings::TerminalSettings>,
    pub highlight_enabled: bool,
    pub highlight_rules: std::sync::Arc<Vec<HighlightRule>>,
    pub clipboard: ClipboardSettings,
    pub paste: crate::terminal::paste::PasteSettings,
    pub confirm_session_close: bool,
    pub scrollback: usize,
    pub appearance: Appearance,
    pub shell: String,
    pub downloads: String,
    pub sidebar_visible: bool,
    pub sidebar_width: f32,
    pub layout: crate::workspace::layout::LayoutPreferences,
    pub shortcuts: crate::workspace::shortcuts::ShortcutMap,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            rdp_certificates: Default::default(),
            ssh_defaults: Default::default(),
            sidebar_visible: true,
            sidebar_width: 226.0,
            layout: crate::workspace::layout::LayoutPreferences::default(),
            shortcuts: Default::default(),
            font_size: 14.0,
            terminal: None,
            highlight_enabled: true,
            highlight_rules: std::sync::Arc::new(Vec::new()),
            clipboard: ClipboardSettings::default(),
            paste: crate::terminal::paste::PasteSettings::default(),
            confirm_session_close: true,
            scrollback: 360000,
            appearance: Appearance::System,
            shell: String::new(),
            downloads: home_dir().to_string_lossy().into(),
        }
    }
}
impl Preferences {
    pub fn terminal_settings(&self) -> crate::terminal::settings::TerminalSettings {
        self.terminal
            .clone()
            .unwrap_or_else(|| crate::terminal::settings::TerminalSettings {
                font_size: self.font_size,
                ..Default::default()
            })
    }

    pub fn set_highlight_rules(&mut self, rules: Vec<HighlightRule>) -> Result<(), String> {
        crate::terminal::highlight::HighlightRules::compile(&rules)?;
        self.highlight_rules = std::sync::Arc::new(rules);
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[serde(default)]
pub struct Store {
    pub sessions: Vec<Session>,
    pub shared_sources: Vec<crate::sessions::shared::SharedSourceConfig>,
    pub tunnels: Vec<Tunnel>,
    pub macros: Vec<Macro>,
    pub preferences: Preferences,
}
impl Store {
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let bytes = fs::read(path).map_err(|e| e.to_string())?;
        Self::decode(&bytes)
    }

    pub fn load_bounded(path: &Path, limit: u64) -> Result<Self, String> {
        let mut bytes = Vec::new();
        fs::File::open(path)
            .and_then(|file| file.take(limit.saturating_add(1)).read_to_end(&mut bytes))
            .map_err(|error| error.to_string())?;
        if bytes.len() as u64 > limit {
            return Err("The workspace backup exceeds the import size limit.".into());
        }
        Self::decode(&bytes)
    }

    fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut store: Self = serde_json::from_slice(bytes)
            .map_err(|e| format!("Settings could not be read: {e}. The original file was kept."))?;
        let mut used = std::collections::HashSet::new();
        for session in &mut store.sessions {
            if session.id.is_empty() || !used.insert(session.id.clone()) {
                session.id = new_session_id();
                used.insert(session.id.clone());
            }
        }
        Ok(store)
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(p) = path.parent() {
            fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("tmp");
        #[cfg(unix)]
        {
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;
            let mut f = fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .mode(0o600)
                .open(&tmp)
                .map_err(|e| e.to_string())?;
            f.write_all(&bytes).map_err(|e| e.to_string())?;
            f.sync_all().map_err(|e| e.to_string())?;
        }
        #[cfg(not(unix))]
        fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
        #[cfg(windows)]
        if path.exists() {
            fs::copy(path, path.with_extension("bak")).map_err(|e| e.to_string())?;
            fs::remove_file(path).map_err(|e| e.to_string())?;
        }
        fs::rename(&tmp, path).map_err(|e| e.to_string())
    }
}
pub fn home_dir() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn copy_new_file(source: &Path, destination: &Path) -> std::io::Result<u64> {
    let mut input = fs::File::open(source)?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    std::io::copy(&mut input, &mut output)
}
pub fn config_dir() -> PathBuf {
    #[cfg(windows)]
    {
        return std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(home_dir)
            .join("Harbor");
    }
    #[cfg(target_os = "macos")]
    {
        return home_dir().join("Library/Application Support/Harbor");
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir().join(".config"))
            .join("harbor")
    }
}

fn vnc_default(options: &crate::desktop::vnc::VncOptions) -> bool {
    options == &Default::default()
}

/// Defaults for workspace-owned listeners, separate from client and system daemon ports.
pub fn managed_service_defaults(
    protocol: crate::services::ServiceProtocol,
) -> (std::net::SocketAddr, u32) {
    use crate::services::ServiceProtocol::*;
    let (port, lifetime) = match protocol {
        Ftp => (2121, 3600),
        Tftp => (6969, 360),
        Nfs => (20490, 360),
        Telnet => (2323, 3600),
        Ssh => (2222, 3600),
        Iperf => (5201, 3600),
        Vnc => (5900, 3600),
        Cron => (0, 0), // Scheduler has no listener; active until explicitly stopped.
    };
    (([127, 0, 0, 1], port).into(), lifetime)
}
