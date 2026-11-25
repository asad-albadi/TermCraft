use super::{
    Auth,
    files::{self, Share},
    supervisor::{self, Context, POLL},
};
use std::{
    io::{self, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};
use zeroize::{Zeroize, Zeroizing};

const IDLE: Duration = Duration::from_secs(120);
const TRANSFER: Duration = Duration::from_secs(120);
const DATA_CONNECT: Duration = Duration::from_secs(5);

enum DataEndpoint {
    Passive(TcpListener),
    Active(SocketAddr),
}

struct Session<'a> {
    control: TcpStream,
    input: Zeroizing<[u8; 4096]>,
    pending: std::ops::Range<usize>,
    share: &'a Share,
    auth: &'a Auth,
    context: &'a Context,
    user: Vec<u8>,
    authenticated: bool,
    failures: u8,
    directory: PathBuf,
    rename: Option<PathBuf>,
    data: Option<DataEndpoint>,
    ascii: bool,
    aborted: bool,
}

pub(super) fn serve(
    stream: TcpStream,
    share: &Share,
    auth: &Auth,
    context: &Context,
) -> io::Result<()> {
    let mut session = Session {
        input: Zeroizing::new([0; 4096]),
        pending: 0..0,
        control: stream,
        share,
        auth,
        context,
        user: Vec::new(),
        authenticated: false,
        failures: 0,
        directory: PathBuf::new(),
        rename: None,
        data: None,
        ascii: false,
        aborted: false,
    };
    session.reply("220 Workspace FTP ready")?;
    let mut line = Zeroizing::new(Vec::with_capacity(2048));
    while session.command_line(&mut line)? {
        let separator = line
            .iter()
            .position(|byte| *byte == b' ')
            .unwrap_or(line.len());
        let command = &line[..separator];
        let argument = line.get(separator + 1..).unwrap_or_default();
        let proceed = session.command(command, argument)?;
        line.as_mut_slice().zeroize();
        line.clear();
        if !proceed {
            break;
        }
    }
    Ok(())
}

