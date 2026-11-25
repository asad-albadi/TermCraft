//! A desktop or Telnet byte stream carried by the embedded SSH worker.
use super::{ChallengeAnswer, ConnectionConfig, ConnectionEvent, ConnectionHandle, Purpose};
use crate::model::{Session, SshEngine, validate_host};
use std::{
    net::{Ipv4Addr, TcpStream},
    time::Duration,
};

/// The protocol endpoint as seen by the SSH server. Use this identity for
/// protocol authentication and certificate validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
}

/// The final SSH gateway profile, including its usual jump and proxy settings.
pub type GatewayConfig = ConnectionConfig;

/// Derive the SSH gateway profile and independent protocol endpoint from a
/// session's jump list. The last hop is the desktop or Telnet egress point.
pub fn profile(target: &Session) -> Result<(Session, Endpoint), String> {
    let hops = target.jump.split(',').map(str::trim).collect::<Vec<_>>();
    if hops.iter().any(|hop| hop.is_empty()) {
        return Err("SSH gateway list contains an empty hop.".into());
    }
    if hops.len() > usize::from(target.ssh_options.jump_limit) {
        return Err("SSH gateway list exceeds the selected hop limit.".into());
    }
    let (last, prior) = hops.split_last().expect("split always returns one item");
    let mut gateway = target.clone();
    gateway.jump = (*last).to_owned();
    gateway.user.clear();
    gateway = super::forward::jump_session(&gateway)?;
    gateway.protocol = "SSH".into();
    gateway.ssh_engine = SshEngine::Embedded;
    gateway.jump = prior.join(",");
    gateway.credential_ref = None;
    Ok((
        gateway,
        Endpoint {
            host: target.host.clone(),
            port: target.port,
        },
    ))
}

/// Allows a protocol helper to reopen the same destination on an existing
/// gateway lease, for example after the user approves an RDP certificate.
#[derive(Clone)]
pub struct GatewayRetry {
    requests: tokio::sync::mpsc::Sender<TcpStream>,
}

impl GatewayRetry {
    pub(crate) async fn closed(&self) {
        self.requests.closed().await;
    }

    pub(crate) async fn connect(&self) -> Result<TcpStream, String> {
        let (stream, bridge) = socket_pair()
            .await
            .map_err(|error| format!("Could not reopen gateway stream: {error}"))?;
        self.requests
            .send(bridge)
            .await
            .map_err(|_| "The SSH gateway lease has closed")?;
        Ok(stream)
    }
}

/// Own this for as long as the protocol uses the forwarded stream. Dropping it
/// cancels this SSH attempt and closes the corresponding direct-tcpip channel.
pub struct GatewayLease {
    handle: ConnectionHandle,
    target: Endpoint,
    stream: tokio::sync::oneshot::Receiver<TcpStream>,
    ready: bool,
    retry: Option<GatewayRetry>,
}

impl GatewayLease {
    /// Starts SSH asynchronously. Poll for host-key and authentication
    /// challenges until the worker reports Connected or Exited.
    pub fn open(config: GatewayConfig, target: Endpoint) -> Result<Self, String> {
        let owner_profile = super::auth_profile_binding(&config.session);
        Self::start(config, target, owner_profile, false)
    }

    /// Keeps the authenticated SSH connection available for fresh channels
    /// until the lease is dropped.
    pub fn open_retryable(config: GatewayConfig, target: Endpoint) -> Result<Self, String> {
        let owner_profile = super::auth_profile_binding(&config.session);
        Self::start(config, target, owner_profile, true)
    }

    /// Opens the gateway for a saved protocol definition. The saved
    /// definition owns credentials while challenge identities use each hop.
    pub fn open_for_session(
        mut config: GatewayConfig,
        target_profile: &Session,
        retryable: bool,
    ) -> Result<Self, String> {
        let owner_profile = super::auth_profile_binding(target_profile);
        let (gateway, target) = profile(target_profile)?;
        config.session = gateway;
        Self::start(config, target, owner_profile, retryable)
    }

    fn start(
        config: GatewayConfig,
        target: Endpoint,
        owner_profile: String,
        retryable: bool,
    ) -> Result<Self, String> {
        validate_host(&target.host)?;
        if target.port == 0 {
            return Err("Choose a gateway target port from 1 to 65535.".into());
        }

        let (ready_stream, stream) = tokio::sync::oneshot::channel();
        let (retry, retries) = if retryable {
            let (requests, receiver) = tokio::sync::mpsc::channel(1);
            (Some(GatewayRetry { requests }), Some(receiver))
        } else {
            (None, None)
        };
        let handle = ConnectionHandle::connect_purpose(
            config,
            Purpose::DesktopGateway {
                target: target.clone(),
                ready_stream,
                retries,
                owner_profile,
            },
        )?;
        Ok(Self {
            handle,
            target,
            stream,
            ready: false,
            retry,
        })
    }

    pub fn retry_connector(&self) -> Option<GatewayRetry> {
        self.retry.clone()
    }

    pub fn target(&self) -> &Endpoint {
        &self.target
    }

    pub fn poll(&mut self) -> Vec<ConnectionEvent> {
        let events = self.handle.poll();
        self.ready |= events
            .iter()
            .any(|event| matches!(event, ConnectionEvent::Connected { .. }));
        events
    }

    pub fn answer(&self, answer: ChallengeAnswer) -> Result<(), String> {
        self.handle.answer(answer)
    }

    /// Takes the connected socket once. The lease retains ownership of the
    /// other end, so the socket cannot outlive the forwarded channel.
    pub fn take_stream(&mut self) -> Option<TcpStream> {
        if self.ready && !self.handle.lifecycle.cancelled() {
            self.stream.try_recv().ok()
        } else {
            None
        }
    }

    pub fn cancel(&self) {
        self.handle.cancel();
    }

    pub fn is_finished(&self) -> bool {
        self.handle.is_finished()
    }
}

/// Create connected sockets for synchronous protocol adapters on every
/// platform. The kernel listener exists only during pairing.
pub(super) async fn socket_pair() -> std::io::Result<(TcpStream, TcpStream)> {
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
    let stream = tokio::time::timeout(
        Duration::from_secs(1),
        tokio::net::TcpStream::connect(listener.local_addr()?),
    )
    .await??;
    let (bridge, peer) = tokio::time::timeout(Duration::from_secs(1), listener.accept()).await??;
    if peer != stream.local_addr()? {
        return Err(std::io::Error::other(
            "Unexpected desktop gateway socket peer",
        ));
    }
    drop(listener);
    stream.set_nodelay(true)?;
    bridge.set_nodelay(true)?;
    let stream = stream.into_std()?;
    stream.set_nonblocking(false)?;
    Ok((stream, bridge.into_std()?))
}
