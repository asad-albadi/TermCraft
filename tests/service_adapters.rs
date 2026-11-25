use harbor_workspace::{
    credentials::{
        Secret,
        keys::{KeyManager, KeySettings},
    },
    runtime::{RuntimeCatalog, ToolId, ToolProvider},
    services::{
        IperfServiceConfig, IperfVersion, ServiceState, ServiceSupervisor, SshAuthorizedUser,
        SshServiceConfig,
    },
};
use std::{
    net::{SocketAddr, TcpListener},
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
struct Client(Arc<Mutex<Option<String>>>);
impl russh::client::Handler for Client {
    type Error = russh::Error;
    async fn check_server_key(
        &mut self,
        key: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        *self.0.lock().unwrap() = Some(
            key.public_key()
                .fingerprint(russh::keys::HashAlg::Sha256)
                .to_string(),
        );
        Ok(true)
    }
}
async fn connect(
    address: SocketAddr,
) -> (russh::client::Handle<Client>, Arc<Mutex<Option<String>>>) {
    let fingerprint = Arc::new(Mutex::new(None));
    let client = tokio::time::timeout(
        Duration::from_secs(5),
        russh::client::connect(
            Arc::new(russh::client::Config::default()),
            address,
            Client(fingerprint.clone()),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    (client, fingerprint)
}
fn stop(supervisor: &ServiceSupervisor, id: harbor_workspace::services::ServiceId) {
    supervisor.stop(id).unwrap();
    let end = Instant::now() + Duration::from_secs(3);
    loop {
        if supervisor.status(id).unwrap().state == ServiceState::Stopped {
            return;
        }
        assert!(Instant::now() < end, "service did not stop");
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn ssh_config(root: &Path) -> SshServiceConfig {
    let mut config = SshServiceConfig::local(root.join("host"), {
        let share = root.join("share");
        std::fs::create_dir_all(&share).unwrap();
        share
    });
    config
        .users
        .push(SshAuthorizedUser::password("alice", Secret::new("secret")));
    config
}
async fn sftp(client: &russh::client::Handle<Client>) -> russh_sftp::client::SftpSession {
    let channel = client.channel_open_session().await.unwrap();
    channel.request_subsystem(true, "sftp").await.unwrap();
    russh_sftp::client::SftpSession::new(channel.into_stream())
        .await
        .unwrap()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ssh_service_auth_and_sftp_match_config() {
    let root = tempfile::tempdir().unwrap();
    let share = root.path().join("share");
    std::fs::create_dir(&share).unwrap();
    std::fs::write(share.join("read.txt"), b"existing").unwrap();
    let key = KeyManager::generate(&root.path().join("client"), KeySettings::default()).unwrap();
    let private = Arc::new(russh::keys::load_secret_key(&key.private, None).unwrap());
    let mut config = ssh_config(root.path());
    config.allow_write = true;
    config.users.push(SshAuthorizedUser::keys(
        "key-user",
        vec![std::fs::read_to_string(key.public).unwrap()],
    ));
    let supervisor = ServiceSupervisor::default();
    let id = supervisor.start_ssh(config).unwrap();
    let address = supervisor.status(id).unwrap().address.unwrap();
    let (mut denied, _) = connect(address).await;
    assert!(
        !denied
            .authenticate_password("mallory", "secret")
            .await
            .unwrap()
            .success()
    );
    let (mut wrong, _) = connect(address).await;
    assert!(
        !wrong
            .authenticate_password("alice", "wrong")
            .await
            .unwrap()
            .success()
    );
    let (mut key_client, _) = connect(address).await;
    assert!(
        key_client
            .authenticate_publickey(
                "key-user",
                russh::keys::PrivateKeyWithHashAlg::new(private.clone(), None)
            )
            .await
            .unwrap()
            .success()
    );
    let (mut denied_key, _) = connect(address).await;
    assert!(
        !denied_key
            .authenticate_publickey(
                "mallory",
                russh::keys::PrivateKeyWithHashAlg::new(private, None)
            )
            .await
            .unwrap()
            .success()
    );
    let (mut allowed, _) = connect(address).await;
    assert!(
        allowed
            .authenticate_password("alice", "secret")
            .await
            .unwrap()
            .success()
    );
    let sftp = sftp(&allowed).await;
    let mut read = sftp.open("/read.txt").await.unwrap();
    let mut content = Vec::new();
    read.read_to_end(&mut content).await.unwrap();
    assert_eq!(content, b"existing");
    read.shutdown().await.unwrap();
    let mut upload = sftp.create("new.txt").await.unwrap();
    upload.write_all(b"uploaded data").await.unwrap();
    upload.shutdown().await.unwrap();
    assert_eq!(
        std::fs::read(share.join("new.txt")).unwrap(),
        b"uploaded data"
    );
    assert!(
        sftp.create("read.txt").await.is_err(),
        "existing files are not overwritten"
    );
    sftp.rename("new.txt", "renamed.txt").await.unwrap();
    assert!(share.join("renamed.txt").is_file());
    assert!(sftp.read_dir("/").await.unwrap().count() >= 2);
    sftp.create_dir("dir").await.unwrap();
    sftp.remove_dir("dir").await.unwrap();
    sftp.remove_file("renamed.txt").await.unwrap();
    assert!(sftp.open("/../../etc/passwd").await.is_err());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("/etc", share.join("escape")).unwrap();
        assert!(sftp.open("/escape/passwd").await.is_err());
    }
    let mut unfinished = sftp.create("unfinished.txt").await.unwrap();
    unfinished.write_all(b"partial").await.unwrap();
    stop(&supervisor, id);
    drop(unfinished);
    drop(sftp);
    assert!(!share.join("unfinished.txt").exists());
    assert!(!std::fs::read_dir(&share).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".harbor-service-stage-")
    }));
    assert!(std::net::TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_err());
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn restart_preserves_service_host_key() {
    let root = tempfile::tempdir().unwrap();
    let supervisor = ServiceSupervisor::default();
    let unrelated = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let id = supervisor.start_ssh(ssh_config(root.path())).unwrap();
    let (mut client, before) = connect(supervisor.status(id).unwrap().address.unwrap()).await;
    assert!(
        client
            .authenticate_password("alice", "secret")
            .await
            .unwrap()
            .success()
    );
    stop(&supervisor, id);
    let id = supervisor.start_ssh(ssh_config(root.path())).unwrap();
    let (mut client, after) = connect(supervisor.status(id).unwrap().address.unwrap()).await;
    assert!(
        client
            .authenticate_password("alice", "secret")
            .await
            .unwrap()
            .success()
    );
    assert_eq!(*before.lock().unwrap(), *after.lock().unwrap());
    assert!(before.lock().unwrap().is_some());
    stop(&supervisor, id);
    assert!(std::net::TcpStream::connect(unrelated.local_addr().unwrap()).is_ok());
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ssh_readonly_and_disabled_capabilities_are_enforced() {
    let root = tempfile::tempdir().unwrap();
    let supervisor = ServiceSupervisor::default();
    let id = supervisor.start_ssh(ssh_config(root.path())).unwrap();
    let (mut client, _) = connect(supervisor.status(id).unwrap().address.unwrap()).await;
    assert!(
        client
            .authenticate_password("alice", "secret")
            .await
            .unwrap()
            .success()
    );
    let sftp = sftp(&client).await;
    assert!(sftp.create("deny.txt").await.is_err());
    stop(&supervisor, id);
    let mut config = ssh_config(root.path());
    config.sftp_root = None;
    let id = supervisor.start_ssh(config).unwrap();
    let (mut client, _) = connect(supervisor.status(id).unwrap().address.unwrap()).await;
    assert!(
        client
            .authenticate_password("alice", "secret")
            .await
            .unwrap()
            .success()
    );
    let mut channel = client.channel_open_session().await.unwrap();
    channel.request_subsystem(true, "sftp").await.unwrap();
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(2), channel.wait())
            .await
            .unwrap(),
        Some(russh::ChannelMsg::Failure)
    ));
    stop(&supervisor, id);
}
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ssh_opt_in_shell_uses_current_account_and_stops() {
    let root = tempfile::tempdir().unwrap();
    let supervisor = ServiceSupervisor::default();
    let mut config = ssh_config(root.path());
    config.shell = Some(harbor_workspace::connection::default_shell().into());
    // default_shell may be a command name; tests select an explicit executable.
    #[cfg(unix)]
    {
        config.shell = Some("/bin/sh".into());
    }
    let id = supervisor.start_ssh(config).unwrap();
    let (mut client, _) = connect(supervisor.status(id).unwrap().address.unwrap()).await;
    assert!(
        client
            .authenticate_password("alice", "secret")
            .await
            .unwrap()
            .success()
    );
    let mut channel = client.channel_open_session().await.unwrap();
    channel
        .request_pty(true, "xterm", 80, 24, 0, 0, &[])
        .await
        .unwrap();
    channel.request_shell(true).await.unwrap();
    channel
        .data(&b"printf 'owned-ssh-shell\\n'\n"[..])
        .await
        .unwrap();
    let mut output = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(message) = channel.wait().await {
            if let russh::ChannelMsg::Data { data } = message {
                output.extend_from_slice(&data);
                if String::from_utf8_lossy(&output).contains("owned-ssh-shell\r\n") {
                    break;
                }
            }
        }
    })
    .await
    .unwrap();
    assert!(String::from_utf8_lossy(&output).contains("owned-ssh-shell"));
    stop(&supervisor, id);
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn iperf_version_and_bind_are_applied() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let catalog = RuntimeCatalog::new(root);
    let supervisor = ServiceSupervisor::default();
    for (version, tool) in [
        (IperfVersion::Two, ToolId::Iperf),
        (IperfVersion::Three, ToolId::Iperf3),
    ] {
        let provider = catalog
            .resolve(tool)
            .expect("qualified runtime must be staged for this evidence test");
        assert!(matches!(provider, ToolProvider::Bundled { .. }));
        let path = provider.executable().unwrap();
        let socket = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = socket.local_addr().unwrap();
        drop(socket);
        let mut config = IperfServiceConfig::local(version, root.into());
        config.bind = address;
        let id = supervisor.start_iperf(config).unwrap();
        assert_eq!(supervisor.status(id).unwrap().address, Some(address));
        let out = std::process::Command::new(path)
            .args([
                "-c",
                "127.0.0.1",
                "-p",
                &address.port().to_string(),
                "-t",
                "1",
                "-b",
                "1M",
            ])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(String::from_utf8_lossy(&out.stdout).contains("bits/sec"));
        stop(&supervisor, id);
        assert!(TcpListener::bind(address).is_ok());
    }
}
#[test]
fn non_loopback_requires_explicit_opt_in() {
    let root = tempfile::tempdir().unwrap();
    let supervisor = ServiceSupervisor::default();
    let mut config = ssh_config(root.path());
    config.bind = ([0, 0, 0, 0], 0).into();
    assert!(supervisor.start_ssh(config).is_err());
    let mut config = IperfServiceConfig::local(IperfVersion::Three, root.path().into());
    config.bind = ([0, 0, 0, 0], 5201).into();
    assert!(supervisor.start_iperf(config).is_err());
}

