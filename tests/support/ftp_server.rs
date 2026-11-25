use harbor_workspace::{
    credentials::Secret,
    protocols::ftp::{FtpDataMode, FtpOptions, FtpTlsMode},
};
#[path = "tls.rs"]
mod tls;
use std::{
    collections::{HashMap, HashSet},
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpSocket, TcpStream},
    task::{JoinHandle, JoinSet},
};

#[derive(Default)]
pub struct ServerOptions {
    pub tls: FtpTlsMode,
    pub reject_protection: bool,
    pub force_clear_data: bool,
    pub force_pasv: bool,
    pub force_port: bool,
    pub foreign_active_peer: bool,
    pub stall_transfer: bool,
    pub listing: Option<Vec<u8>>,
    pub listing_path: Option<String>,
    pub refuse_completion: bool,
    pub short_retr: Option<usize>,
    pub stall_retr: Option<String>,
    pub wait_for_data_close: bool,
    pub misleading_pwd_reply: bool,
    pub raw_pwd: bool,
}

#[derive(Default, Clone)]
pub struct Evidence {
    pub commands: Vec<String>,
    pub uploads: Vec<Vec<u8>>,
    pub data_connections: usize,
    pub active_connections: usize,
    pub foreign_active_connections: usize,
    pub encrypted_control_logins: usize,
    pub encrypted_data_connections: usize,
    pub login_attempts: usize,
    pub clear_protection_requests: usize,
    pub reciprocal_data_closes: usize,
    pub byte_path_commands: Vec<Vec<u8>>,
}

#[derive(Default)]
struct Files {
    contents: HashMap<String, Vec<u8>>,
    directories: HashSet<String>,
}

fn fixture_path(path: &str) -> String {
    if path == "." {
        "/".into()
    } else {
        format!("/{}", path.trim_start_matches("./").trim_matches('/'))
    }
}

impl Files {
    fn listing(&self, directory: &str) -> String {
        let prefix = format!("{}/", directory.trim_end_matches('/'));
        let name = |path: &str| {
            path.strip_prefix(&prefix)
                .filter(|name| !name.is_empty() && !name.contains('/'))
                .map(str::to_owned)
        };
        let mut result = String::new();
        for dir in &self.directories {
            if let Some(name) = name(dir) {
                result.push_str(&format!("type=dir; {name}\r\n"));
            }
        }
        for (path, content) in &self.contents {
            if let Some(name) = name(path) {
                result.push_str(&format!("type=file;size={}; {name}\r\n", content.len()));
            }
        }
        result
    }
}

pub struct Endpoint {
    address: SocketAddr,
    tls: FtpTlsMode,
    tls_fixture: Option<Arc<TlsFixture>>,
    evidence: Arc<Mutex<Evidence>>,
    files: Arc<Mutex<Files>>,
    task: JoinHandle<()>,
}

impl Endpoint {
    pub async fn start(options: ServerOptions) -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let tls = options.tls;
        let tls_fixture = (tls != FtpTlsMode::Plain).then(|| Arc::new(tls_fixture()));
        let worker_tls = tls_fixture.clone();
        let evidence = Arc::new(Mutex::new(Evidence::default()));
        let files = Arc::new(Mutex::new(Files::default()));
        let worker_evidence = evidence.clone();
        let worker_files = files.clone();
        let options = Arc::new(options);
        let task = tokio::spawn(async move {
            let mut clients = JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let Ok((control, _)) = accepted else { break };
                        clients.spawn(serve(control, options.clone(), worker_evidence.clone(), worker_files.clone(), worker_tls.clone()));
                    }
                    _ = clients.join_next(), if !clients.is_empty() => {}
                }
            }
        });
        Self {
            address,
            tls,
            tls_fixture,
            evidence,
            files,
            task,
        }
    }

    #[allow(dead_code)]
    pub fn write_file(&self, path: &str, bytes: &[u8]) {
        self.files
            .lock()
            .unwrap()
            .contents
            .insert(fixture_path(path), bytes.to_vec());
    }

    #[allow(dead_code)]
    pub fn replace_file_with_directory(&self, path: &str) {
        let mut files = self.files.lock().unwrap();
        let path = fixture_path(path);
        assert!(files.contents.remove(&path).is_some());
        files.directories.insert(path);
    }

    #[allow(dead_code)]
    pub fn has_directory(&self, path: &str) -> bool {
        self.files
            .lock()
            .unwrap()
            .directories
            .contains(&fixture_path(path))
    }

    #[allow(dead_code)]
    pub fn read_file(&self, path: &str) -> Option<Vec<u8>> {
        self.files
            .lock()
            .unwrap()
            .contents
            .get(&fixture_path(path))
            .cloned()
    }

    pub fn options(&self) -> FtpOptions {
        let tls = self.tls != FtpTlsMode::Plain;
        FtpOptions {
            host: if tls {
                "localhost".into()
            } else {
                self.address.ip().to_string()
            },
            port: self.address.port(),
            username: "test-user".into(),
            password: Some(Secret::new("test-password")),
            tls: self.tls,
            protect_data: tls,
            data_mode: FtpDataMode::Passive,
            certificate_authority: self
                .tls_fixture
                .as_ref()
                .map(|fixture| fixture.root.path().join("ca.pem")),
            timeout: Duration::from_secs(4),
        }
    }

    pub fn evidence(&self) -> Evidence {
        self.evidence.lock().unwrap().clone()
    }
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        self.task.abort();
    }
}

