use harbor_workspace::services::{ServiceConfig, ServiceProtocol, ServiceState, ServiceSupervisor};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::time::{Duration, Instant};

fn stopped(supervisor: &ServiceSupervisor, id: harbor_workspace::services::ServiceId) {
    let deadline = Instant::now() + Duration::from_secs(4);
    while supervisor.status(id).unwrap().state != ServiceState::Stopped {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn occupied_port_reports_start_failure() {
    let other = UdpSocket::bind("127.0.0.1:0").unwrap();
    let root = tempfile::tempdir().unwrap();
    let mut config = ServiceConfig::tftp(root.path().into());
    config.bind = other.local_addr().unwrap();
    let supervisor = ServiceSupervisor::default();
    assert!(supervisor.start(config).is_err());
    assert!(
        supervisor
            .statuses()
            .iter()
            .all(|status| status.state != ServiceState::Running)
    );
}

#[test]
fn service_stop_closes_only_owned_listener() {
    let other = TcpListener::bind("127.0.0.1:0").unwrap();
    let root = tempfile::tempdir().unwrap();
    let config = ServiceConfig::ftp(
        root.path().into(),
        "test".into(),
        harbor_workspace::credentials::Secret::new("test password"),
    );
    let supervisor = ServiceSupervisor::default();
    let id = supervisor.start(config).unwrap();
    let status = supervisor.status(id).unwrap();
    assert_eq!(status.protocol, ServiceProtocol::Ftp);
    let address = status.address.unwrap();
    assert!(TcpStream::connect(address).is_ok());
    supervisor.stop(id).unwrap();
    stopped(&supervisor, id);
    assert!(TcpStream::connect(address).is_err());
    assert!(TcpListener::bind(address).is_ok());
    assert!(TcpStream::connect(other.local_addr().unwrap()).is_ok());
}

#[test]
fn tftp_cannot_escape_share_root() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("root");
    std::fs::create_dir(&root).unwrap();
    let mut config = ServiceConfig::tftp(root.clone());
    config.allow_write = true;
    let supervisor = ServiceSupervisor::default();
    let id = supervisor.start(config).unwrap();
    let endpoint = supervisor.status(id).unwrap().address.unwrap();
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(parent.path(), root.join("outside")).unwrap();
    for file in [
        "../outside.txt",
        "%2e%2e/outside.txt",
        "outside/outside.txt",
    ] {
        let request = format!("\0\x02{file}\0octet\0");
        socket.send_to(request.as_bytes(), endpoint).unwrap();
        let mut reply = [0; 516];
        let (length, _) = socket.recv_from(&mut reply).unwrap();
        assert!(length >= 4);
        assert_eq!(&reply[..2], &[0, 5]);
    }
    assert!(!parent.path().join("outside.txt").exists());
}

use harbor_workspace::{
    credentials::Secret,
    protocols::ftp::{FtpBackend, FtpDataMode, FtpOptions, FtpTlsMode, FtpTransferType},
    remote_files::{EntryKind, RemotePath},
};
use std::{
    io::{BufRead, BufReader},
    net::SocketAddr,
};

fn remote(path: &str) -> RemotePath {
    RemotePath::from_bytes(path.as_bytes().to_vec()).unwrap()
}

async fn ftp_client(
    address: SocketAddr,
    mode: FtpDataMode,
    password: &str,
) -> Result<FtpBackend, String> {
    FtpBackend::connect(FtpOptions {
        host: address.ip().to_string(),
        port: address.port(),
        username: "fixture".into(),
        password: Some(Secret::new(password)),
        tls: FtpTlsMode::Plain,
        protect_data: false,
        data_mode: mode,
        certificate_authority: None,
        timeout: Duration::from_secs(4),
    })
    .await
}

#[tokio::test]
async fn embedded_ftp_client_transfers_and_mutates_in_both_data_modes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("share");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(temp.path().join("outside"), b"private").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(temp.path().join("outside"), root.join("link")).unwrap();
    let mut config = ServiceConfig::ftp(
        root.clone(),
        "fixture".into(),
        Secret::new("ftp fixture password"),
    );
    config.allow_write = true;
    let supervisor = ServiceSupervisor::default();
    let id = supervisor.start(config).unwrap();
    let endpoint = supervisor.status(id).unwrap().address.unwrap();
    assert!(
        ftp_client(endpoint, FtpDataMode::Passive, "wrong")
            .await
            .is_err()
    );

    for mode in [FtpDataMode::Passive, FtpDataMode::Active] {
        let mut client = ftp_client(endpoint, mode, "ftp fixture password")
            .await
            .unwrap();
        assert_eq!(client.pwd().await.unwrap(), remote("/"));
        assert!(
            client
                .read(&remote("/link"), FtpTransferType::Binary, 32)
                .await
                .is_err()
        );
        assert!(
            client
                .read(&remote("/../outside"), FtpTransferType::Binary, 32)
                .await
                .is_err()
        );
        client.mkdir(&remote("/fixture")).await.unwrap();
        let from = remote("/fixture/naïve file.txt");
        let to = remote("/fixture/renamed.txt");
        let data = b"line\nlast\r";
        client
            .write(&from, FtpTransferType::Ascii, data)
            .await
            .unwrap();
        assert_eq!(
            &std::fs::read(root.join("fixture/naïve file.txt")).unwrap(),
            data
        );
        assert_eq!(
            client
                .read(&from, FtpTransferType::Ascii, 64)
                .await
                .unwrap(),
            data
        );
        assert_eq!(client.size(&from).await.unwrap(), data.len() as u64);
        let listing = client.list(&remote("/fixture")).await.unwrap();
        assert_eq!(listing.len(), 1);
        assert_eq!(listing[0].path, from);
        assert_eq!(listing[0].kind, EntryKind::File);
        assert!(
            client
                .write(&from, FtpTransferType::Binary, b"replace")
                .await
                .is_err()
        );
        assert_eq!(
            &std::fs::read(root.join("fixture/naïve file.txt")).unwrap(),
            data
        );
        client.rename(&from, &to).await.unwrap();
        client.remove(&to, EntryKind::File).await.unwrap();
        client
            .remove(&remote("/fixture"), EntryKind::Directory)
            .await
            .unwrap();
    }
    assert!(
        std::fs::read_dir(root)
            .unwrap()
            .all(|entry| entry.unwrap().file_name() == "link")
    );
    assert!(!format!("{:?}", supervisor.status(id)).contains("ftp fixture password"));
}