#[test]
fn ssh_host_keys_cannot_be_shared_over_sftp() {
    let root = tempfile::tempdir().unwrap();
    let supervisor = ServiceSupervisor::default();
    let mut config = ssh_config(root.path());
    config.sftp_root = Some(root.path().into());
    assert!(supervisor.start_ssh(config).is_err());
}
#[cfg(unix)]
#[test]
fn openssh_sftp_client_interoperates_with_owned_provider() {
    assert!(
        Path::new("/usr/bin/sftp").is_file(),
        "OpenSSH SFTP client is required for this interoperability test"
    );
    let root = tempfile::tempdir().unwrap();
    let mut config = ssh_config(root.path());
    let share = root.path().join("share");
    std::fs::write(share.join("download.txt"), b"from-server").unwrap();
    std::fs::write(root.path().join("upload.txt"), b"from-client").unwrap();
    let keys = KeyManager::generate(&root.path().join("client"), KeySettings::default()).unwrap();
    config.users = vec![SshAuthorizedUser::keys(
        "client",
        vec![std::fs::read_to_string(&keys.public).unwrap()],
    )];
    config.allow_write = true;
    let supervisor = ServiceSupervisor::default();
    let id = supervisor.start_ssh(config).unwrap();
    let address = supervisor.status(id).unwrap().address.unwrap();
    std::fs::write(root.path().join("batch"),"ls /\nget /download.txt local.txt\nput upload.txt new.txt\nrename new.txt renamed.txt\nrm renamed.txt\nmkdir folder\nrmdir folder\n").unwrap();
    let result = std::process::Command::new("/usr/bin/sftp")
        .current_dir(root.path())
        .args(["-b", "batch", "-P", &address.port().to_string(), "-i"])
        .arg(&keys.private)
        .args([
            "-o",
            "BatchMode=yes",
            "-o",
            "StrictHostKeyChecking=accept-new",
            "-o",
            "UserKnownHostsFile=known-hosts",
            "client@127.0.0.1",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        std::fs::read(root.path().join("local.txt")).unwrap(),
        b"from-server"
    );
    stop(&supervisor, id);
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn iperf_udp_clients_reach_selected_version() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let supervisor = ServiceSupervisor::default();
    let catalog = RuntimeCatalog::new(root);
    for (version, tool) in [
        (IperfVersion::Two, ToolId::Iperf),
        (IperfVersion::Three, ToolId::Iperf3),
    ] {
        let reservation = std::net::UdpSocket::bind(("127.0.0.1", 0)).unwrap();
        let address = reservation.local_addr().unwrap();
        drop(reservation);
        let mut config = IperfServiceConfig::local(version, root.into());
        config.bind = address;
        config.udp = version == IperfVersion::Two;
        let id = supervisor.start_iperf(config).unwrap();
        let output =
            std::process::Command::new(catalog.resolve(tool).unwrap().executable().unwrap())
                .args([
                    "-c",
                    "127.0.0.1",
                    "-p",
                    &address.port().to_string(),
                    "-u",
                    "-t",
                    "1",
                    "-b",
                    "1M",
                ])
                .output()
                .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("bits/sec"));
        stop(&supervisor, id);
    }
}
#[test]
fn iperf_missing_package_fails_closed() {
    let root = tempfile::tempdir().unwrap();
    let supervisor = ServiceSupervisor::default();
    let config = IperfServiceConfig::local(IperfVersion::Three, root.path().into());
    assert!(supervisor.start_iperf(config).is_err());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn iperf_tampered_package_fails_closed() {
    let root = tempfile::tempdir().unwrap();
    let supervisor = ServiceSupervisor::default();
    std::fs::create_dir_all(root.path().join("runtime/linux-x86_64")).unwrap();
    std::fs::write(root.path().join("runtime/linux-x86_64/iperf3"), b"replaced").unwrap();
    let config = IperfServiceConfig::local(IperfVersion::Three, root.path().into());
    assert!(supervisor.start_iperf(config).is_err());
    let mut config = IperfServiceConfig::local(IperfVersion::Three, root.path().into());
    config.allow_installed_provider = true;
    let error = supervisor.start_iperf(config).unwrap_err();
    assert!(
        error.contains("failed verification"),
        "corrupt packaged bytes cannot trigger installed fallback: {error}"
    );
}
