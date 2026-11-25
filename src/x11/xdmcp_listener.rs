//! Linux kernel ownership check for the upstream engine's non-inherited TCP listeners.
use std::{collections::BTreeSet, io::Read};
/// Verify every currently listening socket on the fixed display TCP port belongs
/// to this unreaped child. This detects a competing bind but cannot close the
/// upstream engine's pre-bind race: a source patch adopting reserved TCP FDs is
/// required to remove that window entirely.
pub(super) fn verify_tcp_listener(port: u16, pid: u32) -> Result<(), String> {
    let mut owned = BTreeSet::new();
    let entries = std::fs::read_dir(format!("/proc/{pid}/fd"))
        .map_err(|e| format!("Cannot verify owned XDMCP listener process: {e}"))?;
    for (index, entry) in entries.enumerate() {
        if index >= 1024 {
            return Err("Owned XDMCP process exceeded descriptor verification bound".into());
        }
        let entry = entry.map_err(|e| e.to_string())?;
        if let Ok(link) = std::fs::read_link(entry.path()) {
            if let Some(inode) = link
                .to_str()
                .and_then(|s| s.strip_prefix("socket:["))
                .and_then(|s| s.strip_suffix(']'))
                .and_then(|s| s.parse::<u64>().ok())
            {
                owned.insert(inode);
            }
        }
    }
    let mut listeners = BTreeSet::new();
    for table in ["/proc/net/tcp", "/proc/net/tcp6"] {
        let file = match std::fs::File::open(table) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("Cannot inspect XDMCP TCP listener ownership: {e}")),
        };
        let mut text = String::new();
        file.take(4 * 1024 * 1024 + 1)
            .read_to_string(&mut text)
            .map_err(|e| e.to_string())?;
        if text.len() > 4 * 1024 * 1024 {
            return Err("TCP listener verification exceeded its table bound".into());
        }
        for line in text.lines().skip(1) {
            let columns = line.split_whitespace().collect::<Vec<_>>();
            if columns.len() < 10 || columns[3] != "0A" {
                continue;
            }
            let local_port = columns[1]
                .rsplit_once(':')
                .and_then(|(_, p)| u16::from_str_radix(p, 16).ok());
            if local_port != Some(port) {
                continue;
            }
            let inode = columns[9]
                .parse::<u64>()
                .map_err(|_| "Invalid TCP listener identity")?;
            if inode == 0 || !owned.contains(&inode) {
                return Err(
                    "XDMCP TCP port is owned by another process; the owned engine was discarded"
                        .into(),
                );
            }
            listeners.insert(inode);
        }
    }
    if listeners.is_empty() {
        return Err("The owned XDMCP engine has no verified inbound X11 TCP listener".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owned_listener_is_required_and_another_pid_cannot_claim_it() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        verify_tcp_listener(port, std::process::id()).unwrap();
        assert!(verify_tcp_listener(port, 1).is_err());
        drop(listener);
        assert!(verify_tcp_listener(port, std::process::id()).is_err());
    }
}