struct FtpControl(BufReader<TcpStream>);

impl FtpControl {
    fn connect(endpoint: SocketAddr) -> Self {
        let stream = TcpStream::connect(endpoint).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(4)))
            .unwrap();
        let mut client = Self(BufReader::new(stream));
        assert!(client.reply().starts_with("220 "));
        client
    }

    fn reply(&mut self) -> String {
        let mut line = String::new();
        self.0.read_line(&mut line).unwrap();
        line
    }

    fn command(&mut self, line: &str) -> String {
        self.0.get_mut().write_all(line.as_bytes()).unwrap();
        self.reply()
    }

    fn login(&mut self) {
        assert!(self.command("USER fixture\r\n").starts_with("331 "));
        assert!(self.command("PASS password\r\n").starts_with("230 "));
    }

    fn passive_port(&mut self) -> u16 {
        let line = self.command("EPSV\r\n");
        assert!(line.starts_with("229 "));
        line.split('|').nth(3).unwrap().parse().unwrap()
    }
}

#[test]
fn ftp_rejects_bounce_and_keeps_existing_file_on_cancel() {
    let root = tempfile::tempdir().unwrap();
    let existing = root.path().join("existing");
    std::fs::write(&existing, b"keep").unwrap();
    let mut config = ServiceConfig::ftp(
        root.path().into(),
        "fixture".into(),
        Secret::new("password"),
    );
    config.allow_write = true;
    let supervisor = ServiceSupervisor::default();
    let id = supervisor.start(config).unwrap();
    let endpoint = supervisor.status(id).unwrap().address.unwrap();
    let mut control = FtpControl::connect(endpoint);
    assert!(control.command("STOR existing\r\n").starts_with("530 "));
    control.login();
    assert!(
        control
            .command("PORT 127,0,0,2,60,1\r\n")
            .starts_with("501 ")
    );
    assert!(
        control
            .command("EPRT |1|127.0.0.1|80|\r\n")
            .starts_with("501 ")
    );
    assert!(control.command("STOR existing\r\n").starts_with("550 "));
    let passive = control.passive_port();
    let mut data = TcpStream::connect((endpoint.ip(), passive)).unwrap();
    assert!(control.command("STOR incomplete\r\n").starts_with("150 "));
    data.write_all(b"partial bytes").unwrap();
    supervisor.stop(id).unwrap();
    stopped(&supervisor, id);
    assert!(!root.path().join("incomplete").exists());
    assert_eq!(std::fs::read(&existing).unwrap(), b"keep");
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    assert!(TcpListener::bind((endpoint.ip(), passive)).is_ok());
}

