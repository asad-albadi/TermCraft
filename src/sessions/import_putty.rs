use super::{
    candidate, decode,
    report::{ImportReport, SourceLocation},
};
use crate::model::Session;
fn quoted(text: &str) -> Result<String, String> {
    let text = text
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .ok_or("Expected quoted registry string")?;
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                _ => return Err("Unsupported registry string escape".into()),
            }
        } else if c == '"' {
            return Err("Unescaped registry quote".into());
        } else {
            out.push(c);
        }
    }
    Ok(out)
}
fn unescape_name(name: &str) -> Result<String, String> {
    let mut out = Vec::new();
    let mut bytes = name.as_bytes().iter().copied();
    while let Some(b) = bytes.next() {
        if b == b'%' {
            let hi = bytes.next().ok_or("Incomplete escaped session name")?;
            let lo = bytes.next().ok_or("Incomplete escaped session name")?;
            let hex = |b: u8| {
                (b as char)
                    .to_digit(16)
                    .map(|v| v as u8)
                    .ok_or("Invalid escaped session name")
            };
            out.push(hex(hi)? * 16 + hex(lo)?);
        } else {
            out.push(b);
        }
    }
    String::from_utf8(out)
        .map_err(|_| "Session name is not UTF-8; explicit conversion is required".into())
}
pub fn import_putty(bytes: &[u8]) -> Result<ImportReport, String> {
    let (text, encoding) = decode(bytes)?;
    let mut report = ImportReport::new("PuTTY Windows Registry export", encoding);
    let mut sessions: Vec<(String, usize, Vec<(usize, String, String)>)> = Vec::new();
    let mut current = None;
    let mut saw_header = false;
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        let number = index + 1;
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        if matches!(line, "Windows Registry Editor Version 5.00" | "REGEDIT4") {
            saw_header = true;
            continue;
        }
        if line.starts_with('[') {
            if !line.ends_with(']') {
                return Err(format!("Line {number}: incomplete Registry section"));
            }
            let path = &line[1..line.len() - 1];
            let prefix = "HKEY_CURRENT_USER\\Software\\SimonTatham\\PuTTY\\Sessions\\";
            if path
                .get(..prefix.len())
                .is_some_and(|p| p.eq_ignore_ascii_case(prefix))
            {
                let encoded = &path[prefix.len()..];
                if encoded.contains('\\') {
                    return Err("Nested Registry keys are not session definitions".into());
                }
                sessions.push((unescape_name(encoded)?, number, Vec::new()));
                current = Some(sessions.len() - 1);
                if sessions.len() > super::MAX_SESSIONS {
                    return Err("Too many PuTTY sessions".into());
                }
            } else {
                current = None;
                report.warn("Registry section","Only per-user PuTTY session definitions are read; registry commands/host keys are never applied",&SourceLocation::new(number,"Registry",""));
            }
            continue;
        }
        if current.is_none() {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("Line {number}: malformed Registry assignment"))?;
        let key = quoted(key)?;
        if let Some(i) = current {
            if sessions[i].2.len() >= 4096 {
                return Err("PuTTY setting limit exceeded".into());
            }
            sessions[i].2.push((number, key, value.into()));
        }
    }
    if !saw_header {
        return Err("Expected a PuTTY .reg export header; other formats are not assumed".into());
    }
    for (name, line, fields) in sessions {
        let location = SourceLocation::new(line, "PuTTY Sessions", &name);
        let get = |key: &str| -> Result<Option<String>, String> {
            let values: Vec<_> = fields
                .iter()
                .filter(|(_, k, _)| k.eq_ignore_ascii_case(key))
                .collect();
            if values.len() > 1 {
                return Err(format!("Line {line}: duplicate PuTTY setting"));
            }
            values.first().map(|(_, _, v)| quoted(v)).transpose()
        };
        let protocol = get("Protocol")?.unwrap_or_else(|| "ssh".into());
        if protocol != "ssh" {
            report.warn(
                "Protocol",
                "Non-SSH PuTTY definition is not imported",
                &location,
            );
            continue;
        }
        let mut session = Session {
            name: name.clone(),
            host: get("HostName")?.ok_or("PuTTY session has no HostName")?,
            user: get("UserName")?.unwrap_or_default(),
            key: get("PublicKeyFile")?.unwrap_or_default(),
            folder: "PuTTY".into(),
            ..Default::default()
        };
        let ports: Vec<_> = fields
            .iter()
            .filter(|(_, k, _)| k.eq_ignore_ascii_case("PortNumber"))
            .collect();
        if ports.len() > 1 {
            return Err("Duplicate PortNumber".into());
        }
        if let Some((_, _, port)) = ports.first() {
            let value = port
                .strip_prefix("dword:")
                .ok_or("PortNumber must be a DWORD")?;
            session.port = u16::from_str_radix(value, 16).map_err(|_| "Invalid PuTTY port")?;
        }
        for (number, key, _) in fields {
            if ![
                "Protocol",
                "HostName",
                "UserName",
                "PublicKeyFile",
                "PortNumber",
            ]
            .contains(&key.as_str())
            {
                report.warn(
                    &key,
                    "Setting/credential not imported; value omitted",
                    &SourceLocation::new(number, "PuTTY Sessions", &name),
                );
            }
        }
        if session.key.to_ascii_lowercase().ends_with(".ppk") {
            report.warn("PublicKeyFile","PPK path preserved; embedded SSH and key import support recognized PPK files, while installed OpenSSH requires a converted key",&location);
        }
        candidate(&mut report, session, location)?;
    }
    Ok(report)
}
