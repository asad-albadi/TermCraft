use super::{
    files::{self, Share},
    supervisor::{self, Context, POLL, Workers},
};
use std::{
    collections::HashSet,
    io::{self, Read},
    net::{SocketAddr, UdpSocket},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

const BLOCK: usize = 512;
const PACKET: usize = BLOCK + 4;
const RETRY: Duration = Duration::from_millis(300);
const ATTEMPTS: usize = 5;
const TRANSFER: Duration = Duration::from_secs(120);

pub(super) fn serve(
    socket: UdpSocket,
    share: Arc<Share>,
    context: &Arc<Context>,
) -> io::Result<()> {
    let bind = SocketAddr::new(socket.local_addr()?.ip(), 0);
    let active = Arc::new(Mutex::new(HashSet::new()));
    let mut workers = Workers::default();
    let mut bytes = [0; PACKET + 1];
    let outcome = loop {
        if context.cancelled() {
            break Ok(());
        }
        match socket.recv_from(&mut bytes) {
            Ok((len, peer)) => {
                if bytes[..len].starts_with(&[0, 5]) || active.lock().unwrap().contains(&peer) {
                    continue;
                }
                let (write, path) = match request(&bytes[..len]) {
                    Ok(request) => request,
                    Err((code, message)) => {
                        let _ = send_error(&socket, peer, code, message);
                        continue;
                    }
                };
                if !workers.has_room() {
                    let _ = send_error(&socket, peer, 0, "Server busy");
                    continue;
                }
                active.lock().unwrap().insert(peer);
                let request = Active {
                    peers: active.clone(),
                    peer,
                };
                let context = context.clone();
                let share = share.clone();
                if let Err(error) = workers.spawn(move || {
                    let _request = request;
                    if transfer(bind, peer, path, write, &share, &context).is_err()
                        && !context.cancelled()
                    {
                        context.log("TFTP transfer ended with an error.");
                    }
                }) {
                    break Err(error);
                }
            }
            Err(error) if supervisor::would_wait(&error) => thread::sleep(POLL),
            Err(error) => break Err(error),
        }
    };
    drop(socket);
    context.request_stop();
    drop(workers);
    outcome
}

struct Active {
    peers: Arc<Mutex<HashSet<SocketAddr>>>,
    peer: SocketAddr,
}

impl Drop for Active {
    fn drop(&mut self) {
        self.peers.lock().unwrap().remove(&self.peer);
    }
}

fn request(bytes: &[u8]) -> Result<(bool, PathBuf), (u16, &'static str)> {
    let invalid = (4u16, "Invalid TFTP request");
    if bytes.len() < 6 || bytes.len() > PACKET {
        return Err(invalid);
    }
    let write = match word(bytes, 0) {
        1 => false,
        2 => true,
        _ => return Err(invalid),
    };
    let mut fields = bytes[2..].split(|&byte| byte == 0);
    let filename = std::str::from_utf8(fields.next().ok_or(invalid)?).map_err(|_| invalid)?;
    let mode = fields.next().ok_or(invalid)?;
    if !mode.eq_ignore_ascii_case(b"octet") || bytes.last() != Some(&0) {
        return Err((4, "Use octet transfer mode"));
    }
    // Valid RFC 2347 options can be ignored and the base protocol used.
    let mut fields = fields.peekable();
    while let Some(field) = fields.next() {
        if field.is_empty() && fields.peek().is_none() {
            break;
        }
        if field.is_empty() || fields.next().is_none_or(|value| value.is_empty()) {
            return Err(invalid);
        }
    }
    let path =
        files::path(Path::new(""), filename, false).map_err(|_| (2u16, "Path unavailable"))?;
    Ok((write, path))
}

fn transfer(
    bind: SocketAddr,
    peer: SocketAddr,
    path: PathBuf,
    write: bool,
    share: &Share,
    context: &Context,
) -> io::Result<()> {
    let socket = UdpSocket::bind(bind)?;
    socket.set_read_timeout(Some(POLL))?;
    socket.set_write_timeout(Some(POLL))?;
    let transfer = Transfer {
        socket,
        peer,
        context,
        deadline: Instant::now() + TRANSFER,
    };
    let outcome = if write {
        share.stage(&path).and_then(|file| transfer.receive(file))
    } else {
        share.read(&path).and_then(|file| transfer.send(file))
    };
    if let Err(error) = &outcome {
        let (code, message) = match error.kind() {
            io::ErrorKind::AlreadyExists => (6, "File already exists"),
            io::ErrorKind::PermissionDenied => (2, "Access denied"),
            io::ErrorKind::NotFound => (1, "File unavailable"),
            io::ErrorKind::InvalidData => (4, "Invalid transfer"),
            io::ErrorKind::ConnectionAborted => return outcome,
            _ => (0, "Transfer failed"),
        };
        if !context.cancelled() {
            let _ = send_error(&transfer.socket, peer, code, message);
        }
    } else {
        context.log("TFTP transfer complete.");
    }
    outcome
}

struct Transfer<'a> {
    socket: UdpSocket,
    peer: SocketAddr,
    context: &'a Context,
    deadline: Instant,
}

