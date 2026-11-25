use super::rdp_options::ClipboardPolicy;
use flate2::{Compression, Decompress, FlushDecompress, Status, write::ZlibEncoder};
use std::io::Write;
use zeroize::Zeroizing;

pub(super) const ENCODING: i32 = 0xc0a1e5ce_u32 as i32;
pub(super) const TEXT_LIMIT: usize = 8192;
pub(super) const WIRE_LIMIT: usize = 32 * 1024;
const UTF8_WIRE_LIMIT: usize = TEXT_LIMIT * 2 + 1;
const FORMATS: u32 = 0xffff;
const TEXT: u32 = 1;
const CAPS: u32 = 1 << 24;
const REQUEST: u32 = 1 << 25;
const PEEK: u32 = 1 << 26;
const NOTIFY: u32 = 1 << 27;
const PROVIDE: u32 = 1 << 28;

#[derive(Default)]
pub(super) struct Clipboard {
    peer_flags: u32,
    unsolicited_limit: usize,
    local: Option<Zeroizing<String>>,
    requested: bool,
}

#[derive(Default)]
pub(super) struct Reply {
    pub packet: Option<Zeroizing<Vec<u8>>>,
    pub text: Option<String>,
}

fn extended(flags: u32, data: &[u8]) -> Zeroizing<Vec<u8>> {
    let mut packet = Zeroizing::new(vec![6, 0, 0, 0]);
    packet.extend((-(4 + data.len() as i32)).to_be_bytes());
    packet.extend(flags.to_be_bytes());
    packet.extend(data);
    packet
}

fn provide(text: &str) -> Result<Zeroizing<Vec<u8>>, String> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    let normalized = Zeroizing::new(text.replace("\r\n", "\n"));
    let text = Zeroizing::new(normalized.replace('\n', "\r\n"));
    encoder
        .write_all(&((text.len() + 1) as u32).to_be_bytes())
        .and_then(|()| encoder.write_all(text.as_bytes()))
        .and_then(|()| encoder.write_all(&[0]))
        .map_err(|_| "Could not encode VNC Unicode clipboard")?;
    let compressed = Zeroizing::new(
        encoder
            .finish()
            .map_err(|_| "Could not encode VNC Unicode clipboard")?,
    );
    Ok(extended(PROVIDE | TEXT, &compressed))
}

fn decode(data: &[u8], formats: u32) -> Result<String, String> {
    let mut bytes = Zeroizing::new(vec![0; 4 + UTF8_WIRE_LIMIT + 1]);
    let mut decoder = Decompress::new(true);
    if decoder
        .decompress(data, &mut bytes, FlushDecompress::Finish)
        .map_err(|_| "Invalid VNC clipboard compression")?
        != Status::StreamEnd
        || decoder.total_in() as usize != data.len()
    {
        return Err("Invalid or oversized VNC clipboard compression".into());
    }
    let data = &bytes[..decoder.total_out() as usize];
    let length = data.get(..4).ok_or("Missing VNC clipboard text length")?;
    let length = u32::from_be_bytes(length.try_into().unwrap()) as usize;
    if length > UTF8_WIRE_LIMIT {
        return Err("VNC clipboard text exceeds its wire limit".into());
    }
    let text = data
        .get(4..4 + length)
        .ok_or("Truncated VNC clipboard text")?;
    // UTF-8 text is format bit 0, so it precedes other formats if offered.
    // Reject surplus bytes for text-only responses.
    if formats == TEXT && data.len() != 4 + length {
        return Err("Unexpected VNC clipboard text size".into());
    }
    let text = std::str::from_utf8(
        text.strip_suffix(&[0])
            .ok_or("Unterminated VNC clipboard text")?,
    )
    .map_err(|_| "Invalid VNC clipboard UTF-8")?;
    let text = text.replace("\r\n", "\n");
    if text.len() > TEXT_LIMIT || text.contains('\0') {
        return Err("VNC clipboard text exceeds 8 KiB or contains NUL".into());
    }
    Ok(text)
}

