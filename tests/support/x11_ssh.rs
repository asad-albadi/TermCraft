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
    pub logins: Arc<AtomicUsize>,
}

impl server::Server for Handler {
    type Handler = Self;

    fn new_client(&mut self, _: Option<SocketAddr>) -> Self {
        self.clone()
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
        let cookie = Zeroizing::new(
            (0..16)
                .map(|i| u8::from_str_radix(&cookie[2 * i..2 * i + 2], 16).unwrap())
                .collect(),
        );
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
        self.shell = Some(channel);
        session.channel_success(channel)?;
        session.data(channel, b"FIXTURE_SHELL_READY\r\n".to_vec())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        if self.shell == Some(channel) {
            session.data(channel, data.to_vec())?;
        }
        Ok(())
    }
}

pub struct Fixture {
    runtime: tokio::runtime::Runtime,
    endpoint: SocketAddr,
    known_hosts: tempfile::NamedTempFile,
    requests: mpsc::Receiver<Request>,
    pub logins: Arc<AtomicUsize>,
}

impl Fixture {
    pub fn start() -> Self {
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
        let mut handler = Handler {
            requests: sender,
            shell: None,
            logins: logins.clone(),
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
