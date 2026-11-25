use super::{Limits, control::validate_argument};
use crate::remote_files::{EntryKind, RemoteEntry, RemotePath};
use std::{collections::HashSet, net::IpAddr};
use tokio::net::{TcpListener, TcpStream};

pub(super) enum DataEndpoint {
    Passive(TcpStream),
    Active(TcpListener),
}

impl DataEndpoint {
    pub(super) async fn open(self, peer: IpAddr, limits: &Limits) -> Result<TcpStream, String> {
        match self {
            Self::Passive(socket) => Ok(socket),
            Self::Active(listener) => {
                limits
                    .io(
                        "waiting for active data connection from the control peer",
                        async {
                            loop {
                                let (socket, address) = listener.accept().await?;
                                if address.ip().to_canonical() == peer.to_canonical() {
                                    return Ok(socket);
                                }
                            }
                        },
                    )
                    .await
            }
        }
    }
}

fn parenthesized(body: &[u8]) -> Option<&[u8]> {
    let start = body.iter().position(|&byte| byte == b'(')? + 1;
    let end = body[start..].iter().position(|&byte| byte == b')')? + start;
    Some(&body[start..end])
}

pub(super) fn epsv_port(body: &[u8]) -> Result<u16, String> {
    fn parse(body: &[u8]) -> Option<u16> {
        let fields = parenthesized(body)?;
        let delimiter = *fields.first()?;
        if !(33..=126).contains(&delimiter) || !fields.starts_with(&[delimiter; 3]) {
            return None;
        }
        let port = fields[3..].strip_suffix(&[delimiter])?;
        if port.is_empty() || !port.iter().all(u8::is_ascii_digit) {
            return None;
        }
        std::str::from_utf8(port)
            .ok()?
            .parse::<u16>()
            .ok()
            .filter(|&port| port != 0)
    }
    parse(body).ok_or_else(|| "Invalid FTP EPSV port response.".into())
}

pub(super) fn pasv_port(body: &[u8]) -> Result<u16, String> {
    fn parse(body: &[u8]) -> Option<u16> {
        let mut fields = parenthesized(body)?.split(|&byte| byte == b',');
        let mut address = [0u8; 6];
        for octet in &mut address {
            let field = fields.next()?;
            if field.is_empty() || !field.iter().all(u8::is_ascii_digit) {
                return None;
            }
            *octet = std::str::from_utf8(field).ok()?.parse().ok()?;
        }
        if fields.next().is_some() {
            return None;
        }
        // The server-provided address is deliberately ignored. Only the
        // already connected control peer can be a passive data destination.
        let port = u16::from_be_bytes([address[4], address[5]]);
        (port != 0).then_some(port)
    }
    parse(body).ok_or_else(|| "Invalid FTP PASV port response.".into())
}

const MAX_ENTRIES: usize = 10_000;

pub(super) fn parse_listing(parent: &RemotePath, data: &[u8]) -> Result<Vec<RemoteEntry>, String> {
    let mut entries = Vec::new();
    let mut names = HashSet::new();
    for line in data.split_inclusive(|&byte| byte == b'\n') {
        let line = line
            .strip_suffix(b"\r\n")
            .ok_or("Malformed FTP MLSD record ending.")?;
        let separator = line
            .iter()
            .position(|&byte| byte == b' ')
            .ok_or("Malformed FTP MLSD entry.")?;
        let (facts, name_with_space) = line.split_at(separator);
        let name = &name_with_space[1..];
        let (kind, size) = match mlsd_facts(facts)? {
            Some(facts) => facts,
            None => continue,
        };
        if name == b"." || name == b".." {
            continue;
        }
        validate_argument(name)?;
        let path = parent.child(name)?;
        validate_argument(path.as_bytes())?;
        if !names.insert(name) {
            return Err("The FTP directory contains ambiguous duplicate names.".into());
        }
        if entries.len() == MAX_ENTRIES {
            return Err("The FTP directory exceeds the 10,000-entry limit.".into());
        }
        entries.push(RemoteEntry {
            path,
            display_name: RemotePath::from_bytes(name.to_vec())?.display(),
            kind,
            size,
        });
    }
    Ok(entries)
}

fn mlsd_facts(facts: &[u8]) -> Result<Option<(EntryKind, Option<u64>)>, String> {
    if facts.is_empty() {
        return Ok(Some((EntryKind::Other, None)));
    }
    if !facts.ends_with(b";") {
        return Err("Malformed FTP MLSD entry facts.".into());
    }
    let mut kind = None;
    let mut size = None;
    for fact in facts[..facts.len() - 1].split(|&byte| byte == b';') {
        let separator = fact
            .iter()
            .position(|&byte| byte == b'=')
            .ok_or("Malformed FTP MLSD entry fact.")?;
        let (key, value) = (&fact[..separator], &fact[separator + 1..]);
        if key.eq_ignore_ascii_case(b"type") {
            if kind.is_some() {
                return Err("FTP MLSD entry contains multiple types.".into());
            }
            if value.eq_ignore_ascii_case(b"cdir") || value.eq_ignore_ascii_case(b"pdir") {
                return Ok(None);
            }
            kind = Some(if value.eq_ignore_ascii_case(b"dir") {
                EntryKind::Directory
            } else if value.eq_ignore_ascii_case(b"file") {
                EntryKind::File
            } else if value
                .get(..b"OS.unix=slink".len())
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"OS.unix=slink"))
            {
                EntryKind::Symlink
            } else {
                EntryKind::Other
            });
        } else if key.eq_ignore_ascii_case(b"size") {
            size = std::str::from_utf8(value)
                .ok()
                .and_then(|value| value.parse().ok());
        }
    }
    Ok(Some((kind.unwrap_or(EntryKind::Other), size)))
}

#[derive(Clone, Copy)]
pub(super) enum Direction {
    Upload,
    Download,
}

pub(super) struct Newlines {
    direction: Direction,
    pending_cr: bool,
}

impl Newlines {
    pub(super) fn new(direction: Direction) -> Self {
        Self {
            direction,
            pending_cr: false,
        }
    }

    pub(super) fn convert(&mut self, input: &[u8], output: &mut Vec<u8>) {
        output.clear();
        if input.is_empty() && self.pending_cr {
            self.literal_cr(output);
            self.pending_cr = false;
        }
        for &byte in input {
            if self.pending_cr {
                self.pending_cr = false;
                match (self.direction, byte) {
                    (Direction::Download, b'\n') => {
                        output.push(b'\n');
                        continue;
                    }
                    (Direction::Upload, b'\n') => {
                        output.extend_from_slice(b"\r\n");
                        continue;
                    }
                    (Direction::Download, 0) => {
                        output.push(b'\r');
                        continue;
                    }
                    _ => self.literal_cr(output),
                }
            }
            match (self.direction, byte) {
                (_, b'\r') => self.pending_cr = true,
                (Direction::Upload, b'\n') => output.extend_from_slice(b"\r\n"),
                _ => output.push(byte),
            }
        }
    }

    fn literal_cr(&self, output: &mut Vec<u8>) {
        output.push(b'\r');
        if matches!(self.direction, Direction::Upload) {
            output.push(0);
        }
    }
}
