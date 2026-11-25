//! Embedded SSH transport. No protocol I/O runs in the UI thread.
mod auth;
pub mod desktop_gateway;
mod forward;
mod host_keys;
mod lifecycle;
pub mod shell_integration;
mod worker;
pub(crate) mod x11;
use crate::{
    model::{Session, validate_host, validate_user},
    remote_files::{DirectorySnapshot, RemotePath},
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use tokio::sync::{mpsc, watch};
use zeroize::Zeroizing;
pub type ConnectionId = u64;
pub type RequestId = u64;
pub struct ConnectionConfig {
    pub id: ConnectionId,
    pub agent: Option<crate::credentials::agent::AgentEndpoint>,
    pub defaults: crate::model::SshDefaults,
    pub clock: Option<Arc<dyn clock::Clock>>,
    pub session: Session,
    pub app_known_hosts: PathBuf,
    pub user_known_hosts: Vec<PathBuf>,
}
type DisplayResult = Result<crate::x11::XClientDisplay, String>;

pub(super) enum XDisplayBinding {
    Unavailable,
    Ready(crate::x11::XClientDisplay),
    Request(tokio::sync::oneshot::Receiver<DisplayResult>),
}

pub(super) enum Purpose {
    Terminal,
    DesktopGateway {
        target: desktop_gateway::Endpoint,
        ready_stream: tokio::sync::oneshot::Sender<std::net::TcpStream>,
        retries: Option<mpsc::Receiver<std::net::TcpStream>>,
        owner_profile: String,
    },
    ReadFile {
        request: u64,
        path: RemotePath,
        limit: usize,
    },
}
pub enum ConnectionCommand {
    Input(Vec<u8>),
    Resize {
        cols: u32,
        rows: u32,
    },
    List {
        request: RequestId,
        path: RemotePath,
    },
}
#[derive(Clone, Debug)]
pub struct Prompt {
    pub label: String,
    pub echo: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrustDecision {
    Once,
    Remember,
    Reject,
}
pub struct ChallengeAnswer {
    pub id: u64,
    pub values: Vec<Zeroizing<String>>,
    pub trust: Option<TrustDecision>,
}
impl std::fmt::Debug for ChallengeAnswer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChallengeAnswer")
            .field("id", &self.id)
            .field("values", &"[redacted]")
            .finish()
    }
}
#[derive(Clone, Debug)]
pub enum ConnectionEvent {
    FileContents {
        connection: u64,
        request: u64,
        bytes: Vec<u8>,
    },
    FilesCapabilities {
        connection: u64,
        capabilities: crate::remote_files::capabilities::RemoteCapabilities,
    },
    State {
        connection: u64,
        message: String,
    },
    NeedsXDisplay {
        connection: u64,
    },
    ResolvedEndpoint {
        connection: u64,
        host: String,
    },
    Connected {
        connection: u64,
    },
    Challenge {
        connection: u64,
        id: u64,
        title: String,
        prompts: Vec<Prompt>,
        host_key: bool,
        credential: Option<ChallengeCredential>,
    },
    Output {
        connection: u64,
        bytes: Vec<u8>,
    },
    Directory(DirectorySnapshot),
    FilesError {
        connection: u64,
        request: u64,
        message: String,
    },
    Exited {
        connection: u64,
        message: String,
    },
}
pub struct ConnectionHandle {
    input_enabled: bool,
    lifecycle: Arc<lifecycle::ConnectionLifecycle>,
    pub transfers: crate::remote_files::transfer::TransferManager,
    commands: mpsc::Sender<ConnectionCommand>,
    answers: mpsc::Sender<ChallengeAnswer>,
    events: mpsc::Receiver<ConnectionEvent>,
    pending_output: Option<(u64, Vec<u8>, usize)>,
    cancel: watch::Sender<bool>,
    finished: Arc<AtomicBool>,
    pending_input: Arc<AtomicUsize>,
    list_requests: watch::Sender<Option<(u64, RemotePath)>>,
    resize_requests: watch::Sender<Option<(u32, u32)>>,
    final_event: Arc<std::sync::Mutex<Option<ConnectionEvent>>>,
    x11_reply: Option<tokio::sync::oneshot::Sender<DisplayResult>>,
}
impl ConnectionHandle {
    pub fn connect(config: ConnectionConfig) -> Result<Self, String> {
        Self::connect_purpose(config, Purpose::Terminal)
    }
    pub fn connect_with_x11(
        config: ConnectionConfig,
        display: crate::x11::XClientDisplay,
    ) -> Result<Self, String> {
        display.ensure_running()?;
        Self::connect_purpose_with_display(
            config,
            Purpose::Terminal,
            XDisplayBinding::Ready(display),
        )
    }
    /// Resolve the profile once, then ask the workspace for a display only if needed.
    pub fn connect_with_managed_x11(config: ConnectionConfig) -> Result<Self, String> {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let mut handle = Self::connect_purpose_with_display(
            config,
            Purpose::Terminal,
            XDisplayBinding::Request(receiver),
        )?;
        handle.x11_reply = Some(sender);
        Ok(handle)
    }

