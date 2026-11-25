use crate::credentials::Secret;
use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt};
use cap_std::{
    ambient_authority,
    fs::{Dir, File, OpenOptions},
};
use sha2::{Digest, Sha256};
use std::{
    io::{self, Read, Seek, SeekFrom, Write},
    net::{Shutdown, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant, UNIX_EPOCH},
};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

const HEADER_LIMIT: usize = 16 * 1024;
const UPLOAD_LIMIT: u64 = 64 * 1024 * 1024;
const LIST_LIMIT: usize = 4096;
const LIST_BODY_LIMIT: usize = 1024 * 1024;
const WRITE_CHUNK: usize = 16 * 1024;
const STAGING_PREFIX: &str = ".harbor-upload-";

pub struct HttpShareConfig {
    pub root: PathBuf,
    pub bind: String,
    pub timeout: Duration,
    pub auth: Option<Secret>,
    pub uploads: bool,
}

impl HttpShareConfig {
    pub fn new(root: PathBuf, bind: String) -> Self {
        Self {
            root,
            bind,
            timeout: Duration::from_secs(30),
            auth: None,
            uploads: false,
        }
    }
}

struct Share {
    root: Dir,
    timeout: Duration,
    token_digest: Option<[u8; 32]>,
    uploads: bool,
}

pub struct Server {
    pub address: String,
    pub events: Receiver<String>,
    stop: Arc<AtomicBool>,
    connections: Arc<Mutex<Vec<(u64, TcpStream)>>>,
}
impl Server {
    pub fn start(root: &Path, address: &str) -> Result<Self, String> {
        Self::start_with_config(HttpShareConfig::new(root.to_owned(), address.to_owned()))
    }

    pub fn start_with_config(config: HttpShareConfig) -> Result<Self, String> {
        if config.timeout.is_zero() || config.timeout > Duration::from_secs(3600) {
            return Err("HTTP timeout must be positive and at most one hour.".into());
        }
        let token_digest = config
            .auth
            .map(|secret| {
                let token = secret.expose();
                if token.is_empty()
                    || token.len() > 1024
                    || !token
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-._~+/=".contains(&b))
                {
                    return Err(
                        "Enter a bearer token of 1 to 1024 letters, digits or token symbols.",
                    );
                }
                Ok(Sha256::digest(token.as_bytes()).into())
            })
            .transpose()?;
        let share = Arc::new(Share {
            root: Dir::open_ambient_dir(&config.root, ambient_authority())
                .map_err(|e| e.to_string())?,
            timeout: config.timeout,
            token_digest,
            uploads: config.uploads,
        });
        let listener = TcpListener::bind(&config.bind).map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let address = listener
            .local_addr()
            .map_err(|e| e.to_string())?
            .to_string();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let connections = Arc::new(Mutex::new(Vec::<(u64, TcpStream)>::new()));
        let active = connections.clone();
        let (tx, events) = mpsc::sync_channel(100);
        std::thread::spawn(move || {
            let mut id = 0u64;
            while !flag.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, peer)) => {
                        let mut current = active.lock().unwrap();
                        if flag.load(Ordering::Relaxed) || current.len() >= 8 {
                            let _ = stream.shutdown(Shutdown::Both);
                            continue;
                        }
                        let Ok(cancel_socket) = stream.try_clone() else {
                            continue;
                        };
                        id += 1;
                        let request_id = id;
                        current.push((request_id, cancel_socket));
                        drop(current);
                        let share = share.clone();
                        let flag = flag.clone();
                        let tx = tx.clone();
                        let active = active.clone();
                        std::thread::spawn(move || {
                            let result = serve(stream, &share, &flag);
                            let _ =
                                tx.try_send(format!("{peer}  {}", result.unwrap_or_else(|e| e)));
                            active.lock().unwrap().retain(|(i, _)| *i != request_id);
                        });
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(30))
                    }
                    Err(e) => {
                        let _ = tx.try_send(e.to_string());
                        break;
                    }
                }
            }
        });
        Ok(Self {
            address,
            events,
            stop,
            connections,
        })
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for (_, stream) in self.connections.lock().unwrap().iter() {
            let _ = stream.shutdown(Shutdown::Both);
        }
    }
}
fn relative_path(request: &str) -> Result<PathBuf, String> {
    let request = request.split('?').next().unwrap_or("/");
    if !request.starts_with('/') || request.starts_with("//") {
        return Err("Invalid path".into());
    }
    let mut decoded = Vec::new();
    let b = request.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = b.get(i + 1..i + 3).ok_or("Bad URL")?;
            let hex = std::str::from_utf8(hex).map_err(|_| "Bad URL")?;
            if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err("Bad URL".into());
            }
            decoded.push(u8::from_str_radix(hex, 16).map_err(|_| "Bad URL")?);
            i += 3;
        } else {
            decoded.push(b[i]);
            i += 1;
        }
    }
    let decoded = String::from_utf8(decoded).map_err(|_| "Bad URL")?;
    if decoded.contains(['\0', '\\']) || decoded.starts_with("//") {
        return Err("Invalid path".into());
    }
    let mut path = PathBuf::new();
    for name in decoded
        .split('/')
        .filter(|name| !name.is_empty() && *name != ".")
    {
        if name == ".." || is_staging_name(name) {
            return Err("Path is outside the shared folder".into());
        }
        #[cfg(windows)]
        if name.ends_with(['.', ' ']) || name.contains([':', '<', '>', '"', '|', '?', '*']) {
            return Err("Invalid path".into());
        }
        path.push(name);
    }
    Ok(path)
}

