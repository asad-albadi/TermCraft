//! Untrusted desktop launch contract v1. Never accepts credentials, commands or workspace overrides.
use crate::cli::CliAction;
use serde::Deserialize;
use std::{
    io::Read,
    path::{Path, PathBuf},
};
fn decode(text: &str) -> Result<String, String> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err("Malformed URL escape".into());
            }
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3])
                .map_err(|_| "Invalid URL escape")?;
            out.push(u8::from_str_radix(hex, 16).map_err(|_| "Invalid URL escape")?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).map_err(|_| "Launch values require valid UTF-8".into())
}
fn session(id: String) -> Result<CliAction, String> {
    let action = CliAction::OpenSession {
        id,
        cwd: None,
        exit_when_done: false,
    };
    action.validate()?;
    Ok(action)
}
fn folder(path: PathBuf) -> Result<CliAction, String> {
    let path = super::path_map::PathMap::local(&path)?;
    if !path.is_absolute() {
        return Err("Desktop folders must be absolute".into());
    }
    let action = CliAction::OpenFolder {
        path,
        exit_when_done: false,
    };
    action.validate()?;
    Ok(action)
}
pub fn open_url(text: &str) -> Result<CliAction, String> {
    if text.len() > 16384 || text.chars().any(char::is_control) {
        return Err("Invalid launch URL".into());
    }
    // Parse original spelling: URL normalization must not silently remove traversal or escapes.
    if let Some(id) = text.strip_prefix("termcraft://session/") {
        if id.contains(['?', '#', '/']) {
            return Err("Session URL accepts only an encoded bookmark ID".into());
        }
        return session(decode(id)?);
    }
    if let Some(path) = text.strip_prefix("termcraft://cwd?path=") {
        if path.contains(['&', '#', '?']) {
            return Err("Folder URL accepts exactly one encoded absolute path".into());
        }
        return folder(decode(path)?.into());
    }
    Err("Supported launch URLs: termcraft://session/<encoded-id> or termcraft://cwd?path=<encoded-absolute-path>".into())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Link {
    version: u8,
    session: Option<String>,
    cwd: Option<PathBuf>,
}
pub fn open_file(path: &Path) -> Result<CliAction, String> {
    let mapped = super::path_map::PathMap::local(path)?;
    let path = mapped.as_path();
    if path.extension().and_then(|s| s.to_str()) != Some("termcraft-link") {
        return Err("Choose a .termcraft-link bookmark".into());
    }
    if path.as_os_str().len() > 8192
        || path
            .to_str()
            .is_none_or(|s| s.chars().any(char::is_control))
    {
        return Err("Invalid desktop bookmark path".into());
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Opening a FIFO must not block before the regular-file check; reject symlinks as well.
        options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
    }
    let file = options
        .open(path)
        .map_err(|e| format!("Cannot open desktop bookmark: {e}"))?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Bookmark must be a regular file".into());
    }
    let mut content = Vec::new();
    file.take(16385)
        .read_to_end(&mut content)
        .map_err(|e| e.to_string())?;
    if content.len() > 16384 {
        return Err("Desktop bookmark is too large".into());
    }
    let link: Link =
        serde_json::from_slice(&content).map_err(|e| format!("Invalid desktop bookmark: {e}"))?;
    if link.version != 1 {
        return Err("Unsupported desktop bookmark version".into());
    }
    match (link.session, link.cwd) {
        (Some(id), None) => session(id),
        (None, Some(path)) => folder(path),
        _ => Err("Desktop bookmark selects exactly one session or cwd".into()),
    }
}
