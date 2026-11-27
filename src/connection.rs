use crate::model::*;
use crate::runtime::{RuntimeCatalog, ToolId};
use std::{path::PathBuf, process::Command};
#[derive(Clone, Debug)]
pub struct Launch {
    pub program: String,
    pub args: Vec<String>,
    pub directory: Option<PathBuf>,
    /// Keeps the private mstsc profile alive across launch clones and reconnects.
    pub rdp_profile: Option<std::sync::Arc<tempfile::NamedTempFile>>,
}
pub fn available(name: &str) -> bool {
    find_program(name).is_some()
}
pub fn find_program(name: &str) -> Option<PathBuf> {
    crate::runtime::system_program(name)
}
pub fn default_shell() -> String {
    if cfg!(windows) {
        if available("pwsh") {
            "pwsh".into()
        } else {
            "powershell.exe".into()
        }
    } else {
        std::env::var("SHELL")
            .ok()
            .filter(|p| std::path::Path::new(p).exists())
            .unwrap_or_else(|| "/bin/sh".into())
    }
}
pub fn validate_system_x11_launch(session: &Session) -> Result<(), String> {
    let openssh = session.protocol == "SSH" && session.ssh_engine == SshEngine::OpenSsh;
    if !session.x11 || (!openssh && session.protocol != "Local") {
        return Ok(());
    }
    if session.x11_source == crate::x11::XDisplaySource::Owned {
        if openssh {
            return Err("Choose Embedded SSH to use the TermCraft display, or choose System display for OpenSSH compatibility.".into());
        }
    } else if std::env::var_os("DISPLAY").is_none_or(|display| display.is_empty()) {
        return Err("System display is unavailable. Set DISPLAY before starting TermCraft or choose TermCraft display.".into());
    }
    Ok(())
}

