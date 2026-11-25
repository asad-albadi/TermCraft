//! Embedded SSH authentication and confined SFTP. Virtual users grant access only;
//! they never impersonate operating-system accounts.
use super::{Auth as PasswordAuth, ServiceAuth, files, supervisor::Context};
use crate::credentials::{
    Secret,
    keys::{KeyManager, KeySettings},
};
use russh::{
    Channel, ChannelId,
    server::{self, Auth, Msg, Session, VerifiedPublicKey},
};
use russh_sftp::protocol::*;
use std::{
    collections::HashMap,
    future::Future,
    io::{self, Read, Seek, SeekFrom},
    net::{SocketAddr, TcpListener},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context as TaskContext, Poll},
    time::Duration,
};

pub struct SshAuthorizedUser {
    pub username: String,
    pub password: Option<Secret>,
    pub public_keys: Vec<String>,
}
impl SshAuthorizedUser {
    pub fn password(username: impl Into<String>, password: Secret) -> Self {
        Self {
            username: username.into(),
            password: Some(password),
            public_keys: Vec::new(),
        }
    }
    pub fn keys(username: impl Into<String>, public_keys: Vec<String>) -> Self {
        Self {
            username: username.into(),
            password: None,
            public_keys,
        }
    }
}
pub struct SshServiceConfig {
    pub bind: SocketAddr,
    pub allow_non_loopback: bool,
    /// A key-bundle directory created atomically by KeyManager, never system sshd files.
    pub host_key_store: PathBuf,
    pub users: Vec<SshAuthorizedUser>,
    pub sftp_root: Option<PathBuf>,
    pub allow_write: bool,
    /// Explicit full authority under the current workspace OS account. Disabled by default.
    pub shell: Option<PathBuf>,
    pub lifetime: Option<Duration>,
}
impl SshServiceConfig {
    pub fn local(host_key_store: PathBuf, sftp_root: PathBuf) -> Self {
        Self {
            bind: ([127, 0, 0, 1], 0).into(),
            allow_non_loopback: false,
            host_key_store,
            users: Vec::new(),
            sftp_root: Some(sftp_root),
            allow_write: false,
            shell: None,
            lifetime: None,
        }
    }
}
struct User {
    username: String,
    password: PasswordAuth,
    keys: Vec<russh::keys::PublicKey>,
}
pub(super) struct Listener {
    socket: TcpListener,
    config: Arc<server::Config>,
    users: Arc<Vec<User>>,
    share: Option<Arc<files::Share>>,
    shell: Option<Arc<Path>>,
}
impl Listener {
    pub(super) fn bind(config: SshServiceConfig) -> Result<Self, String> {
        if config.users.is_empty() || config.users.len() > 32 {
            return Err("Configure 1 to 32 authorized SSH users.".into());
        }
        if !config.host_key_store.is_absolute() {
            return Err("Choose an absolute app-owned host-key folder.".into());
        }
        let mut users = Vec::new();
        for user in config.users {
            if user.username.is_empty()
                || user.username.len() > 255
                || user.username.chars().any(char::is_control)
                || user.public_keys.len() > 32
                || users.iter().any(|u: &User| u.username == user.username)
            {
                return Err("SSH usernames must be unique, bounded text.".into());
            }
            let mut keys = Vec::new();
            for encoded in user.public_keys {
                if encoded.len() > 16384 {
                    return Err("Authorized SSH key is too large.".into());
                }
                keys.push(
                    russh::keys::PublicKey::from_openssh(&encoded)
                        .map_err(|_| "Invalid authorized SSH public key.")?,
                );
            }
            if keys.is_empty() && user.password.is_none() {
                return Err("Each SSH user needs a password or authorized key.".into());
            }
            let password = match user.password {
                Some(password) => PasswordAuth::prepare(ServiceAuth::Password {
                    username: user.username.clone(),
                    password,
                })?,
                None => PasswordAuth::None,
            };
            users.push(User {
                username: user.username,
                password,
                keys,
            });
        }
        // Called only from the supervisor worker. Generation and disk I/O never run on UI.
        if !config.host_key_store.exists() {
            if let Some(parent) = config.host_key_store.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|_| "Could not create app-owned service data.")?;
            }
            if KeyManager::generate(&config.host_key_store, KeySettings::default()).is_err()
                && !config.host_key_store.is_dir()
            {
                return Err("Could not generate the app-owned SSH host key.".into());
            }
        }
        let key_path = config.host_key_store.join("private_key");
        let metadata = std::fs::symlink_metadata(&key_path)
            .map_err(|_| "Could not open the persistent SSH host key.")?;
        if !metadata.is_file() || metadata.len() > 65536 {
            return Err("Invalid persistent SSH host key file.".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err("SSH host key permissions must be private to your account.".into());
            }
        }
        let key = russh::keys::load_secret_key(&key_path, None)
            .map_err(|_| "Could not decode the persistent SSH host key.")?;
        if let Some(shell) = config.shell.as_ref() {
            super::telnet::validate_shell(shell)?;
        }
        let shell = config.shell.map(Arc::<Path>::from);
        if let Some(root) = config.sftp_root.as_ref() {
            let root = std::fs::canonicalize(root)
                .map_err(|_| "Could not open the SFTP shared folder.")?;
            let key_store = std::fs::canonicalize(&config.host_key_store)
                .map_err(|_| "Could not locate the SSH host-key store.")?;
            if key_store.starts_with(&root) || root.starts_with(&key_store) {
                return Err("Choose an SFTP folder separate from SSH host-key storage.".into());
            }
        }
        let share = config
            .sftp_root
            .as_ref()
            .map(|path| files::Share::open(path, config.allow_write).map(Arc::new))
            .transpose()
            .map_err(|_| "Could not open the SFTP shared folder.")?;
        let socket = TcpListener::bind(config.bind)
            .map_err(|e| format!("Could not bind SSH listener: {e}"))?;
        socket.set_nonblocking(true).map_err(|e| e.to_string())?;
        Ok(Self {
            socket,
            config: Arc::new(server::Config {
                keys: vec![key],
                auth_rejection_time: Duration::from_millis(250),
                auth_rejection_time_initial: Some(Duration::from_millis(250)),
                inactivity_timeout: Some(Duration::from_secs(300)),
                max_auth_attempts: 3,
                ..Default::default()
            }),
            users: Arc::new(users),
            share,
            shell,
        })
    }
    pub(super) fn address(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }
    pub(super) fn serve(self, context: &Arc<Context>) -> io::Result<()> {
        tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async move {
            let listener=tokio::net::TcpListener::from_std(self.socket)?;
            let mut clients=tokio::task::JoinSet::new();
            while !context.cancelled() {
                tokio::select! {
                    _=tokio::time::sleep(Duration::from_millis(50))=>{},
                    Some(_)=clients.join_next(), if !clients.is_empty()=>{},
                    accepted=listener.accept()=>{
                        let (stream,_)=accepted?;
                        if clients.len()>=8 {continue;}
                        let config=self.config.clone();
                        let handler=SshSession { users:self.users.clone(),share:self.share.clone(),channels:HashMap::new(),sftp_workers:HashMap::new(),shell:self.shell.clone(),shells:HashMap::new(),windows:HashMap::new() };
                        let stop=context.clone();
                        clients.spawn(async move {
                            // Both handshake and total connection lifetime are bounded. run_stream
                            // spawns the session on this owned runtime, which is dropped on stop.
                            if let Ok(Ok(mut session))=tokio::time::timeout(Duration::from_secs(15),server::run_stream(config,stream,handler)).await {
                                let handle=session.handle();
                                tokio::select! {
                                    _=&mut session=>{},
                                    _=async { while !stop.cancelled(){tokio::time::sleep(Duration::from_millis(50)).await;} }=>{},
                                    _=tokio::time::sleep(Duration::from_secs(3600))=>{},
                                }
                                let _=tokio::time::timeout(Duration::from_millis(250),handle.disconnect(russh::Disconnect::ByApplication,"Service stopped".into(),"en".into())).await;
                            }
                        });
                    }
                }
            }
            drop(listener);
            // Workers observe cancellation every 50 ms; runtime destruction also owns
            // russh's internal sessions; session drop cancels the actual SFTP streams.
            let _=tokio::time::timeout(Duration::from_secs(1),async {while clients.join_next().await.is_some(){}}).await;
            clients.abort_all();
            while clients.join_next().await.is_some(){}
            Ok(())
        })
    }
}
struct SshSession {
    users: Arc<Vec<User>>,
    share: Option<Arc<files::Share>>,
    channels: HashMap<ChannelId, Channel<Msg>>,
    sftp_workers: HashMap<ChannelId, Arc<SftpControl>>,
    shell: Option<Arc<Path>>,
    shells: HashMap<ChannelId, Shell>,
    windows: HashMap<ChannelId, (u16, u16)>,
}
impl Drop for SshSession {
    fn drop(&mut self) {
        for task in self.sftp_workers.values() {
            task.cancel();
        }
    }
}
impl server::Handler for SshSession {
    type Error = russh::Error;
    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        Ok(
            if password.len() <= 1024
                && self
                    .users
                    .iter()
                    .any(|u| u.password.allows(user.as_bytes(), password.as_bytes()))
            {
                Auth::Accept
            } else {
                Auth::reject()
            },
        )
    }
    async fn auth_publickey_offered(
        &mut self,
        user: &str,
        key: server::PublicKeyOffer<'_>,
    ) -> Result<Auth, Self::Error> {
        Ok(
            if self
                .users
                .iter()
                .any(|u| u.username == user && u.keys.contains(key.public_key))
            {
                Auth::Accept
            } else {
                Auth::reject()
            },
        )
    }
    async fn auth_publickey(
        &mut self,
        user: &str,
        key: VerifiedPublicKey<'_>,
    ) -> Result<Auth, Self::Error> {
        Ok(
            if self
                .users
                .iter()
                .any(|u| u.username == user && u.keys.contains(key.public_key))
            {
                Auth::Accept
            } else {
                Auth::reject()
            },
        )
    }
    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: server::ChannelOpenHandle,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        self.sftp_workers
            .retain(|_, control| !control.finished.load(Ordering::Acquire));
        if self.channels.len() + self.shells.len() + self.sftp_workers.len() >= 4 {
            reply
                .reject(russh::ChannelOpenFailure::ResourceShortage)
                .await;
        } else {
            self.channels.insert(channel.id(), channel);
            reply.accept().await;
        }
        Ok(())
    }
    async fn channel_close(&mut self, id: ChannelId, _: &mut Session) -> Result<(), Self::Error> {
        self.channels.remove(&id);
        self.windows.remove(&id);
        self.shells.remove(&id);
        if let Some(control) = self.sftp_workers.get(&id) {
            control.cancel();
        }
        Ok(())
    }
    async fn exec_request(
        &mut self,
        id: ChannelId,
        _: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_failure(id)?;
        Ok(())
    }
    async fn env_request(
        &mut self,
        id: ChannelId,
        _: &str,
        _: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_failure(id)?;
        Ok(())
    }
    async fn pty_request(
        &mut self,
        id: ChannelId,
        _: &str,
        cols: u32,
        rows: u32,
        _: u32,
        _: u32,
        _: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if self.shell.is_some() && self.channels.contains_key(&id) {
            self.windows
                .insert(id, (cols.clamp(1, 500) as u16, rows.clamp(1, 300) as u16));
            session.channel_success(id)?;
        } else {
            session.channel_failure(id)?;
        }
        Ok(())
    }
    async fn shell_request(
        &mut self,
        id: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some(shell) = self
            .shell
            .clone()
            .filter(|_| self.channels.contains_key(&id))
        {
            let window = self.windows.remove(&id).unwrap_or((80, 24));
            match Shell::start(shell, id, session.handle(), window) {
                Ok(worker) => {
                    self.channels.remove(&id);
                    self.shells.insert(id, worker);
                    session.channel_success(id)?;
                }
                Err(_) => {
                    session.channel_failure(id)?;
                }
            }
        } else {
            session.channel_failure(id)?;
        }
        Ok(())
    }
    async fn window_change_request(
        &mut self,
        id: ChannelId,
        cols: u32,
        rows: u32,
        _: u32,
        _: u32,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some(shell) = self.shells.get(&id) {
            let _ = shell.input.try_send(ShellInput::Resize(
                cols.clamp(1, 500) as u16,
                rows.clamp(1, 300) as u16,
            ));
        }
        Ok(())
    }
    async fn data(
        &mut self,
        id: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some(shell) = self.shells.get(&id) {
            if data.len() > 32768
                || shell
                    .input
                    .try_send(ShellInput::Data(data.to_vec()))
                    .is_err()
            {
                self.shells.remove(&id);
                session.close(id)?;
            }
        }
        Ok(())
    }
    async fn channel_eof(
        &mut self,
        id: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some(control) = self.sftp_workers.get(&id) {
            control.cancel();
        }
        if self.shells.remove(&id).is_some() {
            session.close(id)?;
        }
        Ok(())
    }
    async fn subsystem_request(
        &mut self,
        id: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.sftp_workers
            .retain(|_, control| !control.finished.load(Ordering::Acquire));
        if name == "sftp" && self.sftp_workers.len() < 4 && self.share.is_some() {
            if let Some(channel) = self.channels.remove(&id) {
                session.channel_success(id)?;
                let control = Arc::new(SftpControl::default());
                self.sftp_workers.insert(id, control.clone());
                start_sftp(
                    channel.into_stream(),
                    self.share.clone().unwrap(),
                    control,
                    Duration::from_secs(3600),
                )
                .await;
                return Ok(());
            }
        }
        session.channel_failure(id)?;
        Ok(())
    }
}

