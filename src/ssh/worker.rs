use super::*;
use super::{
    auth::Challenges,
    host_keys::{self, HostStatus},
};
use crate::{
    remote_files::{EntryKind, RemoteEntry},
    x11::{XDisplaySource, XForwardingLease, XTrustMode},
};
use russh::{
    ChannelMsg, client,
    keys::{PublicKeyOrCertificate, ssh_key::HashAlg},
};
use russh_sftp::{client::RawSftpSession, protocol::StatusCode};
use std::{borrow::Cow, time::Duration};
use tokio::{sync::Mutex, task::JoinSet};
struct Handler {
    host: String,
    port: u16,
    app: PathBuf,
    user: Vec<PathBuf>,
    prompts: Challenges,
    role: auth::HopRole,
    verified_key: Arc<std::sync::Mutex<Option<String>>>,
    x11: Option<Arc<super::x11::Forwarder>>,
}
impl client::Handler for Handler {
    type Error = russh::Error;
    async fn server_channel_open_x11(
        &mut self,
        channel: russh::Channel<client::Msg>,
        _: &str,
        _: u32,
        reply: client::ChannelOpenHandle,
        _: &mut client::Session,
    ) -> Result<(), Self::Error> {
        // Dropping the reply rejects unsolicited forwarding on gateway/non-X11 sessions.
        let Some(forwarder) = self.x11.clone() else {
            return Ok(());
        };
        let Some(permit) = forwarder.permit() else {
            return Ok(());
        };
        reply.accept().await;
        let events = self.prompts.events.clone();
        let connection = self.prompts.connection;
        // The connection's runtime owns this task; cancellation drops its local socket too.
        tokio::spawn(async move {
            let _permit = permit;
            if let Err(message) = forwarder.bridge(channel.into_stream()).await {
                let _ = events.try_send(ConnectionEvent::State {
                    connection,
                    message: format!("X11: {message}"),
                });
            }
        });
        Ok(())
    }
    async fn check_server_key(
        &mut self,
        provided: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        if provided.certificate().is_some() {
            return Err(russh::Error::Disconnect);
        }
        let key = provided.public_key();
        let paths = self
            .user
            .iter()
            .map(|p| p.as_path())
            .chain(std::iter::once(self.app.as_path()))
            .collect::<Vec<_>>();
        let accepted = match host_keys::check(&self.host, self.port, &key, &paths) {
            Ok(HostStatus::Known) => Ok(true),
            Err(message) => {
                let _ = self
                    .prompts
                    .events
                    .send(ConnectionEvent::State {
                        connection: self.prompts.connection,
                        message,
                    })
                    .await;
                Err(russh::Error::UnknownKey)
            }
            Ok(HostStatus::Unknown) => {
                let title = format!(
                    "{} SSH · Verify {}:{}\n{}\nTrust this server key only after verifying its fingerprint.",
                    match self.role {
                        auth::HopRole::Gateway => "Gateway",
                        auth::HopRole::Target => "Target",
                    },
                    self.host,
                    self.port,
                    key.fingerprint(HashAlg::Sha256)
                );
                let answer = self
                    .prompts
                    .ask(title, vec![], true)
                    .await
                    .map_err(|_| russh::Error::Disconnect)?;
                // Trust may have changed while this prompt was visible.
                host_keys::check(&self.host, self.port, &key, &paths)
                    .map_err(|_| russh::Error::UnknownKey)?;
                match answer.trust {
                    Some(TrustDecision::Once) => Ok(true),
                    Some(TrustDecision::Remember) => {
                        host_keys::remember(&self.host, self.port, &key, &self.app)
                            .map_err(|_| russh::Error::Disconnect)?;
                        Ok(true)
                    }
                    _ => Ok(false),
                }
            }
        }?;
        if accepted {
            *self.verified_key.lock().unwrap_or_else(|p| p.into_inner()) =
                Some(key.fingerprint(HashAlg::Sha256).to_string());
        }
        Ok(accepted)
    }
}
#[derive(Default)]
struct DirectoryBudget {
    bytes: usize,
    count: usize,
}
impl DirectoryBudget {
    fn reserve(&mut self, parent: usize, name: usize) -> Result<(), String> {
        let bytes = name
            .checked_mul(8)
            .and_then(|n| n.checked_add(parent))
            .and_then(|n| n.checked_add(1))
            .and_then(|n| n.checked_add(self.bytes));
        if self.count >= 10_000 || parent > 32 * 1024 || bytes.is_none_or(|n| n > 4 * 1024 * 1024) {
            return Err("This directory exceeds the browser's entry or memory limit. Choose a smaller directory.".into());
        }
        self.count += 1;
        self.bytes = bytes.unwrap();
        Ok(())
    }
}
struct ListingFailure {
    message: String,
    reset: bool,
}
impl From<String> for ListingFailure {
    fn from(message: String) -> Self {
        Self {
            message,
            reset: false,
        }
    }
}
impl From<&str> for ListingFailure {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}
impl From<russh_sftp::client::error::Error> for ListingFailure {
    fn from(error: russh_sftp::client::error::Error) -> Self {
        use russh_sftp::client::error::Error;
        let reset = match &error {
            Error::Status(status) => matches!(
                status.status_code,
                StatusCode::NoConnection | StatusCode::ConnectionLost
            ),
            Error::Limited(_) => false,
            _ => true,
        };
        Self {
            message: error.to_string(),
            reset,
        }
    }
}
async fn listing(
    sftp: Arc<RawSftpSession>,
    connection: u64,
    request: u64,
    path: RemotePath,
) -> Result<DirectorySnapshot, ListingFailure> {
    tokio::time::timeout(
        Duration::from_secs(5),
        listing_inner(sftp, connection, request, path),
    )
    .await
    .map_err(|_| ListingFailure {
        message: "Remote directory request timed out. Refresh to retry the file channel.".into(),
        reset: true,
    })?
}
async fn listing_inner(
    sftp: Arc<RawSftpSession>,
    connection: u64,
    request: u64,
    path: RemotePath,
) -> Result<DirectorySnapshot, ListingFailure> {
    let canonical = sftp
        .realpath(path.as_bytes())
        .await
        .map_err(ListingFailure::from)?;
    let path = RemotePath::from_bytes(
        canonical
            .files
            .first()
            .ok_or("Server returned no canonical path")?
            .filename
            .as_bytes()
            .to_vec(),
    )?;
    let directory = sftp
        .opendir(path.as_bytes())
        .await
        .map_err(ListingFailure::from)?;
    let handle = directory.handle;
    let result = async {
        let mut entries = Vec::new();
        let mut budget = DirectoryBudget::default();
        loop {
            match sftp.readdir(handle.clone()).await {
                Ok(batch) => {
                    for file in batch.files {
                        let name = file.filename.as_bytes();
                        if name == b"." || name == b".." {
                            continue;
                        }
                        budget.reserve(path.as_bytes().len(), name.len())?;
                        let target = path.child(name)?;
                        let display_name = RemotePath::from_bytes(name.to_vec())?.display();
                        let kind = if file.attrs.is_dir() {
                            EntryKind::Directory
                        } else if file.attrs.is_symlink() {
                            EntryKind::Symlink
                        } else if file.attrs.is_regular() {
                            EntryKind::File
                        } else {
                            EntryKind::Other
                        };
                        entries.push(RemoteEntry {
                            path: target,
                            display_name,
                            kind,
                            size: file.attrs.size,
                        });
                    }
                }
                Err(russh_sftp::client::error::Error::Status(status))
                    if status.status_code == StatusCode::Eof =>
                {
                    break;
                }
                Err(e) => return Err(ListingFailure::from(e)),
            }
        }
        Ok(DirectorySnapshot {
            connection,
            request,
            path,
            entries,
        })
    }
    .await;
    let _ = tokio::time::timeout(Duration::from_millis(250), sftp.close(handle)).await;
    result
}
async fn open_sftp(handle: &client::Handle<Handler>) -> Result<Arc<RawSftpSession>, String> {
    let mut channel = handle
        .channel_open_session()
        .await
        .map_err(|e| e.to_string())?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|e| e.to_string())?;
    confirm(&mut channel, "SFTP subsystem startup").await?;
    let mut sftp = RawSftpSession::new_with_config(
        channel.into_stream(),
        russh_sftp::client::Config {
            max_packet_len: 256 * 1024,
            ..Default::default()
        },
    );
    sftp.set_limits(russh_sftp::client::rawsession::Limits {
        open_handles: Some(8),
        ..Default::default()
    });
    sftp.init()
        .await
        .map_err(|e| format!("SFTP is unavailable: {e}"))?;
    Ok(Arc::new(sftp))
}
async fn read_source_file(
    handle: &client::Handle<Handler>,
    path: &RemotePath,
    limit: usize,
) -> Result<Vec<u8>, String> {
    use russh_sftp::{
        client::error::Error,
        protocol::{FileAttributes, OpenFlags},
    };
    let sftp = open_sftp(handle).await?;
    let file = sftp
        .open(path.as_bytes(), OpenFlags::READ, FileAttributes::default())
        .await
        .map_err(|e| format!("Could not open SFTP source: {e}"))?
        .handle;
    let result = async {
        let attrs = sftp
            .fstat(file.clone())
            .await
            .map_err(|e| e.to_string())?
            .attrs;
        if attrs.size.is_some_and(|size| size > limit as u64) {
            return Err("SFTP source exceeds its size limit".into());
        }
        let mut bytes = Vec::new();
        loop {
            let remaining = limit + 1 - bytes.len();
            match sftp
                .read(
                    file.clone(),
                    bytes.len() as u64,
                    remaining.min(32768) as u32,
                )
                .await
            {
                Ok(data) => {
                    if data.data.is_empty() {
                        return Err("SFTP server returned empty data before EOF".into());
                    }
                    if data.data.len() > limit.saturating_sub(bytes.len()) {
                        return Err("SFTP source exceeds its size limit".into());
                    }
                    bytes.extend(data.data);
                }
                Err(Error::Status(status)) if status.status_code == StatusCode::Eof => break,
                Err(error) => return Err(format!("SFTP source read failed: {error}")),
            }
        }
        Ok(bytes)
    }
    .await;
    let close = sftp
        .close(file)
        .await
        .map_err(|e| format!("SFTP source close failed: {e}"));
    match (result, close) {
        (Ok(bytes), Ok(_)) => Ok(bytes),
        (Err(error), _) => Err(error),
        (_, Err(error)) => Err(error),
    }
}
pub(super) async fn run(
    config: ConnectionConfig,
    purpose: Purpose,
    display: XDisplayBinding,
    mut commands: mpsc::Receiver<ConnectionCommand>,
    mut list_rx: watch::Receiver<Option<(u64, RemotePath)>>,
    mut resize_rx: watch::Receiver<Option<(u32, u32)>>,
    answers: mpsc::Receiver<ChallengeAnswer>,
    events: mpsc::Sender<ConnectionEvent>,
    pending: Arc<AtomicUsize>,
    mut transfers: mpsc::Receiver<crate::remote_files::transfer::Job>,
    lifecycle: Arc<super::lifecycle::ConnectionLifecycle>,
    editor_endpoint: Arc<std::sync::Mutex<crate::remote_files::editor::Endpoint>>,
) -> Result<(), String> {
    let id = config.id;
    let resolved = config::SshOptions::resolve(&config.session, &config.defaults)?;
    if resolved.report.requires_compatibility() {
        let directives = resolved
            .report
            .unsupported
            .iter()
            .map(|f| format!("{} (line {})", f.field, f.line))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "SSH config requires installed OpenSSH compatibility mode: {directives}"
        ));
    }
    let owner_profile = match &purpose {
        Purpose::DesktopGateway { owner_profile, .. } => owner_profile,
        _ => &resolved.owner_profile,
    };
    let s = resolved.session;
    events
        .send(ConnectionEvent::ResolvedEndpoint {
            connection: id,
            host: s.host.clone(),
        })
        .await
        .map_err(|e| e.to_string())?;
    let timeout = Duration::from_secs(resolved.connect_timeout_secs as u64);
    let x11_lease = if s.x11 && matches!(purpose, Purpose::Terminal) {
        Some(match display {
            XDisplayBinding::Ready(display) => XForwardingLease::attach_display(id, display, XTrustMode::Trusted)?,
            _ if s.x11_source == XDisplaySource::System => XForwardingLease::attach_system(id)?,
            XDisplayBinding::Request(reply) => {
                events.send(ConnectionEvent::NeedsXDisplay { connection: id }).await
                    .map_err(|_| "Workspace closed before X11 startup")?;
                let display = reply.await.map_err(|_| "Workspace closed before X11 startup")??;
                XForwardingLease::attach_display(id, display, XTrustMode::Trusted)?
            }
            XDisplayBinding::Unavailable => return Err("The TermCraft X server must be ready before connecting. Choose System display to use your desktop's X server.".into()),
        })
    } else {
        None
    };
    let x11 = x11_lease.as_ref().map(XForwardingLease::forwarder);
    let prompts = Challenges {
        events: events.clone(),
        answers: Arc::new(Mutex::new(answers)),
        lifecycle,
        connection: id,
        context: None,
    };
    let target_key = Arc::new(std::sync::Mutex::new(None));
    let final_role = if matches!(purpose, Purpose::DesktopGateway { .. }) {
        auth::HopRole::Gateway
    } else {
        auth::HopRole::Target
    };

    let make_handler = |profile: &Session, role, verified_key| Handler {
        role,
        verified_key,
        host: profile.host.clone(),
        port: profile.port,
        app: config.app_known_hosts.clone(),
        user: config.user_known_hosts.clone(),
        prompts: prompts.clone(),
        x11: if matches!(role, auth::HopRole::Target) {
            x11.clone()
        } else {
            None
        },
    };
    let mut settings = client::Config {
        keepalive_interval: None,
        nodelay: true,
        channel_buffer_size: 64,
        ..Default::default()
    };
    settings.preferred.compression = if s.compression {
        Cow::Owned(vec![russh::compression::ZLIB, russh::compression::NONE])
    } else {
        Cow::Borrowed(&[russh::compression::NONE])
    };
    let settings = Arc::new(settings);
    events
        .send(ConnectionEvent::State {
            connection: id,
            message: format!("Connecting to {}:{}", s.host, s.port),
        })
        .await
        .map_err(|e| e.to_string())?;
    trait Transport: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send {}
    impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send> Transport for T {}
    let endpoint = resolved.gateways.first().unwrap_or(&s);
    let tcp = super::proxy::connect(
        resolved.proxy.as_ref(),
        endpoint,
        timeout,
        &prompts,
        owner_profile,
    )
    .await?;
    let mut stream: Box<dyn Transport> = Box::new(tcp);
    let mut gateways = Vec::new();
    let mut gateway_keys = Vec::new();
    let mut gateway_users = Vec::new();
    for (index, gateway) in resolved.gateways.iter().enumerate() {
        let key = Arc::new(std::sync::Mutex::new(None));
        let mut connection = handshake(
            client::connect_stream(
                settings.clone(),
                stream,
                make_handler(gateway, auth::HopRole::Gateway, key.clone()),
            ),
            timeout,
            &prompts,
            "Gateway",
        )
        .await?;
        let hop_prompts = prompts.for_hop(
            gateway,
            auth::HopRole::Gateway,
            owner_profile.to_owned(),
            s.gateway_credential_ref.clone(),
            key.clone(),
        );
        gateway_users.push(
            auth::authenticate(
                &mut connection,
                gateway,
                &hop_prompts,
                auth::HopRole::Gateway,
                config.agent.as_ref(),
            )
            .await?,
        );
        gateway_keys.push(
            key.lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone()
                .ok_or("Verified gateway host key is unavailable")?,
        );
        let next = resolved.gateways.get(index + 1).unwrap_or(&s);
        let tunnel = tokio::time::timeout(
            timeout,
            connection.channel_open_direct_tcpip(
                next.host.trim_start_matches('[').trim_end_matches(']'),
                next.port as u32,
                "127.0.0.1",
                0,
            ),
        )
        .await
        .map_err(|_| "Gateway tunnel timed out")?
        .map_err(|e| e.to_string())?;
        stream = Box::new(tunnel.into_stream());
        gateways.push(connection);
    }
    let mut handle = handshake(
        client::connect_stream(
            settings,
            stream,
            make_handler(&s, final_role, target_key.clone()),
        ),
        timeout,
        &prompts,
        match final_role {
            auth::HopRole::Gateway => "Gateway",
            auth::HopRole::Target => "Target",
        },
    )
    .await?;
    let target_prompts = prompts.for_hop(
        &s,
        final_role,
        owner_profile.to_owned(),
        match final_role {
            auth::HopRole::Gateway => s.gateway_credential_ref.clone(),
            auth::HopRole::Target => s.credential_ref.clone(),
        },
        target_key.clone(),
    );
    let user = auth::authenticate(
        &mut handle,
        &s,
        &target_prompts,
        final_role,
        config.agent.as_ref(),
    )
    .await?;
    prompts.lifecycle.ensure_active()?;
    let clock = config
        .clock
        .unwrap_or_else(|| Arc::new(super::clock::TokioClock));
    let interval = resolved
        .keepalive_secs
        .map(|n| Duration::from_secs(n as u64));
    let sleep = || -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        match interval {
            Some(duration) => clock.sleep(duration),
            None => Box::pin(std::future::pending()),
        }
    };
    if let Purpose::DesktopGateway {
        target,
        ready_stream,
        mut retries,
        ..
    } = purpose
    {
        let (stream, mut bridge) = desktop_gateway::socket_pair()
            .await
            .map_err(|error| format!("Could not create desktop gateway stream: {error}"))?;
        let mut ready = Some((ready_stream, stream));
        loop {
            let channel = tokio::time::timeout(
                timeout,
                handle.channel_open_direct_tcpip(
                    target.host.trim_start_matches('[').trim_end_matches(']'),
                    u32::from(target.port),
                    "127.0.0.1",
                    0,
                ),
            )
            .await
            .map_err(|_| format!("Gateway timed out opening {}:{}", target.host, target.port))?
            .map_err(|error| {
                format!(
                    "Gateway could not open {}:{}: {error}",
                    target.host, target.port
                )
            })?;
            let mut socket =
                tokio::net::TcpStream::from_std(bridge).map_err(|error| error.to_string())?;
            let mut channel = channel.into_stream();
            if let Some((ready_stream, stream)) = ready.take() {
                ready_stream
                    .send(stream)
                    .map_err(|_| "Gateway lease closed")?;
                events
                    .send(ConnectionEvent::Connected { connection: id })
                    .await
                    .map_err(|_| "Gateway lease closed")?;
            }

            let next = {
                let copy = tokio::io::copy_bidirectional(&mut socket, &mut channel);
                tokio::pin!(copy);
                loop {
                    tokio::select! {
                        result = &mut copy => {
                            if retries.is_none() {
                                return result.map(|_| ()).map_err(|error| format!("Gateway stream failed: {error}"));
                            }
                            break None;
                        }
                        next = receive_retry(&mut retries), if retries.is_some() => {
                            break Some(next.ok_or("Gateway retry connection closed")?);
                        }
                        result = &mut handle => return gateway_disconnected(result),
                        _ = sleep() => send_keepalive(&handle, &gateways, timeout).await?,
                    }
                }
            };
            drop(channel);
            drop(socket);
            bridge = match next {
                Some(next) => next,
                None => loop {
                    tokio::select! {
                        next = receive_retry(&mut retries) => {
                            break next.ok_or("Gateway retry connection closed")?;
                        }
                        result = &mut handle => return gateway_disconnected(result),
                        _ = sleep() => send_keepalive(&handle, &gateways, timeout).await?,
                    }
                },
            };
        }
    }
    {
        let mut endpoint = editor_endpoint.lock().unwrap_or_else(|p| p.into_inner());
        endpoint.target_key = target_key
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
            .ok_or("Verified target host key is unavailable")?;
        endpoint.host = s.host.clone();
        endpoint.port = s.port;
        endpoint.jump = s.jump.clone();
        endpoint.gateway_key = if gateway_keys.is_empty() {
            String::new()
        } else {
            serde_json::to_string(&gateway_keys).map_err(|e| e.to_string())?
        };
        endpoint.user = user;
        endpoint.gateway_user = if gateway_users.is_empty() {
            String::new()
        } else {
            serde_json::to_string(&gateway_users).map_err(|e| e.to_string())?
        };
        endpoint.authenticated = true;
    }
    let handle = Arc::new(handle);
    if let Purpose::ReadFile {
        request,
        path,
        limit,
    } = purpose
    {
        let bytes = tokio::time::timeout(
            Duration::from_secs(30),
            read_source_file(&handle, &path, limit),
        )
        .await
        .map_err(|_| "SFTP source read timed out")??;
        events
            .send(ConnectionEvent::FileContents {
                connection: id,
                request,
                bytes,
            })
            .await
            .map_err(|_| "Source reader closed")?;
        return Ok(());
    }
    let mut shell = handle
        .channel_open_session()
        .await
        .map_err(|e| e.to_string())?;
    shell
        .request_pty(true, "xterm-256color", 110, 30, 0, 0, &[])
        .await
        .map_err(|e| e.to_string())?;
    // Want-reply requests send success/failure on the channel: confirm them explicitly.
    confirm(&mut shell, "terminal allocation").await?;
    if let Some(forwarder) = &x11 {
        shell
            .request_x11(
                true,
                false,
                "MIT-MAGIC-COOKIE-1",
                forwarder.request_cookie()?,
                forwarder.screen(),
            )
            .await
            .map_err(|e| e.to_string())?;
        confirm(&mut shell, "X11 forwarding").await?;
    }
    if s.command.is_empty() {
        shell.request_shell(true).await.map_err(|e| e.to_string())?;
    } else {
        shell
            .exec(true, s.command.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
    }
    confirm(&mut shell, "shell startup").await?;
    // The desktop gets its own channel, never bytes injected into the terminal. Complete
    // the exec request before Connected, which releases the UI's startup macro queue.
    let mut environment_job = JoinSet::new();
    match crate::x11::remote_environment::resolve_environment(&s.remote_environment) {
        Ok(None) => {}
        resolved => {
            let mut environment_channel = None;
            let launch = async {
                let command = resolved?.ok_or("No remote environment selected")?;
                let forwarder = x11
                    .as_ref()
                    .ok_or("Enable trusted X11 forwarding for the remote environment")?;
                environment_channel = Some(
                    handle
                        .channel_open_session()
                        .await
                        .map_err(|e| e.to_string())?,
                );
                let channel = environment_channel.as_mut().expect("opened above");
                channel
                    .request_x11(
                        true,
                        false,
                        "MIT-MAGIC-COOKIE-1",
                        forwarder.request_cookie()?,
                        forwarder.screen(),
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                confirm(channel, "remote environment X11 forwarding").await?;
                channel
                    .exec(true, command.ssh_exec().as_bytes())
                    .await
                    .map_err(|e| e.to_string())?;
                confirm(channel, "remote environment launch").await?;
                Ok::<_, String>(())
            };
            // russh dispatches every channel through one packet loop. Keep the shell
            // drained while waiting for another channel's replies: a startup banner
            // can otherwise fill its bounded receiver and block those replies.
            let launch_result = {
                let launch_timeout = tokio::time::timeout(Duration::from_secs(10), launch);
                tokio::pin!(launch_timeout);
                loop {
                    tokio::select! {
                        result = &mut launch_timeout => break result,
                        message = shell.wait() => match message {
                            Some(ChannelMsg::Data { data }) | Some(ChannelMsg::ExtendedData { data, .. }) => {
                                for chunk in data.chunks(16 * 1024) {
                                    events.send(ConnectionEvent::Output { connection: id, bytes: chunk.to_vec() }).await.map_err(|e| e.to_string())?;
                                }
                            },
                            Some(ChannelMsg::ExitStatus { exit_status }) => return Err(format!("Process ended with status {exit_status}")),
                            Some(ChannelMsg::Close) | None => return Ok(()),
                            _ => {},
                        },
                    }
                }
            };
            match launch_result {
                Ok(Ok(())) => {
                    let mut channel = environment_channel.take().expect("launch opened a channel");
                    let events = events.clone();
                    environment_job.spawn(async move {
                        loop {
                            match channel.wait().await {
                                Some(ChannelMsg::Data { data })
                                | Some(ChannelMsg::ExtendedData { data, .. }) => {
                                    for chunk in data.chunks(16 * 1024) {
                                        events
                                            .send(ConnectionEvent::Output {
                                                connection: id,
                                                bytes: chunk.to_vec(),
                                            })
                                            .await
                                            .map_err(|_| "Workspace closed".to_string())?;
                                    }
                                }
                                Some(ChannelMsg::ExitStatus { exit_status })
                                    if exit_status != 0 =>
                                {
                                    return Err(format!(
                                        "process exited with status {exit_status}"
                                    ));
                                }
                                Some(ChannelMsg::ExitSignal { .. }) => {
                                    return Err("process terminated by a signal".into());
                                }
                                Some(ChannelMsg::Close) | None => return Ok(()),
                                _ => {}
                            }
                        }
                    });
                }
                result => {
                    if let Some(channel) = environment_channel.take() {
                        let _ = tokio::time::timeout(Duration::from_secs(1), channel.close()).await;
                    }
                    let error = match result {
                        Ok(Err(e)) => e,
                        _ => "launch timed out".into(),
                    };
                    report_environment_failure(&events, id, &error).await?;
                }
            }
        }
    }
    events
        .send(ConnectionEvent::Connected { connection: id })
        .await
        .map_err(|e| e.to_string())?;
    let (mut reader, writer) = shell.split();
    let writer = Arc::new(writer);
    let resize_writer = writer.clone();
    let mut resizing = tokio::spawn(async move {
        loop {
            resize_rx
                .changed()
                .await
                .map_err(|_| "SSH resize controls closed".to_string())?;
            let size = *resize_rx.borrow_and_update();
            if let Some((cols, rows)) = size {
                resize_writer
                    .window_change(cols, rows, 0, 0)
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }
        #[allow(unreachable_code)]
        Ok::<(), String>(())
    });
    let mut writing = tokio::spawn(async move {
        while let Some(command) = commands.recv().await {
            match command {
                ConnectionCommand::Input(bytes) => {
                    let result = writer.data(bytes.as_slice()).await;
                    pending.fetch_sub(bytes.len(), Ordering::AcqRel);
                    result.map_err(|e| e.to_string())?;
                }
                _ => {} // Other controls use independent coalescing channels.
            }
        }
        Ok::<(), String>(())
    });
    let initial = if s.directory.is_empty() {
        RemotePath::from_bytes(b".".to_vec())?
    } else {
        RemotePath::from_bytes(s.directory.as_bytes().to_vec())?
    };
    let mut jobs: JoinSet<(u64, Result<DirectorySnapshot, ListingFailure>)> = JoinSet::new();
    let mut file_jobs = JoinSet::new();
    let mut file_pending = std::collections::VecDeque::new();
    let mut file_controls_open = true;
    let mut sftp: Option<Arc<RawSftpSession>> = None;
    let mut pending_listing = Some((0, initial));
    // SFTP opening/listing runs independently of terminal reads, writes and resizes.
    let sftp_open = open_sftp(&handle);
    tokio::pin!(sftp_open);
    let scp = (s.remote_backend == crate::remote_files::capabilities::RemoteBackendKind::Scp)
        .then(|| crate::remote_files::scp::ScpBackend::new(handle.clone()));
    let mut opening = scp.is_none();
    // SCP has no subsystem-opening future to wake the select loop: dispatch its initial listing now.
    if let Some(backend) = &scp {
        if let Some((request, path)) = pending_listing.take() {
            let backend = backend.clone();
            jobs.spawn(async move {
                (
                    request,
                    backend
                        .list(id, request, path)
                        .await
                        .map_err(|message| ListingFailure {
                            message,
                            reset: false,
                        }),
                )
            });
        }
    }
    let mut keepalive = sleep();
    loop {
        tokio::select! {
            result=environment_job.join_next(),if !environment_job.is_empty()=>{
                match result {
                    Some(Ok(Err(error)))=>report_environment_failure(&events,id,&error).await?,
                    Some(Err(error))=>report_environment_failure(&events,id,&error.to_string()).await?,
                    _=>{},
                }
            }
            _=&mut keepalive=>{
                send_keepalive(&handle, &gateways, timeout).await?;
                keepalive=sleep();
            }
            job=transfers.recv(),if file_controls_open && file_pending.len()<16=>{
                if let Some(job)=job {file_pending.push_back(job);}else{file_controls_open=false;}
            }
            _=file_jobs.join_next(),if !file_jobs.is_empty()=>{},
            result=&mut sftp_open,if opening=>{
                opening=false;
                match result {
                    Ok(session)=>sftp=Some(session),
                    Err(message)=>{let request=pending_listing.as_ref().map(|p|p.0).unwrap_or(0);events.send(ConnectionEvent::FilesError {connection:id,request,message}).await.map_err(|e|e.to_string())?;}
                }
            }
            result=&mut writing=>return result.map_err(|e|e.to_string())?,
            result=&mut resizing=>return result.map_err(|e|e.to_string())?,
            changed=list_rx.changed()=>{
                changed.map_err(|_|"SSH input connection closed")?;
                if let Some((request,path))=list_rx.borrow_and_update().clone() {
                    if scp.is_none() && sftp.is_none() && !opening {
                        sftp_open.set(open_sftp(&handle));opening=true;
                    }
                    pending_listing=Some((request,path));
                }
            }
            result=jobs.join_next(),if !jobs.is_empty()=>if let Some(result)=result {
                let (request,outcome)=result.map_err(|e|e.to_string())?;
                let event=match outcome {
                    Ok(snapshot)=>{
                        if let Some(capabilities) = scp.as_ref().and_then(|backend| backend.capabilities()) {
                            events.send(ConnectionEvent::FilesCapabilities { connection: id, capabilities }).await.map_err(|e| e.to_string())?;
                        }
                        ConnectionEvent::Directory(snapshot)
                    },
                    Err(failure)=>{if failure.reset{sftp=None;} ConnectionEvent::FilesError{connection:id,request,message:failure.message}}
                };
                events.send(event).await.map_err(|e|e.to_string())?;
            },
            message=reader.wait()=>match message {
                Some(ChannelMsg::Data {data})|Some(ChannelMsg::ExtendedData {data,..})=>{
                    for chunk in data.chunks(16*1024){events.send(ConnectionEvent::Output{connection:id,bytes:chunk.to_vec()}).await.map_err(|e|e.to_string())?;}
                }
                Some(ChannelMsg::ExitStatus {exit_status})=>return Err(format!("Process ended with status {exit_status}")),
                Some(ChannelMsg::Close)|None=>return Ok(()),
                _=>{}
            },
        }
        if let Some(backend) = &scp {
            if file_jobs.is_empty() {
                if let Some(job) = file_pending.pop_front() {
                    file_jobs.spawn(crate::remote_files::transfer::run_scp(job, backend.clone()));
                }
            }
        } else if let Some(session) = &sftp {
            while file_jobs.len() < 1 {
                let Some(job) = file_pending.pop_front() else {
                    break;
                };
                file_jobs.spawn(crate::remote_files::transfer::run(job, session.clone()));
            }
        } else if !opening {
            for job in file_pending.drain(..) {
                job.finish(crate::remote_files::transfer::TransferOutcome::Failed(
                    "SFTP is unavailable. Refresh Remote files and retry the transfer.".into(),
                ));
            }
        }
        if jobs.is_empty() {
            if let Some(backend) = &scp {
                if let Some((request, path)) = pending_listing.take() {
                    let backend = backend.clone();
                    jobs.spawn(async move {
                        (
                            request,
                            backend.list(id, request, path).await.map_err(|message| {
                                ListingFailure {
                                    message,
                                    reset: false,
                                }
                            }),
                        )
                    });
                }
            } else if let Some(session) = &sftp {
                if let Some((request, path)) = pending_listing.take() {
                    let session = session.clone();
                    jobs.spawn(async move { (request, listing(session, id, request, path).await) });
                }
            }
        }
    }
}
fn gateway_disconnected(result: Result<(), russh::Error>) -> Result<(), String> {
    let message = match result {
        Ok(()) => "SSH gateway disconnected".into(),
        Err(error) => format!("SSH gateway disconnected: {error}"),
    };
    Err(message)
}

async fn receive_retry(
    retries: &mut Option<mpsc::Receiver<std::net::TcpStream>>,
) -> Option<std::net::TcpStream> {
    retries.as_mut()?.recv().await
}

async fn send_keepalive(
    handle: &client::Handle<Handler>,
    gateways: &[client::Handle<Handler>],
    timeout: Duration,
) -> Result<(), String> {
    tokio::time::timeout(timeout, handle.send_ping())
        .await
        .map_err(|_| "SSH keepalive timed out")?
        .map_err(|error| error.to_string())?;
    for gateway in gateways {
        tokio::time::timeout(timeout, gateway.send_ping())
            .await
            .map_err(|_| "Gateway keepalive timed out")?
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

async fn confirm(shell: &mut russh::Channel<client::Msg>, operation: &str) -> Result<(), String> {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match shell.wait().await {
                Some(ChannelMsg::Success) => return Ok(()),
                Some(ChannelMsg::Failure) | Some(ChannelMsg::Close) | None => {
                    return Err(format!("Server rejected {operation}"));
                }
                Some(ChannelMsg::Data { .. }) | Some(ChannelMsg::ExtendedData { .. }) => {
                    return Err(format!(
                        "Server sent terminal output before confirming {operation}"
                    ));
                }
                _ => {} // Window adjustments are independent of request confirmation.
            }
        }
    })
    .await
    .map_err(|_| format!("Server timed out during {operation}"))?
}

#[cfg(test)]
mod budget_tests {
    use super::*;
    #[test]
    fn directory_budget_bounds_retained_bytes_before_allocating_paths() {
        let mut budget = DirectoryBudget::default();
        let mut accepted = 0;
        while budget.reserve(32 * 1024, 256).is_ok() {
            accepted += 1;
        }
        assert!(accepted < 200);
        assert!(budget.bytes <= 4 * 1024 * 1024);
        assert!(
            DirectoryBudget::default()
                .reserve(usize::MAX, usize::MAX)
                .is_err()
        );
    }
}

/// Bound network handshake work while excluding time spent answering the host-key form.
async fn handshake<T>(
    future: impl std::future::Future<Output = Result<T, russh::Error>>,
    timeout: Duration,
    prompts: &Challenges,
    label: &str,
) -> Result<T, String> {
    tokio::pin!(future);
    let mut waiting = prompts.lifecycle.prompt_state();
    let mut remaining = timeout;
    loop {
        if *waiting.borrow_and_update() {
            tokio::select! {
                result=&mut future=>return result.map_err(|e|format!("{label} handshake/host key verification failed: {e}")),
                changed=waiting.changed()=>{changed.map_err(|_|"SSH handshake cancelled")?;},
            }
        } else {
            let started = tokio::time::Instant::now();
            tokio::select! {
                result=&mut future=>return result.map_err(|e|format!("{label} handshake/host key verification failed: {e}")),
                _=tokio::time::sleep(remaining)=>return Err(format!("{label} SSH handshake timed out")),
                changed=waiting.changed()=>{changed.map_err(|_|"SSH handshake cancelled")?;},
            }
            remaining = remaining.saturating_sub(started.elapsed());
        }
    }
}

async fn report_environment_failure(
    events: &mpsc::Sender<ConnectionEvent>,
    id: ConnectionId,
    error: &str,
) -> Result<(), String> {
    let message = format!("Remote environment failed: {error}. The SSH shell is still available.");
    events
        .send(ConnectionEvent::Output {
            connection: id,
            bytes: format!("\r\n{message}\r\n").into_bytes(),
        })
        .await
        .map_err(|_| "Workspace closed".to_string())?;
    events
        .send(ConnectionEvent::State {
            connection: id,
            message,
        })
        .await
        .map_err(|_| "Workspace closed".to_string())
}
