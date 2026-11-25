//! Embedded Telnet and BSD remote-login transports.
mod rlogin;
mod rsh;
mod telnet;

use crate::model::{Session, validate_host, validate_user};
use std::{
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpSocket, TcpStream, lookup_host},
    sync::{mpsc as async_mpsc, watch},
    time::{interval, timeout},
};

const DEADLINE: Duration = Duration::from_secs(10);
const QUEUE_DEPTH: usize = 32;
const MAX_INPUT: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyProtocol {
    Telnet,
    Rlogin,
    Rsh,
}

#[derive(Debug)]
pub struct LegacyProtocolConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub protocol: LegacyProtocol,
    pub cols: u16,
    pub rows: u16,
    pub command: String,
    /// Connected gateway carrier for the configured Telnet target.
    pub carrier: Option<std::net::TcpStream>,
    pub gateway_required: bool,
}

impl LegacyProtocolConfig {
    pub fn from_session(session: &Session, cols: u16, rows: u16) -> Result<Self, String> {
        let protocol = match session.protocol.as_str() {
            "Telnet" => LegacyProtocol::Telnet,
            "Rlogin" => LegacyProtocol::Rlogin,
            "Rsh" => LegacyProtocol::Rsh,
            _ => {
                return Err(format!(
                    "{} is not a legacy terminal protocol",
                    session.protocol
                ));
            }
        };
        if protocol != LegacyProtocol::Telnet && !session.jump.trim().is_empty() {
            return Err(
                "Rlogin and Rsh require a reserved TCP source port on a direct connection".into(),
            );
        }
        let config = Self {
            host: session.host.clone(),
            port: session.port,
            user: session.user.clone(),
            protocol,
            cols,
            rows,
            command: session.command.clone(),
            carrier: None,
            gateway_required: !session.jump.trim().is_empty(),
        };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), String> {
        validate_host(&self.host)?;
        validate_user(&self.user)?;
        if self.user.len() > 255 {
            return Err("The legacy username exceeds 255 bytes".into());
        }
        if self.port == 0 {
            return Err("Choose a nonzero connection port".into());
        }
        if self.carrier.is_some() && self.protocol != LegacyProtocol::Telnet {
            return Err(
                "Rlogin and Rsh require a reserved TCP source port on a direct connection".into(),
            );
        }
        if self.protocol == LegacyProtocol::Rsh
            && (self.command.trim().is_empty()
                || self.command.contains('\0')
                || self.command.len() > MAX_INPUT)
        {
            return Err("Enter an Rsh command up to 64 KiB without zero bytes".into());
        }
        Ok(())
    }
}

pub struct Transport {
    output: Arc<Mutex<Output>>,
    input: async_mpsc::Sender<Input>,
    size: watch::Sender<(u16, u16)>,
    cancel: watch::Sender<bool>,
    ready: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
    worker: JoinHandle<()>,
}

enum Input {
    Bytes(Vec<u8>),
    Enter,
}

struct Output {
    rx: mpsc::Receiver<Vec<u8>>,
    flushed: bool,
}

impl Transport {
    pub fn connect(config: LegacyProtocolConfig) -> Result<Self, String> {
        config.validate()?;
        if config.gateway_required && config.carrier.is_none() {
            return Err("This session requires a connected SSH gateway carrier".into());
        }
        let (sender, rx) = mpsc::sync_channel(QUEUE_DEPTH);
        let output = Arc::new(Mutex::new(Output { rx, flushed: false }));
        let worker_output = output.clone();
        let (input, commands) = async_mpsc::channel(QUEUE_DEPTH);
        let (size, sizes) = watch::channel((config.cols, config.rows));
        let (cancel, mut stop) = watch::channel(false);
        let ready = Arc::new(AtomicBool::new(false));
        let error = Arc::new(Mutex::new(None));
        let worker_ready = ready.clone();
        let worker_error = error.clone();
        let endpoint = format!("{:?} {} port {}", config.protocol, config.host, config.port);
        let worker = thread::Builder::new()
            .name("legacy-terminal".into())
            .spawn(move || {
                let result = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|error| error.to_string())
                    .and_then(|runtime| {
                        let result = runtime.block_on(async {
                            tokio::select! {
                                biased;
                                _ = stop.changed() => Ok(()),
                                result = run(config, commands, sizes, sender, &worker_output, &worker_ready) => result,
                            }
                        });
                        // A blocking DNS lookup may outlive a cancelled connection.
                        runtime.shutdown_background();
                        result
                    });
                if let Err(error) = result {
                    *worker_error.lock().unwrap() = Some(format!("{endpoint}: {error}"));
                }
            })
            .map_err(|error| format!("Start legacy protocol worker: {error}"))?;
        Ok(Self {
            output,
            input,
            size,
            cancel,
            ready,
            error,
            worker,
        })
    }

    pub fn write(&self, data: &[u8]) -> Result<(), String> {
        if data.len() > MAX_INPUT {
            return Err("Terminal input exceeds 64 KiB per write".into());
        }
        self.submit(Input::Bytes(data.to_vec()))
    }

    /// Queue the Enter key separately from a literal carriage return.
    pub fn enter(&self) -> Result<(), String> {
        self.submit(Input::Enter)
    }

    fn submit(&self, input: Input) -> Result<(), String> {
        if *self.cancel.borrow() {
            return Err("Connection is closed".into());
        }
        self.input.try_send(input).map_err(|error| match error {
            async_mpsc::error::TrySendError::Full(_) => "Terminal input queue is full".into(),
            async_mpsc::error::TrySendError::Closed(_) => "Connection is closed".into(),
        })
    }

    /// Drain immediately available output. If the peer flushed its output,
    /// discard any partially consumed chunk retained by the caller first.
    /// The receiver callback must not block waiting for more bytes.
    pub fn with_output<T>(&self, consume: impl FnOnce(&mpsc::Receiver<Vec<u8>>, bool) -> T) -> T {
        let mut output = self.output.lock().unwrap();
        let flushed = std::mem::take(&mut output.flushed);
        consume(&output.rx, flushed)
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        if *self.cancel.borrow() {
            return Err("Connection is closed".into());
        }
        self.size
            .send((cols, rows))
            .map_err(|_| "Connection is closed".into())
    }

    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    pub fn is_finished(&self) -> bool {
        self.worker.is_finished()
    }

    pub fn input_error(&self) -> Option<String> {
        self.error.lock().unwrap().clone()
    }

    pub fn close(&mut self) {
        self.cancel.send_replace(true);
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        self.close();
    }
}