/// russh-sftp 4.0.0 `run` spawns its request loop and returns immediately.
/// The lease lives in that real loop's handler; the stream receives cancellation
/// and a deadline directly, so neither accounting nor teardown relies on `run`'s
/// scheduling future finishing.
#[derive(Default)]
struct SftpControl {
    cancelled: AtomicBool,
    finished: AtomicBool,
    waker: futures::task::AtomicWaker,
}
impl SftpControl {
    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.waker.wake();
    }
}
struct SftpLease(Arc<SftpControl>);
impl Drop for SftpLease {
    fn drop(&mut self) {
        self.0.finished.store(true, Ordering::Release);
    }
}
struct OwnedSftpStream<S> {
    stream: S,
    control: Arc<SftpControl>,
    deadline: Pin<Box<tokio::time::Sleep>>,
}
impl<S> OwnedSftpStream<S> {
    fn check(&mut self, cx: &mut TaskContext<'_>) -> io::Result<()> {
        self.control.waker.register(cx.waker());
        if self.control.cancelled.load(Ordering::Acquire) {
            return Err(io::ErrorKind::ConnectionAborted.into());
        }
        if self.deadline.as_mut().poll(cx).is_ready() {
            self.control.cancel();
            return Err(io::ErrorKind::TimedOut.into());
        }
        Ok(())
    }
}
impl<S: tokio::io::AsyncRead + Unpin> tokio::io::AsyncRead for OwnedSftpStream<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buffer: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if let Err(error) = self.check(cx) {
            return Poll::Ready(Err(error));
        }
        Pin::new(&mut self.stream).poll_read(cx, buffer)
    }
}
impl<S: tokio::io::AsyncWrite + Unpin> tokio::io::AsyncWrite for OwnedSftpStream<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        if let Err(error) = self.check(cx) {
            return Poll::Ready(Err(error));
        }
        Pin::new(&mut self.stream).poll_write(cx, buffer)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<io::Result<()>> {
        if let Err(error) = self.check(cx) {
            return Poll::Ready(Err(error));
        }
        Pin::new(&mut self.stream).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<io::Result<()>> {
        if let Err(error) = self.check(cx) {
            return Poll::Ready(Err(error));
        }
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}
async fn start_sftp<S>(
    stream: S,
    share: Arc<files::Share>,
    control: Arc<SftpControl>,
    duration: Duration,
) where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let handler = SftpSession {
        share,
        handles: HashMap::new(),
        next: 0,
        _lease: SftpLease(control.clone()),
    };
    let stream = OwnedSftpStream {
        stream,
        control,
        deadline: Box::pin(tokio::time::sleep(duration)),
    };
    russh_sftp::server::run(stream, handler).await;
}