#[test]
fn ftp_read_only_and_bad_protocol_frames_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let config = ServiceConfig::ftp(
        root.path().into(),
        "fixture".into(),
        Secret::new("password"),
    );
    let supervisor = ServiceSupervisor::default();
    let id = supervisor.start(config).unwrap();
    let endpoint = supervisor.status(id).unwrap().address.unwrap();
    let mut control = FtpControl::connect(endpoint);
    control.login();
    assert!(control.command("STOR new\r\n").starts_with("550 "));
    assert!(control.command("MKD new\r\n").starts_with("550 "));
    assert!(control.command("RNFR new\r\n").starts_with("550 "));
    assert!(control.command("NOOP\n").starts_with("500 "));
    assert_eq!(control.reply(), "");
    assert!(!root.path().join("new").exists());
}

fn udp_client() -> UdpSocket {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    socket
}

fn packet(socket: &UdpSocket) -> (Vec<u8>, SocketAddr) {
    let mut buffer = [0; 517];
    let (len, sender) = socket.recv_from(&mut buffer).unwrap();
    (buffer[..len].to_vec(), sender)
}

#[test]
fn tftp_transfers_real_blocks_retries_and_checks_tid() {
    let root = tempfile::tempdir().unwrap();
    let mut config = ServiceConfig::tftp(root.path().into());
    config.allow_write = true;
    let supervisor = ServiceSupervisor::default();
    let id = supervisor.start(config).unwrap();
    let endpoint = supervisor.status(id).unwrap().address.unwrap();
    let writer = udp_client();
    writer.send_to(b"\0\x02sample\0octet\0", endpoint).unwrap();
    let (ack, transfer) = packet(&writer);
    assert_eq!(ack, [0, 4, 0, 0]);
    assert_ne!(transfer.port(), endpoint.port());
    let mut data = vec![0, 3, 0, 1];
    data.extend([b'a'; 512]);
    let wrong_tid = udp_client();
    wrong_tid.send_to(&data, transfer).unwrap();
    assert_eq!(&packet(&wrong_tid).0[..4], &[0, 5, 0, 5]);
    assert_eq!(packet(&writer).0, [0, 4, 0, 0]); // Lost ACK0 triggers retransmission.
    writer.send_to(&data, transfer).unwrap();
    assert_eq!(packet(&writer).0, [0, 4, 0, 1]);
    writer.send_to(b"\0\x03\0\x02last", transfer).unwrap();
    assert_eq!(packet(&writer).0, [0, 4, 0, 2]);
    writer.send_to(b"\0\x03\0\x02last", transfer).unwrap();
    assert_eq!(packet(&writer).0, [0, 4, 0, 2]); // Final ACK dally.

    let reader = udp_client();
    reader.send_to(b"\0\x01sample\0octet\0", endpoint).unwrap();
    let (first, transfer) = packet(&reader);
    assert_eq!(first, data);
    assert_eq!(packet(&reader).0, first);
    reader.send_to(&[0, 4, 0, 1], transfer).unwrap();
    assert_eq!(packet(&reader).0, b"\0\x03\0\x02last");
    reader.send_to(&[0, 4, 0, 2], transfer).unwrap();

    let conflict = udp_client();
    conflict
        .send_to(b"\0\x02sample\0octet\0", endpoint)
        .unwrap();
    assert_eq!(&packet(&conflict).0[..4], &[0, 5, 0, 6]);
    let mut expected = vec![b'a'; 512];
    expected.extend_from_slice(b"last");
    assert_eq!(std::fs::read(root.path().join("sample")).unwrap(), expected);
}