fn is_staging_name(name: &str) -> bool {
    name.get(..STAGING_PREFIX.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(STAGING_PREFIX))
}

pub fn resolve(root: &Path, request: &str) -> Result<PathBuf, String> {
    let p = relative_path(request)?;
    let path = root.join(p).canonicalize().map_err(|e| e.to_string())?;
    if !path.starts_with(root) {
        return Err("Path is outside the shared folder".into());
    }
    Ok(path)
}
fn html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn encode(s: &str) -> String {
    s.as_bytes()
        .iter()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(b) {
                (*b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_encoded_traversal() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .canonicalize()
            .unwrap();
        assert!(resolve(&root, "/%2e%2e/Cargo.toml").is_err());
        assert_eq!(
            resolve(&root, "/Cargo.toml").unwrap(),
            root.join("Cargo.toml")
        );
        assert!(resolve(&root, "/%00").is_err());
    }

    #[test]
    fn listing_writer_checks_deadline_after_partial_progress() {
        struct SlowWriter {
            calls: usize,
        }

        impl Write for SlowWriter {
            fn write(&mut self, data: &[u8]) -> io::Result<usize> {
                assert!(data.len() <= WRITE_CHUNK);
                self.calls += 1;
                std::thread::sleep(Duration::from_millis(25));
                Ok(1)
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let mut writer = SlowWriter { calls: 0 };
        let deadline = Instant::now() + Duration::from_millis(20);
        let result = write_transfer(
            &mut writer,
            &[0; WRITE_CHUNK * 2],
            deadline,
            &AtomicBool::new(false),
        );
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
        assert!(writer.calls <= 1);
    }

    #[test]
    fn listing_writer_checks_cancellation_after_partial_progress() {
        struct CancellingWriter<'a>(&'a AtomicBool);

        impl Write for CancellingWriter<'_> {
            fn write(&mut self, data: &[u8]) -> io::Result<usize> {
                assert!(data.len() <= WRITE_CHUNK);
                assert!(!self.0.swap(true, Ordering::Relaxed));
                Ok(1)
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let stop = AtomicBool::new(false);
        let mut writer = CancellingWriter(&stop);
        let result = write_transfer(
            &mut writer,
            &[0; WRITE_CHUNK * 2],
            Instant::now() + Duration::from_secs(5),
            &stop,
        );
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
    }
}

fn serve(mut stream: TcpStream, share: &Share, stop: &AtomicBool) -> Result<String, String> {
    serve_request(&mut stream, share, stop)
        .map(str::to_owned)
        .map_err(|error| error.to_string())
}

fn serve_request(
    stream: &mut TcpStream,
    share: &Share,
    stop: &AtomicBool,
) -> io::Result<&'static str> {
    stream.set_read_timeout(Some(Duration::from_millis(200).min(share.timeout)))?;
    stream.set_write_timeout(Some(Duration::from_secs(3).min(share.timeout)))?;
    let deadline = Instant::now() + share.timeout;
    let mut received = Zeroizing::new(Vec::new());
    let mut buffer = Zeroizing::new([0; 1024]);
    let header_deadline = deadline.min(Instant::now() + Duration::from_secs(5));
    let header_end = loop {
        let count = read_until(stream, &mut buffer[..], header_deadline, stop)?;
        if count == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        received.extend_from_slice(&buffer[..count]);
        if let Some(end) = received.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
            if end + 4 <= HEADER_LIMIT {
                break end;
            }
        }
        if received.len() >= HEADER_LIMIT {
            return empty(stream, "431 Request Header Fields Too Large", "");
        }
    };

    let Some(request) = Request::parse(&received[..header_end]) else {
        return empty(stream, "400 Bad Request", "");
    };
    if !share.authorized(&request) {
        return empty(stream, "401 Unauthorized", "WWW-Authenticate: Bearer\r\n");
    }
    let allow = if share.uploads {
        "Allow: GET, HEAD, PUT\r\n"
    } else {
        "Allow: GET, HEAD\r\n"
    };
    let is_head = request.method == "HEAD";
    let is_put = request.method == "PUT" && share.uploads;
    if request.method != "GET" && !is_head && !is_put {
        return empty(stream, "405 Method Not Allowed", allow);
    }
    if request.header("transfer-encoding").is_some() {
        return empty(stream, "400 Bad Request", "");
    }
    let Ok(path) = relative_path(request.target) else {
        return empty(stream, "404 Not Found", "");
    };
    if is_put {
        if request
            .target
            .split('?')
            .next()
            .is_some_and(|target| target.ends_with('/'))
        {
            return empty(stream, "404 Not Found", "");
        }
        return upload(
            stream,
            share,
            &request,
            &path,
            &received[header_end + 4..],
            deadline,
            stop,
        );
    }
    if let Ok(directory) = open_directory(&share.root, &path) {
        return list_directory(stream, &directory, &path, is_head, stop, deadline);
    }
    let Some((parent, name)) = path.parent().zip(path.file_name()) else {
        return empty(stream, "404 Not Found", "");
    };
    let file = open_directory(&share.root, parent).and_then(|dir| open_regular(&dir, name));
    match file {
        Ok(file) => download(stream, file, &request, is_head, stop, deadline),
        Err(_) => empty(stream, "404 Not Found", ""),
    }
}

struct Request<'a> {
    method: &'a str,
    target: &'a str,
    headers: Vec<(&'a str, &'a str)>,
}

