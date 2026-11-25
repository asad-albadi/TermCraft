//! Bounded, non-executing session migration. Parsing never mutates a workspace.
pub mod import_moba;
pub mod import_putty;
pub mod report;
pub use import_moba::import_moba;
pub use import_putty::import_putty;
use report::{ImportReport, SourceLocation};
pub const MAX_INPUT: usize = 8 * 1024 * 1024;
pub const MAX_SESSIONS: usize = 10_000;
fn decode(bytes: &[u8]) -> Result<(String, String), String> {
    if bytes.len() > MAX_INPUT {
        return Err("Import exceeds the 8 MiB limit".into());
    }
    let (text, encoding) = if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        if (bytes.len() - 2) % 2 != 0 {
            return Err("Incomplete UTF-16 import".into());
        }
        let le = bytes[0] == 0xff;
        let words = bytes[2..].chunks_exact(2).map(|b| {
            if le {
                u16::from_le_bytes([b[0], b[1]])
            } else {
                u16::from_be_bytes([b[0], b[1]])
            }
        });
        (
            String::from_utf16(&words.collect::<Vec<_>>()).map_err(|_| "Invalid UTF-16 import")?,
            if le { "UTF-16LE" } else { "UTF-16BE" },
        )
    } else {
        let declared_utf8 = bytes.starts_with(&[0xef, 0xbb, 0xbf]);
        let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
        match std::str::from_utf8(bytes) {
            Ok(text) => (text.to_owned(), "UTF-8"),
            Err(_) if declared_utf8 => return Err("Invalid declared UTF-8 input".into()),
            Err(_) => {
                let (text, errors) = encoding_rs::WINDOWS_1252.decode_without_bom_handling(bytes);
                if errors {
                    return Err("Invalid Windows-1252 input".into());
                }
                (text.into_owned(), "Windows-1252")
            }
        }
    };
    if text.contains('\0') || text.lines().any(|line| line.len() > 65536) {
        return Err("Import contains NUL or a line longer than 64 KiB".into());
    }
    Ok((text, encoding.into()))
}
fn candidate(
    report: &mut ImportReport,
    session: crate::model::Session,
    location: SourceLocation,
) -> Result<(), String> {
    report::validate(&session).map_err(|e| format!("Line {}: {e}", location.line))?;
    if report.candidates.len() >= MAX_SESSIONS {
        return Err("Import exceeds 10,000 sessions".into());
    }
    report
        .candidates
        .push(report::Candidate { session, location });
    Ok(())
}
fn folder(value: &str) -> Result<String, String> {
    let value = value.replace('\\', "/");
    if value.chars().any(char::is_control)
        || value.len() > 4096
        || value.split('/').any(|part| part == ".." || part == ".")
    {
        return Err("Unsupported folder path".into());
    }
    let value = value.trim_matches('/');
    Ok(if value.is_empty() {
        "My sessions".into()
    } else {
        value.into()
    })
}

pub mod shared;

pub mod groups;
