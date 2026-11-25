#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemotePath(Vec<u8>);
impl RemotePath {
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, String> {
        if bytes.is_empty() || bytes.contains(&0) {
            return Err("Remote paths must be nonempty and cannot contain NUL.".into());
        }
        Ok(Self(bytes))
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
    pub fn child(&self, name: &[u8]) -> Result<Self, String> {
        if name.is_empty()
            || name == b"."
            || name == b".."
            || name.contains(&b'/')
            || name.contains(&0)
        {
            return Err("Choose one remote filename, without path traversal.".into());
        }
        let mut bytes = self.0.clone();
        if !bytes.ends_with(b"/") {
            bytes.push(b'/');
        }
        bytes.extend_from_slice(name);
        Self::from_bytes(bytes)
    }
    pub fn parent(&self) -> Self {
        let trimmed = self.0.strip_suffix(b"/").unwrap_or(&self.0);
        let parent = match trimmed.iter().rposition(|b| *b == b'/') {
            Some(0) => b"/".to_vec(),
            Some(i) => trimmed[..i].to_vec(),
            None => b".".to_vec(),
        };
        if self.0 == b"/" {
            self.clone()
        } else {
            Self(parent)
        }
    }
    pub fn display(&self) -> String {
        fn valid(out: &mut String, text: &str) {
            for c in text.chars() {
                if c == '\\' {
                    out.push_str("\\\\");
                } else if c.is_control()
                    || matches!(c,'\u{061c}'|'\u{200b}'..='\u{200f}'|'\u{202a}'..='\u{202e}'|'\u{2060}'..='\u{206f}'|'\u{feff}')
                {
                    out.extend(c.escape_default());
                } else {
                    out.push(c);
                }
            }
        }
        let mut out = String::new();
        let mut bytes = self.as_bytes();
        while !bytes.is_empty() {
            match std::str::from_utf8(bytes) {
                Ok(text) => {
                    valid(&mut out, text);
                    break;
                }
                Err(error) => {
                    let good = error.valid_up_to();
                    valid(&mut out, std::str::from_utf8(&bytes[..good]).unwrap());
                    let bad = error.error_len().unwrap_or(bytes.len() - good);
                    for byte in &bytes[good..good + bad] {
                        out.push_str(&format!("\\x{byte:02x}"));
                    }
                    bytes = &bytes[good + bad..];
                }
            }
        }
        out
    }
    /// Byte-exact, ASCII-only form for final destructive confirmations.
    pub fn identity(&self) -> String {
        let mut out = String::new();
        for &b in self.as_bytes() {
            if (32..=126).contains(&b) && b != b'\\' {
                out.push(b as char);
            } else if b == b'\\' {
                out.push_str("\\\\");
            } else {
                out.push_str(&format!("\\x{b:02x}"));
            }
        }
        out
    }
}
