//! Explicit Windows paths and Moba-style /drives/<letter> paths. No environment expansion.
use std::path::PathBuf;
pub struct PathMap;
impl PathMap {
    pub fn to_native(input: &str) -> Result<PathBuf, String> {
        if input.len() > 32760 || input.chars().any(char::is_control) {
            return Err("Path contains controls or exceeds the path limit".into());
        }
        let normalized = if let Some(rest) = input.strip_prefix("/drives/") {
            let bytes = rest.as_bytes();
            if bytes.len() < 2 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b'/' {
                return Err("Use /drives/<letter>/path".into());
            }
            format!(
                "{}:\\{}",
                (bytes[0] as char).to_ascii_uppercase(),
                &rest[2..]
            )
        } else {
            input.to_owned()
        }
        .replace('/', "\\");
        // Explorer appends a final \. so drive roots never end in a quoted backslash.
        let normalized = if let Some(base) = normalized.strip_suffix("\\.") {
            if base.len() == 2 && base.ends_with(':') {
                format!("{base}\\")
            } else {
                base.to_owned()
            }
        } else {
            normalized
        };
        let bytes = normalized.as_bytes();
        let tail = if normalized.starts_with("\\\\") {
            let tail = &normalized[2..];
            let parts: Vec<_> = tail.split('\\').collect();
            if parts.len() < 2
                || parts[0].is_empty()
                || parts[1].is_empty()
                || ["?", "."].contains(&parts[0])
            {
                return Err("Choose an ordinary UNC server/share path".into());
            }
            tail
        } else if bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && bytes[2] == b'\\'
        {
            &normalized[3..]
        } else {
            return Err("Choose an absolute drive or UNC path".into());
        };
        for part in tail.split('\\') {
            if part == "."
                || part == ".."
                || part.contains([':', '"', '<', '>', '|', '?', '*'])
                || part.ends_with([' ', '.'])
            {
                return Err("Ambiguous, traversing or special Windows path".into());
            }
            let base = part.split('.').next().unwrap_or("").to_ascii_uppercase();
            if ["CON", "PRN", "AUX", "NUL"].contains(&base.as_str())
                || (base.len() == 4
                    && (base.starts_with("COM") || base.starts_with("LPT"))
                    && matches!(base.as_bytes()[3], b'1'..=b'9'))
            {
                return Err("Windows device names are not filesystem paths".into());
            }
        }
        Ok(PathBuf::from(normalized))
    }
    /// Applies Windows mapping only on Windows; ordinary Unix paths retain their meaning.
    pub fn local(input: &std::path::Path) -> Result<PathBuf, String> {
        #[cfg(windows)]
        {
            Self::to_native(input.to_str().ok_or("Path requires valid Unicode")?)
        }
        #[cfg(not(windows))]
        {
            Ok(input.to_owned())
        }
    }
}
