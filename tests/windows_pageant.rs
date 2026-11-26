#![cfg(windows)]

use harbor_workspace::credentials::{
    agent::{AgentManager, AgentTarget},
    keys::{KeyManager, KeySettings},
};
use harbor_workspace::{
    model::{AgentMode, Session},
    ssh::{ConnectionConfig, ConnectionEvent, ConnectionHandle},
};
use russh::{
    ChannelId, MethodKind, MethodSet,
    keys::{
        HashAlg, PrivateKey, PublicKey,
        ssh_key::{SshSig, private::Ed25519Keypair},
    },
    server::{self, Auth, Server},
};
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::Arc,
    time::{Duration, Instant},
};
use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, GetWindowThreadProcessId};

fn pageant_window_pid() -> Option<u32> {
    let name: Vec<u16> = "Pageant\0".encode_utf16().collect();
    let window = unsafe { FindWindowW(name.as_ptr(), name.as_ptr()) };
    if window.is_null() {
        return None;
    }
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(window, &mut pid) };
    (pid != 0).then_some(pid)
}

struct Fixture(Child);

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[derive(Clone)]
struct KeyOnlyServer(PublicKey);

impl Server for KeyOnlyServer {
    type Handler = Self;

    fn new_client(&mut self, _: Option<SocketAddr>) -> Self {
        self.clone()
    }
}

impl server::Handler for KeyOnlyServer {
    type Error = russh::Error;

    async fn auth_none(&mut self, _: &str) -> Result<Auth, Self::Error> {
        Ok(Auth::Reject {
            proceed_with_methods: Some(MethodSet::from(&[MethodKind::PublicKey][..])),
            partial_success: false,
        })
    }

    async fn auth_publickey(
        &mut self,
        user: &str,
        key: server::VerifiedPublicKey<'_>,
    ) -> Result<Auth, Self::Error> {
        Ok(
            if user == "pageant-fixture" && key.public_key.key_data() == self.0.key_data() {
                Auth::Accept
            } else {
                Auth::reject()
            },
        )
    }

    async fn channel_open_session(
        &mut self,
        _: russh::Channel<server::Msg>,
        reply: server::ChannelOpenHandle,
        _: &mut server::Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
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
        _: &[(russh::Pty, u32)],
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)
    }
}

async fn ssh_authenticated(port: u16, known_hosts: &Path, mode: AgentMode) -> bool {
    let mut connection = ConnectionHandle::connect(ConnectionConfig {
        id: 1,
        agent: None,
        defaults: Default::default(),
        clock: None,
        session: Session {
            host: "127.0.0.1".into(),
            port,
            user: "pageant-fixture".into(),
            agent_mode: mode,
            ..Default::default()
        },
        app_known_hosts: known_hosts.to_path_buf(),
        user_known_hosts: Vec::new(),
    })
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        for event in connection.poll() {
            match event {
                ConnectionEvent::Connected { .. } => return true,
                ConnectionEvent::Exited { message, .. } => {
                    assert!(
                        message.contains("authentication"),
                        "Unexpected exit: {message}"
                    );
                    return false;
                }
                ConnectionEvent::Challenge { .. } => {
                    panic!("The host and key fixture must not prompt")
                }
                _ => {}
            }
        }
        assert!(Instant::now() < deadline, "SSH authentication timed out");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[test]
#[ignore = "requires a verified standalone Pageant executable on an isolated Windows desktop"]
fn pageant_add_list_and_remove_use_only_the_fixture() {
    let executable = PathBuf::from(std::env::var_os("TERMCRAFT_TEST_PAGEANT_EXE").unwrap())
        .canonicalize()
        .unwrap();
    let scratch = tempfile::tempdir().unwrap();
    assert!(
        pageant_window_pid().is_none(),
        "close the running Pageant before testing"
    );
    let mut fixture = Fixture(
        Command::new(executable)
            .current_dir(scratch.path())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while pageant_window_pid().is_none() {
        assert!(fixture.0.try_wait().unwrap().is_none());
        assert!(
            Instant::now() < deadline,
            "fixture did not publish its agent window"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    let pid = fixture.0.id();
    assert_eq!(pageant_window_pid(), Some(pid));
    let pair = KeyManager::generate(
        &scratch.path().join("qualification key"),
        KeySettings::default(),
    )
    .unwrap();
    let manager = AgentManager::new(None);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let target = AgentTarget::Pageant;
        assert!(manager.list(target).await.unwrap().identities.is_empty());
        assert_eq!(pageant_window_pid(), Some(pid));
        let added = manager
            .add(target, &pair.private, None, None)
            .await
            .unwrap();
        let listing = manager.list(target).await.unwrap();
        assert_eq!(listing.identities.len(), 1);
        assert_eq!(listing.identities[0].fingerprint, added.fingerprint);
        assert_eq!(pageant_window_pid(), Some(pid));
        let public = PublicKey::from_openssh(&added.public_key).unwrap();
        let data = b"TermCraft native Pageant qualification";
        let namespace = "termcraft-pageant-fixture";
        let message = SshSig::signed_data(namespace, HashAlg::Sha256, data).unwrap();
        let mut agent = manager.endpoint(target).unwrap().connect().await.unwrap();
        let signature = agent
            .sign_request_signature(&public, None, &message)
            .await
            .unwrap();
        let signed = SshSig::new(
            public.key_data().clone(),
            namespace,
            HashAlg::Sha256,
            signature,
        )
        .unwrap();
        public.verify(namespace, data, &signed).unwrap();

        let mut seed = [0; 32];
        getrandom::fill(&mut seed).unwrap();
        let host =
            PrivateKey::new(Ed25519Keypair::from_seed(&seed).into(), "Pageant fixture").unwrap();
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let known_hosts = scratch.path().join("known_hosts");
        std::fs::write(
            &known_hosts,
            format!(
                "[127.0.0.1]:{port} {}\n",
                host.public_key().to_openssh().unwrap()
            ),
        )
        .unwrap();
        let mut server = KeyOnlyServer(public.clone());
        let endpoint = tokio::spawn(async move {
            let config = Arc::new(server::Config {
                keys: vec![host],
                ..Default::default()
            });
            server.run_on_socket(config, &listener).await.unwrap();
        });
        assert!(ssh_authenticated(port, &known_hosts, AgentMode::Pageant).await);
        assert!(!ssh_authenticated(port, &known_hosts, AgentMode::Disabled).await);
        manager.remove(target, &added.public_key).await.unwrap();
        assert!(manager.list(target).await.unwrap().identities.is_empty());
        assert!(
            agent
                .sign_request_signature(&public, None, &message)
                .await
                .is_err()
        );
        assert!(!ssh_authenticated(port, &known_hosts, AgentMode::Pageant).await);
        endpoint.abort();
        assert!(endpoint.await.unwrap_err().is_cancelled());
        assert!(std::net::TcpStream::connect(("127.0.0.1", port)).is_err());
    });
    drop(fixture);
    assert!(pageant_window_pid().is_none());
}