impl Session<'_> {
    fn command_line(&mut self, line: &mut Vec<u8>) -> io::Result<bool> {
        let deadline = Instant::now() + IDLE;
        loop {
            self.context.check(deadline)?;
            if self.pending.is_empty() {
                let read =
                    supervisor::read(&mut self.control, &mut *self.input, self.context, deadline)?;
                if read == 0 {
                    return Ok(false);
                }
                self.pending = 0..read;
            }
            let bytes = &mut self.input[self.pending.clone()];
            let end = bytes.iter().position(|byte| *byte == b'\n');
            let take = end.map_or(bytes.len(), |position| position + 1);
            if line.len() + take > 2048 {
                self.reply("500 Command exceeds limit")?;
                return Ok(false);
            }
            line.extend_from_slice(&bytes[..take]);
            bytes[..take].zeroize();
            self.pending.start += take;
            if end.is_some() {
                let length = line.len();
                if length < 2 || line[length - 2] != b'\r' {
                    self.reply("500 Commands must end with CRLF")?;
                    return Ok(false);
                }
                line.truncate(length - 2);
                return Ok(true);
            }
        }
    }

    fn command(&mut self, command: &[u8], argument: &[u8]) -> io::Result<bool> {
        let command = command.to_ascii_uppercase();
        let rename = self.rename.take();
        if argument.iter().any(u8::is_ascii_control) {
            self.reply("501 Invalid command argument")?;
            return Ok(true);
        }
        match command.as_slice() {
            b"QUIT" => {
                self.reply("221 Goodbye")?;
                return Ok(false);
            }
            b"USER" => {
                self.authenticated = false;
                self.data = None;
                self.directory.clear();
                self.user.clear();
                self.user.extend_from_slice(argument);
                self.reply("331 Password required")?;
            }
            b"PASS" => {
                self.authenticated = self.auth.allows(&self.user, argument);
                if self.authenticated {
                    self.context.log("FTP client authenticated.");
                    self.reply("230 Logged in")?;
                } else {
                    self.failures += 1;
                    self.reply("530 Authentication failed")?;
                    if self.failures >= 3 {
                        return Ok(false);
                    }
                }
            }
            b"FEAT" => self.reply(
                "211-Features\r\n EPSV\r\n EPRT\r\n MLST type*;size*;\r\n SIZE\r\n UTF8\r\n211 End",
            )?,
            b"SYST" => self.reply("215 UNIX Type: L8")?,
            b"NOOP" => self.reply("200 OK")?,
            b"AUTH" => self.reply("502 Security mechanism unavailable")?,
            _ if !self.authenticated => self.reply("530 Log in first")?,
            b"OPTS" if argument.eq_ignore_ascii_case(b"UTF8 ON") => {
                self.reply("200 UTF-8 enabled")?
            }
            b"TYPE"
                if argument.eq_ignore_ascii_case(b"I") || argument.eq_ignore_ascii_case(b"A") =>
            {
                self.ascii = argument.eq_ignore_ascii_case(b"A");
                self.reply("200 Transfer type set")?;
            }
            b"MODE" if argument.eq_ignore_ascii_case(b"S") => self.reply("200 Stream mode")?,
            b"STRU" if argument.eq_ignore_ascii_case(b"F") => self.reply("200 File structure")?,
            b"PWD" | b"XPWD" => self.reply(&format!("257 \"{}\"", display(&self.directory)))?,
            b"EPSV" | b"PASV" => self.passive(command == b"EPSV")?,
            b"EPRT" | b"PORT" => self.active(&command, argument)?,
            b"ABOR" => {
                self.data = None;
                self.reply("225 No active transfer")?;
            }
            b"CWD" | b"XCWD" | b"CDUP" | b"XCUP" | b"SIZE" | b"MKD" | b"XMKD" | b"RMD"
            | b"XRMD" | b"DELE" | b"RNFR" | b"RNTO" | b"RETR" | b"STOR" | b"MLSD" | b"LIST"
            | b"NLST" => {
                let argument = if matches!(command.as_slice(), b"CDUP" | b"XCUP") {
                    b".."
                } else {
                    argument
                };
                let path = std::str::from_utf8(argument)
                    .ok()
                    .and_then(|name| files::path(&self.directory, name, true).ok());
                if let Some(path) = path {
                    self.path_command(&command, path, rename.as_deref())?;
                } else {
                    self.data = None;
                    self.reply("550 Invalid share path")?;
                }
            }
            _ => self.reply("502 Command unavailable")?,
        }
        Ok(true)
    }

    fn path_command(
        &mut self,
        command: &[u8],
        path: PathBuf,
        rename: Option<&Path>,
    ) -> io::Result<()> {
        let response = match command {
            b"CWD" | b"XCWD" | b"CDUP" | b"XCUP" => self.share.directory(&path).map(|_| {
                self.directory = path;
                "250 Directory changed".into()
            }),
            b"SIZE" => self
                .share
                .read(&path)
                .and_then(|file| file.metadata())
                .map(|metadata| format!("213 {}", metadata.len())),
            b"MKD" | b"XMKD" => self
                .share
                .create_directory(&path)
                .map(|()| format!("257 \"{}\"", display(&path))),
            b"RMD" | b"XRMD" | b"DELE" => self
                .share
                .remove(&path, command != b"DELE")
                .map(|()| "250 Removed".into()),
            b"RNFR" => self
                .share
                .check_write()
                .and_then(|()| self.share.kind(&path))
                .map(|_| {
                    self.rename = Some(path);
                    "350 Send destination".into()
                }),
            b"RNTO" => rename
                .ok_or_else(|| io::Error::other("Missing source"))
                .and_then(|source| self.share.rename(source, &path))
                .map(|()| "250 Renamed".into()),
            _ => return self.transfer(command, &path),
        };
        match response {
            Ok(reply) => self.reply(&reply),
            Err(error) => self.path_error(&error),
        }
    }

    fn path_error(&mut self, error: &io::Error) -> io::Result<()> {
        self.reply(match error.kind() {
            io::ErrorKind::AlreadyExists => "550 Destination already exists",
            io::ErrorKind::PermissionDenied => "550 Permission denied",
            _ => "550 Path unavailable",
        })
    }

    fn reply(&mut self, response: &str) -> io::Result<()> {
        let deadline = Instant::now() + DATA_CONNECT;
        supervisor::write(
            &mut self.control,
            response.as_bytes(),
            self.context,
            deadline,
        )?;
        supervisor::write(&mut self.control, b"\r\n", self.context, deadline)
    }

    fn passive(&mut self, extended: bool) -> io::Result<()> {
        self.data = None;
        let local = self.control.local_addr()?;
        if !extended && !local.is_ipv4() {
            return self.reply("522 Use EPSV for IPv6");
        }
        let listener = match TcpListener::bind(SocketAddr::new(local.ip(), 0)) {
            Ok(listener) => listener,
            Err(_) => return self.reply("425 Could not bind passive port"),
        };
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        self.data = Some(DataEndpoint::Passive(listener));
        if extended {
            self.reply(&format!("229 Entering Extended Passive Mode (|||{port}|)"))
        } else {
            let std::net::IpAddr::V4(ip) = local.ip() else {
                unreachable!()
            };
            let [a, b, c, d] = ip.octets();
            self.reply(&format!(
                "227 Entering Passive Mode ({a},{b},{c},{d},{},{})",
                port / 256,
                port % 256
            ))
        }
    }

    fn active(&mut self, command: &[u8], argument: &[u8]) -> io::Result<()> {
        self.data = None;
        let peer = self.control.peer_addr()?;
        let address = parse_active(command, argument);
        if let Some(address) =
            address.filter(|address| address.ip() == peer.ip() && address.port() >= 1024)
        {
            self.data = Some(DataEndpoint::Active(address));
            self.reply("200 Active endpoint accepted")
        } else {
            self.reply("501 Data endpoint must match the control peer and use an unprivileged port")
        }
    }

    fn data_stream(&mut self, endpoint: DataEndpoint) -> io::Result<TcpStream> {
        let peer = self.control.peer_addr()?;
        let deadline = Instant::now() + DATA_CONNECT;
        let stream = match endpoint {
            DataEndpoint::Passive(listener) => loop {
                self.check_transfer(deadline)?;
                match listener.accept() {
                    Ok((stream, address)) if address.ip() == peer.ip() => break stream,
                    Ok(_) => {}
                    Err(error) if supervisor::would_wait(&error) => thread::sleep(POLL),
                    Err(error) => return Err(error),
                }
            },
            DataEndpoint::Active(address) => {
                self.check_transfer(deadline)?;
                let connected = TcpStream::connect_timeout(&address, Duration::from_secs(1));
                self.check_transfer(deadline)?;
                connected?
            }
        };
        supervisor::prepare_stream(&stream)?;
        Ok(stream)
    }

    fn transfer(&mut self, command: &[u8], path: &Path) -> io::Result<()> {
        let prepared = match command {
            b"RETR" => self.share.read(path).map(Transfer::Read),
            b"STOR" => self.share.stage(path).map(Transfer::Write),
            _ => self.share.directory(path).map(Transfer::List),
        };
        let transfer = match prepared {
            Ok(transfer) => transfer,
            Err(error) => {
                self.data = None;
                return self.path_error(&error);
            }
        };
        let Some(endpoint) = self.data.take() else {
            return self.reply("425 Select a data endpoint first");
        };
        self.reply("150 Opening data connection")?;
        let stream = match self.data_stream(endpoint) {
            Ok(stream) => stream,
            Err(error) => {
                drop(transfer);
                return self.transfer_failure(error, "425 Could not open data connection");
            }
        };
        let ascii = self.ascii;
        let mut data = TransferIo {
            session: self,
            stream,
            deadline: Instant::now() + TRANSFER,
        };
        let result = match transfer {
            Transfer::Read(mut file) => send_file(&mut file, &mut data, ascii),
            Transfer::Write(mut file) => receive_file(&mut file, &mut data, ascii)
                .and_then(|()| file.publish(|| data.check())),
            Transfer::List(dir) => list(&dir, command, &mut data),
        };
        drop(data);
        match result {
            Ok(()) => self.reply("226 Transfer complete"),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => self.path_error(&error),
            Err(error) => self.transfer_failure(error, "426 Transfer ended before completion"),
        }
    }

    fn transfer_failure(&mut self, error: io::Error, reply: &str) -> io::Result<()> {
        if std::mem::take(&mut self.aborted) {
            self.reply("426 Transfer aborted")?;
            self.reply("226 Abort successful")
        } else if self.context.cancelled() || error.kind() == io::ErrorKind::UnexpectedEof {
            Err(error)
        } else {
            self.reply(reply)
        }
    }

    fn check_transfer(&mut self, deadline: Instant) -> io::Result<()> {
        self.context.check(deadline)?;
        self.read_available_control()?;
        let mut offset = self.pending.start;
        let abort = self.input[self.pending.clone()]
            .split_inclusive(|&byte| byte == b'\n')
            .find_map(|line| {
                let range = offset..offset + line.len();
                offset = range.end;
                line.eq_ignore_ascii_case(b"ABOR\r\n").then_some(range)
            });
        if let Some(range) = abort {
            self.input
                .copy_within(range.end..self.pending.end, range.start);
            let end = self.pending.end - range.len();
            self.input[end..self.pending.end].zeroize();
            self.pending.end = end;
            self.aborted = true;
            return Err(io::ErrorKind::ConnectionAborted.into());
        }
        Ok(())
    }

    fn read_available_control(&mut self) -> io::Result<()> {
        // Preserve partial control commands across data reads and leave commands
        // other than ABOR for the normal command loop.
        let length = self.pending.len();
        if self.pending.start != 0 {
            self.input.copy_within(self.pending.clone(), 0);
            self.input[length..self.pending.end].zeroize();
            self.pending = 0..length;
        }
        if length == self.input.len() {
            return Ok(());
        }
        self.control.set_nonblocking(true)?;
        let result = self.control.read(&mut self.input[length..]);
        self.control.set_nonblocking(false)?;
        match result {
            Ok(0) => Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(count) => {
                self.pending.end += count;
                Ok(())
            }
            Err(error) if supervisor::would_wait(&error) => Ok(()),
            Err(error) => Err(error),
        }
    }
}

