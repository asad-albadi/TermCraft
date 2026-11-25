use super::{
    candidate, decode, folder,
    report::{ImportReport, SourceLocation},
};
use crate::model::Session;
pub fn import_moba(bytes: &[u8]) -> Result<ImportReport, String> {
    let (text, encoding) = decode(bytes)?;
    let first = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("");
    if !first.starts_with('[') && first.contains(',') && !first.contains("=#") {
        return csv(&text, encoding);
    }
    let mut report = ImportReport::new("MobaXterm bookmark INI", encoding);
    let mut sections: Vec<(String, Vec<(usize, String, String)>)> = Vec::new();
    let mut current = None;
    let mut records = 0usize;
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        let number = index + 1;
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            if !line.ends_with(']') {
                return Err(format!("Line {number}: incomplete section header"));
            }
            let name = &line[1..line.len() - 1];
            if name == "Bookmarks"
                || name
                    .strip_prefix("Bookmarks_")
                    .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
            {
                if sections.len() > super::MAX_SESSIONS {
                    return Err("Too many bookmark sections".into());
                }
                sections.push((name.into(), Vec::new()));
                current = Some(sections.len() - 1);
            } else {
                current = None;
                report.warn(
                    "configuration section",
                    "Non-bookmark configuration and credentials are not imported",
                    &SourceLocation::new(number, name, ""),
                );
            }
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("Line {number}: expected a bookmark assignment"))?;
        if current.is_none() && sections.is_empty() && value.trim_start().starts_with('#') {
            sections.push(("Bookmarks".into(), Vec::new()));
            current = Some(0);
        }
        if let Some(i) = current {
            if !matches!(key.trim(), "SubRep" | "ImgNum") {
                records += 1;
                if records > super::MAX_SESSIONS {
                    return Err("Too many bookmark records".into());
                }
            }
            sections[i]
                .1
                .push((number, key.trim().into(), value.trim().into()));
        }
    }
    if sections.is_empty() {
        return Err("No MobaXterm bookmark sections or session records found".into());
    }
    for (section, entries) in sections {
        let path = entries
            .iter()
            .find(|(_, key, _)| key == "SubRep")
            .map(|(_, _, v)| v.as_str())
            .unwrap_or("");
        let path = folder(path)?;
        for (line, name, value) in entries {
            if matches!(name.as_str(), "SubRep" | "ImgNum") {
                continue;
            }
            let location = SourceLocation::new(line, &section, &name);
            let groups: Vec<_> = value.split('#').collect();
            if groups.len() < 3 {
                return Err(format!("Line {line}: incomplete packed session record"));
            }
            let fields: Vec<_> = groups[2].split('%').collect();
            if fields.first() != Some(&"0") {
                report.warn(
                    "session type",
                    "Unsupported session type; definition not imported",
                    &location,
                );
                continue;
            }
            if fields.len() < 4 {
                return Err(format!("Line {line}: incomplete SSH endpoint"));
            }
            let port = fields[2]
                .parse::<u16>()
                .map_err(|_| format!("Line {line}: invalid port"))?;
            let user = if fields[3] == "<default>" {
                report.warn(
                    "username",
                    "Original default login was not copied; choose the local login explicitly",
                    &location,
                );
                ""
            } else {
                fields[3]
            };
            let mut session = Session {
                name: name.clone(),
                host: fields[1].into(),
                port,
                user: user.into(),
                folder: path.clone(),
                key: fields.get(14).copied().unwrap_or("").into(),
                ..Default::default()
            };
            for (index, field) in fields.iter().enumerate().skip(4) {
                match index {
                    5|6=>match *field {"-1"=>if index==5{session.x11=true;}else{session.compression=true;},"0"=>if index==5{session.x11=false;}else{session.compression=false;},""=>{},_=>report.warn(if index==5{"X11 forwarding"}else{"compression"},"Unknown flag; default retained",&location)},
                    14=>{},
                    _ if !field.is_empty()=>report.warn(&format!("SSH field {index}"),"Not imported; review this setting in the source. Values are omitted to protect credentials",&location),
                    _=>{}
                }
            }
            if groups.len() > 3 {
                report.warn(
                    "terminal/bookmark options",
                    "Packed terminal and additional bookmark options are not imported",
                    &location,
                );
            }
            if session.key.contains("_CurrentDrive_")
                || session.key.to_ascii_lowercase().ends_with(".ppk")
            {
                report.warn("private key","Path preserved literally; drive placeholders/key formats require local verification or conversion",&location);
            }
            candidate(&mut report, session, location)?;
        }
    }
    Ok(report)
}
fn rows(text: &str) -> Result<Vec<Vec<String>>, String> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut closed = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                    closed = true;
                }
            } else {
                field.push(c);
            }
        } else {
            match c {
                '"' if field.is_empty() && !closed => quoted = true,
                ',' => {
                    row.push(std::mem::take(&mut field));
                    closed = false;
                }
                '\n' | '\r' => {
                    if c == '\r' && chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    row.push(std::mem::take(&mut field));
                    if row.iter().any(|v| !v.is_empty()) {
                        rows.push(std::mem::take(&mut row));
                    } else {
                        row.clear();
                    }
                    closed = false;
                }
                c if closed && !c.is_whitespace() => {
                    return Err("Unexpected text after a quoted CSV field".into());
                }
                c if !closed => field.push(c),
                _ => {}
            }
        }
        if row.len() > 128 || rows.len() > super::MAX_SESSIONS + 1 || field.len() > 65536 {
            return Err("CSV column/row/field limit exceeded".into());
        }
    }
    if quoted {
        return Err("Unclosed CSV quote".into());
    }
    row.push(field);
    if row.iter().any(|v| !v.is_empty()) {
        rows.push(row);
    }
    Ok(rows)
}
fn csv(text: &str, encoding: String) -> Result<ImportReport, String> {
    let rows = rows(text)?;
    let Some(headers) = rows.first() else {
        return Err("Empty CSV import".into());
    };
    let keys: Vec<_> = headers
        .iter()
        .map(|s| s.to_ascii_lowercase().replace([' ', '_', '-'], ""))
        .collect();
    let mut report = ImportReport::new("MobaXterm documented CSV", encoding);
    for (index, row) in rows.iter().enumerate().skip(1) {
        if row.len() != keys.len() {
            return Err(format!(
                "CSV row {}: column count differs from header",
                index + 1
            ));
        }
        let location = SourceLocation::new(index + 1, "CSV", "");
        let get = |aliases: &[&str]| {
            keys.iter()
                .position(|key| aliases.contains(&key.as_str()))
                .map(|i| row[i].as_str())
                .unwrap_or("")
        };
        let protocol = get(&["sessiontype", "type", "protocol"]);
        if !protocol.eq_ignore_ascii_case("ssh") {
            report.warn(
                "session type",
                "Only SSH CSV definitions are supported; this row was not imported",
                &location,
            );
            continue;
        }
        let host = get(&["remotehost", "hostname", "host"]);
        let port = get(&["port", "portnumber"]);
        let port = if port.is_empty() {
            22
        } else {
            port.parse()
                .map_err(|_| format!("CSV row {}: invalid port", index + 1))?
        };
        let name = get(&["sessionname", "name"]);
        let session = Session {
            host: host.into(),
            name: if name.is_empty() {
                host.into()
            } else {
                name.into()
            },
            user: get(&["username", "user"]).into(),
            port,
            key: get(&["keyfile", "privatekey"]).into(),
            folder: folder(get(&["foldername", "folder"]))?,
            ..Default::default()
        };
        for (key, value) in keys.iter().zip(row) {
            if ![
                "sessiontype",
                "type",
                "protocol",
                "remotehost",
                "hostname",
                "host",
                "port",
                "portnumber",
                "sessionname",
                "name",
                "username",
                "user",
                "keyfile",
                "privatekey",
                "foldername",
                "folder",
            ]
            .contains(&key.as_str())
                && !value.is_empty()
            {
                report.warn(
                    key,
                    "Column not imported; value omitted to protect credentials",
                    &location,
                );
            }
        }
        candidate(&mut report, session, location)?;
    }
    Ok(report)
}
