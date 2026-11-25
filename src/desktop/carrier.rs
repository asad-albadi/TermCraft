//! Authenticated loopback adapter for the separate RDP helper.
use crate::ssh::desktop_gateway::GatewayRetry;
use std::{
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use subtle::ConstantTimeEq;
use tokio::{io::AsyncReadExt, sync::watch};

/// Bridges an existing protocol stream to a local RDP helper. Keep the relay
/// for the lifetime of the helper. Dropping it cancels accept and socket I/O.
pub struct SocketRelay {
    address: SocketAddr,
    token: [u8; 32],
    cancel: watch::Sender<bool>,
    finished: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
}

impl SocketRelay {
    /// Uses an unguessable token for each helper socket, including fresh
    /// target attempts made after a certificate decision.
    pub fn for_rdp(
        stream: TcpStream,
        retry: Option<GatewayRetry>,
        timeout: Duration,
    ) -> Result<Self, String> {
        if timeout.is_zero() {
            return Err("Local connection timeout must be greater than zero".into());
        }
        let mut token = [0; 32];
        getrandom::fill(&mut token)
            .map_err(|error| format!("Could not create RDP carrier token: {error}"))?;
        let listener =
            TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|error| error.to_string())?;
        let address = listener.local_addr().map_err(|error| error.to_string())?;
        listener
            .set_nonblocking(true)
            .map_err(|error| error.to_string())?;
        stream
            .set_nonblocking(true)
            .map_err(|error| error.to_string())?;

        let (cancel, mut cancelled) = watch::channel(false);
        let finished = Arc::new(AtomicBool::new(false));
        let worker_finished = finished.clone();
        let error = Arc::new(Mutex::new(None));
        let worker_error = error.clone();
        std::thread::Builder::new()
            .name("termcraft-local-protocol-relay".into())
            .spawn(move || {
                let result = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|error| error.to_string())
                    .and_then(|runtime| {
                        runtime.block_on(async move {
                            tokio::select! {
                                biased;
                                _ = cancelled.changed() => Ok(()),
                                result = relay(listener, stream, retry, token, timeout) => result,
                            }
                        })
                    });
                *worker_error.lock().unwrap_or_else(|p| p.into_inner()) = result.err();
                worker_finished.store(true, Ordering::Release);
            })
            .map_err(|error| error.to_string())?;
        Ok(Self {
            address,
            token,
            cancel,
            finished,
            error,
        })
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }

    pub fn token(&self) -> [u8; 32] {
        self.token
    }

    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }

    pub fn cancel(&self) {
        let _ = self.cancel.send(true);
    }

    pub fn take_error(&self) -> Option<String> {
        self.error.lock().unwrap_or_else(|p| p.into_inner()).take()
    }
}

impl Drop for SocketRelay {
    fn drop(&mut self) {
        self.cancel();
    }
}

async fn relay(
    listener: TcpListener,
    stream: TcpStream,
    retry: Option<GatewayRetry>,
    token: [u8; 32],
    timeout: Duration,
) -> Result<(), String> {
    let listener =
        tokio::net::TcpListener::from_std(listener).map_err(|error| error.to_string())?;
    if let Some(retry) = retry {
        tokio::select! {
            _ = retry.closed() => Ok(()),
            result = relay_rdp(listener, stream, &retry, token, timeout) => result,
        }
    } else {
        let (mut client, mut target) = accept_first(&listener, stream, &token, timeout).await?;
        drop(listener);
        tokio::io::copy_bidirectional(&mut client, &mut target)
            .await
            .map(|_| ())
            .map_err(|error| format!("Local protocol relay failed: {error}"))
    }
}

async fn accept_first(
    listener: &tokio::net::TcpListener,
    stream: TcpStream,
    token: &[u8; 32],
    timeout: Duration,
) -> Result<(tokio::net::TcpStream, tokio::net::TcpStream), String> {
    let client = tokio::time::timeout(timeout, accept_helper(listener, token))
        .await
        .map_err(|_| "Local protocol helper connection timed out")??;
    let target = tokio::net::TcpStream::from_std(stream).map_err(|error| error.to_string())?;
    Ok((client, target))
}

async fn accept_helper(
    listener: &tokio::net::TcpListener,
    token: &[u8; 32],
) -> Result<tokio::net::TcpStream, String> {
    loop {
        let (mut client, _) = listener.accept().await.map_err(|error| error.to_string())?;
        let mut candidate = [0; 32];
        let received = tokio::time::timeout(
            Duration::from_millis(250),
            client.read_exact(&mut candidate),
        )
        .await;
        if !matches!(received, Ok(Ok(_))) || !bool::from(candidate.ct_eq(token)) {
            continue;
        }
        client
            .set_nodelay(true)
            .map_err(|error| error.to_string())?;
        return Ok(client);
    }
}

async fn relay_rdp(
    listener: tokio::net::TcpListener,
    stream: TcpStream,
    retry: &GatewayRetry,
    token: [u8; 32],
    timeout: Duration,
) -> Result<(), String> {
    let (mut client, mut target) = accept_first(&listener, stream, &token, timeout).await?;
    // The RDP helper has at most three connection attempts, with an
    // interactive certificate decision between attempts.
    for _ in 0..2 {
        complete_rdp_attempt(client, target).await;
        client = accept_helper(&listener, &token).await?;
        let stream = retry.connect().await?;
        stream
            .set_nonblocking(true)
            .map_err(|error| error.to_string())?;
        target = tokio::net::TcpStream::from_std(stream).map_err(|error| error.to_string())?;
    }
    drop(listener);
    complete_rdp_attempt(client, target).await;
    Ok(())
}

async fn complete_rdp_attempt(client: tokio::net::TcpStream, target: tokio::net::TcpStream) {
    // EOF in either direction ends an RDP attempt. In particular, a gateway
    // target that never closes after TLS failure must not block reconnect.
    let (mut client_read, mut client_write) = client.into_split();
    let (mut target_read, mut target_write) = target.into_split();
    tokio::select! {
        _ = tokio::io::copy(&mut client_read, &mut target_write) => {}
        _ = tokio::io::copy(&mut target_read, &mut client_write) => {}
    }
}
