//! Apple Silicon XDMCP. The pinned XQuartz child remains Unix-only. This owner
//! reserves an interface-specific listener and checks every TCP client's X11
//! setup before translating the manager cookie to the private server cookie.
mod wire;

use super::{
    XServerLease, native_macos, native_transport,
    xdmcp::{ClipboardPolicy, DiscoveryRequest, QueryMode, XdmcpConfig, check_cancelled},
};
use std::{
    fs::{self, Permissions},
    io::{self, Read},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use subtle::ConstantTimeEq;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket, UnixStream},
    sync::{mpsc as async_mpsc, oneshot},
    task::JoinSet,
    time::{self, Instant},
};
use zeroize::Zeroizing;

const PHASE: Duration = Duration::from_secs(8);
const MAX_CONNECTIONS: usize = 24;
const MAX_SESSIONS: usize = 8;
static SESSIONS: AtomicUsize = AtomicUsize::new(0);

struct Permit;
impl Permit {
    fn acquire() -> Result<Self, String> {
        SESSIONS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < MAX_SESSIONS).then_some(n + 1)
            })
            .map_err(|_| "Too many owned macOS XDMCP sessions")?;
        Ok(Self)
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        SESSIONS.fetch_sub(1, Ordering::AcqRel);
    }
}

pub(super) fn preflight(config: &XdmcpConfig) -> Result<SocketAddrV4, String> {
    let selected = config
        .selected_endpoint
        .ok_or("Discover and select an XDMCP endpoint first")?;
    DiscoveryRequest::direct(selected).validate()?;
    let SocketAddr::V4(endpoint) = selected else {
        return Err("Native macOS XDMCP requires IPv4".into());
    };
    if !config.allow_network_listener {
        return Err(
            "Explicitly allow unencrypted XDMCP and its inbound X11 TCP listener for this session"
                .into(),
        );
    }
    if config.mode == QueryMode::Broadcast {
        return Err("Broadcast is discovery-only. Choose Direct or Indirect to connect".into());
    }
    if config.display.is_some_and(|n| !(1..=59535).contains(&n)) {
        return Err("Choose display 1–59535".into());
    }
    if config.fullscreen
        || config.monitor.is_some()
        || !config.keyboard.is_empty()
        || config.clipboard != ClipboardPolicy::Disabled
        || config.clipboard_display.is_some()
        || config.clipboard_authority.is_some()
    {
        return Err("Native macOS XDMCP uses windowed XQuartz with host geometry/keyboard and no host clipboard bridge".into());
    }
    if config
        .advertise_address
        .is_some_and(|ip| ip.is_unspecified() || ip.is_multicast() || ip.is_broadcast())
    {
        return Err("Choose a unicast local IPv4 address to bind and advertise".into());
    }
    Ok(endpoint)
}

struct Reservation {
    udp: std::net::UdpSocket,
    listener: std::net::TcpListener,
    local: Ipv4Addr,
    display: u16,
}

impl Reservation {
    fn bind(endpoint: SocketAddrV4, config: &XdmcpConfig) -> Result<Self, String> {
        let local = match config.advertise_address {
            Some(ip) => ip,
            None => {
                let probe = std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
                    .and_then(|socket| {
                        // A UDP connect selects the route without sending a packet.
                        socket.connect(endpoint)?;
                        socket.local_addr()
                    })
                    .map_err(|e| format!("Cannot determine the local XDMCP address: {e}"))?;
                let SocketAddr::V4(probe) = probe else {
                    unreachable!()
                };
                *probe.ip()
            }
        };
        if local.is_unspecified() || local.is_loopback() != endpoint.ip().is_loopback() {
            return Err(
                "The XDMCP endpoint and local IPv4 must have matching loopback scope".into(),
            );
        }
        let udp = std::net::UdpSocket::bind((local, 0))
            .map_err(|e| format!("Could not bind XDMCP UDP socket on {local}: {e}"))?;
        udp.set_nonblocking(true).map_err(|e| e.to_string())?;
        let displays = match config.display {
            Some(n) => n..=n,
            None => 100..=199,
        };
        for display in displays {
            let port = 6000 + display;
            match std::net::TcpListener::bind((local, port)) {
                Ok(listener) => {
                    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
                    return Ok(Self {
                        udp,
                        listener,
                        local,
                        display,
                    });
                }
                Err(e) if config.display.is_none() && e.kind() == io::ErrorKind::AddrInUse => {}
                Err(e) => {
                    return Err(format!(
                        "Could not bind owned XDMCP listener {local}:{port}: {e}"
                    ));
                }
            }
        }
        Err(format!(
            "Could not bind a free XDMCP listener on {local} (displays 100–199)"
        ))
    }
}

