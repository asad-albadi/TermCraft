use super::ssh_server::SshServer;
use harbor_workspace::ssh::ConnectionConfig;
use russh::{
    Channel, ChannelId, MethodKind, MethodSet, Pty,
    server::{self, Auth, Server as _, Session},
};
use std::{
    borrow::Cow,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
#[derive(Clone, Copy)]
pub enum Mode {
    Password,
    PasswordThenMfa,
    X11,
    #[cfg(target_os = "macos")]
    NativeX11,
    Mfa,
    KeyThenMfa,
    ScpStartup,
}
#[derive(Default)]
struct StartupState {
    active: std::sync::Mutex<std::collections::HashSet<ChannelId>>,
    pending: std::sync::Mutex<Vec<(ChannelId, server::Handle)>>,
    requests: AtomicUsize,
    allow: AtomicBool,
    frame: Vec<u8>,
}
#[derive(Default)]
struct Attempts {
    password: AtomicUsize,
    public_key: AtomicUsize,
    interactive_start: AtomicUsize,
    interactive_reply: AtomicUsize,
}
#[derive(Clone)]
struct Handler {
    attempts: Arc<Attempts>,
    startup: Arc<StartupState>,
    mode: Mode,
    shell: Option<ChannelId>,
    count: Arc<AtomicUsize>,
    x11_verified: Arc<AtomicBool>,
}
impl server::Server for Handler {
    type Handler = Self;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
        self.clone()
    }
}
impl server::Handler for Handler {
    type Error = russh::Error;
    async fn auth_none(&mut self, _: &str) -> Result<Auth, Self::Error> {
        if matches!(self.mode, Mode::ScpStartup) {
            return Ok(Auth::Accept);
        }
        let method = match self.mode {
            Mode::ScpStartup | Mode::Password | Mode::PasswordThenMfa | Mode::X11 => {
                MethodKind::Password
            }
            #[cfg(target_os = "macos")]
            Mode::NativeX11 => MethodKind::Password,
            Mode::Mfa => MethodKind::KeyboardInteractive,
            Mode::KeyThenMfa => MethodKind::PublicKey,
        };
        Ok(Auth::Reject {
            proceed_with_methods: Some(MethodSet::from(&[method][..])),
            partial_success: false,
        })
    }
    async fn auth_password(&mut self, _: &str, password: &str) -> Result<Auth, Self::Error> {
        self.attempts.password.fetch_add(1, Ordering::SeqCst);
        Ok(if password == "fixture-password" {
            if matches!(self.mode, Mode::PasswordThenMfa) {
                Auth::Reject {
                    proceed_with_methods: Some(MethodSet::from(
                        &[MethodKind::KeyboardInteractive][..],
                    )),
                    partial_success: true,
                }
            } else {
                Auth::Accept
            }
        } else {
            Auth::reject()
        })
    }
    async fn auth_publickey(
        &mut self,
        _: &str,
        _: server::VerifiedPublicKey<'_>,
    ) -> Result<Auth, Self::Error> {
        self.attempts.public_key.fetch_add(1, Ordering::SeqCst);
        Ok(Auth::Reject {
            proceed_with_methods: Some(MethodSet::from(&[MethodKind::KeyboardInteractive][..])),
            partial_success: true,
        })
    }
    async fn auth_keyboard_interactive<'a>(
        &'a mut self,
        _: &str,
        _: &str,
        response: Option<server::Response<'a>>,
    ) -> Result<Auth, Self::Error> {
        if response.is_some() {
            self.attempts
                .interactive_reply
                .fetch_add(1, Ordering::SeqCst);
        } else {
            self.attempts
                .interactive_start
                .fetch_add(1, Ordering::SeqCst);
        }
        match response {
            None => Ok(Auth::Partial {
                name: Cow::Borrowed("Fixture MFA"),
                instructions: Cow::Borrowed("Test challenges"),
                prompts: Cow::Owned(vec![
                    (Cow::Borrowed("Password"), false),
                    (Cow::Borrowed("Code"), true),
                ]),
            }),
            Some(response) => {
                let values = response.collect::<Vec<_>>();
                Ok(
                    if values.len() == 2
                        && values[0].as_ref() == b"fixture-password"
                        && values[1].as_ref() == b"123456"
                    {
                        Auth::Accept
                    } else {
                        Auth::reject()
                    },
                )
            }
        }
    }
    async fn auth_succeeded(&mut self, _: &mut Session) -> Result<(), Self::Error> {
        self.count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn channel_open_session(
        &mut self,
        channel: Channel<server::Msg>,
        reply: server::ChannelOpenHandle,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        if matches!(self.mode, Mode::ScpStartup) {
            let mut active = self.startup.active.lock().unwrap();
            if active.len() >= 3 {
                return Ok(());
            }
            active.insert(channel.id());
        }
        reply.accept().await;
        Ok(())
    }
    async fn channel_close(
        &mut self,
        channel: ChannelId,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        self.startup.active.lock().unwrap().remove(&channel);
        self.startup
            .pending
            .lock()
            .unwrap()
            .retain(|(id, _)| *id != channel);
        Ok(())
    }
    async fn exec_request(
        &mut self,
        channel: ChannelId,
        _: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.startup.requests.fetch_add(1, Ordering::SeqCst);
        if self.startup.allow.load(Ordering::SeqCst) {
            session.channel_success(channel)?;
            session.data(channel, self.startup.frame.clone())?;
            session.eof(channel)?;
            session.close(channel)?;
        } else {
            self.startup
                .pending
                .lock()
                .unwrap()
                .push((channel, session.handle()));
        }
        Ok(())
    }
    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _: &str,
        _: u32,
        _: u32,
        _: u32,
        _: u32,
        _: &[(Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        Ok(())
    }
    async fn x11_request(
        &mut self,
        channel: ChannelId,
        _: bool,
        protocol: &str,
        cookie: &str,
        _: u32,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        assert_eq!(protocol, "MIT-MAGIC-COOKIE-1");
        assert_eq!(cookie.len(), 32);
        assert_ne!(cookie, "2a".repeat(16));
        session.channel_success(channel)?;
        let handle = session.handle();
        let cookie = (0..16)
            .map(|i| u8::from_str_radix(&cookie[2 * i..2 * i + 2], 16).unwrap())
            .collect::<Vec<_>>();
        let verified = self.x11_verified.clone();
        let mode = self.mode;
        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let channel = handle.channel_open_x11("127.0.0.1", 12345).await.unwrap();
            let mut stream = channel.into_stream();
            let mut setup = vec![b'l', 0, 11, 0, 0, 0, 18, 0, 16, 0, 0, 0];
            setup.extend_from_slice(b"MIT-MAGIC-COOKIE-1\0\0");
            setup.extend_from_slice(&cookie);
            stream.write_all(&setup).await.unwrap();
            #[cfg(not(target_os = "macos"))]
            let _ = mode;
            #[cfg(target_os = "macos")]
            if matches!(mode, Mode::NativeX11) {
                let mut header = [0; 8];
                stream.read_exact(&mut header).await.unwrap();
                assert_eq!(&header[..6], &[1, 0, 11, 0, 0, 0]);
                let mut body = vec![0; u16::from_le_bytes([header[6], header[7]]) as usize * 4];
                stream.read_exact(&mut body).await.unwrap();
                assert!(body.len() >= 32);
                // GetInputFocus must cross the same negotiated forwarding channel.
                stream.write_all(&[43, 0, 1, 0]).await.unwrap();
                let mut reply = [0; 32];
                stream.read_exact(&mut reply).await.unwrap();
                assert_eq!(&reply[2..4], &1u16.to_le_bytes());
                assert_eq!(reply[0], 1);
                verified.store(true, Ordering::SeqCst);
                return;
            }
            let mut reply = [0; 6];
            stream.read_exact(&mut reply).await.unwrap();
            assert_eq!(&reply, b"X11_OK");
            verified.store(true, Ordering::SeqCst);
        });
        Ok(())
    }
    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.shell = Some(channel);
        session.channel_success(channel)?;
        Ok(())
    }
    async fn subsystem_request(
        &mut self,
        channel: ChannelId,
        _: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        Ok(())
    }
    async fn data(
        &mut self,
        channel: ChannelId,
        _: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if self.shell == Some(channel) {
            session.data(channel, b"TERMINAL_OK".to_vec())?;
        }
        Ok(())
    }
}
pub struct AuthServer {
    attempts: Arc<Attempts>,
    startup: Arc<StartupState>,
    owner: SshServer,
    port: u16,
    mode: Mode,
    count: Arc<AtomicUsize>,
    x11_verified: Arc<AtomicBool>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl AuthServer {
    pub fn start(mode: Mode) -> Self {
        let attempts = Arc::new(Attempts::default());
        let attempts_handler = attempts.clone();
        let owner = SshServer::start(true);
        let key = russh::keys::load_secret_key(owner.root.join("host"), None).unwrap();
        let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = socket.local_addr().unwrap().port();
        socket.set_nonblocking(true).unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        let x11_verified = Arc::new(AtomicBool::new(false));
        let verified = x11_verified.clone();
        let mut frame = b"TCFS2\0\x07".to_vec();
        use std::os::unix::ffi::OsStrExt;
        let bytes = owner.root.as_os_str().as_bytes();
        frame.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        frame.extend_from_slice(bytes);
        frame.extend_from_slice(&0u32.to_be_bytes());
        let startup = Arc::new(StartupState {
            frame,
            ..Default::default()
        });
        let startup_handler = startup.clone();
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let thread = std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async move{
                let listener=tokio::net::TcpListener::from_std(socket).unwrap();
                let settings=Arc::new(server::Config{keys:vec![key],auth_rejection_time:Duration::ZERO,auth_rejection_time_initial:Some(Duration::ZERO),..Default::default()});
                let mut handler=Handler{attempts:attempts_handler,mode,shell:None,count:c,x11_verified:verified,startup:startup_handler};
                tokio::select!{_ = stopped=>{},result=handler.run_on_socket(settings,&listener)=>{result.unwrap();}}
            });
        });
        Self {
            attempts,
            startup,
            owner,
            port,
            mode,
            count,
            x11_verified,
            stop: Some(stop),
            thread: Some(thread),
        }
    }
    pub fn config(&self) -> ConnectionConfig {
        let mut config = self.owner.config(81, false);
        config.session.port = self.port;
        config.session.command.clear();
        if matches!(self.mode, Mode::ScpStartup) {
            config.session.remote_backend =
                harbor_workspace::remote_files::capabilities::RemoteBackendKind::Scp;
        }
        if !matches!(self.mode, Mode::KeyThenMfa) {
            config.session.key.clear();
        }
        let public = std::fs::read_to_string(self.owner.root.join("host.pub")).unwrap();
        std::fs::write(
            &config.app_known_hosts,
            format!("[127.0.0.1]:{} {public}", self.port),
        )
        .unwrap();
        config
    }
    pub fn startup_requests(&self) -> usize {
        self.startup.requests.load(Ordering::SeqCst)
    }
    pub fn active_startup_helpers(&self) -> usize {
        self.startup.active.lock().unwrap().len().saturating_sub(1)
    }
    pub fn allow_startup_helpers(&self) {
        self.startup.allow.store(true, Ordering::SeqCst);
        let pending = std::mem::take(&mut *self.startup.pending.lock().unwrap());
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                for (channel, handle) in pending {
                    let _ = handle.channel_success(channel).await;
                    let _ = handle.data(channel, self.startup.frame.clone()).await;
                    let _ = handle.eof(channel).await;
                    let _ = handle.close(channel).await;
                }
            });
    }
    pub fn x11_verified(&self) -> bool {
        self.x11_verified.load(Ordering::SeqCst)
    }
    pub fn attempts(&self) -> [usize; 4] {
        [
            self.attempts.password.load(Ordering::SeqCst),
            self.attempts.public_key.load(Ordering::SeqCst),
            self.attempts.interactive_start.load(Ordering::SeqCst),
            self.attempts.interactive_reply.load(Ordering::SeqCst),
        ]
    }
    pub fn count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
}
impl Drop for AuthServer {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}
