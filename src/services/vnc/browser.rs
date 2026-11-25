//! Same-instance HTTP / binary WebSocket adapter; never accepts a client target.
use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use sha1::{Digest, Sha1};
use std::io::Read;
const HEADER_LIMIT: usize = 8192;
const FRAME_LIMIT: usize = 1_048_576;
const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' data:; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'";
include!("browser_assets.rs");
pub(super) struct Gateway {
    listener: TcpListener,
    upstream: SocketAddr,
}
impl Gateway {
    pub(super) fn bind(bind: SocketAddr, upstream: SocketAddr) -> io::Result<Self> {
        let listener = TcpListener::bind(bind)?;
        listener.set_nonblocking(true)?;
        let ip = if upstream.ip().is_unspecified() {
            if upstream.is_ipv4() {
                std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
            } else {
                std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)
            }
        } else {
            upstream.ip()
        };
        Ok(Self {
            listener,
            upstream: SocketAddr::new(ip, upstream.port()),
        })
    }
    pub(super) fn address(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }
    pub(super) fn serve(self, context: &Arc<Context>) -> io::Result<()> {
        let upstream = self.upstream;
        supervisor::serve_tcp(self.listener, context, move |mut stream, ctx| {
            handle(&mut stream, ctx, upstream)
        })
    }
}
fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "Invalid browser viewer request")
}
fn response(
    s: &mut TcpStream,
    ctx: &Context,
    status: &str,
    mime: &str,
    body: &[u8],
    head: bool,
) -> io::Result<()> {
    send(s, format!("HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: {CSP}\r\nCross-Origin-Resource-Policy: same-origin\r\n\r\n", body.len()).as_bytes(), ctx)?;
    if !head {
        send(s, body, ctx)?;
    }
    Ok(())
}
fn handle(s: &mut TcpStream, ctx: &Context, upstream: SocketAddr) -> io::Result<()> {
    let deadline = Instant::now() + IO_LIMIT;
    let mut request = Vec::new();
    while !request.ends_with(b"\r\n\r\n") {
        if request.len() >= HEADER_LIMIT {
            return response(
                s,
                ctx,
                "431 Request Header Fields Too Large",
                "text/plain",
                b"Request too large",
                false,
            );
        }
        let mut b = [0];
        exact(s, &mut b, ctx, deadline)?;
        request.push(b[0]);
    }
    let text = std::str::from_utf8(&request).map_err(|_| invalid())?;
    let mut lines = text.split("\r\n");
    let mut first = lines.next().ok_or_else(invalid)?.split(' ');
    let method = first.next().ok_or_else(invalid)?;
    let path = first.next().ok_or_else(invalid)?;
    if first.next() != Some("HTTP/1.1") || first.next().is_some() {
        return Err(invalid());
    }
    let mut headers = std::collections::BTreeMap::new();
    for line in lines {
        if line.is_empty() {
            break;
        }
        let (name, value) = line.split_once(':').ok_or_else(invalid)?;
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
            return Err(invalid());
        }
        if value.bytes().any(|b| (b < 32 && b != b'\t') || b == 127) {
            return Err(invalid());
        }
        if headers
            .insert(name.to_ascii_lowercase(), value.trim())
            .is_some()
        {
            return Err(invalid());
        }
    }
    let host = *headers.get("host").ok_or_else(invalid)?;
    let local = s.local_addr()?;
    let localhost = format!("localhost:{}", local.port());
    if host != local.to_string() && !(local.ip().is_loopback() && host == localhost) {
        return response(s, ctx, "403 Forbidden", "text/plain", b"Host denied", false);
    }
    let origin = format!("http://{host}");
    if headers.get("origin").is_some_and(|v| *v != origin)
        || headers.contains_key("transfer-encoding")
        || headers.get("content-length").is_some_and(|v| *v != "0")
    {
        return response(
            s,
            ctx,
            "403 Forbidden",
            "text/plain",
            b"Request denied",
            false,
        );
    }
    if method != "GET" && method != "HEAD" {
        return response(
            s,
            ctx,
            "405 Method Not Allowed",
            "text/plain",
            b"GET or HEAD required",
            false,
        );
    }
    if path == "/websockify" {
        if method != "GET"
            || headers.get("origin") != Some(&origin.as_str())
            || headers.get("sec-websocket-version") != Some(&"13")
            || !headers
                .get("upgrade")
                .is_some_and(|v| v.eq_ignore_ascii_case("websocket"))
            || !headers.get("connection").is_some_and(|v| {
                v.split(',')
                    .any(|w| w.trim().eq_ignore_ascii_case("upgrade"))
            })
        {
            return response(
                s,
                ctx,
                "403 Forbidden",
                "text/plain",
                b"WebSocket denied",
                false,
            );
        }
        let key = *headers.get("sec-websocket-key").ok_or_else(invalid)?;
        if STANDARD.decode(key).map_err(|_| invalid())?.len() != 16 {
            return Err(invalid());
        }
        // Extension negotiation deliberately absent: bounded uncompressed binary frames only.
        let accept = STANDARD.encode(Sha1::digest(
            format!("{key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11").as_bytes(),
        ));
        let protocol = if headers
            .get("sec-websocket-protocol")
            .is_some_and(|v| v.split(',').any(|p| p.trim() == "binary"))
        {
            "Sec-WebSocket-Protocol: binary\r\n"
        } else {
            ""
        };
        // Only our listener-selected RFB endpoint can be reached, after request validation.
        let mut rfb = TcpStream::connect_timeout(&upstream, IO_LIMIT)?;
        supervisor::prepare_stream(&rfb)?;
        send(s,format!("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n{protocol}\r\n").as_bytes(),ctx)?;
        return bridge(s, &mut rfb, ctx);
    }
    if let Some((mime, body)) = asset(path) {
        response(s, ctx, "200 OK", mime, body, method == "HEAD")
    } else {
        response(
            s,
            ctx,
            "404 Not Found",
            "text/plain",
            b"Viewer path not found",
            method == "HEAD",
        )
    }
}
fn ws_send(s: &mut TcpStream, ctx: &Context, opcode: u8, b: &[u8]) -> io::Result<()> {
    let mut h = vec![0x80 | opcode];
    if b.len() < 126 {
        h.push(b.len() as u8);
    } else if b.len() <= 65535 {
        h.push(126);
        h.extend_from_slice(&(b.len() as u16).to_be_bytes());
    } else {
        h.push(127);
        h.extend_from_slice(&(b.len() as u64).to_be_bytes());
    }
    send(s, &h, ctx)?;
    send(s, b, ctx)
}
fn frame(buffer: &[u8]) -> io::Result<Option<(usize, u8, Vec<u8>)>> {
    if buffer.len() < 2 {
        return Ok(None);
    }
    let op = buffer[0] & 15;
    if buffer[0] & 0xf0 != 0x80 || !matches!(op, 2 | 8 | 9 | 10) || buffer[1] & 128 == 0 {
        return Err(invalid());
    }
    let (n, start) = match buffer[1] & 127 {
        126 => {
            if buffer.len() < 4 {
                return Ok(None);
            }
            (
                u16::from_be_bytes(buffer[2..4].try_into().unwrap()) as u64,
                4,
            )
        }
        127 => {
            if buffer.len() < 10 {
                return Ok(None);
            }
            (u64::from_be_bytes(buffer[2..10].try_into().unwrap()), 10)
        }
        n => (u64::from(n), 2),
    };
    if n > FRAME_LIMIT as u64 || (op >= 8 && n > 125) {
        return Err(invalid());
    }
    if (start == 4 && n < 126) || (start == 10 && n < 65536) {
        return Err(invalid());
    }
    let end = start + 4 + n as usize;
    if buffer.len() < end {
        return Ok(None);
    }
    let mask = &buffer[start..start + 4];
    let data = buffer[start + 4..end]
        .iter()
        .enumerate()
        .map(|(i, b)| b ^ mask[i % 4])
        .collect();
    Ok(Some((end, op, data)))
}
fn bridge(ws: &mut TcpStream, rfb: &mut TcpStream, ctx: &Context) -> io::Result<()> {
    ws.set_nonblocking(true)?;
    rfb.set_nonblocking(true)?;
    let mut buffer = Vec::new();
    let mut scratch = [0; 16384];
    let mut activity = Instant::now();
    let mut partial = None;
    loop {
        ctx.check(activity + IDLE_LIMIT)?;
        if partial.is_some_and(|t: Instant| Instant::now().duration_since(t) > IO_LIMIT) {
            return Err(io::ErrorKind::TimedOut.into());
        }
        let mut progress = false;
        match ws.read(&mut scratch) {
            Ok(0) => return Ok(()),
            Ok(n) => {
                if buffer.len() + n > FRAME_LIMIT + 14 {
                    return Err(invalid());
                }
                if buffer.is_empty() {
                    partial = Some(Instant::now())
                }
                buffer.extend_from_slice(&scratch[..n]);
                progress = true;
            }
            Err(e) if supervisor::would_wait(&e) => {}
            Err(e) => return Err(e),
        }
        while let Some((n, op, data)) = frame(&buffer)? {
            match op {
                2 => supervisor::write(rfb, &data, ctx, Instant::now() + IO_LIMIT)?,
                8 => {
                    ws_send(ws, ctx, 8, &[])?;
                    return Ok(());
                }
                9 => ws_send(ws, ctx, 10, &data)?,
                _ => {}
            }
            buffer.drain(..n);
            partial = if buffer.is_empty() {
                None
            } else {
                Some(Instant::now())
            };
        }
        match rfb.read(&mut scratch) {
            Ok(0) => return Ok(()),
            Ok(n) => {
                ws_send(ws, ctx, 2, &scratch[..n])?;
                progress = true;
            }
            Err(e) if supervisor::would_wait(&e) => {}
            Err(e) => return Err(e),
        }
        if progress {
            activity = Instant::now()
        } else {
            thread::sleep(Duration::from_millis(5));
        }
    }
}
#[cfg(test)]
pub(super) mod tests;
