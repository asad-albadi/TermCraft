//! Connection-scoped OSC 7 metadata. These URLs never open a new network/local endpoint.
use crate::remote_files::RemotePath;
mod scripts;
pub use scripts::{ShellFlavor, integration_script};
const MAX_NOTICE: usize = 128 * 1024;
const MAX_PATH: usize = 32 * 1024;
#[derive(Clone, Copy, Default)]
enum State {
    #[default]
    Ground,
    Escape,
    Osc,
    OscEscape,
    Discard,
    DiscardEscape,
}
pub struct ShellDirectoryTracker {
    host: String,
    state: State,
    body: Vec<u8>,
    last: Option<RemotePath>,
    reason: Option<&'static str>,
}
impl ShellDirectoryTracker {
    pub fn new(host: &str) -> Self {
        Self {
            host: normalize_host(host),
            state: State::Ground,
            body: Vec::new(),
            last: None,
            reason: Some(
                "No compatible shell directory notices received; manual navigation is available.",
            ),
        }
    }
    pub fn last_directory(&self) -> Option<&RemotePath> {
        self.last.as_ref()
    }
    pub fn manual_reason(&self) -> Option<&'static str> {
        self.reason
    }
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<RemotePath> {
        let mut paths = Vec::new();
        for &byte in bytes {
            match self.state {
                State::Ground => {
                    if byte == 27 {
                        self.state = State::Escape;
                    }
                }
                State::Escape => {
                    if byte == b']' {
                        self.body.clear();
                        self.state = State::Osc;
                    } else if byte != 27 {
                        self.state = State::Ground;
                    }
                }
                State::Osc => match byte {
                    7 => self.finish(&mut paths),
                    27 => self.state = State::OscEscape,
                    0..=31 => {
                        if self.body.starts_with(b"7;") {
                            self.reason =
                                Some("Invalid directory notice; manual navigation is available.");
                        }
                        self.body.clear();
                        self.state = State::Discard;
                    }
                    _ => {
                        if self.body.len() < MAX_NOTICE {
                            self.body.push(byte);
                        } else {
                            if self.body.starts_with(b"7;") {
                                self.reason = Some(
                                    "Directory notice exceeded its size limit; manual navigation is available.",
                                );
                            }
                            self.body.clear();
                            self.state = State::Discard;
                        }
                    }
                },
                State::OscEscape => {
                    if byte == b'\\' {
                        self.finish(&mut paths);
                    } else if byte == 7 {
                        self.body.clear();
                        self.state = State::Ground;
                    } else {
                        self.body.clear();
                        self.state = if byte == 27 {
                            State::DiscardEscape
                        } else {
                            State::Discard
                        };
                    }
                }
                State::Discard => {
                    if byte == 7 {
                        self.state = State::Ground;
                    } else if byte == 27 {
                        self.state = State::DiscardEscape;
                    }
                }
                State::DiscardEscape => {
                    self.state = if byte == b'\\' || byte == 7 {
                        State::Ground
                    } else if byte == 27 {
                        State::DiscardEscape
                    } else {
                        State::Discard
                    };
                }
            }
        }
        paths
    }
    fn finish(&mut self, paths: &mut Vec<RemotePath>) {
        if let Some(uri) = self.body.strip_prefix(b"7;") {
            match parse_directory_uri(&self.host, uri) {
                Ok(path) => {
                    self.reason = None;
                    if self.last.as_ref() != Some(&path) {
                        self.last = Some(path.clone());
                        paths.push(path);
                        if paths.len() > 32 {
                            paths.remove(0);
                        }
                    }
                }
                Err(reason) => {
                    if self.last.is_none() || reason != "Directory notice belongs to another host."
                    {
                        self.reason = Some(reason);
                    }
                }
            }
        }
        self.body.clear();
        self.state = State::Ground;
    }
}
fn normalize_host(host: &str) -> String {
    let host = host
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(host);
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return ip.to_string();
    }
    if let Some((address, scope)) = host.split_once('%') {
        if let Ok(ip) = address.parse::<std::net::Ipv6Addr>() {
            return format!("{ip}%{scope}");
        }
    }
    host.trim_end_matches('.').to_ascii_lowercase()
}
fn decode(bytes: &[u8], limit: usize) -> Result<Vec<u8>, &'static str> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let byte = if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err("Malformed directory URL escape.");
            }
            let hex = |b: u8| match b {
                b'0'..=b'9' => Some(b - b'0'),
                b'a'..=b'f' => Some(b - b'a' + 10),
                b'A'..=b'F' => Some(b - b'A' + 10),
                _ => None,
            };
            let a = hex(bytes[i + 1]).ok_or("Malformed directory URL escape.")?;
            let b = hex(bytes[i + 2]).ok_or("Malformed directory URL escape.")?;
            i += 3;
            (a << 4) | b
        } else {
            let b = bytes[i];
            i += 1;
            b
        };
        if byte == 0 {
            return Err("Directory URL contains NUL.");
        }
        if out.len() >= limit {
            return Err("Directory path exceeded its size limit.");
        }
        out.push(byte);
    }
    Ok(out)
}
fn parse_directory_uri(host: &str, uri: &[u8]) -> Result<RemotePath, &'static str> {
    if uri.len() < 7 || !uri[..7].eq_ignore_ascii_case(b"file://") {
        return Err("Unsupported directory notice; manual navigation is available.");
    }
    let rest = &uri[7..];
    let split = rest
        .iter()
        .position(|b| *b == b'/')
        .ok_or("Directory URL has no absolute path.")?;
    let authority = decode(&rest[..split], 1024)?;
    let authority = std::str::from_utf8(&authority).map_err(|_| "Invalid directory URL host.")?;
    if authority.is_empty()
        || authority
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '@' | '/' | '\\'))
    {
        return Err("Invalid directory URL host.");
    }
    if normalize_host(authority) != host {
        return Err("Directory notice belongs to another host.");
    }
    let raw_path = &rest[split..];
    if raw_path
        .iter()
        .any(|b| matches!(*b, b'?' | b'#') || *b < 32 || *b == 127)
    {
        return Err("Directory URL has an unescaped query, fragment or control character.");
    }
    let path = decode(raw_path, MAX_PATH)?;
    if !path.starts_with(b"/") {
        return Err("Directory URL path must be absolute.");
    }
    RemotePath::from_bytes(path).map_err(|_| "Invalid remote directory path.")
}
