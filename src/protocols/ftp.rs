//! FTP and FTPS transport for a background task. FTP commands and data transfers
//! share one control channel and are serialized by an exclusive backend borrow.
use crate::{
    credentials::Secret,
    remote_files::{EntryKind, RemoteEntry, RemotePath, capabilities::RemoteCapabilities},
};
use rustls::pki_types::ServerName;
use std::{
    future::Future,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
    sync::watch,
};

mod control;
mod data;
mod security;

use control::{Control, Reply, Stream};
use data::{DataEndpoint, Direction, Newlines};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FtpTlsMode {
    #[default]
    Plain,
    Explicit,
    Implicit,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FtpDataMode {
    #[default]
    Passive,
    Active,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FtpTransferType {
    #[default]
    Binary,
    Ascii,
}

/// The password is held in the workspace secret container. Options are neither
/// printable nor part of the persisted session model.
pub struct FtpOptions {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: Option<Secret>,
    pub tls: FtpTlsMode,
    pub protect_data: bool,
    pub data_mode: FtpDataMode,
    pub certificate_authority: Option<PathBuf>,
    pub timeout: Duration,
}

impl FtpOptions {
    pub fn validate(&self) -> Result<(), String> {
        let host = self.server_host();
        if self.port == 0
            || host.is_empty()
            || ServerName::try_from(host).is_err()
            || self.timeout.is_zero()
            || self.timeout > Duration::from_secs(600)
        {
            return Err("Enter a valid FTP host, port and timeout (up to 600 seconds).".into());
        }
        if self.tls == FtpTlsMode::Plain && self.protect_data {
            return Err("Private FTP data requires explicit or implicit TLS.".into());
        }
        if self.tls == FtpTlsMode::Plain && self.certificate_authority.is_some() {
            return Err("An FTP certificate authority requires explicit or implicit TLS.".into());
        }
        if self.username.is_empty() || control::validate_argument(self.username.as_bytes()).is_err()
        {
            return Err("Enter a valid FTP username without NUL or line breaks.".into());
        }
        if self.password.as_ref().is_some_and(|password| {
            control::validate_argument(password.expose().as_bytes()).is_err()
        }) {
            return Err("The FTP password is too long or contains NUL or line breaks.".into());
        }
        Ok(())
    }

    fn server_host(&self) -> &str {
        self.host
            .strip_prefix('[')
            .and_then(|host| host.strip_suffix(']'))
            .unwrap_or(&self.host)
    }
}

/// Cancellation closes the connection if an operation is in progress. A transfer
/// may already have changed a server-side staging path when cancellation occurs.
#[derive(Clone)]
pub struct FtpCancel(watch::Sender<bool>);

impl Default for FtpCancel {
    fn default() -> Self {
        Self(watch::channel(false).0)
    }
}
impl FtpCancel {
    pub fn cancel(&self) {
        self.0.send_replace(true);
    }

    pub fn is_cancelled(&self) -> bool {
        *self.0.borrow()
    }

    pub async fn cancelled(&self) {
        let mut receiver = self.0.subscribe();
        let _ = receiver.wait_for(|state| *state).await;
    }
}

#[derive(Clone)]
struct Limits {
    timeout: Duration,
    cancel: FtpCancel,
}
impl Limits {
    async fn run<T>(
        &self,
        operation: &'static str,
        pending: impl Future<Output = Result<T, String>>,
    ) -> Result<T, String> {
        tokio::select! {
            biased;
            _ = self.cancel.cancelled() => Err("FTP operation cancelled.".into()),
            outcome = tokio::time::timeout(self.timeout, pending) => {
                outcome.map_err(|_| format!("FTP {operation} timed out."))?
            }
        }
    }

    async fn io<T>(
        &self,
        operation: &'static str,
        pending: impl Future<Output = std::io::Result<T>>,
    ) -> Result<T, String> {
        self.run(operation, async {
            pending
                .await
                .map_err(|error| format!("FTP {operation}: {error}"))
        })
        .await
    }
}

pub struct FtpBackend {
    control: Option<Control<Stream>>,
    peer: SocketAddr,
    local: SocketAddr,
    data_mode: FtpDataMode,
    data_tls: Option<security::Tls>,
    certificate_fingerprint: Option<String>,
    limits: Limits,
}

impl FtpBackend {
    pub async fn connect(options: FtpOptions) -> Result<Self, String> {
        Self::connect_with_cancel(options, FtpCancel::default()).await
    }

    pub async fn connect_with_cancel(
        options: FtpOptions,
        cancel: FtpCancel,
    ) -> Result<Self, String> {
        options.validate()?;
        let limits = Limits {
            timeout: options.timeout,
            cancel,
        };
        limits
            .run("connection setup", Self::open(options, limits.clone()))
            .await
    }

    async fn open(options: FtpOptions, limits: Limits) -> Result<Self, String> {
        let host = options.server_host();
        let tls = if options.tls == FtpTlsMode::Plain {
            None
        } else {
            let ca = options.certificate_authority.clone();
            let name = ServerName::try_from(host.to_owned())
                .map_err(|_| "Invalid FTP TLS server name.")?;
            Some(
                limits
                    .run("CA loading", async move {
                        tokio::task::spawn_blocking(move || security::Tls::new(name, ca.as_deref()))
                            .await
                            .map_err(|_| "FTP certificate trust initialization failed.")?
                    })
                    .await?,
            )
        };
        let stream = limits
            .io(
                "control connection",
                TcpStream::connect((host, options.port)),
            )
            .await?;
        let peer = stream
            .peer_addr()
            .map_err(|_| "Could not identify the FTP control peer.")?;
        let local = stream
            .local_addr()
            .map_err(|_| "Could not identify the local FTP control address.")?;
        let (stream, certificate_fingerprint): (Stream, Option<String>) = if let Some(tls) = &tls {
            let stream = if options.tls == FtpTlsMode::Explicit {
                let mut plain = Control::new(stream, limits.clone());
                plain.greeting().await?;
                plain
                    .command("AUTH", Some(b"TLS"))
                    .await?
                    .require("AUTH TLS", &[234])?;
                plain.into_inner()?
            } else {
                stream
            };
            let stream = tls.wrap(stream, &limits, "control").await?;
            let fingerprint = security::fingerprint(&stream)?;
            (Box::new(stream), Some(fingerprint))
        } else {
            (Box::new(stream), None)
        };
        let mut control = Control::new(stream, limits.clone());
        if options.tls != FtpTlsMode::Explicit {
            control.greeting().await?;
        }
        let reply = control
            .command("USER", Some(options.username.as_bytes()))
            .await?;
        if reply.code == 331 {
            let password = options
                .password
                .as_ref()
                .ok_or("The FTP server requires a password.")?;
            control
                .command("PASS", Some(password.expose().as_bytes()))
                .await?
                .require("PASS authentication", &[230, 202])?;
        } else {
            reply.require("USER authentication", &[230])?;
        }
        if tls.is_some() {
            control
                .command("PBSZ", Some(b"0"))
                .await?
                .require("PBSZ 0", &[200])?;
            if options.protect_data {
                control
                    .command("PROT", Some(b"P"))
                    .await?
                    .require("PROT P private data", &[200])?;
            } else {
                control
                    .command("PROT", Some(b"C"))
                    .await?
                    .require("PROT C clear data", &[200])?;
            }
        }
        Ok(Self {
            control: Some(control),
            peer,
            local,
            data_mode: options.data_mode,
            data_tls: tls.filter(|_| options.protect_data),
            certificate_fingerprint,
            limits,
        })
    }

    pub fn capabilities(&self) -> RemoteCapabilities {
        RemoteCapabilities {
            browse: true,
            upload_files: true,
            download_files: true,
            exclusive_create: false,
            upload_folders: true,
            download_folders: true,
            edit: true,
            mutate_entries: true,
            recursive_remove: true,
        }
    }

    /// SHA-256 of the verified control peer leaf certificate, if FTPS is in use.
    pub fn certificate_fingerprint(&self) -> Option<&str> {
        self.certificate_fingerprint.as_deref()
    }

    pub fn cancel_handle(&self) -> FtpCancel {
        self.limits.cancel.clone()
    }

    pub fn is_connected(&self) -> bool {
        self.control.is_some() && !self.limits.cancel.is_cancelled()
    }

    pub async fn pwd(&mut self) -> Result<RemotePath, String> {
        let reply = self.command("PWD", None).await?.require("PWD", &[257])?;
        control::quoted_path(&reply.body)
    }

    pub async fn list(&mut self, path: &RemotePath) -> Result<Vec<RemoteEntry>, String> {
        let listing = self
            .collect("MLSD", path, FtpTransferType::Binary, 8 * 1024 * 1024)
            .await?;
        data::parse_listing(path, &listing)
    }

    /// Read a preview or small document with an explicit maximum byte count.
    pub async fn read(
        &mut self,
        path: &RemotePath,
        transfer_type: FtpTransferType,
        limit: usize,
    ) -> Result<Vec<u8>, String> {
        self.collect("RETR", path, transfer_type, limit as u64)
            .await
    }

    async fn collect(
        &mut self,
        command: &'static str,
        path: &RemotePath,
        transfer_type: FtpTransferType,
        limit: u64,
    ) -> Result<Vec<u8>, String> {
        let mut data = Vec::new();
        match tokio::time::timeout(
            self.limits.timeout,
            self.download(command, path, transfer_type, &mut data, limit),
        )
        .await
        {
            Ok(result) => result.map(|_| data),
            Err(_) => {
                self.control = None;
                Err(format!("FTP {command} timed out."))
            }
        }
    }

    /// Streams to a caller-owned staging writer. Commit that destination only
    /// after this method succeeds. ASCII reads convert NVT CR LF to local LF.
    pub async fn read_to<W: AsyncWrite + Unpin>(
        &mut self,
        path: &RemotePath,
        transfer_type: FtpTransferType,
        writer: &mut W,
        limit: u64,
    ) -> Result<u64, String> {
        self.download("RETR", path, transfer_type, writer, limit)
            .await
    }

    async fn download<W: AsyncWrite + Unpin>(
        &mut self,
        command: &'static str,
        path: &RemotePath,
        transfer_type: FtpTransferType,
        writer: &mut W,
        limit: u64,
    ) -> Result<u64, String> {
        let (control, mut stream) = self.start_transfer(command, path, transfer_type).await?;
        let bytes = self
            .copy(
                &mut stream,
                writer,
                transfer_type,
                Direction::Download,
                limit,
            )
            .await?;
        if self.data_tls.is_some() {
            self.limits
                .io("closing data download", async {
                    match stream.shutdown().await {
                        // The TLS read already reached an authenticated EOF.
                        // Windows can reset the write side after the peer has
                        // closed its socket. The control reply is still checked.
                        #[cfg(windows)]
                        Err(error)
                            if matches!(
                                error.kind(),
                                std::io::ErrorKind::ConnectionReset
                                    | std::io::ErrorKind::BrokenPipe
                            ) =>
                        {
                            Ok(())
                        }
                        result => result,
                    }
                })
                .await?;
        }
        drop(stream);
        self.finish_transfer(control).await?;
        Ok(bytes)
    }

    pub async fn write(
        &mut self,
        path: &RemotePath,
        transfer_type: FtpTransferType,
        mut bytes: &[u8],
    ) -> Result<(), String> {
        self.write_from(path, transfer_type, &mut bytes)
            .await
            .map(|_| ())
    }

    /// ASCII writes normalize local LF and CR LF to NVT CR LF, and encode a
    /// bare CR as CR NUL. Binary copies preserve every byte.
    pub async fn write_from<R: AsyncRead + Unpin>(
        &mut self,
        path: &RemotePath,
        transfer_type: FtpTransferType,
        reader: &mut R,
    ) -> Result<u64, String> {
        let (control, mut stream) = self.start_transfer("STOR", path, transfer_type).await?;
        let bytes = self
            .copy(
                reader,
                &mut stream,
                transfer_type,
                Direction::Upload,
                u64::MAX,
            )
            .await?;
        self.limits
            .io("closing data upload", stream.shutdown())
            .await?;
        self.finish_transfer(control).await?;
        drop(stream);
        Ok(bytes)
    }

    async fn copy<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
        &self,
        reader: &mut R,
        writer: &mut W,
        transfer_type: FtpTransferType,
        direction: Direction,
        limit: u64,
    ) -> Result<u64, String> {
        let mut buffer = [0; 16 * 1024];
        let mut ascii = Newlines::new(direction);
        let mut converted = Vec::new();
        let mut input_total = 0u64;
        let mut output_total = 0u64;
        loop {
            let read = self
                .limits
                .io("transfer read", reader.read(&mut buffer))
                .await?;
            input_total = input_total
                .checked_add(read as u64)
                .ok_or("FTP transfer size overflow.")?;
            let chunk = if transfer_type == FtpTransferType::Ascii {
                ascii.convert(&buffer[..read], &mut converted);
                &converted[..]
            } else {
                &buffer[..read]
            };
            output_total = output_total
                .checked_add(chunk.len() as u64)
                .ok_or("FTP transfer size overflow.")?;
            if output_total > limit {
                return Err("FTP transfer exceeds the permitted size.".into());
            }
            self.limits
                .io("transfer write", writer.write_all(chunk))
                .await?;
            if read == 0 {
                self.limits.io("flushing transfer", writer.flush()).await?;
                return Ok(match direction {
                    Direction::Download => output_total,
                    Direction::Upload => input_total,
                });
            }
        }
    }

    pub async fn mkdir(&mut self, path: &RemotePath) -> Result<(), String> {
        self.path_command("MKD", path, &[257]).await
    }

    pub async fn rename(
        &mut self,
        source: &RemotePath,
        destination: &RemotePath,
    ) -> Result<(), String> {
        control::validate_argument(destination.as_bytes())?;
        self.path_command("RNFR", source, &[350]).await?;
        self.path_command("RNTO", destination, &[250]).await
    }

    pub async fn remove(&mut self, path: &RemotePath, kind: EntryKind) -> Result<(), String> {
        let command = if kind == EntryKind::Directory {
            "RMD"
        } else {
            "DELE"
        };
        self.path_command(command, path, &[250]).await
    }

    pub async fn size(&mut self, path: &RemotePath) -> Result<u64, String> {
        control::validate_argument(path.as_bytes())?;
        self.command("TYPE", Some(b"I"))
            .await?
            .require("TYPE I", &[200])?;
        let reply = self
            .command("SIZE", Some(path.as_bytes()))
            .await?
            .require("SIZE", &[213])?;
        std::str::from_utf8(&reply.body)
            .ok()
            .and_then(|size| size.parse().ok())
            .ok_or_else(|| "Invalid FTP size response.".into())
    }

    async fn path_command(
        &mut self,
        command: &'static str,
        path: &RemotePath,
        expected: &[u16],
    ) -> Result<(), String> {
        self.command(command, Some(path.as_bytes()))
            .await?
            .require(command, expected)?;
        Ok(())
    }

    async fn command(
        &mut self,
        verb: &'static str,
        argument: Option<&[u8]>,
    ) -> Result<Reply, String> {
        if let Some(argument) = argument {
            control::validate_argument(argument)?;
        }
        let mut control = self.take_control()?;
        let reply = control.command(verb, argument).await?;
        if (100..200).contains(&reply.code) {
            // A final reply may still be queued. It cannot belong to a subsequent command.
            return Err(reply.failure(verb));
        }
        if reply.code != 421 {
            self.control = Some(control);
        }
        Ok(reply)
    }

    fn take_control(&mut self) -> Result<Control<Stream>, String> {
        if self.limits.cancel.is_cancelled() {
            self.control = None;
            return Err("FTP operation cancelled.".into());
        }
        self.control
            .take()
            .ok_or_else(|| "FTP connection is closed. Reconnect to continue.".into())
    }

    async fn start_transfer(
        &mut self,
        command: &'static str,
        path: &RemotePath,
        transfer_type: FtpTransferType,
    ) -> Result<(Control<Stream>, Stream), String> {
        let path = path.as_bytes();
        control::validate_argument(path)?;
        // MLSD defines its own data representation (RFC 3659 §7.2).
        if command != "MLSD" {
            let record_type = match transfer_type {
                FtpTransferType::Binary => b"I",
                FtpTransferType::Ascii => b"A",
            };
            self.command("TYPE", Some(record_type))
                .await?
                .require("TYPE", &[200])?;
        }
        let endpoint = match self.data_mode {
            FtpDataMode::Passive => self.passive().await?,
            FtpDataMode::Active => self.active().await?,
        };
        let mut control = self.take_control()?;
        let reply = control.command(command, Some(path)).await?;
        if !(100..200).contains(&reply.code) {
            if reply.code != 421 {
                self.control = Some(control);
            }
            return Err(reply.failure(command));
        }
        reply.require(command, &[125, 150])?;
        let stream = endpoint.open(self.peer.ip(), &self.limits).await?;
        let stream = match &self.data_tls {
            Some(tls) => Box::new(tls.wrap(stream, &self.limits, "data").await?) as Stream,
            None => Box::new(stream) as Stream,
        };
        Ok((control, stream))
    }

    async fn finish_transfer(&mut self, mut control: Control<Stream>) -> Result<(), String> {
        control
            .reply()
            .await?
            .require("data completion", &[226, 250])?;
        self.control = Some(control);
        Ok(())
    }

    async fn passive(&mut self) -> Result<DataEndpoint, String> {
        let epsv = self.command("EPSV", None).await?;
        let port = if epsv.code == 229 {
            data::epsv_port(&epsv.body)?
        } else if self.peer.is_ipv4() && epsv.unsupported() {
            let pasv = self.command("PASV", None).await?.require("PASV", &[227])?;
            data::pasv_port(&pasv.body)?
        } else {
            return Err(epsv.failure("EPSV"));
        };
        let mut address = self.peer;
        address.set_port(port);
        let socket = self
            .limits
            .io("passive data connection", TcpStream::connect(address))
            .await?;
        Ok(DataEndpoint::Passive(socket))
    }

    async fn active(&mut self) -> Result<DataEndpoint, String> {
        let listener = self
            .limits
            .io(
                "active data listener",
                tokio::net::TcpListener::bind({
                    let mut address = self.local;
                    address.set_port(0);
                    address
                }),
            )
            .await?;
        let port = listener
            .local_addr()
            .map_err(|_| "Could not identify the FTP active listener.")?
            .port();
        let family = if self.local.is_ipv4() { 1 } else { 2 };
        let destination = format!("|{family}|{}|{port}|", self.local.ip());
        let eprt = self.command("EPRT", Some(destination.as_bytes())).await?;
        if eprt.code != 200 {
            if let IpAddr::V4(address) = self.local.ip()
                && eprt.unsupported()
            {
                let [a, b, c, d] = address.octets();
                let [high, low] = port.to_be_bytes();
                let destination = format!("{a},{b},{c},{d},{high},{low}");
                self.command("PORT", Some(destination.as_bytes()))
                    .await?
                    .require("PORT", &[200])?;
            } else {
                return Err(eprt.failure("EPRT"));
            }
        }
        Ok(DataEndpoint::Active(listener))
    }
}
