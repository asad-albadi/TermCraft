use harbor_workspace::{
    model::Session,
    ssh::{ConnectionConfig, ConnectionEvent, ConnectionHandle},
    x11::{XDisplaySource, XServerLease},
};
use russh::{
    ChannelId,
    keys::{PrivateKey, ssh_key::private::Ed25519Keypair},
    server::{self, Auth, Server as _},
};
use std::{
    net::{SocketAddr, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use x11rb::rust_connection::{DefaultStream, RustConnection};
use zeroize::Zeroizing;

const LIMIT: Duration = Duration::from_secs(5);

pub struct Request {
    handle: server::Handle,
    pub cookie: Zeroizing<Vec<u8>>,
}

#[derive(Clone)]
struct Handler {
    requests: mpsc::Sender<Request>,
    shell: Option<ChannelId>,
    displays: std::collections::BTreeMap<ChannelId, (String, Arc<tempfile::NamedTempFile>)>,
    processes: std::collections::BTreeMap<ChannelId, u32>,
    shell_input: Option<Arc<std::sync::Mutex<std::process::ChildStdin>>>,
    startup_frames: usize,
    pub startup_frames_sent: Arc<AtomicUsize>,
    pub logins: Arc<AtomicUsize>,
}

impl server::Server for Handler {
    type Handler = Self;

    fn new_client(&mut self, _: Option<SocketAddr>) -> Self {
        Self {
            requests: self.requests.clone(),
            shell: None,
            shell_input: None,
            logins: self.logins.clone(),
            startup_frames: self.startup_frames,
            startup_frames_sent: self.startup_frames_sent.clone(),
            displays: Default::default(),
            processes: Default::default(),
        }
    }
}

impl server::Handler for Handler {
    type Error = russh::Error;

    async fn auth_none(&mut self, _: &str) -> Result<Auth, Self::Error> {
        self.logins.fetch_add(1, Ordering::Relaxed);
        Ok(Auth::Accept)
    }

    async fn channel_open_session(
        &mut self,
        _: russh::Channel<server::Msg>,
        reply: server::ChannelOpenHandle,
        _: &mut server::Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _: &str,
        _: u32,
        _: u32,
        _: u32,
        _: u32,
        _: &[(russh::Pty, u32)],
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)
    }

    async fn x11_request(
        &mut self,
        channel: ChannelId,
        _: bool,
        protocol: &str,
        cookie: &str,
        _: u32,
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        assert_eq!(protocol, "MIT-MAGIC-COOKIE-1");
        assert_eq!(cookie.len(), 32);
        let cookie: Zeroizing<Vec<u8>> = Zeroizing::new(
            (0..16)
                .map(|i| u8::from_str_radix(&cookie[2 * i..2 * i + 2], 16).unwrap())
                .collect(),
        );
        // Actual remote child receives a private TCP X proxy, using this SSH login's cookie.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let number = listener.local_addr().unwrap().port() - 6000;
        let authority = Arc::new(tempfile::NamedTempFile::new().unwrap());
        let mut record = Vec::new();
        record.extend_from_slice(&65535u16.to_be_bytes());
        for field in [
            b"".as_slice(),
            number.to_string().as_bytes(),
            b"MIT-MAGIC-COOKIE-1",
            cookie.as_slice(),
        ] {
            record.extend_from_slice(&(field.len() as u16).to_be_bytes());
            record.extend_from_slice(field);
        }
        std::fs::write(authority.path(), record).unwrap();
        self.displays
            .insert(channel, (format!("127.0.0.1:{number}"), authority));
        let relay_handle = session.handle();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let handle = relay_handle.clone();
                tokio::spawn(async move {
                    if let Ok(channel) = handle.channel_open_x11("127.0.0.1", 12345).await {
                        let _ =
                            tokio::io::copy_bidirectional(&mut socket, &mut channel.into_stream())
                                .await;
                    }
                });
            }
        });
        self.requests
            .send(Request {
                handle: session.handle(),
                cookie,
            })
            .map_err(|_| russh::Error::Disconnect)?;
        session.channel_success(channel)
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        use std::io::{Read, Write};
        use std::os::unix::process::CommandExt;
        self.shell = Some(channel);
        let mut child = std::process::Command::new("sh")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .process_group(0)
            .spawn()
            .unwrap();
        self.processes.insert(channel, child.id());
        let mut input = child.stdin.take().unwrap();
        input
            .write_all(b"printf 'FIXTURE_SHELL_READY\\n'\n")
            .unwrap();
        self.shell_input = Some(Arc::new(std::sync::Mutex::new(input)));
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let handle = session.handle();
        let runtime = tokio::runtime::Handle::current();
        session.channel_success(channel)?;
        // Queue distinct SSH channel-data messages before any desktop reply. Transport
        // coalescing does not merge these SSH frames in russh's channel dispatcher.
        for index in 0..self.startup_frames {
            session.data(
                channel,
                format!("STARTUP_FRAME_{index:03}:{}\n", "x".repeat(1024)).into_bytes(),
            )?;
            self.startup_frames_sent.fetch_add(1, Ordering::SeqCst);
        }
        std::thread::spawn(move || {
            let mut readers = Vec::new();
            for mut stream in [Box::new(stdout) as Box<dyn Read + Send>, Box::new(stderr)] {
                let handle = handle.clone();
                let runtime = runtime.clone();
                readers.push(std::thread::spawn(move || {
                    let mut bytes = [0; 1024];
                    while let Ok(n) = stream.read(&mut bytes) {
                        if n == 0 {
                            break;
                        }
                        if runtime
                            .block_on(handle.data(channel, bytes[..n].to_vec()))
                            .is_err()
                        {
                            break;
                        }
                    }
                }));
            }
            let _ = child.wait();
            for reader in readers {
                let _ = reader.join();
            }
        });
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        command: &[u8],
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        use std::io::Read;
        use std::os::unix::process::CommandExt;
        let command = std::str::from_utf8(command).unwrap();
        if command.contains("X05_HOLD_EXEC_REPLY") {
            return Ok(());
        }
        let (display, authority) = self
            .displays
            .get(&channel)
            .expect("desktop exec must request X11")
            .clone();
        let mut child = std::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .env("DISPLAY", display)
            .env("XAUTHORITY", authority.path())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .process_group(0)
            .spawn()
            .unwrap();
        self.processes.insert(channel, child.id());
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let handle = session.handle();
        let runtime = tokio::runtime::Handle::current();
        session.channel_success(channel)?;
        std::thread::spawn(move || {
            let readers = [Box::new(stdout) as Box<dyn Read + Send>, Box::new(stderr)];
            let mut threads = Vec::new();
            for mut reader in readers {
                let handle = handle.clone();
                let runtime = runtime.clone();
                threads.push(std::thread::spawn(move || {
                    let mut bytes = [0; 1024];
                    while let Ok(n) = reader.read(&mut bytes) {
                        if n == 0 {
                            break;
                        }
                        if runtime
                            .block_on(handle.data(channel, bytes[..n].to_vec()))
                            .is_err()
                        {
                            break;
                        }
                    }
                }));
            }
            let status = child.wait().unwrap();
            for reader in threads {
                let _ = reader.join();
            }
            runtime.block_on(async {
                let _ = handle
                    .exit_status_request(channel, status.code().unwrap_or(128) as u32)
                    .await;
                let _ = handle.close(channel).await;
            });
            drop(authority);
        });
        Ok(())
    }
    async fn channel_close(
        &mut self,
        channel: ChannelId,
        _: &mut server::Session,
    ) -> Result<(), Self::Error> {
        if let Some(pid) = self.processes.remove(&channel) {
            unsafe {
                libc::kill(-(pid as i32), libc::SIGTERM);
            }
        }
        Ok(())
    }
    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        _session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        if self.shell == Some(channel) {
            use std::io::Write;
            self.shell_input
                .as_ref()
                .unwrap()
                .lock()
                .unwrap()
                .write_all(data)
                .unwrap();
        }
        Ok(())
    }
}

