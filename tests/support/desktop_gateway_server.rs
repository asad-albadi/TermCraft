use harbor_workspace::{
    credentials::{CredentialBinding, CredentialRole},
    model::{AgentMode, Session},
    ssh::{
        ChallengeAnswer, ConnectionConfig, ConnectionEvent, TrustDecision,
        desktop_gateway::{Endpoint, GatewayLease},
    },
};
use russh::{
    Channel,
    keys::{
        PrivateKey,
        ssh_key::{
            HashAlg,
            private::{Ed25519PrivateKey, KeypairData},
        },
    },
    server::{self, Auth, Server},
};
use std::{
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering::SeqCst},
    },
    time::{Duration, Instant},
};
use tempfile::TempDir;
use zeroize::Zeroizing;

pub const TARGET: &str = "private-desktop.invalid";
pub const USER: &str = "gateway-user";
pub const WAIT: Duration = Duration::from_secs(5);

#[derive(Default)]
pub struct Counts {
    pub authenticated: AtomicUsize,
    pub attempted: AtomicUsize,
    pub active: AtomicUsize,
}

#[derive(Clone)]
struct ForwardingServer {
    target_host: &'static str,
    target_port: u16,
    counts: Arc<Counts>,
}

impl Server for ForwardingServer {
    type Handler = Self;

    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
        self.clone()
    }
}

struct ForwardCount(Arc<Counts>);

impl Drop for ForwardCount {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, SeqCst);
    }
}

impl server::Handler for ForwardingServer {
    type Error = russh::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        if user == USER && password == "fixture-gateway-secret" {
            self.counts.authenticated.fetch_add(1, SeqCst);
            Ok(Auth::Accept)
        } else {
            Ok(Auth::reject())
        }
    }

    async fn channel_open_direct_tcpip(
        &mut self,
        channel: Channel<server::Msg>,
        host: &str,
        port: u32,
        _: &str,
        _: u32,
        reply: server::ChannelOpenHandle,
        _: &mut server::Session,
    ) -> Result<(), Self::Error> {
        self.counts.attempted.fetch_add(1, SeqCst);
        if host != self.target_host || port != u32::from(self.target_port) {
            return Ok(());
        }
        let Ok(target) = tokio::net::TcpStream::connect(("127.0.0.1", self.target_port)).await
        else {
            return Ok(());
        };
        self.counts.active.fetch_add(1, SeqCst);
        let count = ForwardCount(self.counts.clone());
        reply.accept().await;
        tokio::spawn(async move {
            let _count = count;
            let (mut tunnel_read, mut tunnel_write) = tokio::io::split(channel.into_stream());
            let (mut target_read, mut target_write) = target.into_split();
            tokio::select! {
                _ = tokio::io::copy(&mut tunnel_read, &mut target_write) => {}
                _ = tokio::io::copy(&mut target_read, &mut tunnel_write) => {}
            }
        });
        Ok(())
    }
}

pub struct Fixture {
    root: TempDir,
    pub counts: Arc<Counts>,
    pub gateway_port: u16,
    pub target_port: u16,
    pub host_key: String,
    pub target: TcpListener,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Fixture {
    pub fn start() -> Self {
        Self::with_target(TcpListener::bind(("127.0.0.1", 0)).unwrap())
    }

    pub fn with_target(target: TcpListener) -> Self {
        let target_port = target.local_addr().unwrap().port();
        Self::with_route(target, TARGET, target_port)
    }

    pub fn with_jump_to(next: &Fixture) -> Self {
        Self::with_external_route("127.0.0.1", next.gateway_port)
    }

    pub fn with_external_target(port: u16) -> Self {
        Self::with_external_route(TARGET, port)
    }

    fn with_external_route(host: &'static str, port: u16) -> Self {
        let unused_target = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        Self::with_route(unused_target, host, port)
    }

    fn with_route(target: TcpListener, route_host: &'static str, route_port: u16) -> Self {
        let root = tempfile::tempdir().unwrap();
        let key = fixture_key();
        let host_key = key.public_key().fingerprint(HashAlg::Sha256).to_string();
        let gateway = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        gateway.set_nonblocking(true).unwrap();
        let gateway_port = gateway.local_addr().unwrap().port();
        target.set_nonblocking(true).unwrap();
        let target_port = target.local_addr().unwrap().port();
        let mut handler = ForwardingServer {
            target_host: route_host,
            target_port: route_port,
            counts: Arc::new(Counts::default()),
        };
        let counts = handler.counts.clone();
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let thread = std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async move {
                    let gateway = tokio::net::TcpListener::from_std(gateway).unwrap();
                    let config = Arc::new(server::Config {
                        keys: vec![key],
                        auth_rejection_time: Duration::ZERO,
                        auth_rejection_time_initial: Some(Duration::ZERO),
                        ..Default::default()
                    });
                    tokio::select! {
                        _ = stopped => {}
                        result = handler.run_on_socket(config, &gateway) => result.unwrap(),
                    }
                });
        });
        Self {
            root,
            counts,
            gateway_port,
            target_port,
            host_key,
            target,
            stop: Some(stop),
            thread: Some(thread),
        }
    }

    pub fn config(&self, id: u64) -> ConnectionConfig {
        ConnectionConfig {
            id,
            session: Session {
                host: "127.0.0.1".into(),
                port: self.gateway_port,
                user: USER.into(),
                agent_mode: AgentMode::Disabled,
                ..Default::default()
            },
            agent: None,
            defaults: Default::default(),
            clock: None,
            app_known_hosts: self.root.path().join("known_hosts"),
            user_known_hosts: vec![],
        }
    }

    pub fn endpoint(&self) -> Endpoint {
        Endpoint {
            host: TARGET.into(),
            port: self.target_port,
        }
    }

    pub fn accept_target(&self) -> TcpStream {
        let deadline = Instant::now() + WAIT;
        loop {
            match self.target.accept() {
                Ok((stream, _)) => {
                    stream.set_read_timeout(Some(WAIT)).unwrap();
                    stream.set_write_timeout(Some(WAIT)).unwrap();
                    return stream;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => pause(deadline),
                Err(error) => panic!("{error}"),
            }
        }
    }
}