async fn run(
    mut config: LegacyProtocolConfig,
    mut input: async_mpsc::Receiver<Input>,
    mut sizes: watch::Receiver<(u16, u16)>,
    output: mpsc::SyncSender<Vec<u8>>,
    output_queue: &Mutex<Output>,
    ready: &AtomicBool,
) -> Result<(), String> {
    let mut stream = timeout(DEADLINE, open(&mut config))
        .await
        .map_err(|_| "Connection timed out".to_string())??;
    let mut size = *sizes.borrow_and_update();
    let mut protocol = match config.protocol {
        LegacyProtocol::Telnet => Wire::Telnet(Box::new(telnet::Telnet::new(config.user, size))),
        LegacyProtocol::Rlogin | LegacyProtocol::Rsh => {
            let local = local_user()?;
            let remote = if config.user.is_empty() {
                &local
            } else {
                &config.user
            };
            let greeting = match config.protocol {
                LegacyProtocol::Rlogin => rlogin::greeting(&local, remote),
                _ => rsh::greeting(&local, remote, &config.command),
            };
            timeout(DEADLINE, handshake(&mut stream, &greeting))
                .await
                .map_err(|_| "Server did not acknowledge the login handshake".to_string())??;
            if config.protocol == LegacyProtocol::Rlogin {
                Wire::Rlogin(rlogin::Rlogin::default())
            } else {
                Wire::Rsh
            }
        }
    };
    ready.store(true, Ordering::Release);

    let mut buffer = [0; 8192];
    let mut pending = Vec::new();
    let mut written = 0;
    let mut display = Vec::new();
    let mut timer = interval(Duration::from_millis(10));
    loop {
        if !display.is_empty() {
            match output.try_send(std::mem::take(&mut display)) {
                Ok(()) => {}
                Err(mpsc::TrySendError::Full(bytes)) => display = bytes,
                Err(mpsc::TrySendError::Disconnected(_)) => return Ok(()),
            }
        }
        let have_wire = written < pending.len();
        tokio::select! {
            // Drain ready output and prioritize resize changes before keyboard input.
            biased;
            _ = timer.tick(), if !display.is_empty() || matches!(protocol, Wire::Rlogin(_)) => {
                if let Wire::Rlogin(login) = &mut protocol {
                    if login.receive_control(&stream, size, &mut pending)? {
                        flush_output(output_queue, &mut display);
                    }
                }
            }
            changed = sizes.changed(), if !have_wire => {
                if changed.is_err() {
                    return Ok(());
                }
                size = *sizes.borrow_and_update();
                match &mut protocol {
                    Wire::Telnet(telnet) => telnet.resize(size, &mut pending),
                    Wire::Rlogin(login) => login.resize(size, &mut pending),
                    Wire::Rsh => {} // Rsh has no terminal sizing message.
                }
            }
            result = stream.writable(), if have_wire => {
                result.map_err(|error| error.to_string())?;
                match stream.try_write(&pending[written..]) {
                    Ok(0) => return Err("Connection stopped accepting input".into()),
                    Ok(count) => written += count,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {},
                    Err(error) => return Err(format!("Write: {error}")),
                }
                if written == pending.len() {
                    pending.clear();
                    written = 0;
                }
            }
            command = input.recv(), if !have_wire && display.is_empty() => {
                match (&protocol, command) {
                    (_, None) => return Ok(()),
                    (Wire::Telnet(telnet), Some(Input::Bytes(data))) => {
                        telnet.input(&data, &mut pending);
                        telnet.echo_input(&data, &mut display);
                    }
                    (Wire::Telnet(telnet), Some(Input::Enter)) => {
                        telnet.enter(&mut pending, &mut display);
                    }
                    (_, Some(Input::Bytes(data))) => pending = data,
                    (_, Some(Input::Enter)) => pending.push(b'\r'),
                }
            }
            result = stream.readable(), if display.is_empty() && pending.len() < 2 * MAX_INPUT + buffer.len() => {
                result.map_err(|error| error.to_string())?;
                let discard = if let Wire::Rlogin(login) = &mut protocol {
                    if login.receive_control(&stream, size, &mut pending)? {
                        flush_output(output_queue, &mut display);
                    }
                    login.discard_before_mark(&stream)?
                } else {
                    false
                };
                match stream.try_read(&mut buffer) {
                    Ok(0) => return Ok(()),
                    Ok(_) if discard => {},
                    Ok(count) => {
                        let bytes = &buffer[..count];
                        match &mut protocol {
                            Wire::Telnet(telnet) => telnet.receive(bytes, &mut pending, &mut display),
                            _ => display.extend_from_slice(bytes),
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {},
                    Err(error) => return Err(format!("Read: {error}")),
                }
            }
        }
    }
}

fn flush_output(output: &Mutex<Output>, display: &mut Vec<u8>) {
    display.clear();
    let mut output = output.lock().unwrap();
    while output.rx.try_recv().is_ok() {}
    output.flushed = true;
}

enum Wire {
    Telnet(Box<telnet::Telnet>),
    Rlogin(rlogin::Rlogin),
    Rsh,
}

async fn open(config: &mut LegacyProtocolConfig) -> Result<TcpStream, String> {
    if let Some(carrier) = config.carrier.take() {
        carrier
            .set_nonblocking(true)
            .map_err(|error| error.to_string())?;
        return TcpStream::from_std(carrier).map_err(|error| error.to_string());
    }

    let host = config
        .host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(&config.host);
    let addresses = lookup_host((host, config.port))
        .await
        .map_err(|error| error.to_string())?;
    let mut last_error = "Host resolved to no TCP addresses".to_owned();
    for address in addresses {
        let result = if config.protocol == LegacyProtocol::Telnet {
            TcpStream::connect(address)
                .await
                .map_err(|error| error.to_string())
        } else {
            match reserved_socket(address) {
                Ok(socket) => socket
                    .connect(address)
                    .await
                    .map_err(|error| error.to_string()),
                Err(error) => Err(error),
            }
        };
        match result {
            Ok(stream) => return Ok(stream),
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}

fn reserved_socket(remote: SocketAddr) -> Result<TcpSocket, String> {
    let socket = if remote.is_ipv4() {
        TcpSocket::new_v4()
    } else {
        TcpSocket::new_v6()
    }
    .map_err(|error| error.to_string())?;
    let ip = if remote.is_ipv4() {
        IpAddr::V4(Ipv4Addr::UNSPECIFIED)
    } else {
        IpAddr::V6(Ipv6Addr::UNSPECIFIED)
    };
    let message = "Cannot bind a reserved TCP source port (512–1023) for Rlogin/Rsh";
    for port in (512..=1023).rev() {
        match socket.bind(SocketAddr::new(ip, port)) {
            Ok(()) => return Ok(socket),
            Err(error) if error.kind() == io::ErrorKind::AddrInUse => {}
            Err(error) => return Err(format!("{message}: {error}")),
        }
    }
    Err(format!("{message}: all ports are in use"))
}

async fn handshake(stream: &mut TcpStream, greeting: &[u8]) -> Result<(), String> {
    stream
        .write_all(greeting)
        .await
        .map_err(|error| error.to_string())?;
    let status = stream
        .read_u8()
        .await
        .map_err(|error| format!("Read login status: {error}"))?;
    if status == 0 {
        return Ok(());
    }
    let mut reason = Vec::new();
    let _ = timeout(Duration::from_millis(400), async {
        while reason.len() < 1024 {
            let Ok(byte) = stream.read_u8().await else {
                break;
            };
            if byte == b'\n' {
                break;
            }
            reason.push(byte);
        }
    })
    .await;
    let reason = String::from_utf8_lossy(&reason);
    let reason: String = reason.chars().filter(|ch| !ch.is_control()).collect();
    Err(format!(
        "Server refused the legacy login (status {status}): {reason}"
    ))
}

fn local_user() -> Result<String, String> {
    let variable = if cfg!(windows) { "USERNAME" } else { "USER" };
    std::env::var(variable)
        .ok()
        .filter(|user| !user.is_empty() && validate_user(user).is_ok())
        .ok_or_else(|| format!("Rlogin/Rsh requires a local account name in {variable}"))
}

pub mod ftp;
pub mod ftp_actor;