impl Drop for Handler {
    fn drop(&mut self) {
        for pid in self.processes.values() {
            unsafe {
                libc::kill(-(*pid as i32), libc::SIGTERM);
            }
        }
    }
}

pub struct Fixture {
    runtime: tokio::runtime::Runtime,
    endpoint: SocketAddr,
    known_hosts: tempfile::NamedTempFile,
    requests: mpsc::Receiver<Request>,
    startup_frames: usize,
    pub startup_frames_sent: Arc<AtomicUsize>,
    pub logins: Arc<AtomicUsize>,
}

impl Fixture {
    pub fn start() -> Self {
        Self::start_with_startup_frames(0)
    }

    pub fn start_with_startup_frames(startup_frames: usize) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let listener = runtime
            .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
            .unwrap();
        let endpoint = listener.local_addr().unwrap();
        let key =
            PrivateKey::new(Ed25519Keypair::from_seed(&[41; 32]).into(), "X11 fixture").unwrap();
        let known_hosts = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            known_hosts.path(),
            format!(
                "[127.0.0.1]:{} {}\n",
                endpoint.port(),
                key.public_key().to_openssh().unwrap()
            ),
        )
        .unwrap();
        let (sender, requests) = mpsc::channel();
        let logins = Arc::new(AtomicUsize::new(0));
        let startup_frames_sent = Arc::new(AtomicUsize::new(0));
        let mut handler = Handler {
            requests: sender,
            shell: None,
            shell_input: None,
            displays: Default::default(),
            processes: Default::default(),
            logins: logins.clone(),
            startup_frames,
            startup_frames_sent: startup_frames_sent.clone(),
        };
        runtime.spawn(async move {
            let _ = handler
                .run_on_socket(
                    Arc::new(server::Config {
                        keys: vec![key],
                        auth_rejection_time: Duration::ZERO,
                        ..Default::default()
                    }),
                    &listener,
                )
                .await;
        });
        Self {
            runtime,
            endpoint,
            known_hosts,
            requests,
            logins,
            startup_frames,
            startup_frames_sent,
        }
    }

    pub fn config(&self, id: u64) -> ConnectionConfig {
        ConnectionConfig {
            id,
            agent: None,
            defaults: Default::default(),
            clock: None,
            session: Session {
                host: "127.0.0.1".into(),
                port: self.endpoint.port(),
                user: "fixture".into(),
                x11: true,
                x11_source: XDisplaySource::Owned,
                ..Default::default()
            },
            app_known_hosts: self.known_hosts.path().into(),
            user_known_hosts: vec![],
        }
    }

    pub fn take_request(&self) -> Option<Request> {
        self.requests.try_recv().ok()
    }

    pub fn connect(&self, id: u64, owned: &XServerLease) -> (ConnectionHandle, Request) {
        let mut ssh =
            ConnectionHandle::connect_with_x11(self.config(id), owned.client_display()).unwrap();
        let deadline = Instant::now() + LIMIT;
        loop {
            for event in ssh.poll() {
                match event {
                    ConnectionEvent::Connected { .. } => {
                        return (ssh, self.requests.recv_timeout(LIMIT).unwrap());
                    }
                    ConnectionEvent::Challenge { .. } => panic!("unexpected login challenge"),
                    ConnectionEvent::Exited { message, .. } => panic!("SSH failed: {message}"),
                    _ => {}
                }
            }
            assert!(Instant::now() < deadline, "SSH did not get ready");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn x_client(
        &self,
        request: &Request,
        valid: bool,
    ) -> Result<RustConnection, x11rb::errors::ConnectError> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        client.set_read_timeout(Some(LIMIT)).unwrap();
        client.set_write_timeout(Some(LIMIT)).unwrap();
        let (relay, _) = listener.accept().unwrap();
        relay.set_nonblocking(true).unwrap();
        let handle = request.handle.clone();
        self.runtime.spawn(async move {
            let channel = tokio::time::timeout(LIMIT, handle.channel_open_x11("127.0.0.1", 12345))
                .await
                .unwrap()
                .unwrap();
            let mut stream = channel.into_stream();
            let mut relay = tokio::net::TcpStream::from_std(relay).unwrap();
            let _ = tokio::io::copy_bidirectional(&mut stream, &mut relay).await;
        });
        let mut cookie = request.cookie.to_vec();
        if !valid {
            cookie[0] ^= 0xff;
        }
        let (stream, _) = DefaultStream::from_tcp_stream(client).unwrap();
        RustConnection::connect_to_stream_with_auth_info(
            stream,
            0,
            b"MIT-MAGIC-COOKIE-1".to_vec(),
            cookie,
        )
    }
}