pub struct Session {
    worker: Worker,
    authority: PathBuf,
    display: String,
    pid: Arc<AtomicU32>,
    pub(super) ready_message: String,
}

impl Session {
    pub fn display(&self) -> &str {
        &self.display
    }
    pub fn authority_path(&self) -> &Path {
        &self.authority
    }
    pub fn process_id(&self) -> Option<u32> {
        match self.pid.load(Ordering::Acquire) {
            0 => None,
            pid => Some(pid),
        }
    }
    pub fn is_running(&mut self) -> Result<bool, String> {
        if self
            .worker
            .thread
            .as_ref()
            .is_some_and(|t| !t.is_finished())
        {
            return Ok(true);
        }
        self.worker.join().map(|()| false)
    }
    pub fn stop_owned(mut self) -> Result<(), String> {
        self.worker.stop();
        self.worker.join()
    }
}

struct Worker {
    cancelled: Arc<AtomicBool>,
    shutdown: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<Result<(), String>>>,
}
impl Worker {
    fn stop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        if let Some(sender) = self.shutdown.take() {
            let _ = sender.send(());
        }
    }
    fn join(&mut self) -> Result<(), String> {
        match self.thread.take() {
            Some(thread) => thread.join().map_err(|_| "Native XDMCP owner panicked")?,
            None => Ok(()),
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.stop();
        let _ = self.join();
    }
}

pub(super) fn start(
    root: &Path,
    config: XdmcpConfig,
    _: u32,
    _: u32,
    cancelled: &AtomicBool,
) -> Result<Session, String> {
    check_cancelled(cancelled)?;
    let endpoint = preflight(&config)?;
    let permit = Permit::acquire()?;
    let reservation = Reservation::bind(endpoint, &config)?;
    let display = format!("{}:{}", reservation.local, reservation.display);
    let runtime = tempfile::Builder::new()
        .prefix("termcraft-xdmcp-")
        .permissions(Permissions::from_mode(0o700))
        .tempdir_in("/private/tmp")
        .map_err(|e| e.to_string())?;
    let authority = runtime.path().join("Xauthority");
    let root = root.to_owned();
    let pid = Arc::new(AtomicU32::new(0));
    let (ready, observed) = mpsc::sync_channel(1);
    let (shutdown, stopped) = oneshot::channel();
    let flag = Arc::new(AtomicBool::new(false));
    let session_pid = pid.clone();
    let stop_flag = flag.clone();
    let authority_path = authority.clone();
    let indirect = config.mode == QueryMode::Indirect;
    let thread = thread::Builder::new()
        .name("termcraft-macos-xdmcp".into())
        .spawn(move || {
            let _permit = permit;
            let _runtime = runtime;
            let result = (|| {
                let reactor = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| e.to_string())?;
                reactor.block_on(async {
                    let owner = Owner {
                        root,
                        authority: authority_path,
                        pid: session_pid.clone(),
                        udp: UdpSocket::from_std(reservation.udp).map_err(|e| e.to_string())?,
                        listener: TcpListener::from_std(reservation.listener)
                            .map_err(|e| e.to_string())?,
                        local: reservation.local,
                        display: reservation.display,
                        endpoint,
                        indirect,
                    };
                    tokio::select! {
                        biased;
                        _ = stopped => Ok(()),
                        result = owner.run(&ready, &stop_flag) => result,
                    }
                })
            })();
            session_pid.store(0, Ordering::Release);
            if let Err(error) = &result {
                let _ = ready.try_send(Err(error.clone()));
            }
            result
        })
        .map_err(|e| format!("Cannot start native XDMCP owner: {e}"))?;
    let mut session = Session {
        worker: Worker {
            cancelled: flag,
            shutdown: Some(shutdown),
            thread: Some(thread),
        },
        authority,
        display,
        pid,
        ready_message: String::new(),
    };
    loop {
        check_cancelled(cancelled)?;
        match observed.recv_timeout(Duration::from_millis(25)) {
            Ok(result) => {
                result?;
                check_cancelled(cancelled)?;
                session.ready_message =
                    format!("Authenticated XDMCP display {} is ready", session.display);
                return Ok(session);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("Native XDMCP owner exited during startup".into());
            }
        }
    }
}