#[test]
fn tftp_partial_upload_times_out_and_read_only_never_creates_files() {
    let root = tempfile::tempdir().unwrap();
    let mut config = ServiceConfig::tftp(root.path().into());
    config.allow_write = true;
    let supervisor = ServiceSupervisor::default();
    let id = supervisor.start(config).unwrap();
    let endpoint = supervisor.status(id).unwrap().address.unwrap();
    let client = udp_client();
    client.send_to(b"\0\x02partial\0octet\0", endpoint).unwrap();
    let (_, transfer) = packet(&client);
    let mut data = vec![0, 3, 0, 1];
    data.extend([b'x'; 512]);
    client.send_to(&data, transfer).unwrap();
    assert_eq!(packet(&client).0, [0, 4, 0, 1]);
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        let (received, _) = packet(&client);
        if received.starts_with(&[0, 5]) {
            break;
        }
        assert!(Instant::now() < deadline);
    }
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    supervisor.stop(id).unwrap();
    stopped(&supervisor, id);
    let mut config = ServiceConfig::tftp(root.path().into());
    config.bind = endpoint;
    let restarted = supervisor.start(config).unwrap();
    assert_ne!(id, restarted);
    client.send_to(b"\0\x02blocked\0octet\0", endpoint).unwrap();
    assert_eq!(&packet(&client).0[..4], &[0, 5, 0, 2]);
    client.send_to(b"\0\x01broken\0", endpoint).unwrap();
    assert_eq!(&packet(&client).0[..4], &[0, 5, 0, 4]);
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn network_exposure_requires_explicit_permission() {
    let root = tempfile::tempdir().unwrap();
    let mut config = ServiceConfig::tftp(root.path().into());
    config.bind = "0.0.0.0:0".parse().unwrap();
    let supervisor = ServiceSupervisor::default();
    let id = supervisor.begin(config).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let status = supervisor.status(id).unwrap();
        assert!(status.address.is_none());
        if status.state == ServiceState::Failed {
            assert!(status.error.unwrap().contains("non-loopback"));
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(any(unix, windows))]
fn wait_for(stream: &mut TcpStream, text: &str) -> String {
    let mut result = Vec::new();
    let mut buffer = [0; 1024];
    let deadline = Instant::now() + Duration::from_secs(4);
    while !result
        .windows(text.len())
        .any(|part| part == text.as_bytes())
    {
        assert!(Instant::now() < deadline);
        let count = stream.read(&mut buffer).unwrap();
        assert_ne!(count, 0, "{}", String::from_utf8_lossy(&result));
        result.extend_from_slice(&buffer[..count]);
    }
    String::from_utf8_lossy(&result).into_owned()
}

#[cfg(unix)]
#[test]
fn telnet_authenticates_before_spawning_and_stops_only_owned_pty() {
    use std::{os::unix::fs::PermissionsExt, process::Command};
    let root = tempfile::tempdir().unwrap();
    let marker = root.path().join("launched");
    let shell = root.path().join("fixture-shell");
    std::fs::write(&shell, format!(
        "#!/bin/sh\n[ -t 0 ] && [ -t 1 ] || exit 1\nprintf '%s' \"$$\" > '{}'\nprintf 'PTY_READY\\n'\nexec /bin/sh\n",
        marker.display(),
    )).unwrap();
    std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut unrelated = Command::new("sleep").arg("15").spawn().unwrap();
    let supervisor = ServiceSupervisor::default();
    let id = supervisor
        .start(ServiceConfig::telnet(
            shell,
            "fixture".into(),
            Secret::new("pty password"),
        ))
        .unwrap();
    let endpoint = supervisor.status(id).unwrap().address.unwrap();
    let connect = || {
        let stream = TcpStream::connect(endpoint).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(4)))
            .unwrap();
        stream
    };
    let mut denied = connect();
    wait_for(&mut denied, "Workspace login: ");
    denied.write_all(b"fixture\r\n").unwrap();
    wait_for(&mut denied, "Password: ");
    denied.write_all(b"wrong\r\n").unwrap();
    wait_for(&mut denied, "Authentication failed.");
    assert!(!marker.exists());

    let mut allowed = connect();
    wait_for(&mut allowed, "Workspace login: ");
    allowed
        .write_all(&[255, 251, 31, 255, 250, 31, 0, 99, 0, 35, 255, 240])
        .unwrap();
    allowed.write_all(b"fixture\r\n").unwrap();
    wait_for(&mut allowed, "Password: ");
    assert!(!marker.exists());
    allowed.write_all(b"pty password\r\n").unwrap();
    assert!(!wait_for(&mut allowed, "PTY_READY").contains("pty password"));
    let owned: i32 = std::fs::read_to_string(marker).unwrap().parse().unwrap();
    allowed
        .write_all(b"printf 'RESULT:%s\\n' \"$(stty size)\"\r\n")
        .unwrap();
    wait_for(&mut allowed, "RESULT:35 99");
    supervisor.stop(id).unwrap();
    stopped(&supervisor, id);
    assert_eq!(unsafe { libc::kill(owned, 0) }, -1);
    assert!(unrelated.try_wait().unwrap().is_none());
    unrelated.kill().unwrap();
    unrelated.wait().unwrap();
}