pub fn fixture_key() -> PrivateKey {
    let mut seed = [0; 32];
    getrandom::fill(&mut seed).unwrap();
    let private = Ed25519PrivateKey::from_bytes(&seed);
    PrivateKey::new(KeypairData::Ed25519(private.into()), "gateway fixture").unwrap()
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.stop.take().unwrap().send(());
        self.thread.take().unwrap().join().unwrap();
    }
}

pub fn pause(deadline: Instant) {
    assert!(Instant::now() < deadline, "gateway test timed out");
    std::thread::sleep(Duration::from_millis(5));
}

pub fn authenticate(
    fixture: &Fixture,
    lease: &mut GatewayLease,
    trust: TrustDecision,
) -> Result<(), String> {
    authenticate_chain(&[fixture], lease, trust)
}

pub fn authenticate_chain(
    fixtures: &[&Fixture],
    lease: &mut GatewayLease,
    trust: TrustDecision,
) -> Result<(), String> {
    let deadline = Instant::now() + WAIT;
    loop {
        for event in lease.poll() {
            match event {
                ConnectionEvent::Challenge {
                    id,
                    host_key: true,
                    title,
                    ..
                } => {
                    let fixture = fixtures
                        .iter()
                        .find(|fixture| {
                            title.contains(&format!(
                                "Gateway SSH · Verify 127.0.0.1:{}\n",
                                fixture.gateway_port
                            ))
                        })
                        .expect("unknown gateway in trust challenge");
                    assert!(title.contains(&fixture.host_key));
                    assert_eq!(fixture.counts.authenticated.load(SeqCst), 0);
                    lease
                        .answer(ChallengeAnswer {
                            id,
                            values: vec![],
                            trust: Some(trust),
                        })
                        .unwrap();
                }
                ConnectionEvent::Challenge {
                    id,
                    host_key: false,
                    credential: Some(credential),
                    ..
                } => {
                    let CredentialBinding::Ssh {
                        role,
                        host,
                        port,
                        user,
                        host_key,
                        ..
                    } = credential.binding
                    else {
                        panic!("Expected SSH credential identity");
                    };
                    assert_eq!(role, CredentialRole::Gateway);
                    assert_eq!(host, "127.0.0.1");
                    let fixture = fixtures
                        .iter()
                        .find(|fixture| fixture.gateway_port == port)
                        .expect("unknown gateway in credential challenge");
                    assert_eq!(user, USER);
                    assert_eq!(host_key, fixture.host_key);
                    lease
                        .answer(ChallengeAnswer {
                            id,
                            values: vec![Zeroizing::new("fixture-gateway-secret".into())],
                            trust: None,
                        })
                        .unwrap();
                }
                ConnectionEvent::Connected { .. } => return Ok(()),
                ConnectionEvent::Exited { message, .. } => return Err(message),
                ConnectionEvent::Challenge { .. } => panic!("Unbound gateway credential challenge"),
                _ => {}
            }
        }
        pause(deadline);
    }
}