struct TransferIo<'a, 'session> {
    session: &'a mut Session<'session>,
    stream: TcpStream,
    deadline: Instant,
}

impl TransferIo<'_, '_> {
    fn check(&mut self) -> io::Result<()> {
        self.session.check_transfer(self.deadline)
    }

    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        loop {
            self.check()?;
            match self.stream.read(buffer) {
                Err(error) if supervisor::would_wait(&error) => {}
                result => return result,
            }
        }
    }

    fn write(&mut self, mut buffer: &[u8]) -> io::Result<()> {
        while !buffer.is_empty() {
            self.check()?;
            match self.stream.write(buffer) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(bytes) => buffer = &buffer[bytes..],
                Err(error) if supervisor::would_wait(&error) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}

enum Transfer {
    Read(cap_std::fs::File),
    Write(files::Staged),
    List(cap_std::fs::Dir),
}

fn display(path: &Path) -> String {
    format!(
        "/{}",
        path.iter()
            .map(|part| part.to_string_lossy())
            .collect::<Vec<_>>()
            .join("/")
    )
}

fn parse_active(command: &[u8], argument: &[u8]) -> Option<SocketAddr> {
    let argument = std::str::from_utf8(argument).ok()?;
    if command == b"PORT" {
        let parts: Vec<_> = argument
            .split(',')
            .map(str::parse::<u8>)
            .collect::<Result<_, _>>()
            .ok()?;
        let [a, b, c, d, high, low] = parts.as_slice() else {
            return None;
        };
        return Some(SocketAddr::from((
            [*a, *b, *c, *d],
            u16::from_be_bytes([*high, *low]),
        )));
    }
    let delimiter = argument.chars().next().filter(|ch| ch.is_ascii_graphic())?;
    let mut parts = argument.split(delimiter);
    if parts.next()? != "" {
        return None;
    }
    let family = parts.next()?;
    let ip: std::net::IpAddr = parts.next()?.parse().ok()?;
    let port = parts.next()?.parse().ok()?;
    if (family != if ip.is_ipv4() { "1" } else { "2" })
        || parts.next()? != ""
        || parts.next().is_some()
    {
        return None;
    }
    Some(SocketAddr::new(ip, port))
}

fn list(dir: &cap_std::fs::Dir, command: &[u8], data: &mut TransferIo<'_, '_>) -> io::Result<()> {
    for (index, entry) in dir.entries()?.enumerate() {
        data.check()?;
        if index >= 4096 {
            return Err(io::Error::other("Directory entry limit"));
        }
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str().filter(|name| files::usable_name(name)) else {
            continue;
        };
        let metadata = dir.symlink_metadata(name)?;
        if !(metadata.is_dir() || metadata.is_file()) {
            continue;
        }
        let kind = if metadata.is_dir() { "dir" } else { "file" };
        let row = match command {
            b"MLSD" => format!("type={kind};size={}; {name}\r\n", metadata.len()),
            b"NLST" => format!("{name}\r\n"),
            _ => format!(
                "{}rwxr-xr-x 1 workspace workspace {} Jan 01 00:00 {name}\r\n",
                if metadata.is_dir() { 'd' } else { '-' },
                metadata.len()
            ),
        };
        data.write(row.as_bytes())?;
    }
    Ok(())
}

fn send_file(
    file: &mut cap_std::fs::File,
    wire: &mut TransferIo<'_, '_>,
    ascii: bool,
) -> io::Result<()> {
    let mut buffer = [0; 16 * 1024];
    let mut converted = Vec::with_capacity(buffer.len() * 2);
    loop {
        wire.check()?;
        let count = file.read(&mut buffer)?;
        if count == 0 {
            return Ok(());
        }
        let data = &buffer[..count];
        if ascii {
            converted.clear();
            for &byte in data {
                match byte {
                    b'\n' => converted.extend_from_slice(b"\r\n"),
                    b'\r' => converted.extend_from_slice(b"\r\0"),
                    byte => converted.push(byte),
                }
            }
        }
        wire.write(if ascii { &converted } else { data })?;
    }
}

fn receive_file(
    file: &mut files::Staged,
    wire: &mut TransferIo<'_, '_>,
    ascii: bool,
) -> io::Result<()> {
    let mut buffer = [0; 16 * 1024];
    let mut converted = Vec::with_capacity(buffer.len());
    let mut total = 0;
    let mut pending_cr = false;
    loop {
        let count = wire.read(&mut buffer)?;
        if count == 0 {
            return if pending_cr {
                Err(io::ErrorKind::InvalidData.into())
            } else {
                Ok(())
            };
        }
        total += count as u64;
        if total > files::UPLOAD_LIMIT {
            return Err(io::Error::other("Upload exceeds limit"));
        }
        let data = &buffer[..count];
        if ascii {
            converted.clear();
            for &byte in data {
                if pending_cr {
                    pending_cr = false;
                    match byte {
                        b'\n' => converted.push(b'\n'),
                        0 => converted.push(b'\r'),
                        _ => return Err(io::ErrorKind::InvalidData.into()),
                    }
                } else if byte == b'\r' {
                    pending_cr = true;
                } else {
                    converted.push(byte);
                }
            }
        }
        file.append(if ascii { &converted } else { data })?;
    }
}