#[cfg(windows)]
#[test]
fn windows_telnet_bridges_native_conpty_and_stops_the_owned_child() {
    use std::process::Command;
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };

    struct ChildHandle(HANDLE);
    impl Drop for ChildHandle {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }

    let root = tempfile::tempdir().unwrap();
    let shell = root.path().join("isolated console fixture.exe");
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/support/windows_telnet_shell.rs");
    let compile = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
        .arg(source)
        .arg("-o")
        .arg(&shell)
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let supervisor = ServiceSupervisor::default();
    let id = supervisor
        .start(ServiceConfig::telnet(
            shell,
            "fixture".into(),
            Secret::new("fixture-password"),
        ))
        .unwrap();
    let endpoint = supervisor.status(id).unwrap().address.unwrap();
    let login = |password: &[u8]| {
        let mut stream = TcpStream::connect(endpoint).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(4)))
            .unwrap();
        wait_for(&mut stream, "Workspace login: ");
        stream.write_all(b"fixture\r\n").unwrap();
        wait_for(&mut stream, "Password: ");
        stream.write_all(password).unwrap();
        stream
    };
    let mut denied = login(b"wrong\r\n");
    let rejection = wait_for(&mut denied, "Authentication failed.");
    assert!(!rejection.contains("TC_PID:"));

    let mut client = login(b"fixture-password\r\n");
    let response = wait_for(&mut client, ":END");
    assert!(!response.contains("fixture-password"));
    let (_, response) = response.split_once("TC_PID:").unwrap();
    let pid = response
        .split_once(":END")
        .unwrap()
        .0
        .parse::<u32>()
        .unwrap();
    let handle = ChildHandle(unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) });
    assert!(!handle.0.is_null());
    assert_eq!(unsafe { WaitForSingleObject(handle.0, 0) }, WAIT_TIMEOUT);
    client.write_all(b"round trip\r\n").unwrap();
    wait_for(&mut client, "TC_RESULT:round trip:END");
    supervisor.stop(id).unwrap();
    stopped(&supervisor, id);
    assert_eq!(
        unsafe { WaitForSingleObject(handle.0, 4000) },
        WAIT_OBJECT_0
    );
    assert!(TcpStream::connect_timeout(&endpoint, Duration::from_millis(500)).is_err());
}

#[test]
fn service_lifetime_stops_and_releases_listener() {
    let root = tempfile::tempdir().unwrap();
    let mut config = ServiceConfig::tftp(root.path().into());
    config.lifetime = Some(Duration::from_secs(1));
    let supervisor = ServiceSupervisor::default();
    let id = supervisor.start(config).unwrap();
    let endpoint = supervisor.status(id).unwrap().address.unwrap();
    stopped(&supervisor, id);
    assert!(UdpSocket::bind(endpoint).is_ok());
}