impl Clipboard {
    pub fn classic_remote(&mut self) {
        self.local = None;
        self.requested = false;
    }

    pub fn upload(&mut self, text: &str) -> Result<Zeroizing<Vec<u8>>, String> {
        if text.len() > TEXT_LIMIT || text.contains('\0') {
            return Err("VNC clipboard text exceeds 8 KiB or contains NUL".into());
        }
        if self.peer_flags & (TEXT | NOTIFY | PROVIDE) == TEXT | NOTIFY | PROVIDE {
            self.local = Some(Zeroizing::new(text.into()));
            return Ok(extended(NOTIFY | TEXT, &[]));
        }
        if self.peer_flags & (TEXT | PROVIDE) == TEXT | PROVIDE
            && text.len() + text.bytes().filter(|b| *b == b'\n').count() + 1
                <= self.unsolicited_limit
        {
            self.local = Some(Zeroizing::new(text.into()));
            return provide(text);
        }
        if text.chars().any(|c| c as u32 > 255) {
            return Err("VNC server has not enabled Unicode clipboard. This connection can send Latin-1 text.".into());
        }
        self.local = None;
        let mut packet = Zeroizing::new(vec![6, 0, 0, 0]);
        let count = text.chars().count() as u32;
        packet.extend(count.to_be_bytes());
        packet.extend(text.chars().map(|c| c as u8));
        Ok(packet)
    }

    pub fn incoming(
        &mut self,
        data: &[u8],
        policy: ClipboardPolicy,
        view_only: bool,
    ) -> Result<Reply, String> {
        let flags = u32::from_be_bytes(
            data.get(..4)
                .ok_or("Truncated VNC clipboard flags")?
                .try_into()
                .unwrap(),
        );
        let format = flags & FORMATS;
        let actions = flags & !FORMATS;
        let data = &data[4..];
        let receive = policy.from_remote();
        let upload = policy.to_remote() && !view_only;
        let mut reply = Reply::default();
        if flags & CAPS != 0 {
            if data.len() != format.count_ones() as usize * 4 {
                return Err("Invalid VNC clipboard capabilities".into());
            }
            self.peer_flags = flags;
            self.unsolicited_limit = if format & TEXT != 0 {
                u32::from_be_bytes(data[..4].try_into().unwrap()) as usize
            } else {
                0
            };
            let mut caps = CAPS;
            if receive {
                caps |= TEXT | NOTIFY | PROVIDE;
            }
            if upload {
                caps |= TEXT | REQUEST | PEEK;
            }
            reply.packet = Some(extended(caps, if caps & TEXT != 0 { &[0; 4] } else { &[] }));
            return Ok(reply);
        }
        if self.peer_flags & CAPS == 0 {
            return Err("VNC extended clipboard arrived before negotiation".into());
        }
        if actions != PROVIDE && !data.is_empty() {
            return Err("Invalid VNC clipboard action length".into());
        }
        match actions {
            REQUEST if upload && format & TEXT != 0 && self.peer_flags & PROVIDE != 0 => {
                if let Some(text) = &self.local {
                    reply.packet = Some(provide(text)?);
                }
            }
            PEEK if upload && self.peer_flags & NOTIFY != 0 => {
                reply.packet = Some(extended(
                    NOTIFY | if self.local.is_some() { TEXT } else { 0 },
                    &[],
                ));
            }
            NOTIFY if receive => {
                self.requested = false;
                if format & TEXT != 0 {
                    self.local = None;
                    if self.peer_flags & REQUEST != 0 {
                        self.requested = true;
                        reply.packet = Some(extended(REQUEST | TEXT, &[]));
                    }
                }
            }
            PROVIDE if receive && format & TEXT != 0 && self.requested => {
                self.requested = false;
                reply.text = Some(decode(data, format)?);
            }
            REQUEST | PEEK | NOTIFY | PROVIDE => {}
            _ => return Err("Unsupported VNC clipboard action".into()),
        }
        Ok(reply)
    }
}
