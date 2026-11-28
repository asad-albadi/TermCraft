//! Versioned private parent/helper protocol. No terminal, log or process-argument transport.
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};
use zeroize::{Zeroize, Zeroizing};
pub const VERSION: u32 = 2;
#[path = "damage.rs"]
pub mod damage;
pub use damage::patch_bounds;
pub const MAX_CONTROL: usize = 64 * 1024;
pub const MAX_PIXELS: usize = 16 * 1024 * 1024;
pub const MAX_FRAME: usize = MAX_PIXELS * 4 + 20;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    #[serde(default)]
    pub options: super::rdp_options::RdpOptions,
    #[serde(default)]
    pub connect_to: Option<std::net::SocketAddr>,
    #[serde(default)]
    pub carrier_token: Option<[u8; 32]>,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub width: u16,
    pub height: u16,
    pub timeout_secs: u32,
    pub certificate_authority: Option<std::path::PathBuf>,
    pub remembered_certificate: Option<String>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum TrustDecision {
    Once,
    Remember,
    Reject,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Input {
    Configure(Config),
    Trust {
        id: u64,
        decision: TrustDecision,
    },
    Authenticate {
        id: u64,
        user: String,
        password: String,
        domain: String,
    },
    Pointer {
        x: u16,
        y: u16,
        button: Option<u8>,
        pressed: bool,
    },
    Scroll {
        x: u16,
        y: u16,
        delta: i16,
        horizontal: bool,
    },
    Key {
        code: u8,
        extended: bool,
        pressed: bool,
    },
    Text {
        value: String,
    },
    Resize {
        width: u16,
        height: u16,
    },
    Clipboard {
        text: String,
    },
    ReleaseAll,
    Close,
}
impl Drop for Input {
    fn drop(&mut self) {
        match self {
            Self::Authenticate { password, .. } => password.zeroize(),
            Self::Text { value } => value.zeroize(),
            Self::Clipboard { text } => text.zeroize(),
            _ => {}
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Event {
    ClipboardReady,
    Clipboard {
        text: String,
    },
    Ready {
        version: u32,
    },
    Connected {
        width: u16,
        height: u16,
    },
    Certificate {
        id: u64,
        host: String,
        port: u16,
        fingerprint: String,
        previous: Option<String>,
        reason: String,
        changed: bool,
    },
    TrustConfirmed {
        fingerprint: String,
        remember: bool,
    },
    Authentication {
        id: u64,
        host: String,
        port: u16,
        fingerprint: String,
        user: String,
    },
    Cursor {
        x: u16,
        y: u16,
        visible: bool,
    },
    Status {
        message: String,
    },
    Disconnected {
        message: String,
    },
}
pub fn dimensions(width: u16, height: u16) -> Result<usize, String> {
    let pixels = usize::from(width) * usize::from(height);
    if width == 0 || height == 0 || width > 8192 || height > 8192 || pixels > MAX_PIXELS {
        return Err("Desktop dimensions exceed the supported framebuffer limit".into());
    }
    Ok(pixels * 4)
}
pub fn read_packet(reader: &mut impl Read) -> io::Result<(u8, Zeroizing<Vec<u8>>)> {
    let mut header = [0; 5];
    reader.read_exact(&mut header)?;
    let length = u32::from_be_bytes(header[1..].try_into().unwrap()) as usize;
    let max = match header[0] {
        1 => MAX_CONTROL,
        2 | 3 => MAX_FRAME,
        _ => return Err(io::Error::other("Invalid desktop packet kind")),
    };
    if length == 0 || length > max {
        return Err(io::Error::other("Desktop packet exceeds its bound"));
    }
    let mut bytes = Zeroizing::new(vec![0; length]);
    reader.read_exact(&mut bytes)?;
    Ok((header[0], bytes))
}
pub fn write_packet(writer: &mut impl Write, kind: u8, bytes: &[u8]) -> io::Result<()> {
    let max = match kind {
        1 => MAX_CONTROL,
        2 | 3 => MAX_FRAME,
        _ => return Err(io::Error::other("Invalid desktop packet kind")),
    };
    if bytes.is_empty() || bytes.len() > max {
        return Err(io::Error::other("Desktop packet exceeds its bound"));
    }
    writer.write_all(&[kind])?;
    writer.write_all(&(bytes.len() as u32).to_be_bytes())?;
    writer.write_all(bytes)?;
    writer.flush()
}
pub fn write_json(writer: &mut impl Write, value: &impl Serialize) -> io::Result<()> {
    let bytes = Zeroizing::new(
        serde_json::to_vec(value)
            .map_err(|_| io::Error::other("Could not encode desktop message"))?,
    );
    write_packet(writer, 1, &bytes)
}