#[test]
fn concurrent_ftp_uploads_cannot_replace_a_published_file() {
    use std::net::Shutdown;

    let root = tempfile::tempdir().unwrap();
    let mut config = ServiceConfig::ftp(
        root.path().into(),
        "fixture".into(),
        Secret::new("password"),
    );
    config.allow_write = true;
    let supervisor = ServiceSupervisor::default();
    let id = supervisor.start(config).unwrap();
    let endpoint = supervisor.status(id).unwrap().address.unwrap();
    let upload = || {
        let mut control = FtpControl::connect(endpoint);
        control.login();
        let passive = control.passive_port();
        let data = TcpStream::connect((endpoint.ip(), passive)).unwrap();
        assert!(control.command("STOR shared\r\n").starts_with("150 "));
        (control, data)
    };
    let (mut first_control, mut first_data) = upload();
    let (mut second_control, mut second_data) = upload();
    first_data.write_all(b"first").unwrap();
    second_data.write_all(b"second").unwrap();
    first_data.shutdown(Shutdown::Write).unwrap();
    assert!(first_control.reply().starts_with("226 "));
    second_data.shutdown(Shutdown::Write).unwrap();
    assert!(second_control.reply().starts_with("550 "));
    assert_eq!(std::fs::read(root.path().join("shared")).unwrap(), b"first");
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn passive_ftp_rejects_a_data_socket_from_another_peer() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("sample"), b"private-bytes").unwrap();
    let config = ServiceConfig::ftp(
        root.path().into(),
        "fixture".into(),
        Secret::new("password"),
    );
    let supervisor = ServiceSupervisor::default();
    let id = supervisor.start(config).unwrap();
    let endpoint = supervisor.status(id).unwrap().address.unwrap();
    let mut control = FtpControl::connect(endpoint);
    control.login();
    let passive = control.passive_port();
    let data_endpoint = SocketAddr::new(endpoint.ip(), passive);

    let socket = tokio::net::TcpSocket::new_v4().unwrap();
    socket.bind("127.0.0.2:0".parse().unwrap()).unwrap();
    let wrong_peer = socket.connect(data_endpoint).await.unwrap();
    let mut expected_peer = TcpStream::connect(data_endpoint).unwrap();
    expected_peer
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    assert!(control.command("RETR sample\r\n").starts_with("150 "));
    let mut data = Vec::new();
    expected_peer.read_to_end(&mut data).unwrap();
    assert_eq!(data, b"private-bytes");
    assert!(control.reply().starts_with("226 "));
    let mut other = [0; 32];
    tokio::time::timeout(Duration::from_secs(3), wrong_peer.readable())
        .await
        .unwrap()
        .unwrap();
    match wrong_peer.try_read(&mut other) {
        Ok(0) => {}
        Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => {}
        result => panic!("Unexpected passive socket result: {result:?}"),
    }
}

#[test]
fn tftp_does_not_answer_error_datagrams() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("sample"), b"bytes").unwrap();
    let supervisor = ServiceSupervisor::default();
    let id = supervisor
        .start(ServiceConfig::tftp(root.path().into()))
        .unwrap();
    let endpoint = supervisor.status(id).unwrap().address.unwrap();
    let foreign = udp_client();
    foreign
        .set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    let error = b"\0\x05\0\x05Unknown transfer ID\0";
    let response = |target| {
        foreign.send_to(error, target).unwrap();
        foreign.recv_from(&mut [0; 517])
    };

    let reader = udp_client();
    reader.send_to(b"\0\x01sample\0octet\0", endpoint).unwrap();
    let (data, transfer) = packet(&reader);
    assert_eq!(data, b"\0\x03\0\x01bytes");
    let listener = response(endpoint);
    let worker = response(transfer);
    reader.send_to(&[0, 4, 0, 1], transfer).unwrap();
    let silence = |result: &std::io::Result<_>| {
        result.as_ref().is_err_and(|error| {
            matches!(
                error.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            )
        })
    };
    assert!(
        silence(&listener) && silence(&worker),
        "TFTP listener: {listener:?}, transfer: {worker:?}"
    );
}