struct Owner {
    root: PathBuf,
    authority: PathBuf,
    pid: Arc<AtomicU32>,
    udp: UdpSocket,
    listener: TcpListener,
    local: Ipv4Addr,
    endpoint: SocketAddrV4,
    display: u16,
    indirect: bool,
}

const RETRIES: [Duration; 3] = [
    Duration::from_millis(500),
    Duration::from_millis(1000),
    Duration::from_millis(2000),
];

impl Owner {
    async fn run(
        &self,
        ready: &mpsc::SyncSender<Result<(), String>>,
        flag: &AtomicBool,
    ) -> Result<(), String> {
        let image = native_macos::snapshot(&self.root, flag)?;
        let mut prior = None;
        for epoch in 0..=usize::from(self.indirect) {
            check_cancelled(flag)?;
            let mut lease = native_macos::launch_verified(Arc::clone(&image), flag)?;
            self.pid
                .store(lease.process_id().unwrap_or(0), Ordering::Release);
            let mut auth = Zeroizing::new(Vec::new());
            fs::File::open(lease.authority_path())
                .and_then(|file| file.take(1025).read_to_end(&mut auth))
                .map_err(|e| e.to_string())?;
            if auth.len() > 1024 {
                return Err("Invalid private XQuartz authority".into());
            }
            let backend_cookie = crate::ssh::x11::authority_cookie(&auth, 0, b"", true)?;
            drop(auth);
            let deadline = Instant::now() + PHASE;
            let (peer, accepted) = self.negotiate(deadline, prior).await?;
            self.serve(
                peer,
                &accepted,
                deadline,
                &backend_cookie,
                &mut lease,
                (epoch == 0).then_some(ready),
            )
            .await?;
            // An indirect chooser ends its initial X connection after selection.
            // Drop every client and restart only this private display before
            // requerying on the same UDP socket (the chooser's return address).
            lease.stop_owned()?;
            self.pid.store(0, Ordering::Release);
            fs::remove_file(&self.authority).map_err(|e| e.to_string())?;
            if !self.indirect || peer != SocketAddr::V4(self.endpoint) || epoch != 0 {
                return Ok(());
            }
            prior = Some((peer, accepted.id));
        }
        Ok(())
    }

    fn allows_willing(&self, from: SocketAddr) -> bool {
        if from == SocketAddr::V4(self.endpoint) {
            return true;
        }
        let SocketAddr::V4(from) = from else {
            return false;
        };
        // Indirect explicitly permits a manager to forward to another host.
        self.indirect
            && (from.port() == 177
                || from.port() == self.endpoint.port()
                || from.ip() == self.endpoint.ip())
            && from.ip().is_loopback() == self.local.is_loopback()
            && !from.ip().is_unspecified()
            && !from.ip().is_broadcast()
            && !from.ip().is_multicast()
    }

    async fn negotiate(
        &self,
        deadline: Instant,
        prior: Option<(SocketAddr, u32)>,
    ) -> Result<(SocketAddr, wire::Accepted), String> {
        let peer = self
            .exchange(
                &wire::query(self.indirect),
                self.endpoint.into(),
                deadline,
                "WILLING",
                |op, body, from| {
                    if op == wire::WILLING
                        && self.allows_willing(from)
                        && wire::willing(body).is_some()
                    {
                        return Ok(Some(from));
                    }
                    if from == self.endpoint.into()
                        && op == wire::UNWILLING
                        && wire::fields(body, 2).is_some()
                    {
                        return Err("The XDMCP manager is unwilling to manage this display".into());
                    }
                    Ok(None)
                },
            )
            .await?;
        let accepted = self
            .exchange(
                &wire::request(self.display, self.local),
                peer,
                deadline,
                "ACCEPT",
                |op, body, from| {
                    if from != peer {
                        return Ok(None);
                    }
                    if op == wire::DECLINE && wire::declined(body).is_some() {
                        return Err(
                            "The XDMCP manager declined MIT-MAGIC-COOKIE-1 authorization".into(),
                        );
                    }
                    Ok((op == wire::ACCEPT)
                        .then(|| wire::accept(body))
                        .flatten()
                        .filter(|a| prior != Some((from, a.id))))
                },
            )
            .await?;
        Ok((peer, accepted))
    }