enum Open {
    Read(cap_std::fs::File),
    Write(files::Staged, u64),
    Directory(Vec<File>),
}
struct SftpSession {
    share: Arc<files::Share>,
    handles: HashMap<Vec<u8>, Open>,
    next: u64,
    // Declared last: file handles/staging are dropped before admission is released.
    _lease: SftpLease,
}
fn code(e: io::Error) -> StatusCode {
    match e.kind() {
        io::ErrorKind::NotFound => StatusCode::NoSuchFile,
        io::ErrorKind::PermissionDenied => StatusCode::PermissionDenied,
        _ => StatusCode::Failure,
    }
}
fn path(p: SftpString) -> Result<PathBuf, StatusCode> {
    let p = std::str::from_utf8(p.as_bytes()).map_err(|_| StatusCode::BadMessage)?;
    files::path(Path::new(""), p, true).map_err(code)
}
fn ok(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: String::new(),
        language_tag: String::new(),
    }
}
fn attrs(size: u64, directory: bool) -> FileAttributes {
    FileAttributes {
        size: Some(size),
        permissions: Some(if directory { 0o40755 } else { 0o100644 }),
        ..Default::default()
    }
}
impl SftpSession {
    fn insert(&mut self, id: u32, open: Open) -> Result<Handle, StatusCode> {
        if self.handles.len() >= 32 {
            return Err(StatusCode::Failure);
        }
        self.next = self.next.checked_add(1).ok_or(StatusCode::Failure)?;
        let h = self.next.to_string().into_bytes();
        self.handles.insert(h.clone(), open);
        Ok(Handle {
            id,
            handle: h.into(),
        })
    }
    fn stat_path(&self, p: &Path) -> Result<FileAttributes, StatusCode> {
        if p.as_os_str().is_empty() {
            return Ok(attrs(0, true));
        }
        let kind = self.share.kind(p).map_err(code)?;
        Ok(attrs(
            if kind.is_file() {
                self.share
                    .read(p)
                    .map_err(code)?
                    .metadata()
                    .map_err(code)?
                    .len()
            } else {
                0
            },
            kind.is_dir(),
        ))
    }
}
impl russh_sftp::server::Handler for SftpSession {
    type Error = StatusCode;
    fn unimplemented(&self) -> StatusCode {
        StatusCode::OpUnsupported
    }
    async fn realpath(&mut self, id: u32, p: SftpString) -> Result<Name, StatusCode> {
        let p = path(p)?;
        Ok(Name {
            id,
            files: vec![File::dummy(format!("/{}", p.to_string_lossy()))],
        })
    }
    async fn open(
        &mut self,
        id: u32,
        p: SftpString,
        flags: OpenFlags,
        _: FileAttributes,
    ) -> Result<Handle, StatusCode> {
        let p = path(p)?;
        if flags.bits() == OpenFlags::READ.bits() {
            let f = self.share.read(&p).map_err(code)?;
            self.insert(id, Open::Read(f))
        } else if flags.contains(OpenFlags::WRITE | OpenFlags::CREATE)
            && !flags.intersects(OpenFlags::READ | OpenFlags::APPEND)
        {
            let stage = self.share.stage(&p).map_err(code)?;
            self.insert(id, Open::Write(stage, 0))
        } else {
            Err(StatusCode::OpUnsupported)
        }
    }
    async fn read(
        &mut self,
        id: u32,
        h: SftpHandle,
        offset: u64,
        len: u32,
    ) -> Result<Data, StatusCode> {
        let Some(Open::Read(f)) = self.handles.get_mut(h.as_bytes()) else {
            return Err(StatusCode::Failure);
        };
        f.seek(SeekFrom::Start(offset)).map_err(code)?;
        let mut data = vec![0; len.min(32768) as usize];
        let n = f.read(&mut data).map_err(code)?;
        if n == 0 {
            return Err(StatusCode::Eof);
        }
        data.truncate(n);
        Ok(Data { id, data })
    }
    async fn write(
        &mut self,
        id: u32,
        h: SftpHandle,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<Status, StatusCode> {
        let key = h.as_bytes().to_vec();
        let Some(Open::Write(mut stage, size)) = self.handles.remove(&key) else {
            return Err(StatusCode::Failure);
        };
        if offset != size
            || data.len() > 32768
            || offset.saturating_add(data.len() as u64) > files::UPLOAD_LIMIT
        {
            return Err(StatusCode::Failure);
        }
        stage.append(&data).map_err(code)?;
        self.handles
            .insert(key, Open::Write(stage, size + data.len() as u64));
        Ok(ok(id))
    }
    async fn close(&mut self, id: u32, h: SftpHandle) -> Result<Status, StatusCode> {
        match self
            .handles
            .remove(h.as_bytes())
            .ok_or(StatusCode::Failure)?
        {
            Open::Write(stage, _) => stage.publish(|| Ok(())).map_err(code)?,
            _ => {}
        }
        Ok(ok(id))
    }
    async fn stat(&mut self, id: u32, p: SftpString) -> Result<Attrs, StatusCode> {
        Ok(Attrs {
            id,
            attrs: self.stat_path(&path(p)?)?,
        })
    }
    async fn lstat(&mut self, id: u32, p: SftpString) -> Result<Attrs, StatusCode> {
        self.stat(id, p).await
    }
    async fn fstat(&mut self, id: u32, h: SftpHandle) -> Result<Attrs, StatusCode> {
        let a = match self.handles.get(h.as_bytes()).ok_or(StatusCode::Failure)? {
            Open::Read(f) => attrs(f.metadata().map_err(code)?.len(), false),
            Open::Write(_, size) => attrs(*size, false),
            Open::Directory(_) => attrs(0, true),
        };
        Ok(Attrs { id, attrs: a })
    }
    async fn opendir(&mut self, id: u32, p: SftpString) -> Result<Handle, StatusCode> {
        let dir = self.share.directory(&path(p)?).map_err(code)?;
        let mut entries = Vec::new();
        for entry in dir.entries().map_err(code)? {
            let entry = entry.map_err(code)?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if !files::usable_name(name) {
                continue;
            }
            let kind = entry.file_type().map_err(code)?;
            if !(kind.is_dir() || kind.is_file()) {
                continue;
            }
            if entries.len() >= 4096 {
                return Err(StatusCode::Failure);
            }
            entries.push(File::new(name, attrs(0, kind.is_dir())));
        }
        self.insert(id, Open::Directory(entries))
    }
    async fn readdir(&mut self, id: u32, h: SftpHandle) -> Result<Name, StatusCode> {
        let Some(Open::Directory(entries)) = self.handles.get_mut(h.as_bytes()) else {
            return Err(StatusCode::Failure);
        };
        if entries.is_empty() {
            return Err(StatusCode::Eof);
        }
        let count = entries.len().min(64);
        let files = entries.drain(..count).collect();
        Ok(Name { id, files })
    }
    async fn mkdir(
        &mut self,
        id: u32,
        p: SftpString,
        _: FileAttributes,
    ) -> Result<Status, StatusCode> {
        self.share.create_directory(&path(p)?).map_err(code)?;
        Ok(ok(id))
    }
    async fn rmdir(&mut self, id: u32, p: SftpString) -> Result<Status, StatusCode> {
        self.share.remove(&path(p)?, true).map_err(code)?;
        Ok(ok(id))
    }
    async fn remove(&mut self, id: u32, p: SftpString) -> Result<Status, StatusCode> {
        self.share.remove(&path(p)?, false).map_err(code)?;
        Ok(ok(id))
    }
    async fn rename(
        &mut self,
        id: u32,
        from: SftpString,
        to: SftpString,
    ) -> Result<Status, StatusCode> {
        self.share.rename(&path(from)?, &path(to)?).map_err(code)?;
        Ok(ok(id))
    }
}

enum ShellInput {
    Data(Vec<u8>),
    Resize(u16, u16),
}
struct Shell {
    input: std::sync::mpsc::SyncSender<ShellInput>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl Drop for Shell {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
impl Shell {
    fn start(
        shell: Arc<Path>,
        id: ChannelId,
        handle: server::Handle,
        window: (u16, u16),
    ) -> io::Result<Self> {
        let (input, rx) = std::sync::mpsc::sync_channel(8);
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cancelled = stop.clone();
        let worker = std::thread::Builder::new()
            .name("ssh-shell".into())
            .spawn(move || {
                // The PTY is constructed and destroyed on its owning worker on every OS.
                let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                else {
                    return;
                };
                let Ok(mut process) = crate::process::Process::spawn_with_environment_cancellable(
                    shell.to_str().unwrap_or(""),
                    &[],
                    None,
                    window.0,
                    window.1,
                    &[],
                    &cancelled,
                ) else {
                    let _ = runtime.block_on(handle.close(id));
                    return;
                };
                let deadline = std::time::Instant::now() + Duration::from_secs(3600);
                let mut idle = std::time::Instant::now() + Duration::from_secs(300);
                'session: while !cancelled.load(std::sync::atomic::Ordering::Acquire)
                    && std::time::Instant::now() < deadline.min(idle)
                {
                    for _ in 0..8 {
                        match rx.try_recv() {
                            Ok(ShellInput::Data(data)) => {
                                if process.write(&data).is_err() {
                                    break 'session;
                                }
                                idle = std::time::Instant::now() + Duration::from_secs(300);
                            }
                            Ok(ShellInput::Resize(cols, rows)) => process.resize(cols, rows),
                            Err(std::sync::mpsc::TryRecvError::Disconnected) => break 'session,
                            Err(std::sync::mpsc::TryRecvError::Empty) => break,
                        }
                    }
                    for _ in 0..8 {
                        match process.rx.try_recv() {
                            Ok(data) => {
                                if !matches!(
                                    runtime.block_on(async {
                                        tokio::time::timeout(
                                            Duration::from_millis(250),
                                            handle.data(id, data),
                                        )
                                        .await
                                    }),
                                    Ok(Ok(()))
                                ) {
                                    break 'session;
                                }
                            }
                            Err(std::sync::mpsc::TryRecvError::Disconnected) => break 'session,
                            Err(std::sync::mpsc::TryRecvError::Empty) => break,
                        }
                    }
                    process.poll();
                    std::thread::sleep(Duration::from_millis(25));
                }
                drop(process);
                let _ = runtime.block_on(async {
                    let _ = tokio::time::timeout(Duration::from_millis(250), handle.eof(id)).await;
                    let _ =
                        tokio::time::timeout(Duration::from_millis(250), handle.close(id)).await;
                });
            })?;
        Ok(Self {
            input,
            stop,
            worker: Some(worker),
        })
    }
}

#[cfg(test)]
mod ownership_tests {
    use super::*;
    struct Client;
    impl russh::client::Handler for Client {
        type Error = russh::Error;
        async fn check_server_key(
            &mut self,
            _: &russh::keys::PublicKeyOrCertificate,
        ) -> Result<bool, Self::Error> {
            Ok(true)
        }
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn live_sftp_subsystems_retain_four_channel_admission_slots() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("share");
        std::fs::create_dir(&root).unwrap();
        let mut config = SshServiceConfig::local(directory.path().join("host"), root);
        config.users.push(SshAuthorizedUser::password(
            "allowed",
            Secret::new("secret"),
        ));
        let supervisor = crate::services::ServiceSupervisor::default();
        let id = supervisor.start_ssh(config).unwrap();
        let mut client = russh::client::connect(
            Arc::new(russh::client::Config::default()),
            supervisor.status(id).unwrap().address.unwrap(),
            Client,
        )
        .await
        .unwrap();
        assert!(
            client
                .authenticate_password("allowed", "secret")
                .await
                .unwrap()
                .success()
        );
        let mut active = Vec::new();
        for _ in 0..4 {
            let channel = client.channel_open_session().await.unwrap();
            channel.request_subsystem(true, "sftp").await.unwrap();
            active.push(
                russh_sftp::client::SftpSession::new(channel.into_stream())
                    .await
                    .unwrap(),
            );
        }
        let fifth = tokio::time::timeout(Duration::from_secs(2), client.channel_open_session())
            .await
            .unwrap();
        assert!(
            fifth.is_err(),
            "four initialized retained SFTP workers must exhaust channel admission"
        );
        for _ in 0..6 {
            active.pop().unwrap().close().await.unwrap();
            let channel = tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    match client.channel_open_session().await {
                        Ok(channel) => break channel,
                        Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
                    }
                }
            })
            .await
            .unwrap();
            channel.request_subsystem(true, "sftp").await.unwrap();
            active.push(
                russh_sftp::client::SftpSession::new(channel.into_stream())
                    .await
                    .unwrap(),
            );
            assert!(
                client.channel_open_session().await.is_err(),
                "recycled channels must retain the same four-worker bound"
            );
        }
        drop(active);
    }
    async fn staging_is_gone(root: &Path) {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if !std::fs::read_dir(root).unwrap().any(|entry| {
                    entry
                        .unwrap()
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".harbor-service-stage-")
                }) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("real SFTP loop must release unpublished upload handles");
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn parent_ssh_disconnect_cancels_retained_sftp_upload_workers() {
        use tokio::io::AsyncWriteExt;
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("share");
        std::fs::create_dir(&root).unwrap();
        let mut config = SshServiceConfig::local(directory.path().join("host"), root.clone());
        config.allow_write = true;
        config.users.push(SshAuthorizedUser::password(
            "allowed",
            Secret::new("secret"),
        ));
        let supervisor = crate::services::ServiceSupervisor::default();
        let id = supervisor.start_ssh(config).unwrap();
        let mut client = russh::client::connect(
            Arc::new(russh::client::Config::default()),
            supervisor.status(id).unwrap().address.unwrap(),
            Client,
        )
        .await
        .unwrap();
        assert!(
            client
                .authenticate_password("allowed", "secret")
                .await
                .unwrap()
                .success()
        );
        let channel = client.channel_open_session().await.unwrap();
        channel.request_subsystem(true, "sftp").await.unwrap();
        let sftp = russh_sftp::client::SftpSession::new(channel.into_stream())
            .await
            .unwrap();
        let mut upload = sftp.create("unfinished").await.unwrap();
        upload.write_all(b"partial upload").await.unwrap();
        upload.flush().await.unwrap();
        assert!(std::fs::read_dir(&root).unwrap().next().is_some());
        client
            .disconnect(russh::Disconnect::ByApplication, "closing parent", "en")
            .await
            .unwrap();
        staging_is_gone(&root).await;
        assert!(!root.join("unfinished").exists());
        assert_eq!(
            supervisor.status(id).unwrap().state,
            crate::services::ServiceState::Running,
            "cleanup must follow parent disconnect without stopping the service"
        );
        drop(upload);
        drop(sftp);
    }
    #[tokio::test]
    async fn real_spawned_sftp_loop_obeys_stream_deadline_and_cancellation() {
        use tokio::io::AsyncWriteExt;
        for cancel in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let share = Arc::new(files::Share::open(root.path(), true).unwrap());
            let control = Arc::new(SftpControl::default());
            let (server, client) = tokio::io::duplex(65536);
            start_sftp(
                server,
                share,
                control.clone(),
                if cancel {
                    Duration::from_secs(3600)
                } else {
                    Duration::from_millis(500)
                },
            )
            .await;
            let sftp = russh_sftp::client::SftpSession::new(client).await.unwrap();
            let mut upload = sftp.create("unfinished").await.unwrap();
            upload.write_all(b"partial").await.unwrap();
            upload.flush().await.unwrap();
            assert!(
                !control.finished.load(Ordering::Acquire),
                "the completed run() scheduling future is not loop completion"
            );
            assert!(std::fs::read_dir(root.path()).unwrap().next().is_some());
            if cancel {
                control.cancel();
            }
            tokio::time::timeout(Duration::from_secs(3), async {
                while !control.finished.load(Ordering::Acquire) {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("actual library worker must finish");
            staging_is_gone(root.path()).await;
            assert!(!root.path().join("unfinished").exists());
            drop(upload);
            drop(sftp);
        }
    }
}