#[cfg(unix)]
#[test]
fn telnet_delivers_complete_pty_output_after_shell_exit() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let shell = root.path().join("finite-shell");
    let payload_size = 512 * 1024;
    std::fs::write(
        &shell,
        format!(
            "#!/bin/sh\nhead -c {payload_size} /dev/zero | tr '\\000' Q\nprintf 'PTY_DONE\\n'\n",
        ),
    )
    .unwrap();
    std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o700)).unwrap();
    let supervisor = ServiceSupervisor::default();
    let id = supervisor
        .start(ServiceConfig::telnet(
            shell,
            "fixture".into(),
            Secret::new("password"),
        ))
        .unwrap();
    let endpoint = supervisor.status(id).unwrap().address.unwrap();
    let mut client = TcpStream::connect(endpoint).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(6)))
        .unwrap();
    wait_for(&mut client, "Workspace login: ");
    client.write_all(b"fixture\r\n").unwrap();
    wait_for(&mut client, "Password: ");
    client.write_all(b"password\r\n").unwrap();
    let mut output = Vec::new();
    client.read_to_end(&mut output).unwrap();
    assert_eq!(
        output.iter().filter(|&&byte| byte == b'Q').count(),
        payload_size
    );
    assert!(output.ends_with(b"PTY_DONE\r\n"));
}

#[test]
fn ftp_abor_interrupts_idle_upload_and_preserves_control_session() {
    let root = tempfile::tempdir().unwrap();
    let mut config = ServiceConfig::ftp(
        root.path().into(),
        "fixture".into(),
        Secret::new("password"),
    );
    config.allow_write = true;
    let supervisor = ServiceSupervisor::default();
    let id = supervisor.start(config).unwrap();
    let endpoint = supervisor.status(id).unwrap().address.unwrap();
    let mut control = FtpControl::connect(endpoint);
    control.login();
    control
        .0
        .get_mut()
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();

    for (mode, connect) in [
        (FtpDataMode::Passive, true),
        (FtpDataMode::Passive, false),
        (FtpDataMode::Active, true),
    ] {
        let mut passive_data = None;
        let mut active_listener = None;
        match mode {
            FtpDataMode::Passive => {
                let port = control.passive_port();
                if connect {
                    passive_data = Some(TcpStream::connect((endpoint.ip(), port)).unwrap());
                }
            }
            FtpDataMode::Active => {
                let listener = TcpListener::bind((endpoint.ip(), 0)).unwrap();
                let address = listener.local_addr().unwrap();
                assert!(
                    control
                        .command(&format!("EPRT |1|{}|{}|\r\n", address.ip(), address.port()))
                        .starts_with("200 ")
                );
                active_listener = Some(listener);
            }
        }
        assert!(
            control
                .command("STOR incompletely-uploaded\r\n")
                .starts_with("150 ")
        );
        let mut data = active_listener
            .map(|listener| listener.accept().unwrap().0)
            .or(passive_data);
        if let Some(data) = data.as_mut() {
            data.write_all(b"uncommitted bytes").unwrap();
        }
        control.0.get_mut().write_all(b"NOOP\r\nAB").unwrap();
        std::thread::sleep(Duration::from_millis(75));
        control.0.get_mut().write_all(b"OR\r\nNOOP\r\n").unwrap();
        assert!(control.reply().starts_with("426 "));
        assert!(control.reply().starts_with("226 "));
        assert!(control.reply().starts_with("200 "));
        assert!(control.reply().starts_with("200 "));
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }
}

#[test]
fn queued_ftp_noop_does_not_discard_a_completed_upload() {
    use std::net::Shutdown;

    let root = tempfile::tempdir().unwrap();
    let mut config = ServiceConfig::ftp(
        root.path().into(),
        "fixture".into(),
        Secret::new("password"),
    );
    config.allow_write = true;
    let supervisor = ServiceSupervisor::default();
    let id = supervisor.start(config).unwrap();
    let endpoint = supervisor.status(id).unwrap().address.unwrap();
    let mut control = FtpControl::connect(endpoint);
    control.login();
    let passive = control.passive_port();
    let mut data = TcpStream::connect((endpoint.ip(), passive)).unwrap();
    assert!(control.command("STOR upload\r\n").starts_with("150 "));
    data.write_all(b"complete payload").unwrap();
    control.0.get_mut().write_all(b"NOOP\r\n").unwrap();
    std::thread::sleep(Duration::from_millis(150));
    data.shutdown(Shutdown::Write).unwrap();

    let completed = control.reply();
    let queued = control.reply();
    let stored = std::fs::read(root.path().join("upload"));
    assert!(
        completed.starts_with("226 ") && queued.starts_with("200 "),
        "transfer {completed:?}, NOOP {queued:?}, upload {stored:?}"
    );
    assert_eq!(stored.unwrap(), b"complete payload");
}