impl<'a> Request<'a> {
    fn parse(bytes: &'a [u8]) -> Option<Self> {
        let mut lines = std::str::from_utf8(bytes).ok()?.split("\r\n");
        let mut fields = lines.next()?.split(' ');
        let method = fields.next()?;
        let target = fields.next()?;
        let version = fields.next()?;
        if !matches!(version, "HTTP/1.0" | "HTTP/1.1")
            || fields.next().is_some()
            || method.is_empty()
            || target.is_empty()
            || target.bytes().any(|b| b.is_ascii_control())
        {
            return None;
        }

        let mut request = Self {
            method,
            target,
            headers: Vec::new(),
        };
        for line in lines {
            let (name, value) = line.split_once(':')?;
            if name.is_empty()
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^`_|~".contains(&b))
                || value.bytes().any(|b| b.is_ascii_control() && b != b'\t')
            {
                return None;
            }
            if request.headers.len() >= 100 || request.header(name).is_some() {
                return None;
            }
            request
                .headers
                .push((name, value.trim_matches([' ', '\t'])));
        }
        Some(request)
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find_map(|(key, value)| key.eq_ignore_ascii_case(name).then_some(*value))
    }
}

impl Share {
    fn authorized(&self, request: &Request<'_>) -> bool {
        let Some(expected) = self.token_digest else {
            return true;
        };
        let Some((scheme, token)) = request
            .header("authorization")
            .and_then(|value| value.split_once(' '))
        else {
            return false;
        };
        scheme.eq_ignore_ascii_case("Bearer")
            && bool::from(Sha256::digest(token.as_bytes()).as_slice().ct_eq(&expected))
    }
}

fn check_transfer(deadline: Instant, stop: &AtomicBool) -> io::Result<()> {
    if stop.load(Ordering::Relaxed) {
        return Err(io::ErrorKind::Interrupted.into());
    }
    if Instant::now() >= deadline {
        return Err(io::ErrorKind::TimedOut.into());
    }
    Ok(())
}

fn read_until(
    stream: &mut TcpStream,
    buffer: &mut [u8],
    deadline: Instant,
    stop: &AtomicBool,
) -> io::Result<usize> {
    loop {
        check_transfer(deadline, stop)?;
        match stream.read(buffer) {
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut
                        | io::ErrorKind::WouldBlock
                        | io::ErrorKind::Interrupted
                ) => {}
            result => return result,
        }
    }
}

fn response_headers(
    stream: &mut TcpStream,
    status: &str,
    size: Option<u64>,
    extra: &str,
) -> io::Result<()> {
    stream.write_all(render_headers(status, size, extra).as_bytes())
}

fn render_headers(status: &str, size: Option<u64>, extra: &str) -> String {
    use std::fmt::Write as _;
    let mut headers =
        format!("HTTP/1.1 {status}\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\n");
    if let Some(size) = size {
        write!(headers, "Content-Length: {size}\r\n").expect("writing to String");
    }
    headers.push_str(extra);
    headers.push_str("\r\n");
    headers
}

fn write_transfer(
    writer: &mut impl Write,
    mut bytes: &[u8],
    deadline: Instant,
    stop: &AtomicBool,
) -> io::Result<()> {
    while !bytes.is_empty() {
        check_transfer(deadline, stop)?;
        match writer.write(&bytes[..bytes.len().min(WRITE_CHUNK)]) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(count) => bytes = &bytes[count..],
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn empty(stream: &mut TcpStream, status: &'static str, extra: &str) -> io::Result<&'static str> {
    response_headers(
        stream,
        status,
        (status != "304 Not Modified").then_some(0),
        extra,
    )?;
    Ok(status)
}

fn open_directory(root: &Dir, path: &Path) -> io::Result<Dir> {
    let mut directory = root.try_clone()?;
    for component in path {
        directory = directory.open_dir_nofollow(component)?;
    }
    Ok(directory)
}

fn open_regular(dir: &Dir, name: &std::ffi::OsStr) -> io::Result<File> {
    if !dir.symlink_metadata(name)?.is_file() {
        return Err(io::ErrorKind::NotFound.into());
    }
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = dir.open_with(name, &options)?;
    if !file.metadata()?.is_file() {
        return Err(io::ErrorKind::NotFound.into());
    }
    Ok(file)
}

fn list_directory(
    stream: &mut TcpStream,
    dir: &Dir,
    path: &Path,
    head: bool,
    stop: &AtomicBool,
    deadline: Instant,
) -> io::Result<&'static str> {
    let mut items = Vec::new();
    for (index, entry) in dir.entries()?.enumerate() {
        check_transfer(deadline, stop)?;
        if index == LIST_LIMIT {
            return listing_over_limit(stream, head, deadline, stop);
        }
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str().filter(|name| !is_staging_name(name)) else {
            continue;
        };
        let kind = entry.file_type()?;
        if kind.is_file() || kind.is_dir() {
            items.push((name.to_owned(), kind.is_dir()));
        }
    }
    items.sort_unstable();
    let base: String = path
        .iter()
        .map(|part| format!("/{}", encode(&part.to_string_lossy())))
        .collect();
    let mut body = format!(
        "<!doctype html><meta charset=utf-8><base href=\"{base}/\"><title>Shared files</title><style>body{{font:16px system-ui;max-width:800px;margin:48px auto}}li{{margin:12px}}</style><h1>Shared files</h1><ul>",
    );
    for (name, directory) in items {
        check_transfer(deadline, stop)?;
        let suffix = if directory { "/" } else { "" };
        let row = format!(
            "<li><a href=\"./{}{suffix}\">{}{suffix}</a></li>",
            encode(&name),
            html(&name)
        );
        if body.len() + row.len() + "</ul>".len() > LIST_BODY_LIMIT {
            return listing_over_limit(stream, head, deadline, stop);
        }
        body.push_str(&row);
    }
    body.push_str("</ul>");
    let headers = render_headers(
        "200 OK",
        Some(body.len() as u64),
        "Content-Type: text/html; charset=utf-8\r\nCache-Control: no-store\r\n",
    );
    write_transfer(stream, headers.as_bytes(), deadline, stop)?;
    if !head {
        write_transfer(stream, body.as_bytes(), deadline, stop)?;
    }
    Ok("200 OK")
}

fn listing_over_limit(
    stream: &mut TcpStream,
    head: bool,
    deadline: Instant,
    stop: &AtomicBool,
) -> io::Result<&'static str> {
    let status = "503 Service Unavailable";
    let body = b"Directory listing exceeds the server's entry or size limit.\n";
    let headers = render_headers(
        status,
        Some(body.len() as u64),
        "Content-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\n",
    );
    write_transfer(stream, headers.as_bytes(), deadline, stop)?;
    if !head {
        write_transfer(stream, body, deadline, stop)?;
    }
    Ok(status)
}

enum ByteRange {
    Full,
    Partial { start: u64, end: u64 },
    Unsatisfiable,
}

fn byte_range(value: Option<&str>, size: u64) -> ByteRange {
    let Some((unit, range)) = value.and_then(|value| value.split_once('=')) else {
        return ByteRange::Full;
    };
    if !unit.eq_ignore_ascii_case("bytes") || range.contains(',') {
        return ByteRange::Full;
    }
    let Some((first, last)) = range.split_once('-') else {
        return ByteRange::Full;
    };
    if first.is_empty() {
        return match decimal(last) {
            Some(length) if length > 0 && size > 0 => ByteRange::Partial {
                start: size.saturating_sub(length),
                end: size - 1,
            },
            Some(_) => ByteRange::Unsatisfiable,
            None => ByteRange::Full,
        };
    }
    let Some(start) = decimal(first) else {
        return ByteRange::Full;
    };
    let end = if last.is_empty() {
        size.saturating_sub(1)
    } else {
        let Some(end) = decimal(last) else {
            return ByteRange::Full;
        };
        end
    };
    if start >= size || end < start {
        return ByteRange::Unsatisfiable;
    }
    ByteRange::Partial {
        start,
        end: end.min(size - 1),
    }
}

fn decimal(value: &str) -> Option<u64> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(value.parse().unwrap_or(u64::MAX))
}

fn download(
    stream: &mut TcpStream,
    mut file: File,
    request: &Request<'_>,
    head: bool,
    stop: &AtomicBool,
    deadline: Instant,
) -> io::Result<&'static str> {
    let metadata = file.metadata()?;
    let size = metadata.len();
    let mut headers = String::from("Cache-Control: private, no-cache\r\nAccept-Ranges: bytes\r\n");
    if let Ok(modified) = metadata.modified().and_then(|time| {
        time.into_std()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)
    }) {
        let tag = format!("\"{size:x}-{:x}\"", modified.as_nanos());
        headers.push_str(&format!("ETag: W/{tag}\r\n"));
        if request.header("if-none-match").is_some_and(|value| {
            value.split(',').any(|candidate| {
                let candidate = candidate.trim();
                candidate == "*" || candidate.strip_prefix("W/").unwrap_or(candidate) == tag
            })
        }) {
            return empty(stream, "304 Not Modified", &headers);
        }
    } else if request.header("if-none-match") == Some("*") {
        return empty(stream, "304 Not Modified", &headers);
    }

    let range = if head || request.header("if-range").is_some() {
        ByteRange::Full
    } else {
        byte_range(request.header("range"), size)
    };
    let (status, start, len) = match range {
        ByteRange::Full => ("200 OK", 0, size),
        ByteRange::Partial { start, end } => {
            headers.push_str(&format!("Content-Range: bytes {start}-{end}/{size}\r\n"));
            ("206 Partial Content", start, end - start + 1)
        }
        ByteRange::Unsatisfiable => {
            headers.push_str(&format!("Content-Range: bytes */{size}\r\n"));
            return empty(stream, "416 Range Not Satisfiable", &headers);
        }
    };
    headers
        .push_str("Content-Type: application/octet-stream\r\nContent-Disposition: attachment\r\n");
    response_headers(stream, status, Some(len), &headers)?;
    if !head {
        file.seek(SeekFrom::Start(start))?;
        let mut source = file.take(len);
        let mut buffer = [0; 16 * 1024];
        while source.limit() != 0 {
            check_transfer(deadline, stop)?;
            let count = source.read(&mut buffer)?;
            if count == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            stream.write_all(&buffer[..count])?;
        }
    }
    Ok(status)
}

struct StagedUpload<'a> {
    directory: &'a Dir,
    name: String,
}

impl Drop for StagedUpload<'_> {
    fn drop(&mut self) {
        let _ = self.directory.remove_file(&self.name);
    }
}

fn upload(
    stream: &mut TcpStream,
    share: &Share,
    request: &Request<'_>,
    path: &Path,
    initial: &[u8],
    deadline: Instant,
    stop: &AtomicBool,
) -> io::Result<&'static str> {
    let Some(length) = request.header("content-length") else {
        return empty(stream, "411 Length Required", "");
    };
    let Some(length) = decimal(length) else {
        return empty(stream, "400 Bad Request", "");
    };
    if length > UPLOAD_LIMIT {
        return empty(stream, "413 Content Too Large", "");
    }
    if initial.len() as u64 > length {
        return empty(stream, "400 Bad Request", "");
    }
    let Some((parent, name)) = path.parent().zip(path.file_name()) else {
        return empty(stream, "404 Not Found", "");
    };
    let Ok(directory) = open_directory(&share.root, parent) else {
        return empty(stream, "404 Not Found", "");
    };
    match directory.symlink_metadata(name) {
        Ok(_) => return empty(stream, "409 Conflict", ""),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(_) => return empty(stream, "403 Forbidden", ""),
    }
    if let Some(expect) = request.header("expect") {
        if !expect.eq_ignore_ascii_case("100-continue") {
            return empty(stream, "417 Expectation Failed", "");
        }
        stream.write_all(b"HTTP/1.1 100 Continue\r\n\r\n")?;
    }

    let mut random = [0; 16];
    getrandom::fill(&mut random).map_err(|e| io::Error::other(e.to_string()))?;
    let temporary = format!("{STAGING_PREFIX}{:032x}", u128::from_be_bytes(random));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = match directory.open_with(&temporary, &options) {
        Ok(file) => file,
        Err(_) => return empty(stream, "403 Forbidden", ""),
    };
    let staged = StagedUpload {
        directory: &directory,
        name: temporary,
    };
    let write_result = write_upload(stream, &mut file, length, initial, deadline, stop);
    drop(file);
    let status = match write_result {
        Ok(()) => {
            check_transfer(deadline, stop)?;
            let result = directory.hard_link(&staged.name, &directory, name);
            drop(staged);
            return match result {
                Ok(()) => empty(stream, "201 Created", ""),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    empty(stream, "409 Conflict", "")
                }
                Err(_) => {
                    let status = "500 Internal Server Error";
                    let body = b"Could not publish the upload. The shared filesystem must support hard links for uploads.\n";
                    let headers = render_headers(
                        status,
                        Some(body.len() as u64),
                        "Content-Type: text/plain; charset=utf-8\r\n",
                    );
                    write_transfer(stream, headers.as_bytes(), deadline, stop)?;
                    write_transfer(stream, body, deadline, stop)?;
                    Ok(status)
                }
            };
        }
        Err(error) if error.kind() == io::ErrorKind::Interrupted => return Err(error),
        Err(error) => match error.kind() {
            io::ErrorKind::UnexpectedEof => "400 Bad Request",
            io::ErrorKind::TimedOut => "408 Request Timeout",
            _ => "500 Internal Server Error",
        },
    };
    drop(staged);
    empty(stream, status, "")
}

fn write_upload(
    stream: &mut TcpStream,
    file: &mut File,
    length: u64,
    initial: &[u8],
    deadline: Instant,
    stop: &AtomicBool,
) -> io::Result<()> {
    file.write_all(initial)?;
    let mut remaining = length - initial.len() as u64;
    let mut buffer = [0; 16 * 1024];
    while remaining > 0 {
        let limit = remaining.min(buffer.len() as u64) as usize;
        let count = read_until(stream, &mut buffer[..limit], deadline, stop)?;
        if count == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        file.write_all(&buffer[..count])?;
        remaining -= count as u64;
    }
    check_transfer(deadline, stop)?;
    file.sync_all()
}