    pub fn supply_x_display(&mut self, display: DisplayResult) {
        if let Some(reply) = self.x11_reply.take() {
            let _ = reply.send(display);
        }
    }

    pub fn read_file(
        mut config: ConnectionConfig,
        request: u64,
        path: RemotePath,
        limit: usize,
    ) -> Result<Self, String> {
        if limit == 0 || limit > 8 * 1024 * 1024 {
            return Err("Source read limit must be between 1 byte and 8 MiB".into());
        }
        config.session.command.clear();
        config.session.x11 = false;
        config.session.remote_backend = crate::remote_files::capabilities::RemoteBackendKind::Sftp;
        Self::connect_purpose(
            config,
            Purpose::ReadFile {
                request,
                path,
                limit,
            },
        )
    }
    fn connect_purpose(config: ConnectionConfig, purpose: Purpose) -> Result<Self, String> {
        Self::connect_purpose_with_display(config, purpose, XDisplayBinding::Unavailable)
    }
    fn connect_purpose_with_display(
        config: ConnectionConfig,
        purpose: Purpose,
        display: XDisplayBinding,
    ) -> Result<Self, String> {
        let input_enabled = matches!(purpose, Purpose::Terminal);
        if config.session.agent_mode == crate::model::AgentMode::Owned
            && config.session.key.is_empty()
            && config.agent.is_none()
        {
            return Err("Start the TermCraft-owned agent before connecting this session.".into());
        }
        if config.session.agent_mode == crate::model::AgentMode::Pageant
            && config.session.key.is_empty()
            && !cfg!(windows)
        {
            return Err("Pageant agent selection is available only on Windows.".into());
        }
        validate_host(&config.session.host)?;
        validate_user(&config.session.user)?;
        if config.session.port == 0 {
            return Err("Choose an SSH port from 1 to 65535.".into());
        }
        config.session.ssh_options.validate(&config.defaults)?;
        let (mut transfers, transfer_rx) =
            crate::remote_files::transfer::TransferManager::channel();
        transfers.connection = Some(config.id);
        let endpoint = Arc::new(std::sync::Mutex::new(
            crate::remote_files::editor::Endpoint {
                host: config.session.host.clone(),
                port: config.session.port,
                user: config.session.user.clone(),
                jump: config.session.jump.clone(),
                gateway_user: String::new(),
                authenticated: false,
                target_key: String::new(),
                gateway_key: String::new(),
            },
        ));
        transfers.editor_endpoint = Some(endpoint.clone());
        let lifecycle = Arc::new(lifecycle::ConnectionLifecycle::new());
        let worker_lifecycle = lifecycle.clone();
        let (commands, command_rx) = mpsc::channel(128);
        let (list_requests, list_rx) = watch::channel(None);
        let (resize_requests, resize_rx) = watch::channel(None);
        let final_event = Arc::new(std::sync::Mutex::new(None));
        let final_state = final_event.clone();
        let (answers, answer_rx) = mpsc::channel(8);
        let (events, event_rx) = mpsc::channel(64);
        let (cancel, mut cancelled) = watch::channel(false);
        let finished = Arc::new(AtomicBool::new(false));
        let done = finished.clone();
        let pending_input = Arc::new(AtomicUsize::new(0));
        let pending = pending_input.clone();
        let id = config.id;
        std::thread::Builder::new().name(format!("termcraft-ssh-{id}")).spawn(move||{
            let result=match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                Ok(runtime)=>runtime.block_on(async {
                    let result=tokio::select! { biased;
                        _=cancelled.changed()=>Err("Connection cancelled".into()),
                        result=worker::run(config,purpose,display,command_rx,list_rx,resize_rx,answer_rx,events.clone(),pending,transfer_rx,worker_lifecycle.clone(),endpoint)=>result,
                    };
                    // UI disappearance must not strand a worker behind a full event queue.
                    let message=result.err().unwrap_or_else(||"Connection ended".into());
                    *final_state.lock().unwrap_or_else(|p|p.into_inner())=Some(ConnectionEvent::Exited{connection:id,message});
                }),
                Err(_)=>{*final_state.lock().unwrap_or_else(|p|p.into_inner())=Some(ConnectionEvent::Exited{connection:id,message:"Could not start SSH runtime".into()});},
            };
            let _=result; done.store(true,Ordering::Release);
        }).map_err(|e|e.to_string())?;
        Ok(Self {
            input_enabled,
            lifecycle,
            transfers,
            commands,
            answers,
            events: event_rx,
            pending_output: None,
            cancel,
            finished,
            pending_input,
            list_requests,
            resize_requests,
            final_event,
            x11_reply: None,
        })
    }
    pub fn try_send(&self, command: ConnectionCommand) -> Result<(), String> {
        if !self.input_enabled {
            return Err(
                "Source-only SSH connection does not accept terminal/browser controls".into(),
            );
        }
        if self.is_finished() || self.lifecycle.cancelled() {
            return Err("SSH connection is disconnected.".into());
        }
        let command = match command {
            ConnectionCommand::List { request, path } => {
                self.list_requests.send_replace(Some((request, path)));
                return Ok(());
            }
            ConnectionCommand::Resize { cols, rows } => {
                self.resize_requests.send_replace(Some((cols, rows)));
                return Ok(());
            }
            other => other,
        };
        let bytes = if let ConnectionCommand::Input(ref b) = command {
            b.len()
        } else {
            0
        };
        if bytes > 0 {
            #[allow(deprecated)]
            self.pending_input
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |v| {
                    v.checked_add(bytes).filter(|n| *n <= 1024 * 1024)
                })
                .map_err(|_| {
                    "SSH input queue is full. Wait for the server to receive pending input."
                        .to_string()
                })?;
        }
        if self.commands.try_send(command).is_err() {
            self.pending_input.fetch_sub(bytes, Ordering::AcqRel);
            return Err("SSH connection is busy or disconnected. Wait, then retry.".into());
        }
        Ok(())
    }
    pub fn answer(&self, answer: ChallengeAnswer) -> Result<(), String> {
        self.lifecycle.submit(answer.id, || {
            self.answers
                .try_send(answer)
                .map_err(|_| "The authentication prompt has closed or is busy.".into())
        })
    }
    pub fn poll(&mut self) -> Vec<ConnectionEvent> {
        self.poll_with_output_budget(usize::MAX)
    }
    pub fn poll_with_output_budget(&mut self, mut budget: usize) -> Vec<ConnectionEvent> {
        let mut events = Vec::new();
        if self.lifecycle.cancelled() {
            self.pending_output = None;
        }
        if let Some((connection, bytes, offset)) = self.pending_output.take() {
            let take = budget.min(bytes.len() - offset);
            if take > 0 {
                events.push(ConnectionEvent::Output {
                    connection,
                    bytes: bytes[offset..offset + take].to_vec(),
                });
                budget -= take;
            }
            if offset + take < bytes.len() {
                self.pending_output = Some((connection, bytes, offset + take));
            }
        }
        if self.pending_output.is_none() {
            for _ in 0..256 {
                match self.events.try_recv() {
                    Ok(_) if self.lifecycle.cancelled() => {}
                    Ok(ConnectionEvent::Output { connection, bytes }) => {
                        let take = budget.min(bytes.len());
                        if take < bytes.len() {
                            if take > 0 {
                                events.push(ConnectionEvent::Output {
                                    connection,
                                    bytes: bytes[..take].to_vec(),
                                });
                            }
                            self.pending_output = Some((connection, bytes, take));
                            break;
                        }
                        budget -= take;
                        events.push(ConnectionEvent::Output { connection, bytes });
                        if budget == 0 {
                            break;
                        }
                    }
                    Ok(event) => events.push(event),
                    Err(_) => break,
                }
            }
        }
        if let Some(event) = self
            .final_event
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
        {
            self.transfers.disconnect();
            events.push(event);
        }
        events
    }
    pub fn cancel(&self) {
        self.lifecycle.cancel();
        let _ = self.cancel.send(true);
    }
    pub fn output_drained(&self) -> bool {
        self.is_finished() && self.pending_output.is_none() && self.events.is_empty()
    }
    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }
}
impl Drop for ConnectionHandle {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[derive(Clone, Debug)]
pub struct ChallengeCredential {
    pub reference: Option<String>,
    pub binding: crate::credentials::CredentialBinding,
}
pub use auth::profile_binding as auth_profile_binding;

pub mod clock;
pub mod config;

pub use auth::gateway_reference_key;

mod proxy;