    async fn exchange<T>(
        &self,
        packet: &[u8],
        peer: SocketAddr,
        deadline: Instant,
        expected: &str,
        mut receive: impl FnMut(u16, &[u8], SocketAddr) -> Result<Option<T>, String>,
    ) -> Result<T, String> {
        let mut bytes = Zeroizing::new([0u8; wire::MAX_PACKET + 1]);
        for wait in RETRIES.into_iter().chain([PHASE]) {
            let until = deadline.min(Instant::now() + wait);
            time::timeout_at(until, self.udp.send_to(packet, peer))
                .await
                .map_err(|_| format!("Timed out sending XDMCP packet for {expected}"))?
                .map_err(|e| format!("Could not send XDMCP packet: {e}"))?;
            while Instant::now() < until {
                match time::timeout_at(until, self.udp.recv_from(bytes.as_mut())).await {
                    Ok(Ok((n, from))) => {
                        if let Some((op, body)) = wire::decode(&bytes[..n])
                            && let Some(value) = receive(op, body, from)?
                        {
                            return Ok(value);
                        }
                    }
                    Ok(Err(e)) => return Err(format!("Could not receive XDMCP packet: {e}")),
                    Err(_) => break,
                }
            }
            if Instant::now() >= deadline {
                break;
            }
        }
        Err(format!("Timed out waiting for XDMCP {expected}"))
    }

    async fn serve(
        &self,
        peer: SocketAddr,
        accepted: &wire::Accepted,
        deadline: Instant,
        backend_cookie: &[u8; 16],
        lease: &mut XServerLease,
        ready: Option<&mpsc::SyncSender<Result<(), String>>>,
    ) -> Result<(), String> {
        let mut clients = JoinSet::new();
        let (admit, mut admitted) = async_mpsc::channel(MAX_CONNECTIONS);
        let mut initial = None;
        let mut next_id = 0usize;
        let mut manage_tries = 0;
        let mut next_manage = Instant::now();
        let mut next_alive = next_manage;
        let mut last_alive = next_manage;
        let mut tick = time::interval(Duration::from_millis(50));
        tick.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
        let manage = wire::manage(self.display, accepted.id);
        let keepalive = wire::keepalive(self.display, accepted.id);
        let mut bytes = Zeroizing::new([0u8; wire::MAX_PACKET + 1]);
        loop {
            tokio::select! {
                biased;
                _ = tick.tick() => {
                    let now = Instant::now();
                    if !lease.is_running()? {
                        return Err("Private XQuartz exited during XDMCP".into());
                    }
                    if initial.is_none() {
                        if now >= deadline {
                            return Err("Timed out waiting for a cookie-authorized XDMCP client after MANAGE".into());
                        }
                        if now >= next_manage && manage_tries <= RETRIES.len() {
                            self.udp.try_send_to(&manage, peer)
                                .or_else(ignore_would_block).map_err(|e| e.to_string())?;
                            next_manage = now + RETRIES.get(manage_tries).copied().unwrap_or(PHASE);
                            manage_tries += 1;
                        }
                    } else if now >= next_alive {
                        if now.duration_since(last_alive) >= Duration::from_secs(36) {
                            return Err("XDMCP manager stopped answering KEEPALIVE".into());
                        }
                        self.udp.try_send_to(&keepalive, peer)
                            .or_else(ignore_would_block).map_err(|e| e.to_string())?;
                        next_alive = now + Duration::from_secs(12);
                    }
                }
                Some(id) = admitted.recv() => if initial.is_none() {
                    initial = Some(id);
                    native_transport::write_authority(&self.authority, self.display, &accepted.cookie)?;
                    last_alive = Instant::now();
                    next_alive = last_alive + Duration::from_secs(12);
                    if let Some(ready) = ready {
                        ready.try_send(Ok(())).map_err(|_| "XDMCP startup receiver closed")?;
                    }
                },
                Some(completed) = clients.join_next(), if !clients.is_empty() => {
                    let id = completed.map_err(|_| "XDMCP client relay panicked")?;
                    if initial == Some(id) {
                        clients.shutdown().await;
                        return Ok(());
                    }
                },
                incoming = self.udp.recv_from(bytes.as_mut()) => {
                    let (n, from) = incoming.map_err(|e| e.to_string())?;
                    if from == peer && let Some((op, body)) = wire::decode(&bytes[..n]) {
                        if initial.is_none() && wire::manage_failed(op, body, accepted.id) {
                            return Err("XDMCP manager refused or failed MANAGE".into());
                        }
                        if initial.is_some() && op == wire::ALIVE {
                            match wire::alive(body, accepted.id) {
                                Some(true) => last_alive = Instant::now(),
                                Some(false) => return Err("XDMCP manager ended the session".into()),
                                None => {},
                            }
                        }
                    }
                },
                // Before authorization, the listener is already reserved. Never
                // forward a connection until MANAGE and a cookie are in effect.
                incoming = self.listener.accept(), if manage_tries != 0 => {
                    let (mut client, from) = incoming.map_err(|e| e.to_string())?;
                    if from.ip() != peer.ip() || clients.len() >= MAX_CONNECTIONS { continue; }
                    let path = PathBuf::from(lease.display());
                    let cookie = accepted.cookie.clone();
                    let backend_cookie = Zeroizing::new(*backend_cookie);
                    let admit = admit.clone();
                    let id = next_id;
                    next_id = next_id.wrapping_add(1);
                    clients.spawn(async move {
                        if let Ok(Ok(mut backend)) = time::timeout(Duration::from_secs(2),
                            authorize(&mut client, &path, &cookie, &backend_cookie)).await
                        {
                            let _ = admit.send(id).await;
                            // Stop on either EOF (not only when both halves close).
                            // The first authorized connection is the XDMCP session anchor.
                            let (mut from_client, mut to_client) = client.split();
                            let (mut from_server, mut to_server) = backend.split();
                            tokio::select! {
                                _ = tokio::io::copy(&mut from_client, &mut to_server) => {},
                                _ = tokio::io::copy(&mut from_server, &mut to_client) => {},
                            }
                        }
                        id
                    });
                }
            }
        }
    }
}