impl Transfer<'_> {
    fn exchange(
        &self,
        response: &[u8],
        opcode: u16,
        block: u16,
        buffer: &mut [u8; PACKET + 1],
    ) -> io::Result<usize> {
        for _ in 0..ATTEMPTS {
            self.context.check(self.deadline)?;
            self.socket.send_to(response, self.peer)?;
            let retry = Instant::now() + RETRY;
            while Instant::now() < retry {
                self.context.check(self.deadline)?;
                match self.receive_packet(buffer)? {
                    Some(_) if word(buffer, 0) == 5 => {
                        return Err(io::ErrorKind::ConnectionAborted.into());
                    }
                    Some(size) if word(buffer, 0) == opcode && word(buffer, 2) == block => {
                        if (opcode == 4 && size != 4) || size > PACKET {
                            return Err(io::ErrorKind::InvalidData.into());
                        }
                        return Ok(size);
                    }
                    _ => {}
                }
            }
        }
        Err(io::ErrorKind::TimedOut.into())
    }

    fn receive_packet(&self, buffer: &mut [u8; PACKET + 1]) -> io::Result<Option<usize>> {
        match self.socket.recv_from(buffer) {
            Ok((size, sender)) if sender != self.peer => {
                if !buffer[..size].starts_with(&[0, 5]) {
                    let _ = send_error(&self.socket, sender, 5, "Unknown transfer ID");
                }
                Ok(None)
            }
            Ok((size, _)) => Ok((size >= 4).then_some(size)),
            Err(error) if supervisor::would_wait(&error) => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn send(&self, mut file: cap_std::fs::File) -> io::Result<()> {
        let mut packet = [0; PACKET];
        let mut reply = [0; PACKET + 1];
        for block in 1..=u16::MAX {
            self.context.check(self.deadline)?;
            packet[..4].copy_from_slice(&header(3, block));
            let mut size = 0;
            while size < BLOCK {
                let count = file.read(&mut packet[4 + size..])?;
                if count == 0 {
                    break;
                }
                size += count;
            }
            if size == BLOCK && block == u16::MAX {
                return Err(io::ErrorKind::InvalidData.into());
            }
            self.exchange(&packet[..4 + size], 4, block, &mut reply)?;
            if size != BLOCK {
                return Ok(());
            }
        }
        Err(io::ErrorKind::InvalidData.into())
    }

    fn receive(&self, mut file: files::Staged) -> io::Result<()> {
        let mut packet = [0; PACKET + 1];
        for block in 1..=u16::MAX {
            let size = self.exchange(&header(4, block - 1), 3, block, &mut packet)?;
            file.append(&packet[4..size])?;
            if size < PACKET {
                file.publish(|| self.context.check(self.deadline))?;
                let ack = header(4, block);
                self.socket.send_to(&ack, self.peer)?;
                self.dally(block, &ack, &mut packet)?;
                return Ok(());
            }
        }
        Err(io::ErrorKind::InvalidData.into())
    }

    fn dally(&self, block: u16, ack: &[u8], buffer: &mut [u8; PACKET + 1]) -> io::Result<()> {
        let deadline = Instant::now() + RETRY * 3;
        while self.context.check(deadline).is_ok() {
            if self
                .receive_packet(buffer)?
                .is_some_and(|size| size < PACKET)
                && word(buffer, 0) == 3
                && word(buffer, 2) == block
            {
                self.socket.send_to(ack, self.peer)?;
            }
        }
        Ok(())
    }
}

fn word(buffer: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes([buffer[offset], buffer[offset + 1]])
}

fn header(opcode: u16, value: u16) -> [u8; 4] {
    let [a, b] = opcode.to_be_bytes();
    let [c, d] = value.to_be_bytes();
    [a, b, c, d]
}

fn send_error(socket: &UdpSocket, peer: SocketAddr, code: u16, message: &str) -> io::Result<()> {
    let mut packet = header(5, code).to_vec();
    packet.extend_from_slice(message.as_bytes());
    packet.push(0);
    socket.send_to(&packet, peer)?;
    Ok(())
}
