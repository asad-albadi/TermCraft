use super::{Auth, ServiceConfig, ServiceProtocol, ServiceRuntime, files, ftp, telnet, tftp};
use std::{
    collections::{BTreeMap, VecDeque},
    io::{self, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream, UdpSocket},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const MAX_SERVICES: usize = 12;
const MAX_WORKERS: usize = 8;
const MAX_LOGS: usize = 32;
pub(super) const POLL: Duration = Duration::from_millis(50);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ServiceId(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceState {
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed,
}

#[derive(Clone, Debug)]
pub struct ServiceStatus {
    pub id: ServiceId,
    pub protocol: ServiceProtocol,
    pub state: ServiceState,
    pub address: Option<SocketAddr>,
    pub browser_address: Option<SocketAddr>,
    pub error: Option<String>,
    pub cron: Option<super::cron::CronStatus>,
    pub log: VecDeque<&'static str>,
}

#[derive(Default)]
pub struct ServiceSupervisor {
    next: Mutex<u64>,
    services: Mutex<BTreeMap<ServiceId, Arc<Context>>>,
}

impl ServiceSupervisor {
    pub fn start_vnc(&self, config: super::VncServiceConfig) -> Result<ServiceId, String> {
        self.start(vnc_config(config))
    }
    pub fn begin_vnc(&self, config: super::VncServiceConfig) -> Result<ServiceId, String> {
        self.begin(vnc_config(config))
    }
    pub fn start_cron(&self, config: super::cron::CronServiceConfig) -> Result<ServiceId, String> {
        self.start(cron_config(config))
    }
    pub fn begin_cron(&self, config: super::cron::CronServiceConfig) -> Result<ServiceId, String> {
        self.begin(cron_config(config))
    }

    pub fn start_ssh(&self, config: super::SshServiceConfig) -> Result<ServiceId, String> {
        self.start(ssh_config(config))
    }
    pub fn begin_ssh(&self, config: super::SshServiceConfig) -> Result<ServiceId, String> {
        self.begin(ssh_config(config))
    }
    pub fn start_iperf(&self, config: super::IperfServiceConfig) -> Result<ServiceId, String> {
        self.start(iperf_config(config))
    }
    pub fn begin_iperf(&self, config: super::IperfServiceConfig) -> Result<ServiceId, String> {
        self.begin(iperf_config(config))
    }

    /// The synchronous entry point for background callers that need the bind result.
    pub fn start(&self, config: ServiceConfig) -> Result<ServiceId, String> {
        let (id, ready) = self.launch(config)?;
        ready
            .recv()
            .map_err(|_| "Service startup ended unexpectedly.")??;
        Ok(id)
    }

    /// Queue startup without waiting for filesystem validation or listener binding.
    pub fn begin(&self, config: ServiceConfig) -> Result<ServiceId, String> {
        self.launch(config).map(|(id, _)| id)
    }

    fn launch(
        &self,
        config: ServiceConfig,
    ) -> Result<(ServiceId, mpsc::Receiver<Result<(), String>>), String> {
        let mut services = self.services.lock().unwrap();
        if services.len() == MAX_SERVICES {
            let retired = services.iter().find_map(|(id, instance)| {
                matches!(
                    instance.status.lock().unwrap().state,
                    ServiceState::Stopped | ServiceState::Failed
                )
                .then_some(*id)
            });
            services.remove(&retired.ok_or("Too many services are active.")?);
        }
        let mut next = self.next.lock().unwrap();
        *next = next.checked_add(1).ok_or("Service ID limit exceeded.")?;
        let id = ServiceId(*next);
        let context = Arc::new(Context {
            stop: AtomicBool::new(false),
            deadline: OnceLock::new(),
            worker: Mutex::new(None),
            status: Mutex::new(ServiceStatus {
                id,
                protocol: config.protocol,
                state: ServiceState::Starting,
                address: None,
                browser_address: None,
                error: None,
                cron: None,
                log: VecDeque::new(),
            }),
        });
        let owned = context.clone();
        let (ready_tx, ready) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name(format!("service-{}", id.0))
            .spawn(move || {
                run(config, &owned, ready_tx);
            })
            .map_err(|error| format!("Could not create service worker: {error}"))?;
        *context.worker.lock().unwrap() = Some(worker);
        services.insert(id, context);
        Ok((id, ready))
    }

    pub fn stop(&self, id: ServiceId) -> Result<(), String> {
        self.services
            .lock()
            .unwrap()
            .get(&id)
            .ok_or("Unknown service.")?
            .request_stop();
        Ok(())
    }

    pub fn status(&self, id: ServiceId) -> Option<ServiceStatus> {
        self.services
            .lock()
            .unwrap()
            .get(&id)
            .map(|instance| instance.status.lock().unwrap().clone())
    }

    pub fn statuses(&self) -> Vec<ServiceStatus> {
        self.services
            .lock()
            .unwrap()
            .values()
            .map(|instance| instance.status.lock().unwrap().clone())
            .collect()
    }
}

impl Drop for ServiceSupervisor {
    fn drop(&mut self) {
        let services = self.services.get_mut().unwrap();
        for instance in services.values() {
            instance.request_stop();
        }
        for instance in services.values() {
            if let Some(worker) = instance.worker.lock().unwrap().take() {
                let _ = worker.join();
            }
        }
    }
}

pub(super) struct Context {
    stop: AtomicBool,
    worker: Mutex<Option<JoinHandle<()>>>,
    deadline: OnceLock<Instant>,
    status: Mutex<ServiceStatus>,
}

impl Context {
    pub(super) fn stop_flag(&self) -> &AtomicBool {
        &self.stop
    }
    pub(super) fn cron_started(&self, cron: super::cron::CronStatus) {
        let mut s = self.status.lock().unwrap();
        s.state = ServiceState::Running;
        s.cron = Some(cron);
    }
    pub(super) fn cron_update(&self, cron: super::cron::CronStatus) {
        self.status.lock().unwrap().cron = Some(cron);
    }
    pub(super) fn cron_failed(&self, error: String) {
        let mut s = self.status.lock().unwrap();
        s.state = ServiceState::Failed;
        s.error = Some(error);
    }
    pub(super) fn cron_stopped(&self) {
        self.status.lock().unwrap().state = ServiceState::Stopped;
    }

    pub(super) fn request_stop(&self) {
        self.stop.store(true, Ordering::Release);
        let mut status = self.status.lock().unwrap();
        if matches!(status.state, ServiceState::Starting | ServiceState::Running) {
            status.state = ServiceState::Stopping;
        }
    }

    pub(super) fn cancelled(&self) -> bool {
        self.stop.load(Ordering::Acquire)
            || self
                .deadline
                .get()
                .is_some_and(|deadline| Instant::now() >= *deadline)
    }

    pub(super) fn check(&self, deadline: Instant) -> io::Result<()> {
        if self.cancelled() {
            Err(io::ErrorKind::ConnectionAborted.into())
        } else if Instant::now() >= deadline {
            Err(io::ErrorKind::TimedOut.into())
        } else {
            Ok(())
        }
    }

    pub(super) fn log(&self, event: &'static str) {
        let mut status = self.status.lock().unwrap();
        if status.log.len() == MAX_LOGS {
            status.log.pop_front();
        }
        status.log.push_back(event);
    }
}

fn cron_config(config: super::cron::CronServiceConfig) -> ServiceConfig {
    ServiceConfig {
        protocol: ServiceProtocol::Cron,
        bind: ([127, 0, 0, 1], 0).into(),
        allow_non_loopback: false,
        root: None,
        auth: super::ServiceAuth::Unauthenticated,
        allow_write: false,
        lifetime: config.lifetime,
        runtime: ServiceRuntime::Cron(config),
    }
}
fn ssh_config(config: super::SshServiceConfig) -> ServiceConfig {
    ServiceConfig {
        protocol: ServiceProtocol::Ssh,
        bind: config.bind,
        allow_non_loopback: config.allow_non_loopback,
        root: None,
        auth: super::ServiceAuth::Unauthenticated,
        allow_write: false,
        lifetime: config.lifetime,
        runtime: ServiceRuntime::Ssh(config),
    }
}
fn iperf_config(config: super::IperfServiceConfig) -> ServiceConfig {
    ServiceConfig {
        protocol: ServiceProtocol::Iperf,
        bind: config.bind,
        allow_non_loopback: config.allow_non_loopback,
        root: None,
        auth: super::ServiceAuth::Unauthenticated,
        allow_write: false,
        lifetime: config.lifetime,
        runtime: ServiceRuntime::Iperf(config),
    }
}
fn vnc_config(config: super::VncServiceConfig) -> ServiceConfig {
    ServiceConfig {
        protocol: ServiceProtocol::Vnc,
        bind: config.bind,
        allow_non_loopback: config.allow_non_loopback,
        root: None,
        auth: super::ServiceAuth::Unauthenticated,
        allow_write: false,
        lifetime: config.lifetime,
        runtime: ServiceRuntime::Vnc(config),
    }
}
enum Listener {
    Vnc(super::vnc::Listener),
    Nfs(super::nfs::Listener),
    Ssh(super::sshd::Listener),
    Iperf(super::iperf::Listener),
    Ftp(TcpListener, Arc<files::Share>, Arc<Auth>),
    Tftp(UdpSocket, Arc<files::Share>),
    Telnet(TcpListener, Arc<std::path::Path>, Arc<Auth>),
}

impl Listener {
    fn bind(config: ServiceConfig, context: &Arc<Context>) -> Result<Self, String> {
        if !config.bind.ip().is_loopback() && !config.allow_non_loopback {
            return Err("Enable network access before using a non-loopback address.".into());
        }
        if config.lifetime.is_some_and(|duration| {
            duration < Duration::from_secs(1) || duration > Duration::from_secs(86400)
        }) {
            return Err("The service lifetime must be 1 second to 24 hours.".into());
        }
        let auth = Arc::new(Auth::prepare(config.auth)?);
        let root = || {
            let path = config.root.as_deref().ok_or("Choose a shared folder.")?;
            files::Share::open(path, config.allow_write)
                .map(Arc::new)
                .map_err(|error| format!("Could not open shared folder: {error}"))
        };
        let tcp = || {
            let listener = TcpListener::bind(config.bind).map_err(bind_error)?;
            listener.set_nonblocking(true).map_err(bind_error)?;
            Ok::<_, String>(listener)
        };
        match (config.protocol, config.runtime) {
            (ServiceProtocol::Vnc, ServiceRuntime::Vnc(vnc))
                if vnc.bind == config.bind
                    && vnc.allow_non_loopback == config.allow_non_loopback =>
            {
                let owned = context.clone();
                super::vnc::Listener::bind_cancel(vnc, Arc::new(move || owned.cancelled()))
                    .map(Self::Vnc)
            }
            (ServiceProtocol::Ssh, ServiceRuntime::Ssh(ssh))
                if ssh.bind == config.bind
                    && ssh.allow_non_loopback == config.allow_non_loopback =>
            {
                super::sshd::Listener::bind(ssh).map(Self::Ssh)
            }
            (ServiceProtocol::Iperf, ServiceRuntime::Iperf(iperf))
                if iperf.bind == config.bind
                    && iperf.allow_non_loopback == config.allow_non_loopback =>
            {
                super::iperf::Listener::bind(iperf).map(Self::Iperf)
            }
            (ServiceProtocol::Ftp, ServiceRuntime::Embedded)
                if matches!(*auth, Auth::Password { .. } | Auth::Anonymous) =>
            {
                if config.allow_write && matches!(*auth, Auth::Anonymous) {
                    return Err("Anonymous FTP shares are read-only.".into());
                }
                Ok(Self::Ftp(tcp()?, root()?, auth))
            }
            (ServiceProtocol::Nfs, ServiceRuntime::Embedded) if matches!(*auth, Auth::None) => {
                let root = config.root.as_deref().ok_or("Choose a shared folder.")?;
                super::nfs::Listener::bind(config.bind, root, config.allow_write).map(Self::Nfs)
            }
            (ServiceProtocol::Tftp, ServiceRuntime::Embedded) if matches!(*auth, Auth::None) => {
                let socket = UdpSocket::bind(config.bind).map_err(bind_error)?;
                socket.set_nonblocking(true).map_err(bind_error)?;
                Ok(Self::Tftp(socket, root()?))
            }
            (ServiceProtocol::Telnet, ServiceRuntime::Shell { executable })
                if matches!(*auth, Auth::Password { .. }) =>
            {
                telnet::validate_shell(&executable)?;
                Ok(Self::Telnet(tcp()?, executable.into(), auth))
            }
            _ => Err("The authentication or runtime does not match the selected service.".into()),
        }
    }

    fn address(&self) -> io::Result<SocketAddr> {
        match self {
            Self::Ftp(listener, ..) | Self::Telnet(listener, ..) => listener.local_addr(),
            Self::Tftp(socket, ..) => socket.local_addr(),
            Self::Nfs(listener) => listener.address(),
            Self::Vnc(listener) => listener.address(),
            Self::Ssh(listener) => listener.address(),
            Self::Iperf(listener) => listener.address(),
        }
    }

    fn serve(self, context: &Arc<Context>) -> io::Result<()> {
        match self {
            Self::Vnc(listener) => listener.serve(context),
            Self::Ssh(listener) => listener.serve(context),
            Self::Nfs(listener) => listener.serve(|| context.cancelled()),
            Self::Iperf(listener) => listener.serve(context),
            Self::Ftp(listener, share, auth) => {
                serve_tcp(listener, context, move |stream, context| {
                    ftp::serve(stream, &share, &auth, context)
                })
            }
            Self::Tftp(socket, share) => tftp::serve(socket, share, context),
            Self::Telnet(listener, shell, auth) => {
                serve_tcp(listener, context, move |stream, context| {
                    telnet::serve(stream, &shell, &auth, context)
                })
            }
        }
    }
}

fn bind_error(error: io::Error) -> String {
    format!("Could not bind service listener: {error}")
}

fn run(config: ServiceConfig, context: &Arc<Context>, ready: mpsc::SyncSender<Result<(), String>>) {
    if matches!(config.protocol, ServiceProtocol::Cron) {
        if let ServiceRuntime::Cron(cron) = config.runtime {
            super::cron::run(cron, context, ready);
        } else {
            context.cron_failed("Cron requires a scheduler configuration.".into());
            let _ = ready.send(Err("Cron requires a scheduler configuration.".into()));
        }
        return;
    }
    let lifetime = config.lifetime;
    let setup = Listener::bind(config, context).and_then(|listener| {
        let address = listener.address().map_err(bind_error)?;
        let browser_address = match &listener {
            Listener::Vnc(vnc) => vnc.browser_address().map_err(bind_error)?,
            _ => None,
        };
        Ok((listener, address, browser_address))
    });
    {
        let mut status = context.status.lock().unwrap();
        if context.cancelled() {
            drop(setup);
            status.state = ServiceState::Stopped;
            let _ = ready.try_send(Err("Service startup cancelled.".into()));
            return;
        }
        match setup {
            Err(error) => {
                status.error = Some(error.clone());
                status.state = ServiceState::Failed;
                let _ = ready.try_send(Err(error));
                return;
            }
            Ok((ref _listener, address, browser_address)) => {
                if let Some(deadline) =
                    lifetime.and_then(|duration| Instant::now().checked_add(duration))
                {
                    let _ = context.deadline.set(deadline);
                }
                status.address = Some(address);
                status.browser_address = browser_address;
                status.state = ServiceState::Running;
            }
        }
    }
    let Ok((listener, _, _)) = setup else { return };
    context.log("Listener started.");
    let _ = ready.try_send(Ok(()));
    let result = listener.serve(context);
    let mut status = context.status.lock().unwrap();
    match result {
        Ok(()) => status.state = ServiceState::Stopped,
        Err(error) => {
            status.error = Some(format!("Service listener failed: {error}"));
            status.state = ServiceState::Failed;
        }
    }
}

#[derive(Default)]
pub(super) struct Workers(Vec<JoinHandle<()>>);

impl Workers {
    pub(super) fn has_room(&mut self) -> bool {
        self.0.retain(|worker| !worker.is_finished());
        self.0.len() < MAX_WORKERS
    }

    pub(super) fn spawn(&mut self, task: impl FnOnce() + Send + 'static) -> io::Result<()> {
        self.0.push(
            thread::Builder::new()
                .name("service-client".into())
                .spawn(task)?,
        );
        Ok(())
    }
}

impl Drop for Workers {
    fn drop(&mut self) {
        for worker in self.0.drain(..) {
            let _ = worker.join();
        }
    }
}

pub(super) fn serve_tcp(
    listener: TcpListener,
    context: &Arc<Context>,
    handler: impl Fn(TcpStream, &Context) -> io::Result<()> + Send + Sync + 'static,
) -> io::Result<()> {
    let handler = Arc::new(handler);
    let mut workers = Workers::default();
    let outcome = loop {
        if context.cancelled() {
            break Ok(());
        }
        match listener.accept() {
            Ok((stream, _)) if workers.has_room() => {
                let handler = handler.clone();
                let client = context.clone();
                if let Err(error) = workers.spawn(move || {
                    if prepare_stream(&stream)
                        .and_then(|()| handler(stream, &client))
                        .is_err()
                        && !client.cancelled()
                    {
                        client.log("Client session ended with an error.");
                    }
                }) {
                    break Err(error);
                }
            }
            Ok(_) => {}
            Err(error) if would_wait(&error) => thread::sleep(POLL),
            Err(error) => break Err(error),
        }
    };
    drop(listener);
    context.request_stop();
    drop(workers);
    outcome
}

pub(super) fn would_wait(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
    )
}

pub(super) fn prepare_stream(stream: &TcpStream) -> io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(POLL))?;
    stream.set_write_timeout(Some(POLL))?;
    Ok(())
}

pub(super) fn read(
    stream: &mut TcpStream,
    buffer: &mut [u8],
    context: &Context,
    deadline: Instant,
) -> io::Result<usize> {
    loop {
        context.check(deadline)?;
        match stream.read(buffer) {
            Err(error) if would_wait(&error) => {}
            result => return result,
        }
    }
}

pub(super) fn write(
    stream: &mut TcpStream,
    mut buffer: &[u8],
    context: &Context,
    deadline: Instant,
) -> io::Result<()> {
    while !buffer.is_empty() {
        context.check(deadline)?;
        match stream.write(buffer) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(size) => buffer = &buffer[size..],
            Err(error) if would_wait(&error) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}
