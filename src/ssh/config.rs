//! Supported OpenSSH configuration only. Call resolution on a worker, never during painting.
pub use crate::model::ssh_options::{ProxyKind, ProxySettings, SshDefaults, SshOptions};
use crate::model::{Session, validate_host, validate_user};
use std::{
    collections::{BTreeMap, HashSet},
    io::Read,
    path::Path,
};
#[derive(Clone, Debug)]
pub struct ConfigField {
    pub field: String,
    pub source: String,
    pub line: usize,
    pub applied: bool,
}
#[derive(Clone, Debug, Default)]
pub struct ConfigReport {
    pub fields: Vec<ConfigField>,
    pub unsupported: Vec<ConfigField>,
}
impl ConfigReport {
    pub fn requires_compatibility(&self) -> bool {
        !self.unsupported.is_empty()
    }
}
pub struct ResolvedSshOptions {
    pub session: Session,
    pub gateways: Vec<Session>,
    pub keepalive_secs: Option<u32>,
    pub connect_timeout_secs: u32,
    pub proxy: Option<ProxySettings>,
    pub report: ConfigReport,
    pub owner_profile: String,
}
#[derive(Clone)]
struct Value {
    text: String,
    line: usize,
}
impl SshOptions {
    pub fn parse_timeout(value: &str) -> Result<u32, String> {
        let value = value
            .parse::<u32>()
            .map_err(|_| "Enter a timeout between 1 and 3600 seconds.")?;
        if !(1..=3600).contains(&value) {
            return Err("Enter a timeout between 1 and 3600 seconds.".into());
        }
        Ok(value)
    }
    pub fn validate(&self, defaults: &SshDefaults) -> Result<(), String> {
        Self::parse_timeout(&defaults.connect_timeout_secs.to_string())?;
        if let Some(timeout) = self.connect_timeout_secs {
            Self::parse_timeout(&timeout.to_string())?;
        }
        if defaults.keepalive_secs == 0
            || defaults.keepalive_secs > 86400
            || self.keepalive_secs.is_some_and(|n| n == 0 || n > 86400)
        {
            return Err(
                "Keepalive interval must be 1–86400 seconds, or explicitly disabled.".into(),
            );
        }
        if self.jump_limit == 0 || self.jump_limit > 4 {
            return Err("Choose a jump-chain limit between 1 and 4.".into());
        }
        if let Some(path) = &self.config_file {
            if path.as_os_str().is_empty() {
                return Err("Choose an SSH configuration file.".into());
            }
        }
        if let Some(proxy) = &self.proxy {
            validate_host(&proxy.host)?;
            if proxy.host.len() > 255 || proxy.port == 0 {
                return Err("Choose a proxy hostname and port from 1 to 65535.".into());
            }
            if proxy
                .credential_ref
                .as_ref()
                .is_some_and(|v| v.is_empty() || v.len() > 256 || v.chars().any(char::is_control))
            {
                return Err("Invalid proxy credential reference.".into());
            }
        }
        Ok(())
    }
    pub fn resolve(
        session: &Session,
        defaults: &SshDefaults,
    ) -> Result<ResolvedSshOptions, String> {
        session.ssh_options.validate(defaults)?;
        let text = if let Some(path) = &session.ssh_options.config_file {
            read_config(path)?
        } else {
            String::new()
        };
        Self::resolve_text(
            session,
            defaults,
            &text,
            session
                .ssh_options
                .config_file
                .as_ref()
                .map(|p| p.display().to_string())
                .as_deref()
                .unwrap_or("session"),
        )
    }
    pub fn resolve_text(
        session: &Session,
        defaults: &SshDefaults,
        text: &str,
        source: &str,
    ) -> Result<ResolvedSshOptions, String> {
        session.ssh_options.validate(defaults)?;
        if text.len() > 256 * 1024 {
            return Err("SSH config exceeds 256 KiB.".into());
        }
        let mut report = ConfigReport::default();
        let values = parse(text, &session.host, source, &mut report)?;
        let mut effective = identity(session.clone(), &values, source, &mut report, false)?;
        let timeout = if let Some(value) = session.ssh_options.connect_timeout_secs {
            value
        } else if let Some(value) = values.get("connecttimeout") {
            Self::parse_timeout(&value.text)?
        } else {
            defaults.connect_timeout_secs
        };
        let keepalive = if session.ssh_options.keepalive_disabled {
            None
        } else if let Some(value) = session.ssh_options.keepalive_secs {
            Some(value)
        } else if let Some(value) = values.get("serveraliveinterval") {
            let n = value
                .text
                .parse::<u32>()
                .map_err(|_| "Invalid ServerAliveInterval")?;
            if n > 86400 {
                return Err("ServerAliveInterval exceeds 86400 seconds.".into());
            }
            if n == 0 { None } else { Some(n) }
        } else {
            Some(defaults.keepalive_secs)
        };
        let chain = effective.jump.clone();
        let tokens = if chain.is_empty() {
            vec![]
        } else {
            chain
                .split(',')
                .map(str::trim)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        if tokens.len() > session.ssh_options.jump_limit as usize {
            return Err("SSH jump chain exceeds the selected hop limit.".into());
        }
        let mut gateways = Vec::new();
        let mut seen = HashSet::new();
        for token in tokens {
            if token.is_empty() {
                return Err("SSH jump chain contains an empty hop.".into());
            }
            let mut target = effective.clone();
            target.jump = token.clone();
            let gateway = super::forward::jump_session(&target)?;
            let config = parse(text, &gateway.host, source, &mut report)?;
            let mut gateway = identity(gateway, &config, source, &mut report, true)?;
            // Explicit user/port in a jump token override its Host block.
            if let Some((user, _)) = token.rsplit_once('@') {
                gateway.user = user.into();
            }
            let address = token.rsplit('@').next().unwrap();
            let explicit_port = if let Some((_, tail)) = address.rsplit_once(']') {
                tail.starts_with(':')
            } else {
                address.matches(':').count() == 1
            };
            if explicit_port {
                let mut specified = effective.clone();
                specified.jump = token;
                gateway.port = super::forward::jump_session(&specified)?.port;
            }
            if let Some(value) = config.get("proxyjump") {
                if value.text != "none" {
                    report.unsupported.push(ConfigField {
                        field: "Nested ProxyJump".into(),
                        source: source.into(),
                        line: value.line,
                        applied: false,
                    });
                }
            }
            gateway.jump.clear();
            gateway.x11 = false;
            gateway.command.clear();
            if !seen.insert((
                gateway.host.to_ascii_lowercase(),
                gateway.port,
                gateway.user.clone(),
            )) {
                return Err("SSH jump chain repeats a gateway.".into());
            }
            gateways.push(gateway);
        }
        effective.jump = gateways
            .iter()
            .map(|s| {
                format!(
                    "{}@{}:{}",
                    s.user,
                    if s.host.contains(':') {
                        format!("[{}]", s.host)
                    } else {
                        s.host.clone()
                    },
                    s.port
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        validate_host(&effective.host)?;
        validate_user(&effective.user)?;
        if effective.port == 0 {
            return Err("SSH port must be 1–65535.".into());
        }
        Ok(ResolvedSshOptions {
            session: effective,
            gateways,
            keepalive_secs: keepalive,
            connect_timeout_secs: timeout,
            proxy: session.ssh_options.proxy.clone(),
            report,
            owner_profile: super::auth::profile_binding(session),
        })
    }
}
fn read_config(path: &Path) -> Result<String, String> {
    let metadata = std::fs::metadata(path).map_err(|_| "SSH config file is unavailable")?;
    if !metadata.is_file() || metadata.len() > 256 * 1024 {
        return Err("Choose a regular SSH config file up to 256 KiB.".into());
    }
    let mut text = String::new();
    {
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK);
        }
        let file = options
            .open(path)
            .map_err(|_| "Could not open SSH config")?;
        if !file
            .metadata()
            .map_err(|_| "Could not inspect SSH config")?
            .is_file()
        {
            return Err("SSH config must be a regular file".into());
        }
        file
    }
    .take(256 * 1024 + 1)
    .read_to_string(&mut text)
    .map_err(|_| "SSH config must be UTF-8 text")?;
    if text.len() > 256 * 1024 {
        return Err("SSH config exceeds 256 KiB.".into());
    }
    Ok(text)
}
fn parse(
    text: &str,
    host: &str,
    source: &str,
    report: &mut ConfigReport,
) -> Result<BTreeMap<String, Value>, String> {
    let mut values = BTreeMap::new();
    let mut active = true;
    for (index, line) in text.lines().enumerate() {
        if index > 4096 || line.len() > 16384 {
            return Err("SSH config line/count limit exceeded.".into());
        }
        let tokens = words(line)?;
        if tokens.is_empty() {
            continue;
        }
        let mut rest = tokens[1..].to_vec();
        let key = if let Some((key, value)) = tokens[0].split_once('=') {
            rest.insert(0, value.into());
            key.to_ascii_lowercase()
        } else {
            tokens[0].to_ascii_lowercase()
        };
        if rest.first().is_some_and(|s| s == "=") {
            rest.remove(0);
        }
        if let Some(value) = rest.first_mut() {
            if value.starts_with('=') {
                *value = value[1..].into();
            }
        }
        if key == "host" {
            active = matches_host(host, &rest)?;
            continue;
        }
        if key == "match" {
            report.unsupported.push(ConfigField {
                field: "Match".into(),
                source: source.into(),
                line: index + 1,
                applied: false,
            });
            active = false;
            continue;
        }
        if !active {
            continue;
        }
        if !matches!(
            key.as_str(),
            "hostname"
                | "user"
                | "port"
                | "identityfile"
                | "proxyjump"
                | "compression"
                | "forwardx11"
                | "forwardx11trusted"
                | "connecttimeout"
                | "serveraliveinterval"
        ) {
            report.unsupported.push(ConfigField {
                field: unsupported_name(&key),
                source: source.into(),
                line: index + 1,
                applied: false,
            });
            continue;
        }
        if rest.len() != 1 || rest[0].is_empty() {
            return Err(format!("Invalid SSH config value at line {}", index + 1));
        }
        let applied = !values.contains_key(&key);
        if !applied && key == "identityfile" {
            report.unsupported.push(ConfigField {
                field: "Multiple IdentityFile directives".into(),
                source: source.into(),
                line: index + 1,
                applied: false,
            });
        }
        report.fields.push(ConfigField {
            field: key.clone(),
            source: source.into(),
            line: index + 1,
            applied,
        });
        if applied {
            values.insert(
                key,
                Value {
                    text: rest.remove(0),
                    line: index + 1,
                },
            );
        }
    }
    Ok(values)
}
fn identity(
    mut session: Session,
    values: &BTreeMap<String, Value>,
    source: &str,
    report: &mut ConfigReport,
    gateway: bool,
) -> Result<Session, String> {
    let prefer = session.ssh_options.prefer_config || gateway;
    for (key, value) in values {
        let apply = match key.as_str() {
            "hostname" => {
                if value.text.contains('%') {
                    report.unsupported.push(ConfigField {
                        field: "HostName expansion".into(),
                        source: source.into(),
                        line: value.line,
                        applied: false,
                    });
                    continue;
                }
                validate_host(&value.text)?;
                session.host = value.text.clone();
                true
            }
            "user" => {
                validate_user(&value.text)?;
                if prefer || session.user.is_empty() {
                    session.user = value.text.clone();
                    true
                } else {
                    false
                }
            }
            "port" => {
                let port = value
                    .text
                    .parse::<u16>()
                    .map_err(|_| "Invalid SSH config Port")?;
                if port == 0 {
                    return Err("SSH config Port must be 1–65535.".into());
                }
                if prefer {
                    session.port = port;
                    true
                } else {
                    false
                }
            }
            "identityfile" => {
                if value.text.contains('%')
                    || (value.text.starts_with('~') && !value.text.starts_with("~/"))
                {
                    report.unsupported.push(ConfigField {
                        field: "IdentityFile expansion".into(),
                        source: source.into(),
                        line: value.line,
                        applied: false,
                    });
                    false
                } else if prefer || session.key.is_empty() {
                    session.key = if let Some(path) = value.text.strip_prefix("~/") {
                        crate::model::home_dir()
                            .join(path)
                            .to_str()
                            .ok_or("IdentityFile requires a Unicode path")?
                            .into()
                    } else {
                        value.text.clone()
                    };
                    true
                } else {
                    false
                }
            }
            "proxyjump" => {
                if prefer || session.jump.is_empty() {
                    session.jump = if value.text == "none" {
                        String::new()
                    } else {
                        value.text.clone()
                    };
                    true
                } else {
                    false
                }
            }
            "compression" | "forwardx11" => {
                let value = boolean(&value.text)?;
                if prefer && (key == "compression" || !gateway) {
                    if key == "compression" {
                        session.compression = value
                    } else {
                        session.x11 = value
                    }
                    true
                } else {
                    false
                }
            }
            "forwardx11trusted" => {
                boolean(&value.text)?;
                prefer && !gateway
            }
            _ => true,
        };
        if !apply {
            report.fields.push(ConfigField {
                field: key.clone(),
                source: "session override".into(),
                line: 0,
                applied: false,
            });
        }
    }
    if prefer
        && !gateway
        && session.x11
        && (values.contains_key("forwardx11") || values.contains_key("forwardx11trusted"))
    {
        let trusted = values
            .get("forwardx11trusted")
            .map(|v| boolean(&v.text))
            .transpose()?
            .unwrap_or(false);
        if !trusted {
            report.unsupported.push(ConfigField {
                field: "Untrusted X11 forwarding".into(),
                source: source.into(),
                line: values
                    .get("forwardx11")
                    .or_else(|| values.get("forwardx11trusted"))
                    .map(|v| v.line)
                    .unwrap_or(0),
                applied: false,
            });
        }
    }
    Ok(session)
}
fn boolean(value: &str) -> Result<bool, String> {
    match value.to_ascii_lowercase().as_str() {
        "yes" => Ok(true),
        "no" => Ok(false),
        _ => Err("SSH boolean settings require yes or no.".into()),
    }
}
fn matches_host(host: &str, patterns: &[String]) -> Result<bool, String> {
    if patterns.is_empty() || patterns.len() > 128 {
        return Err("Invalid Host pattern list.".into());
    }
    let mut found = false;
    for pattern in patterns {
        let (negative, pattern) = if let Some(p) = pattern.strip_prefix('!') {
            (true, p)
        } else {
            (false, pattern.as_str())
        };
        let escaped = regex::escape(pattern)
            .replace(r"\*", ".*")
            .replace(r"\?", ".");
        let regex = regex::RegexBuilder::new(&format!("^{escaped}$"))
            .case_insensitive(true)
            .size_limit(65536)
            .build()
            .map_err(|_| "Invalid Host pattern")?;
        if regex.is_match(host) {
            if negative {
                return Ok(false);
            }
            found = true
        }
    }
    Ok(found)
}
fn words(line: &str) -> Result<Vec<String>, String> {
    let mut words = vec![];
    let mut word = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => quoted = !quoted,
            '#' if !quoted => break,
            c if c.is_whitespace() && !quoted => {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
            }
            '\\' => {
                if chars
                    .peek()
                    .is_some_and(|c| *c == '"' || *c == '\\' || (!quoted && c.is_whitespace()))
                {
                    word.push(chars.next().unwrap())
                } else {
                    word.push(c)
                }
            }
            _ => word.push(c),
        }
    }
    if quoted {
        return Err("Unclosed quote in SSH config.".into());
    }
    if !word.is_empty() {
        words.push(word)
    }
    Ok(words)
}

fn unsupported_name(key: &str) -> String {
    if matches!(
        key,
        "proxycommand"
            | "include"
            | "match"
            | "hostkeyalias"
            | "identityagent"
            | "forwardagent"
            | "certificatefile"
            | "canonicalizehostname"
            | "knownhostscommand"
            | "localcommand"
            | "permitlocalcommand"
            | "sendenv"
            | "setenv"
            | "userknownhostsfile"
            | "stricthostkeychecking"
            | "serveralivecountmax"
            | "hostkeyalgorithms"
            | "pubkeyacceptedalgorithms"
            | "passwordauthentication"
            | "kbdinteractiveauthentication"
    ) {
        key.into()
    } else {
        "Unrecognized directive".into()
    }
}