fn ignore_would_block(error: io::Error) -> io::Result<usize> {
    if error.kind() == io::ErrorKind::WouldBlock {
        Ok(0)
    } else {
        Err(error)
    }
}

async fn authorize(
    client: &mut TcpStream,
    path: &Path,
    cookie: &[u8; 16],
    backend_cookie: &[u8; 16],
) -> io::Result<UnixStream> {
    let denied = || {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            "X11 setup authorization rejected",
        )
    };
    let mut setup = Zeroizing::new([0; 48]);
    client.read_exact(&mut setup[..12]).await?;
    let word = match setup[0] {
        b'l' => u16::from_le_bytes,
        b'B' => u16::from_be_bytes,
        _ => return Err(denied()),
    };
    if (
        word([setup[2], setup[3]]),
        word([setup[4], setup[5]]),
        word([setup[6], setup[7]]),
        word([setup[8], setup[9]]),
    ) != (11, 0, 18, 16)
    {
        return Err(denied());
    }
    client.read_exact(&mut setup[12..]).await?;
    if &setup[12..30] != wire::COOKIE_NAME || setup[32..].ct_eq(cookie).unwrap_u8() != 1 {
        return Err(denied());
    }
    setup[32..].copy_from_slice(backend_cookie);
    let mut backend = UnixStream::connect(path).await?;
    backend.write_all(setup.as_ref()).await?;
    let mut header = [0; 8];
    backend.read_exact(&mut header).await?;
    let length = usize::from(word([header[6], header[7]])) * 4;
    if header[0] != 1
        || word([header[2], header[3]]) != 11
        || word([header[4], header[5]]) != 0
        || length < 32
    {
        return Err(denied());
    }
    let mut payload = vec![0; length];
    backend.read_exact(&mut payload).await?;
    client.write_all(&header).await?;
    client.write_all(&payload).await?;
    Ok(backend)
}
