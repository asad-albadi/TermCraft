//! Background FTP workspace using the shared browser, transfer queue and editor.
use super::ftp::{FtpBackend, FtpCancel, FtpDataMode, FtpOptions, FtpTlsMode, FtpTransferType};
use crate::remote_files::{
    BrowserState, DirectorySnapshot, RemotePath,
    editor::{Endpoint, RemoteDocument},
    transfer::{TransferEvent, TransferManager, TransferOperation},
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::sync::mpsc;

mod operations;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub enum LaunchMode {
    Embedded,
    #[default]
    External,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct SessionOptions {
    pub launch: LaunchMode,
    pub tls: FtpTlsMode,
    pub protect_data: bool,
    pub data_mode: FtpDataMode,
    pub transfer_type: FtpTransferType,
    pub certificate_authority: Option<PathBuf>,
}

impl SessionOptions {
    pub fn is_legacy(&self) -> bool {
        self == &Self::default()
    }

    pub fn embedded() -> Self {
        Self {
            launch: LaunchMode::Embedded,
            tls: FtpTlsMode::Explicit,
            protect_data: true,
            ..Self::default()
        }
    }
}

struct List {
    request: u64,
    path: RemotePath,
}

enum Event {
    Connected(Option<String>),
    Snapshot(DirectorySnapshot),
    ListFailed(u64, String),
    Closed(String),
}

pub struct Workspace {
    pub browser: BrowserState,
    transfers: TransferManager,
    listing: mpsc::Sender<List>,
    events: mpsc::Receiver<Event>,
    cancel: FtpCancel,
    ready: bool,
    closed: bool,
    status: String,
    fingerprint: Option<String>,
}

impl Workspace {
    pub fn start(
        connection: u64,
        options: FtpOptions,
        transfer_type: FtpTransferType,
    ) -> Result<Self, String> {
        options.validate()?;
        let (mut transfers, jobs) = TransferManager::channel();
        transfers.connection = Some(connection);
        let endpoint = Arc::new(Mutex::new(Endpoint {
            host: options.host.clone(),
            port: options.port,
            user: options.username.clone(),
            jump: format!(
                "FTP {:?}, data protection {}",
                options.tls, options.protect_data
            ),
            gateway_user: String::new(),
            authenticated: false,
            target_key: String::new(),
            gateway_key: String::new(),
        }));
        transfers.editor_endpoint = Some(endpoint.clone());
        let (listing, lists) = mpsc::channel(8);
        let (output, events) = mpsc::channel(16);
        let cancel = FtpCancel::default();
        let worker_cancel = cancel.clone();
        std::thread::Builder::new()
            .name("termcraft-ftp".into())
            .spawn(move || {
                let result: Result<(), String> = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|error| format!("FTP worker could not start: {error}"))
                    .and_then(|runtime| {
                        runtime.block_on(async {
                            let mut backend =
                                FtpBackend::connect_with_cancel(options, worker_cancel.clone())
                                    .await?;
                            let fingerprint = backend.certificate_fingerprint().map(str::to_owned);
                            if let Some(fingerprint) = &fingerprint {
                                let mut endpoint =
                                    endpoint.lock().unwrap_or_else(|p| p.into_inner());
                                endpoint.target_key.clone_from(fingerprint);
                                endpoint.authenticated = true;
                            }
                            let home = backend.pwd().await?;
                            if !home.as_bytes().starts_with(b"/") {
                                return Err(
                                    "The FTP server returned a non-absolute home directory.".into(),
                                );
                            }
                            if output.send(Event::Connected(fingerprint)).await.is_ok() {
                                let initial = List {
                                    request: 0,
                                    path: home,
                                };
                                run(
                                    &mut backend,
                                    transfer_type,
                                    connection,
                                    initial,
                                    lists,
                                    jobs,
                                    &output,
                                )
                                .await;
                            }
                            if worker_cancel.is_cancelled() {
                                Err("FTP disconnected.".into())
                            } else {
                                Err("FTP connection ended. Reconnect to continue.".into())
                            }
                        })
                    });
                if let Err(message) = result {
                    let _ = output.blocking_send(Event::Closed(message));
                }
            })
            .map_err(|error| format!("FTP worker could not start: {error}"))?;
        Ok(Self {
            browser: BrowserState::new(connection),
            transfers,
            listing,
            events,
            cancel,
            ready: false,
            closed: false,
            status: "Connecting…".into(),
            fingerprint: None,
        })
    }

    pub fn navigate(&mut self, path: RemotePath) -> Result<(), String> {
        if !self.ready() || !path.as_bytes().starts_with(b"/") {
            return Err("Choose an absolute directory on the connected FTP server.".into());
        }
        let request = self.browser.request.wrapping_add(1);
        self.listing
            .try_send(List { request, path })
            .map_err(|_| "FTP browser is busy or disconnected.")?;
        self.browser.begin(request);
        Ok(())
    }

    pub fn submit(&mut self, operation: TransferOperation) -> Result<u64, String> {
        if !self.ready() {
            return Err("Connect to the FTP server before submitting file operations.".into());
        }
        self.transfers.submit(operation)
    }

    pub fn open_document(
        &self,
        path: RemotePath,
        recovery_root: PathBuf,
    ) -> Result<RemoteDocument, String> {
        if !self.ready() {
            return Err("Connect to the FTP server to open documents.".into());
        }
        RemoteDocument::open(
            self.browser.connection,
            path,
            &self.transfers,
            recovery_root,
        )
    }

    pub fn cancel_transfer(&self, id: u64) {
        self.transfers.cancel(id);
    }

    pub fn disconnect(&self) {
        self.cancel.cancel();
    }

    pub fn ready(&self) -> bool {
        self.ready && !self.cancel.is_cancelled()
    }

    pub fn closed(&self) -> bool {
        self.closed
    }
    pub fn status(&self) -> &str {
        &self.status
    }
    pub fn certificate_fingerprint(&self) -> Option<&str> {
        self.fingerprint.as_deref()
    }

    pub fn poll(&mut self) -> Vec<TransferEvent> {
        for _ in 0..16 {
            match self.events.try_recv() {
                Ok(Event::Connected(fingerprint)) => {
                    self.fingerprint = fingerprint;
                    self.ready = true;
                    self.status = if self.fingerprint.is_some() {
                        "FTPS connected"
                    } else {
                        "FTP connected"
                    }
                    .into();
                }
                Ok(Event::Snapshot(snapshot)) => {
                    self.browser.apply_snapshot(snapshot);
                }
                Ok(Event::ListFailed(request, error)) => {
                    self.browser.fail(self.browser.connection, request, error);
                }
                Ok(Event::Closed(message)) => self.handle_closed(message),
                Err(mpsc::error::TryRecvError::Disconnected) if !self.closed => {
                    self.handle_closed("FTP worker ended. Reconnect to continue.".into());
                }
                Err(_) => break,
            }
        }
        self.transfers.poll()
    }

    fn handle_closed(&mut self, message: String) {
        self.ready = false;
        self.closed = true;
        self.browser.fail(
            self.browser.connection,
            self.browser.request,
            message.clone(),
        );
        self.status = message;
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        self.disconnect();
    }
}

async fn publish_list(
    backend: &mut FtpBackend,
    connection: u64,
    list: List,
    output: &mpsc::Sender<Event>,
) {
    let List { request, path } = list;
    let event = match backend.list(&path).await {
        Ok(entries) => Event::Snapshot(DirectorySnapshot {
            connection,
            request,
            path,
            entries,
        }),
        Err(error) => Event::ListFailed(request, error),
    };
    let _ = output.send(event).await;
}

async fn run(
    backend: &mut FtpBackend,
    transfer_type: FtpTransferType,
    connection: u64,
    initial: List,
    mut lists: mpsc::Receiver<List>,
    mut jobs: mpsc::Receiver<crate::remote_files::transfer::Job>,
    output: &mpsc::Sender<Event>,
) {
    publish_list(backend, connection, initial, output).await;
    let cancel = backend.cancel_handle();
    while backend.is_connected() {
        tokio::select! {
            _ = output.closed() => break,
            _ = cancel.cancelled() => break,
            Some(list) = lists.recv() => publish_list(backend, connection, list, output).await,
            Some(job) = jobs.recv() => operations::run(backend, transfer_type, job).await,
        }
    }
    jobs.close();
    while let Some(job) = jobs.recv().await {
        job.finish(crate::remote_files::transfer::TransferOutcome::Failed(
            "FTP connection ended before this operation started.".into(),
        ));
    }
}
