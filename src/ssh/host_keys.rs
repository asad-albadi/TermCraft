use hmac::{Hmac, KeyInit, Mac};
use russh::keys::{
    self,
    ssh_key::{
        PublicKey,
        known_hosts::{HostPatterns, KnownHosts, Marker},
    },
};
use sha1::Sha1;
use std::path::Path;
#[derive(Debug, PartialEq, Eq)]
pub enum HostStatus {
    Known,
    Unknown,
}
fn endpoint(host: &str, port: u16) -> String {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let host = host
        .parse::<std::net::IpAddr>()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| host.to_ascii_lowercase());
    if port == 22 {
        host
    } else {
        format!("[{host}]:{port}")
    }
}
fn glob(pattern: &str, text: &str) -> bool {
    let (p, t) = (pattern.as_bytes(), text.as_bytes());
    let (mut i, mut j, mut star, mut restart) = (0, 0, None, 0);
    while j < t.len() {
        if i < p.len() && (p[i] == b'?' || p[i].eq_ignore_ascii_case(&t[j])) {
            i += 1;
            j += 1;
        } else if i < p.len() && p[i] == b'*' {
            star = Some(i);
            i += 1;
            restart = j;
        } else if let Some(s) = star {
            restart += 1;
            j = restart;
            i = s + 1;
        } else {
            return false;
        }
    }
    while i < p.len() && p[i] == b'*' {
        i += 1;
    }
    i == p.len()
}
fn matches(patterns: &HostPatterns, target: &str) -> bool {
    match patterns {
        HostPatterns::Patterns(patterns) => {
            let mut positive = false;
            for pattern in patterns {
                if let Some(negative) = pattern.strip_prefix('!') {
                    if glob(negative, target) {
                        return false;
                    }
                } else if glob(pattern, target) {
                    positive = true;
                }
            }
            positive
        }
        HostPatterns::HashedName { salt, hash } => {
            Hmac::<Sha1>::new_from_slice(salt).is_ok_and(|mac| {
                mac.chain_update(target.as_bytes())
                    .verify_slice(hash)
                    .is_ok()
            })
        }
    }
}
pub fn check(
    host: &str,
    port: u16,
    key: &PublicKey,
    paths: &[&Path],
) -> Result<HostStatus, String> {
    let target = endpoint(host, port);
    let (mut pinned, mut found, mut ca) = (false, false, false);
    for path in paths {
        if !path.exists() {
            continue;
        }
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("Cannot read known hosts {}: {e}", path.display()))?;
        // Normalize legal whitespace before using the marker/key parser.
        let normalized = text
            .lines()
            .filter_map(|line| {
                let line = line.split('#').next().unwrap_or("").trim();
                if line.is_empty() {
                    None
                } else {
                    Some(line.split_whitespace().collect::<Vec<_>>().join(" "))
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        for entry in KnownHosts::new(&normalized) {
            let entry = entry
                .map_err(|e| format!("Invalid known-host policy in {}: {e}", path.display()))?;
            if !matches(entry.host_patterns(), &target) {
                continue;
            }
            match entry.marker() {
                Some(Marker::Revoked) => {
                    if entry.public_key().key_data() == key.key_data() {
                        return Err(format!(
                            "The host key for {target} is revoked. Authentication stopped."
                        ));
                    }
                }
                Some(Marker::CertAuthority) => ca = true,
                None => {
                    pinned = true;
                    found |= entry.public_key().key_data() == key.key_data();
                }
            }
        }
    }
    if found {
        Ok(HostStatus::Known)
    } else if pinned {
        Err(format!(
            "Host key changed for {target}. Verify the server identity before reconnecting."
        ))
    } else if ca {
        Err("This endpoint requires host-certificate policy, which is not supported by the embedded engine yet. Select OpenSSH compatibility.".into())
    } else {
        Ok(HostStatus::Unknown)
    }
}
pub fn remember(host: &str, port: u16, key: &PublicKey, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let lock_path = path.with_extension("lock");
    let mut options = std::fs::OpenOptions::new();
    options.create(true).read(true).write(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let lock = options.open(lock_path).map_err(|e| e.to_string())?;
    // Never block the connection runtime on a second application's trust prompt.
    lock.try_lock().map_err(|_| {
        "Another connection is updating host trust. Reconnect and verify again.".to_string()
    })?;
    if check(host, port, key, &[path])? == HostStatus::Known {
        return Ok(());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(path)
            .map_err(|e| e.to_string())?;
    }
    let host = host.trim_start_matches('[').trim_end_matches(']');
    keys::known_hosts::learn_known_hosts_path(host, port, key, path).map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remembering_a_conflicting_key_cannot_expand_the_pin_set() {
        let path = std::env::temp_dir().join(format!(
            "termcraft-trust-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let a: PublicKey = KEY_A.parse().unwrap();
        let b: PublicKey = KEY_B.parse().unwrap();
        remember("fixture", 2222, &a, &path).unwrap();
        assert!(remember("fixture", 2222, &b, &path).is_err());
        assert_eq!(
            check("fixture", 2222, &a, &[&path]).unwrap(),
            HostStatus::Known
        );
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("lock"));
    }
    const KEY_A: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAINSZpn/uS3WBD7fQbSnXet5mEHhZc5/8buY2fHxJIgef";
    const KEY_B: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOXKLN9xrxw+g0l9p8a86XxeEwobtc4x0R0k+suotA85";
}