trait Channel: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Channel for T {}
type Stream = Box<dyn Channel>;

const FOREIGN_LOOPBACK: Ipv4Addr = Ipv4Addr::new(127, 0, 0, 2);

fn foreign_address() -> std::io::Result<Ipv4Addr> {
    if std::net::TcpListener::bind((FOREIGN_LOOPBACK, 0)).is_ok() {
        return Ok(FOREIGN_LOOPBACK);
    }
    #[cfg(unix)]
    {
        // Darwin only permits configured source addresses. Use another address
        // on this host while still connecting to the fixture's loopback listener.
        let mut interfaces = std::ptr::null_mut();
        if unsafe { libc::getifaddrs(&mut interfaces) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        let mut entry = interfaces;
        let mut address = None;
        while !entry.is_null() {
            let interface = unsafe { &*entry };
            if interface.ifa_flags & libc::IFF_UP as u32 != 0
                && !interface.ifa_addr.is_null()
                && unsafe { (*interface.ifa_addr).sa_family as i32 } == libc::AF_INET
            {
                let socket = unsafe { &*interface.ifa_addr.cast::<libc::sockaddr_in>() };
                let ip = Ipv4Addr::from(socket.sin_addr.s_addr.to_ne_bytes());
                if ip != Ipv4Addr::LOCALHOST && !ip.is_unspecified() {
                    address = Some(ip);
                    break;
                }
            }
            entry = interface.ifa_next;
        }
        unsafe { libc::freeifaddrs(interfaces) };
        if let Some(address) = address {
            return Ok(address);
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AddrNotAvailable,
        "active FTP fixture requires a second local IPv4 source address",
    ))
}

struct TlsFixture {
    root: tempfile::TempDir,
    acceptor: tokio_rustls::TlsAcceptor,
}

fn tls_fixture() -> TlsFixture {
    let root = tempfile::tempdir().expect("FTPS fixture directory");
    let acceptor = tokio_rustls::TlsAcceptor::from(tls::localhost_server(root.path()));
    TlsFixture { root, acceptor }
}

async fn reply(control: &mut BufReader<Stream>, line: &str) -> std::io::Result<()> {
    control.get_mut().write_all(line.as_bytes()).await
}

async fn serve(
    mut socket: TcpStream,
    options: Arc<ServerOptions>,
    evidence: Arc<Mutex<Evidence>>,
    files: Arc<Mutex<Files>>,
    tls_fixture: Option<Arc<TlsFixture>>,
) -> std::io::Result<()> {
    let acceptor = tls_fixture.as_ref().map(|fixture| &fixture.acceptor);
    let channel: Stream = match options.tls {
        FtpTlsMode::Plain => Box::new(socket),
        FtpTlsMode::Explicit => {
            // Read exactly one clear command before changing the wire protocol.
            socket
                .write_all(b"220-Welcome to the fixture\r\n220 Ready\r\n")
                .await?;
            let mut command = Vec::new();
            loop {
                let byte = socket.read_u8().await?;
                command.push(byte);
                if byte == b'\n' {
                    break;
                }
            }
            if command.starts_with(b"USER ") || command.starts_with(b"PASS ") {
                evidence.lock().unwrap().login_attempts += 1;
            }
            if command != b"AUTH TLS\r\n" {
                return Ok(());
            }
            socket.write_all(b"234 Ready for TLS\r\n").await?;
            Box::new(acceptor.unwrap().accept(socket).await?)
        }
        FtpTlsMode::Implicit => {
            let mut first_bytes = [0; 5];
            let count = socket.peek(&mut first_bytes).await?;
            if first_bytes[..count]
                .first()
                .is_some_and(|&byte| matches!(byte, b'U' | b'P'))
            {
                evidence.lock().unwrap().login_attempts += 1;
            }
            Box::new(acceptor.unwrap().accept(socket).await?)
        }
    };
    let mut control = BufReader::new(channel);
    if options.tls != FtpTlsMode::Explicit {
        reply(&mut control, "220 Ready\r\n").await?;
    }
    let mut protected = false;
    let mut passive = None;
    let mut active = None;
    let mut rename_from = String::new();
    loop {
        let mut wire_line = Vec::new();
        if control.read_until(b'\n', &mut wire_line).await? == 0 {
            return Ok(());
        }
        if options.raw_pwd && wire_line.starts_with(b"MKD ") {
            let accepted = wire_line == b"MKD /\xff\xff\r\n";
            evidence.lock().unwrap().byte_path_commands.push(wire_line);
            let response = if accepted {
                "257 Directory created\r\n"
            } else {
                "553 Invalid path\r\n"
            };
            reply(&mut control, response).await?;
            continue;
        }
        let line = std::str::from_utf8(&wire_line)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?
            .trim_end_matches(['\r', '\n']);
        let (command, argument) = line.split_once(' ').unwrap_or((line, ""));
        evidence.lock().unwrap().commands.push(command.to_owned());
        let path = fixture_path(argument);
        match command {
            "USER" => {
                evidence.lock().unwrap().login_attempts += 1;
                reply(&mut control, "331 Password required\r\n").await?;
            }
            "PASS" => {
                if options.tls != FtpTlsMode::Plain {
                    evidence.lock().unwrap().encrypted_control_logins += 1;
                }
                reply(&mut control, "230 Welcome\r\n").await?;
            }
            "PBSZ" if argument == "0" => reply(&mut control, "200 Buffer size\r\n").await?,
            "PROT" if argument == "P" && !options.reject_protection => {
                protected = true;
                reply(&mut control, "200 Private data\r\n").await?;
            }
            "PROT" if argument == "C" => {
                evidence.lock().unwrap().clear_protection_requests += 1;
                reply(&mut control, "200 Clear data\r\n").await?;
            }
            "EPSV" | "PASV" if command == "PASV" || !options.force_pasv => {
                let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
                let port = listener.local_addr()?.port();
                let response = if command == "EPSV" {
                    format!("229 Passive data (|||{port}|)\r\n")
                } else {
                    format!(
                        "227 Passive data (127,0,0,2,{},{})\r\n",
                        port / 256,
                        port % 256
                    )
                };
                passive = Some(listener);
                reply(&mut control, &response).await?;
            }
            "EPRT" if !options.force_port => {
                let fields = argument.split('|').collect::<Vec<_>>();
                active = Some(SocketAddr::new(
                    fields[2].parse().unwrap(),
                    fields[3].parse().unwrap(),
                ));
                reply(&mut control, "200 Active connection\r\n").await?;
            }
            "PORT" => {
                let fields = argument
                    .split(',')
                    .map(|part| part.parse::<u8>().unwrap())
                    .collect::<Vec<_>>();
                active = Some(SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::new(fields[0], fields[1], fields[2], fields[3])),
                    u16::from_be_bytes([fields[4], fields[5]]),
                ));
                reply(&mut control, "200 Active connection\r\n").await?;
            }
            "TYPE" => reply(&mut control, "200 Type set\r\n").await?,
            "RETR" | "STOR" | "MLSD" => {
                let download = if command == "RETR" {
                    let bytes = files
                        .lock()
                        .unwrap()
                        .contents
                        .get(&path)
                        .cloned()
                        .or_else(|| (path == "/seed").then(|| b"seed bytes".to_vec()));
                    let Some(bytes) = bytes else {
                        passive.take();
                        active.take();
                        reply(&mut control, "550 Unavailable\r\n").await?;
                        continue;
                    };
                    bytes
                } else {
                    Vec::new()
                };
                reply(&mut control, "150 Opening data channel\r\n").await?;
                let mut foreign = None;
                let socket = if let Some(listener) = passive.take() {
                    listener.accept().await?.0
                } else {
                    let address = active.take().unwrap();
                    if options.foreign_active_peer {
                        let wrong = TcpSocket::new_v4()?;
                        wrong.bind((foreign_address()?, 0).into())?;
                        let mut wrong = wrong.connect(address).await?;
                        wrong.write_all(b"foreign bytes").await?;
                        evidence.lock().unwrap().foreign_active_connections += 1;
                        foreign = Some(wrong);
                    }
                    evidence.lock().unwrap().active_connections += 1;
                    TcpStream::connect(address).await?
                };
                evidence.lock().unwrap().data_connections += 1;
                let mut data: Stream = if protected && !options.force_clear_data {
                    let tls = acceptor.unwrap().accept(socket).await?;
                    evidence.lock().unwrap().encrypted_data_connections += 1;
                    Box::new(tls)
                } else {
                    Box::new(socket)
                };
                if options.stall_transfer {
                    let _ = data.read_u8().await;
                    return Ok(());
                }
                match command {
                    "STOR" => {
                        let mut bytes = Vec::new();
                        data.read_to_end(&mut bytes).await?;
                        evidence.lock().unwrap().uploads.push(bytes.clone());
                        files.lock().unwrap().contents.insert(path, bytes);
                    }
                    "RETR" => {
                        if options.stall_retr.as_ref() == Some(&path) {
                            data.write_all(&download[..download.len().min(4096)])
                                .await?;
                            let _ = data.read_u8().await;
                            return Ok(());
                        }
                        let length = options
                            .short_retr
                            .unwrap_or(download.len())
                            .min(download.len());
                        data.write_all(&download[..length]).await?;
                    }
                    "MLSD" => {
                        if let Some(listing) = &options.listing
                            && options
                                .listing_path
                                .as_ref()
                                .is_none_or(|selected| selected == &path)
                        {
                            data.write_all(listing).await?;
                        }
                        let listing = files.lock().unwrap().listing(&path);
                        data.write_all(listing.as_bytes()).await?;
                    }
                    _ => unreachable!(),
                }
                data.shutdown().await?;
                if options.wait_for_data_close && command != "STOR" {
                    let received = data.read(&mut [0u8; 1]).await?;
                    if received != 0 {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "unexpected data from download client",
                        ));
                    }
                    evidence.lock().unwrap().reciprocal_data_closes += 1;
                }
                drop(foreign);
                let completion = if options.refuse_completion {
                    "426 Transfer aborted\r\n"
                } else {
                    "226 Done\r\n"
                };
                reply(&mut control, completion).await?;
            }
            "SIZE" => {
                let size = files.lock().unwrap().contents.get(&path).map(Vec::len);
                if let Some(size) = size {
                    reply(&mut control, &format!("213 {size}\r\n")).await?;
                } else {
                    reply(&mut control, "550 Unavailable\r\n").await?;
                }
            }
            "RNFR" => {
                rename_from = path;
                reply(&mut control, "350 Send destination\r\n").await?;
            }
            "RNTO" => {
                {
                    let mut files = files.lock().unwrap();
                    if let Some(bytes) = files.contents.remove(&rename_from) {
                        files.contents.insert(path, bytes);
                    } else if files.directories.remove(&rename_from) {
                        let prefix = format!("{rename_from}/");
                        let owned = files
                            .directories
                            .iter()
                            .filter(|name| name.starts_with(&prefix))
                            .cloned()
                            .collect::<Vec<_>>();
                        for name in owned {
                            files.directories.remove(&name);
                            files
                                .directories
                                .insert(format!("{path}{}", &name[rename_from.len()..]));
                        }
                        let owned = files
                            .contents
                            .keys()
                            .filter(|name| name.starts_with(&prefix))
                            .cloned()
                            .collect::<Vec<_>>();
                        for name in owned {
                            let bytes = files.contents.remove(&name).unwrap();
                            files
                                .contents
                                .insert(format!("{path}{}", &name[rename_from.len()..]), bytes);
                        }
                        files.directories.insert(path);
                    }
                }
                reply(&mut control, "250 Renamed\r\n").await?;
            }
            "MKD" if options.misleading_pwd_reply => {
                reply(&mut control, "550 Refusing the directory mutation\r\n").await?;
            }
            "MKD" => {
                files.lock().unwrap().directories.insert(path);
                reply(&mut control, "257 Directory created\r\n").await?;
            }
            "DELE" | "RMD" => {
                {
                    let mut files = files.lock().unwrap();
                    if command == "RMD" {
                        files.directories.remove(&path);
                    } else {
                        files.contents.remove(&path);
                    }
                }
                reply(&mut control, "250 Removed\r\n").await?;
            }
            "PWD" if options.misleading_pwd_reply => {
                reply(
                    &mut control,
                    "150 PWD reply follows\r\n257 \"/\" current directory\r\n",
                )
                .await?;
            }
            "PWD" if options.raw_pwd => {
                control
                    .get_mut()
                    .write_all(b"257 \"/\xff\xff\" current directory\r\n")
                    .await?;
            }
            "PWD" => reply(&mut control, "257 \"/\" current directory\r\n").await?,
            _ => reply(&mut control, "502 Command unavailable\r\n").await?,
        }
    }
}
