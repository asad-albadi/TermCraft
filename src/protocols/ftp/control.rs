use super::Limits;
use crate::remote_files::RemotePath;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use zeroize::Zeroizing;

pub(super) trait Channel: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Channel for T {}
pub(super) type Stream = Box<dyn Channel>;

const MAX_COMMAND_ARGUMENT: usize = 8192;
const MAX_REPLY_BYTES: usize = 64 * 1024;
const MAX_REPLY_LINES: usize = 128;

pub(super) fn validate_argument(argument: &[u8]) -> Result<(), String> {
    if argument.len() > MAX_COMMAND_ARGUMENT
        || argument
            .iter()
            .any(|&byte| matches!(byte, 0 | b'\r' | b'\n'))
    {
        return Err("An FTP command argument is too long or contains NUL or line breaks.".into());
    }
    Ok(())
}

pub(super) struct Reply {
    pub(super) code: u16,
    pub(super) body: Vec<u8>,
}

impl Reply {
    pub(super) fn require(self, command: &'static str, expected: &[u16]) -> Result<Self, String> {
        if expected.contains(&self.code) {
            Ok(self)
        } else {
            Err(self.failure(command))
        }
    }

    pub(super) fn unsupported(&self) -> bool {
        matches!(self.code, 500 | 501 | 502 | 504 | 522)
    }

    pub(super) fn failure(&self, command: &'static str) -> String {
        // The server can echo credentials or control characters in a reply.
        // Use standardized status descriptions for displayed error messages.
        let detail = match self.code {
            421 => "service closed the connection",
            425 => "cannot open a data connection",
            426 => "transfer aborted",
            430 | 530 => "authentication rejected",
            450 => "file unavailable",
            451 => "server processing error",
            452 | 552 => "server storage limit",
            500..=504 => "command unsupported or rejected",
            522 => "network protocol unsupported",
            532 => "a server account is required",
            534 | 536 => "server rejected the selected security policy",
            550 => "path unavailable or permission denied",
            553 => "file name rejected",
            _ => "unexpected server response",
        };
        format!("FTP {command}: {detail} ({}).", self.code)
    }
}

pub(super) struct Control<S> {
    wire: BufReader<S>,
    limits: Limits,
}

impl<S: AsyncRead + AsyncWrite + Unpin> Control<S> {
    pub(super) fn new(stream: S, limits: Limits) -> Self {
        Self {
            wire: BufReader::new(stream),
            limits,
        }
    }

    pub(super) fn into_inner(self) -> Result<S, String> {
        if !self.wire.buffer().is_empty() {
            return Err("Unexpected clear data after the FTP TLS upgrade reply.".into());
        }
        Ok(self.wire.into_inner())
    }

    pub(super) async fn greeting(&mut self) -> Result<(), String> {
        let reply = self.reply().await?;
        let reply = if reply.code == 120 {
            self.reply().await?
        } else {
            reply
        };
        reply.require("greeting", &[220]).map(|_| ())
    }

    pub(super) async fn command(
        &mut self,
        verb: &'static str,
        argument: Option<&[u8]>,
    ) -> Result<Reply, String> {
        let mut line = Zeroizing::new(Vec::with_capacity(
            verb.len() + 3 + argument.map_or(0, |arg| 2 * arg.len()),
        ));
        line.extend_from_slice(verb.as_bytes());
        if let Some(argument) = argument {
            validate_argument(argument)?;
            line.push(b' ');
            for &byte in argument {
                line.push(byte);
                if byte == 255 {
                    // FTP uses a Telnet control stream. Escape an IAC in a byte path.
                    line.push(byte);
                }
            }
        }
        line.extend_from_slice(b"\r\n");
        self.limits
            .io("control write", self.wire.get_mut().write_all(&line))
            .await?;
        self.limits
            .io("control flush", self.wire.get_mut().flush())
            .await?;
        drop(line);
        self.reply().await
    }

    pub(super) async fn reply(&mut self) -> Result<Reply, String> {
        let mut remaining = MAX_REPLY_BYTES;
        let mut line = Vec::new();
        let limits = self.limits.clone();
        limits
            .run("control response", async {
                read_line(&mut self.wire, &mut line, &mut remaining).await?;
                if line.len() < 4
                    || !line[..3].iter().all(u8::is_ascii_digit)
                    || !matches!(line[3], b' ' | b'-')
                {
                    return Err("Malformed FTP reply.".into());
                }
                let code = u16::from(line[0] - b'0') * 100
                    + u16::from(line[1] - b'0') * 10
                    + u16::from(line[2] - b'0');
                if line[3] == b'-' {
                    let terminator = [line[0], line[1], line[2], b' '];
                    for _ in 1..MAX_REPLY_LINES {
                        read_line(&mut self.wire, &mut line, &mut remaining).await?;
                        if line.starts_with(&terminator) {
                            return Ok(Reply {
                                code,
                                body: line.split_off(4),
                            });
                        }
                    }
                    return Err("FTP multiline reply exceeds 128 lines.".into());
                }
                Ok(Reply {
                    code,
                    body: line.split_off(4),
                })
            })
            .await
    }
}

async fn read_line<S: AsyncRead + Unpin>(
    wire: &mut BufReader<S>,
    line: &mut Vec<u8>,
    remaining: &mut usize,
) -> Result<(), String> {
    line.clear();
    loop {
        let chunk = wire
            .fill_buf()
            .await
            .map_err(|error| format!("FTP control read: {error}"))?;
        if chunk.is_empty() {
            return Err("FTP server closed the control connection.".into());
        }
        let newline = chunk.iter().position(|&byte| byte == b'\n');
        let length = newline.map_or(chunk.len(), |index| index + 1);
        *remaining = remaining
            .checked_sub(length)
            .ok_or("FTP reply exceeds 64 KiB.")?;
        line.extend_from_slice(&chunk[..length]);
        wire.consume(length);
        if newline.is_some() {
            line.truncate(line.len() - 1);
            if line.ends_with(b"\r") {
                line.pop();
            }
            return decode_telnet(line);
        }
    }
}

fn decode_telnet(line: &mut Vec<u8>) -> Result<(), String> {
    let Some(first_escape) = line.iter().position(|&byte| byte == 255) else {
        return Ok(());
    };
    let mut input = first_escape;
    let mut output = first_escape;
    while input < line.len() {
        let byte = line[input];
        input += 1;
        if byte == 255 {
            if line.get(input) != Some(&255) {
                return Err("Unexpected Telnet control sequence in FTP reply.".into());
            }
            input += 1;
        }
        line[output] = byte;
        output += 1;
    }
    line.truncate(output);
    Ok(())
}

pub(super) fn quoted_path(body: &[u8]) -> Result<RemotePath, String> {
    let mut source = body
        .strip_prefix(b"\"")
        .ok_or("FTP PWD did not return a quoted path.")?
        .iter()
        .copied()
        .peekable();
    let mut path = Vec::new();
    while let Some(byte) = source.next() {
        if byte == b'"' {
            if source.peek() != Some(&b'"') {
                validate_argument(&path)?;
                return RemotePath::from_bytes(path);
            }
            source.next();
        }
        path.push(byte);
    }
    Err("FTP PWD returned an unterminated quoted path.".into())
}