pub fn launch(s: &Session, p: &Preferences) -> Result<Launch, String> {
    if s.protocol == "SSH" && !s.remote_environment.is_none() {
        return Err("Remote desktop environment startup requires Embedded SSH. Choose Embedded SSH or set Remote desktop to None.".into());
    }
    if matches!(s.protocol.as_str(), "RDP" | "VNC" | "Telnet") && !s.jump.trim().is_empty() {
        return Err("SSH gateway profiles must use the managed embedded gateway path; direct installed-client fallback is unavailable".into());
    }
    if matches!(s.protocol.as_str(), "SSH" | "SFTP" | "Mosh")
        && matches!(s.agent_mode, AgentMode::Owned | AgentMode::Pageant)
    {
        return Err("TermCraft-owned/Pageant agent selection requires embedded SSH. Choose the system agent for installed-client compatibility.".into());
    }
    if matches!(s.protocol.as_str(), "SSH" | "SFTP" | "Mosh") {
        s.ssh_options.validate(&p.ssh_defaults)?;
        if s.ssh_options.proxy.is_some() {
            return Err("Typed HTTP/SOCKS proxy settings require embedded SSH; configure ProxyCommand in an OpenSSH config file for compatibility mode.".into());
        }
        if s.ssh_options.prefer_config {
            return Err("Config-first identity precedence requires embedded SSH; clear that option for installed OpenSSH compatibility.".into());
        }
    }
    let mut directory = None;
    let (program, args) = match s.protocol.as_str() {
        "Local" => {
            let shell = if s.command.trim().is_empty() {
                if p.shell.trim().is_empty() {
                    default_shell()
                } else {
                    p.shell.clone()
                }
            } else {
                s.command.clone()
            };
            if !s.directory.is_empty() {
                directory = Some(PathBuf::from(&s.directory));
            } else {
                directory = Some(home_dir());
            }
            let lower = shell.to_lowercase();
            let args = if lower.ends_with("cmd") || lower.ends_with("cmd.exe") {
                vec!["/Q".into()]
            } else if lower.contains("powershell")
                || lower.ends_with("pwsh")
                || lower.ends_with("pwsh.exe")
            {
                vec!["-NoLogo".into()]
            } else if cfg!(windows) {
                Vec::new()
            } else {
                vec!["-l".into()]
            };
            (shell, args)
        }
        "SSH" => {
            let mut args = s.ssh_args()?;
            apply_ssh_options(&mut args, s, p)?;
            if !s.command.is_empty() {
                args.push(s.command.clone());
            }
            ("ssh".into(), args)
        }
        "SFTP" => {
            let mut a = s.ssh_args()?;
            apply_ssh_options(&mut a, s, p)?;
            a.remove(0);
            a.insert(0, "-P".into());
            if let Some(i) = a.iter().position(|a| a == "-Y") {
                a.remove(i);
            }
            let target = a.pop().unwrap();
            a.extend([
                "-R".into(),
                SFTP_PIPELINE.to_string(),
                "-B".into(),
                DOWNLOAD_BLOCK.to_string(),
                target,
            ]);
            ("sftp".into(), a)
        }
        "Telnet" => {
            validate_host(&s.host)?;
            ("telnet".into(), vec![s.host.clone(), s.port.to_string()])
        }
        "Rsh" | "Rlogin" => {
            validate_host(&s.host)?;
            validate_user(&s.user)?;
            let mut a = Vec::new();
            if !s.user.is_empty() {
                a.extend(["-l".into(), s.user.clone()]);
            }
            a.push(s.host.clone());
            (s.protocol.to_lowercase(), a)
        }
        "FTP" => {
            validate_host(&s.host)?;
            ("ftp".into(), vec![s.host.clone(), s.port.to_string()])
        }
        "Mosh" => {
            validate_host(&s.host)?;
            validate_user(&s.user)?;
            let runtime = RuntimeCatalog::for_current_executable()?;
            let (wrapper, client, ssh_program) = match s.mosh_provider {
                MoshProvider::Bundled => {
                    let (wrapper, client, ssh) = runtime.bundled_mosh_paths()?;
                    (
                        program_path(wrapper)?,
                        Some(program_path(client)?),
                        program_path(ssh)?,
                    )
                }
                MoshProvider::System => (
                    provider_path(&runtime, ToolId::SystemMosh)?,
                    None,
                    provider_path(&runtime, ToolId::OpenSsh)?,
                ),
            };
            let mut ssh = ssh_arguments(
                &s.host,
                &s.user,
                s.mosh_ssh_port,
                &s.key,
                &s.jump,
                false,
                s.compression,
            )?;
            if s.agent_mode == AgentMode::Disabled {
                ssh.splice(0..0, ["-o".into(), "IdentityAgent=none".into()]);
            }
            apply_ssh_options(&mut ssh, s, p)?;
            let target = ssh.pop().unwrap();
            let quote = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
            let command = std::iter::once(ssh_program)
                .chain(ssh)
                .map(|arg| quote(&arg))
                .collect::<Vec<_>>()
                .join(" ");
            let mut args = vec!["--port".into(), s.port.to_string(), "--ssh".into(), command];
            if let Some(client) = client {
                args.push(format!("--client={client}"));
            }
            if !s.jump.trim().is_empty()
                || s.ssh_options.config_file.is_some()
                || wrapper.contains('%')
            {
                args.push("--experimental-remote-ip=remote".into());
            }
            args.extend(["--".into(), target]);
            (wrapper, args)
        }
        "Serial" => return Err("Serial sessions use the direct device transport.".into()),
        "RDP" => {
            validate_host(&s.host)?;
            validate_user(&s.user)?;
            if cfg!(windows) {
                let mut launch = windows_rdp_launch(s)?;
                launch.program = provider_path(
                    &RuntimeCatalog::for_current_executable()?,
                    ToolId::ExternalRdp,
                )?;
                return Ok(launch);
            } else {
                ("FreeRDP".into(), freerdp_arguments(s)?)
            }
        }
        "VNC" => {
            validate_host(&s.host)?;
            #[cfg(target_os = "macos")]
            {
                ("open".into(), vec![format!("vnc://{}:{}", s.host, s.port)])
            }
            #[cfg(not(target_os = "macos"))]
            {
                let mut a = vec![format!("{}::{}", s.host, s.port)];
                if s.view_only {
                    a.push("-ViewOnly=1".into());
                }
                if s.fullscreen {
                    a.push("-FullScreen=1".into());
                }
                ("vncviewer".into(), a)
            }
        }
        "Browser" => {
            if s.browser_options.embedded {
                return Err("Open this Browser session inside the TermCraft workspace.".into());
            }
            let address = crate::browser::validated_address(&s.host, false)?;
            #[cfg(target_os = "linux")]
            {
                ("xdg-open".into(), vec![address])
            }
            #[cfg(target_os = "macos")]
            {
                ("open".into(), vec![address])
            }
            #[cfg(windows)]
            {
                ("explorer.exe".into(), vec![address])
            }
        }
        "XDMCP" => {
            return Err("XDMCP uses its per-session owned engine. Connect through the workspace; installed-client fallback is not automatic.".into());
        }
        _ => return Err(format!("{} is not a supported session type.", s.protocol)),
    };
    let optional_provider = match s.protocol.as_str() {
        "SSH" => Some(ToolId::OpenSsh),
        "SFTP" => Some(ToolId::OpenSftp),
        "RDP" => Some(ToolId::ExternalRdp),
        "VNC" => Some(ToolId::ExternalVnc),
        "XDMCP" => Some(ToolId::Xdmcp),
        _ => None,
    };
    let program = if let Some(tool) = optional_provider {
        provider_path(&RuntimeCatalog::for_current_executable()?, tool)?
    } else if s.protocol == "Mosh" {
        program
    } else {
        find_program(&program)
            .ok_or_else(|| format!("The selected system program {program} is not installed or executable. Install the client, then reconnect."))?
            .into_os_string()
            .into_string()
            .map_err(|_| "The selected system program path is not Unicode")?
    };
    let mut args = args;
    if s.agent_mode == AgentMode::Disabled && matches!(s.protocol.as_str(), "SSH" | "SFTP") {
        args.splice(0..0, ["-o".into(), "IdentityAgent=none".into()]);
    }
    Ok(Launch {
        rdp_profile: None,
        program,
        args,
        directory,
    })
}
fn provider_path(catalog: &RuntimeCatalog, tool: ToolId) -> Result<String, String> {
    program_path(catalog.resolve(tool)?.executable()?)
}
fn program_path(path: PathBuf) -> Result<String, String> {
    path.into_os_string()
        .into_string()
        .map_err(|_| "The selected runtime program path is not Unicode".into())
}
pub fn sftp_quote(p: &str) -> Result<String, String> {
    if p.contains(['\n', '\r', '\0']) {
        return Err("File paths cannot contain newlines or a zero byte.".into());
    }
    Ok(format!(
        "\"{}\"",
        p.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}
pub fn sftp_quote_path(path: &std::path::Path) -> Result<Vec<u8>, String> {
    #[cfg(unix)]
    let bytes = {
        use std::os::unix::ffi::OsStrExt;
        path.as_os_str().as_bytes()
    };
    #[cfg(not(unix))]
    let bytes = path
        .to_str()
        .ok_or("The SFTP client cannot represent this native filename as UTF-8.")?
        .as_bytes();
    if bytes.iter().any(|b| matches!(b, b'\n' | b'\r' | 0)) {
        return Err("File paths cannot contain newlines or a zero byte.".into());
    }
    let mut quoted = vec![b'"'];
    for byte in bytes {
        if matches!(*byte, b'\\' | b'"') {
            quoted.push(b'\\');
        }
        quoted.push(*byte);
    }
    quoted.push(b'"');
    Ok(quoted)
}
pub fn open_path(path: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    let mut c = Command::new("xdg-open");
    #[cfg(target_os = "macos")]
    let mut c = Command::new("open");
    #[cfg(windows)]
    let mut c = Command::new("explorer.exe");
    c.arg(path).spawn().map(|_| ()).map_err(|e| e.to_string())
}

fn apply_ssh_options(
    args: &mut Vec<String>,
    session: &Session,
    prefs: &Preferences,
) -> Result<(), String> {
    if let Some(index) = args.iter().position(|a| a == "ServerAliveInterval=60") {
        args.drain(index - 1..=index);
    }
    let mut options = Vec::new();
    if let Some(path) = &session.ssh_options.config_file {
        options.extend([
            "-F".into(),
            path.to_str()
                .ok_or("OpenSSH config requires a Unicode path")?
                .into(),
        ]);
    }
    if session.ssh_options.config_file.is_none()
        || session.ssh_options.keepalive_secs.is_some()
        || session.ssh_options.keepalive_disabled
    {
        let interval = if session.ssh_options.keepalive_disabled {
            0
        } else {
            session
                .ssh_options
                .keepalive_secs
                .unwrap_or(prefs.ssh_defaults.keepalive_secs)
        };
        options.extend(["-o".into(), format!("ServerAliveInterval={interval}")]);
    }
    if session.ssh_options.config_file.is_none()
        || session.ssh_options.connect_timeout_secs.is_some()
    {
        options.extend([
            "-o".into(),
            format!(
                "ConnectTimeout={}",
                session
                    .ssh_options
                    .connect_timeout_secs
                    .unwrap_or(prefs.ssh_defaults.connect_timeout_secs)
            ),
        ]);
    }
    // Keep the port flag first: the SFTP adapter translates it to -P.
    args.splice(2..2, options);
    Ok(())
}

/// Build the actual Windows compatibility launch without placing credentials in arguments.
pub fn windows_rdp_launch(session: &Session) -> Result<Launch, String> {
    use std::io::Write;
    let bytes = crate::desktop::rdp_options::windows_profile(session)?;
    let mut profile = tempfile::Builder::new()
        .prefix("termcraft-rdp-")
        .suffix(".rdp")
        .make(crate::platform::private_file::create_new)
        .map_err(|e| format!("Create private RDP profile: {e}"))?;
    profile
        .write_all(&bytes)
        .map_err(|e| format!("Write RDP profile: {e}"))?;
    profile
        .flush()
        .map_err(|e| format!("Flush RDP profile: {e}"))?;
    let path = profile
        .path()
        .to_str()
        .ok_or("RDP profile path is not Unicode")?
        .to_owned();
    Ok(Launch {
        program: "mstsc.exe".into(),
        args: vec![path],
        directory: None,
        rdp_profile: Some(std::sync::Arc::new(profile)),
    })
}

/// Arguments for the supported installed FreeRDP subset. Never ignore selected options.
pub fn freerdp_arguments(session: &Session) -> Result<Vec<String>, String> {
    use crate::desktop::rdp_options::{ClipboardPolicy, KeyboardRouting, RdpSecurity};
    crate::model::validate_host(&session.host)?;
    crate::model::validate_user(&session.user)?;
    let options = &session.rdp_options;
    options.validate()?;
    if session.port == 0 {
        return Err("Choose a nonzero RDP port".into());
    }
    if session.view_only
        || !matches!(
            options.clipboard,
            ClipboardPolicy::Disabled | ClipboardPolicy::Bidirectional
        )
        || options.keyboard != KeyboardRouting::Remote
        || !options.drives.is_empty()
        || options.serial_ports
        || !options.serial_mappings.is_empty()
        || options.printers
        || options.printer_mapping.is_some()
        || options.smartcard
        || options.smartcard_reader.is_some()
        || options.certificate_authority.is_some()
        || options.keyboard_layout != 0
        || options.security != RdpSecurity::Nla
    {
        return Err("The installed FreeRDP adapter does not implement the selected clipboard direction, keyboard, device or security option. Use supported embedded options or change the selection.".into());
    }
    let host = if session.host.contains(':') && !session.host.starts_with('[') {
        format!("[{}]", session.host)
    } else {
        session.host.clone()
    };
    let mut args = vec![
        format!("/v:{host}:{}", session.port),
        format!("/size:{}x{}", session.width, session.height),
        "/sec:nla".into(),
        if options.clipboard == ClipboardPolicy::Disabled {
            "-clipboard"
        } else {
            "+clipboard"
        }
        .into(),
    ];
    if !session.user.is_empty() {
        args.push(format!("/u:{}", session.user));
    }
    if !options.domain.is_empty() {
        args.push(format!("/d:{}", options.domain));
    }
    if options.administrative {
        args.push("/admin".into());
    }
    if session.fullscreen {
        args.push("/f".into());
    }
    Ok(args)
}
