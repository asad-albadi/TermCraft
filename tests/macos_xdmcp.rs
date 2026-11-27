#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

use harbor_workspace::x11::{ManagedXdmcp, XServerManager, xdmcp::XdmcpConfig};
use std::{
    net::{Ipv4Addr, TcpListener},
    sync::atomic::AtomicBool,
};

#[test]
fn macos_xdmcp_opt_in_and_listener_collision_fail_before_package_access() {
    let held = (6100..6200)
        .find_map(|port| TcpListener::bind((Ipv4Addr::LOCALHOST, port)).ok())
        .expect("available owned display port");
    let port = held.local_addr().unwrap().port();
    let empty = tempfile::tempdir().unwrap();
    let manager = XServerManager::for_installation(empty.path().into());
    let mut config = XdmcpConfig {
        selected_endpoint: Some("127.0.0.1:1177".parse().unwrap()),
        display: Some(port - 6000),
        ..Default::default()
    };
    let error = ManagedXdmcp::start(manager.clone(), config.clone(), 800, 600)
        .err()
        .expect("missing opt-in must fail synchronously");
    assert!(error.contains("listener"), "{error}");
    config.allow_network_listener = true;
    let error = manager
        .start_xdmcp_cancellable(config, 800, 600, &AtomicBool::new(false))
        .err()
        .expect("occupied display must not be adopted");
    assert!(
        error.contains("bind") && error.contains(&port.to_string()),
        "{error}"
    );
    assert_eq!(held.local_addr().unwrap().port(), port);
}

use harbor_workspace::x11::{XdmcpSession, xdmcp::QueryMode};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream, UdpSocket},
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};
use x11rb::{
    connection::Connection,
    protocol::xproto::{self, ConnectionExt},
    rust_connection::{DefaultStream, RustConnection},
};

const MAGIC: &[u8] = b"MIT-MAGIC-COOKIE-1";

fn manager_socket() -> UdpSocket {
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    socket
}

fn send(socket: &UdpSocket, remote: SocketAddr, opcode: u16, body: &[u8]) {
    let packet = [
        1u16.to_be_bytes(),
        opcode.to_be_bytes(),
        (body.len() as u16).to_be_bytes(),
    ]
    .concat();
    socket
        .send_to(&[packet.as_slice(), body].concat(), remote)
        .unwrap();
}

fn receive(socket: &UdpSocket, wanted: u16) -> (Vec<u8>, SocketAddr) {
    let mut data = [0; 1024];
    loop {
        let (read, sender) = socket
            .recv_from(&mut data)
            .unwrap_or_else(|e| panic!("waiting for XDMCP opcode {wanted}: {e}"));
        assert!(read >= 6);
        assert_eq!(data[..2], [0, 1]);
        assert_eq!(
            usize::from(u16::from_be_bytes(data[4..6].try_into().unwrap())),
            read - 6
        );
        if u16::from_be_bytes(data[2..4].try_into().unwrap()) == wanted {
            return (data[6..read].to_vec(), sender);
        }
    }
}

fn willing(socket: &UdpSocket, endpoint: SocketAddr) {
    send(socket, endpoint, 5, b"\0\0\0\x04peer\0\x05ready");
}

fn negotiate(socket: &UdpSocket, endpoint: SocketAddr, cookie: &[u8; 16], id: u32) -> SocketAddr {
    willing(socket, endpoint);
    let (req, sender) = receive(socket, 7);
    assert_eq!(sender, endpoint);
    let display = u16::from_be_bytes(req[..2].try_into().unwrap());
    let mut expected = b"\x01\0\0\x01\0\x04\x7f\0\0\x01\0\0\0\0\x01\0\x12".to_vec();
    expected.extend(MAGIC);
    expected.extend(b"\0\0");
    assert_eq!(&req[2..], &expected);
    let mut accept = id.to_be_bytes().to_vec();
    accept.extend(b"\0\0\0\0\0\x12");
    accept.extend(MAGIC);
    accept.extend(b"\0\x10");
    accept.extend(cookie);
    send(socket, endpoint, 8, &accept);
    let (manage, sender) = receive(socket, 10);
    assert_eq!(sender, endpoint);
    assert_eq!(
        manage,
        [id.to_be_bytes().as_slice(), &req[..2], b"\0\0"].concat()
    );
    SocketAddr::from((Ipv4Addr::LOCALHOST, 6000 + display))
}

fn connect(address: SocketAddr) -> TcpStream {
    let stream = TcpStream::connect_timeout(&address, Duration::from_secs(1)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream
}

fn raw_setup(big_endian: bool, cookie: Option<&[u8; 16]>) -> Vec<u8> {
    let mut request = vec![0; if cookie.is_some() { 48 } else { 12 }];
    request[0] = if big_endian { b'B' } else { b'l' };
    let word = if big_endian {
        u16::to_be_bytes
    } else {
        u16::to_le_bytes
    };
    request[2..4].copy_from_slice(&word(11));
    if let Some(cookie) = cookie {
        request[6..8].copy_from_slice(&word(18));
        request[8..10].copy_from_slice(&word(16));
        request[12..30].copy_from_slice(MAGIC);
        request[32..].copy_from_slice(cookie);
    }
    request
}

fn rejected(address: SocketAddr, cookie: Option<&[u8; 16]>) {
    let mut stream = connect(address);
    stream.write_all(&raw_setup(false, cookie)).unwrap();
    match stream.read(&mut [0]) {
        Ok(0) => {}
        Err(e) if e.kind() == std::io::ErrorKind::ConnectionReset => {}
        result => panic!("TCP client was not rejected: {result:?}"),
    }
}

fn mapped_green_window(address: SocketAddr, cookie: &[u8; 16]) -> RustConnection {
    let (stream, _) = DefaultStream::from_tcp_stream(connect(address)).unwrap();
    let client = RustConnection::connect_to_stream_with_auth_info(
        stream,
        0,
        MAGIC.to_vec(),
        cookie.to_vec(),
    )
    .unwrap();
    let screen = &client.setup().roots[0];
    let window = client.generate_id().unwrap();
    client
        .create_window(
            screen.root_depth,
            window,
            screen.root,
            45,
            45,
            150,
            120,
            0,
            xproto::WindowClass::INPUT_OUTPUT,
            screen.root_visual,
            &xproto::CreateWindowAux::new()
                .override_redirect(1)
                .background_pixel(0x229944),
        )
        .unwrap()
        .check()
        .unwrap();
    client.map_window(window).unwrap().check().unwrap();
    assert_eq!(
        client
            .get_window_attributes(window)
            .unwrap()
            .reply()
            .unwrap()
            .map_state,
        xproto::MapState::VIEWABLE
    );
    let pixel = client
        .get_image(
            xproto::ImageFormat::Z_PIXMAP,
            window,
            25,
            25,
            1,
            1,
            u32::MAX,
        )
        .unwrap()
        .reply()
        .unwrap()
        .data;
    assert_eq!(pixel[..3], [0x44, 0x99, 0x22], "{pixel:?}");
    client
}

fn packaged_manager() -> XServerManager {
    match std::env::var_os("TERMCRAFT_TEST_XQUARTZ_ROOT") {
        Some(root) => XServerManager::for_installation(PathBuf::from(root)),
        None => {
            assert!(
                std::env::current_exe()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .ends_with("Contents/MacOS"),
                "stage this test under Contents/MacOS or set TERMCRAFT_TEST_XQUARTZ_ROOT"
            );
            XServerManager::default()
        }
    }
}

fn start(socket: &UdpSocket, mode: QueryMode) -> XdmcpSession {
    let manager = packaged_manager();
    manager
        .start_xdmcp_cancellable(
            XdmcpConfig {
                selected_endpoint: Some(socket.local_addr().unwrap()),
                mode,
                allow_network_listener: true,
                ..Default::default()
            },
            800,
            600,
            &AtomicBool::new(false),
        )
        .unwrap()
}

fn wait_until_finished(session: &mut XdmcpSession) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while session.is_running().unwrap() {
        assert!(
            Instant::now() < deadline,
            "session did not end with the initial X connection"
        );
        thread::sleep(Duration::from_millis(25));
    }
}

fn assert_clean(authority: &std::path::Path, pid: u32, address: SocketAddr) {
    assert!(
        !authority.exists() && !authority.parent().unwrap().exists(),
        "authority or private directory survived"
    );
    assert_eq!(unsafe { libc::kill(pid as i32, 0) }, -1);
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ESRCH)
    );
    assert!(TcpStream::connect_timeout(&address, Duration::from_millis(200)).is_err());
}

#[test]
#[ignore = "requires the staged pinned XQuartz package and a macOS GUI session"]
fn macos_xdmcp_direct_cookie_pixel_and_owned_cleanup() {
    let manager = manager_socket();
    let remote = manager.try_clone().unwrap();
    let (close, hold) = std::sync::mpsc::channel();
    let peer = thread::spawn(move || {
        let (query, endpoint) = receive(&remote, 2);
        assert_eq!(query, [0]);
        let decoy = manager_socket();
        willing(&decoy, endpoint);
        // A correct-looking response from the wrong UDP source must not get REQUEST.
        decoy
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        assert!(decoy.recv_from(&mut [0; 64]).is_err());
        let cookie = [0x4d; 16];
        let address = negotiate(&remote, endpoint, &cookie, 0x23324567);
        rejected(address, None);
        rejected(address, Some(&[0x17; 16]));
        let client = mapped_green_window(address, &cookie);
        let mut big_endian = connect(address);
        big_endian
            .write_all(&raw_setup(true, Some(&cookie)))
            .unwrap();
        let mut status = [0];
        big_endian.read_exact(&mut status).unwrap();
        assert_eq!(status, [1]);
        let mut prefix_rest = [0; 7];
        big_endian.read_exact(&mut prefix_rest).unwrap();
        assert_eq!(&prefix_rest[1..5], &[0, 11, 0, 0]);
        hold.recv_timeout(Duration::from_secs(8)).unwrap();
        drop(client);
        // Test that loss of the anchor closes even an additional authenticated client.
        let mut body =
            vec![0; usize::from(u16::from_be_bytes([prefix_rest[5], prefix_rest[6]])) * 4];
        big_endian.read_exact(&mut body).unwrap();
        assert_eq!(big_endian.read(&mut [0]).unwrap(), 0);
        address
    });
    let mut session = start(&manager, QueryMode::Direct);
    assert!(session.is_running().unwrap());
    let pid = session.process_id().unwrap();
    assert_eq!(unsafe { libc::getpgid(pid as i32) }, pid as i32);
    let authority = session.authority_path().to_owned();
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        authority.metadata().unwrap().permissions().mode() & 0o777,
        0o600
    );
    close.send(()).unwrap();
    let address = peer.join().unwrap();
    wait_until_finished(&mut session);
    drop(session);
    assert_clean(&authority, pid, address);
    assert!(manager.local_addr().is_ok());
    eprintln!(
        "Direct: exact UDP source, cookie denial, both byte orders, mapped green pixel, anchor EOF and owned PID/listener/authority cleanup"
    );
}

#[test]
#[ignore = "requires the staged pinned XQuartz package and a macOS GUI session"]
fn macos_xdmcp_indirect_requeries_and_rotates_display_after_chooser() {
    let primary = manager_socket();
    let first = primary.try_clone().unwrap();
    let second = manager_socket();
    let (close, hold) = std::sync::mpsc::channel();
    let (report, observed) = std::sync::mpsc::channel();
    let (finish, wait) = std::sync::mpsc::channel();
    let peer = thread::spawn(move || {
        let (query, endpoint) = receive(&first, 3);
        assert_eq!(query, [0]);
        let old_cookie = [0x71; 16];
        let first_address = negotiate(&first, endpoint, &old_cookie, 0x1234);
        let chooser = mapped_green_window(first_address, &old_cookie);
        hold.recv_timeout(Duration::from_secs(8)).unwrap();
        drop(chooser);
        let (query, again) = receive(&first, 3);
        assert_eq!(query, [0]);
        assert_eq!(
            again, endpoint,
            "chooser handoff must use the same UDP return address"
        );
        let cookie = [0x72; 16];
        let address = negotiate(&second, again, &cookie, 0x5678);
        assert_eq!(
            address, first_address,
            "handoff must retain the reserved display"
        );
        rejected(address, Some(&old_cookie));
        let desktop = mapped_green_window(address, &cookie);
        report.send(address).unwrap();
        wait.recv_timeout(Duration::from_secs(8)).unwrap();
        drop(desktop);
    });
    let mut session = start(&primary, QueryMode::Indirect);
    let first_pid = session.process_id().unwrap();
    let authority = session.authority_path().to_owned();
    close.send(()).unwrap();
    let address = observed.recv_timeout(Duration::from_secs(30)).unwrap();
    let second_pid = session.process_id().unwrap();
    assert_ne!(first_pid, second_pid);
    assert_eq!(unsafe { libc::kill(first_pid as i32, 0) }, -1);
    assert!(session.is_running().unwrap());
    session.stop_owned().unwrap();
    assert_clean(&authority, second_pid, address);
    finish.send(()).unwrap();
    peer.join().unwrap();
    eprintln!(
        "Indirect: chooser EOF, second INDIRECT_QUERY on same UDP socket, forwarded peer with new cookie, old cookie denied, fresh XQuartz green pixel and explicit owned stop"
    );
}

#[test]
#[ignore = "requires the staged pinned XQuartz package and a macOS GUI session"]
fn macos_xdmcp_managed_cancel_interrupts_silent_manager() {
    let manager = manager_socket();
    let mut chosen = None;
    for port in 6200..6300 {
        if let Ok(reservation) = TcpListener::bind((Ipv4Addr::LOCALHOST, port)) {
            chosen = Some((port, reservation));
            break;
        }
    }
    let (port, reservation) = chosen.expect("free display port");
    drop(reservation);
    let worker = ManagedXdmcp::start_hostname(
        packaged_manager(),
        XdmcpConfig {
            allow_network_listener: true,
            display: Some(port - 6000),
            ..Default::default()
        },
        "localhost".into(),
        manager.local_addr().unwrap().port(),
        800,
        600,
    )
    .unwrap();
    // Drain even on fixture failure before this test process can exit.
    struct Drain(ManagedXdmcp);
    impl Drop for Drain {
        fn drop(&mut self) {
            self.0.cancel();
            let _ = ManagedXdmcp::wait_for_shutdown(Duration::from_secs(3));
        }
    }
    let worker = Drain(worker);
    let (query, _) = receive(&manager, 2);
    assert_eq!(query, [0]);
    assert!(worker.0.poll().is_none());
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    assert!(
        TcpListener::bind(address).is_err(),
        "session did not reserve the network port"
    );
    worker.0.cancel();
    assert!(ManagedXdmcp::wait_for_shutdown(Duration::from_secs(3)));
    let deadline = Instant::now() + Duration::from_secs(3);
    while !worker.0.is_finished() {
        assert!(
            Instant::now() < deadline,
            "cancellation did not interrupt manager startup"
        );
        thread::sleep(Duration::from_millis(20));
    }
    let terminal = loop {
        if let Some(value) = worker.0.poll() {
            break value;
        }
        assert!(Instant::now() < deadline, "missing terminal result");
        thread::sleep(Duration::from_millis(5));
    };
    assert!(terminal.unwrap_err().contains("cancel"));
    assert!(TcpListener::bind(address).is_ok());
    assert!(manager.local_addr().is_ok());
    eprintln!(
        "Managed hostname startup: silent owned UDP peer, no readiness without a client, cancelled before timeout, TCP port released"
    );
}

#[test]
#[ignore = "requires an explicitly configured owned IPv4 xdm and a macOS GUI session"]
fn macos_xdmcp_external_owned_manager() {
    let endpoint: SocketAddr = std::env::var("TERMCRAFT_TEST_XDMCP_ENDPOINT")
        .expect("set TERMCRAFT_TEST_XDMCP_ENDPOINT to the owned manager's IPv4:UDP-port")
        .parse()
        .unwrap();
    let mode = match std::env::var("TERMCRAFT_TEST_XDMCP_MODE").as_deref() {
        Ok("direct") => QueryMode::Direct,
        Ok("indirect") => QueryMode::Indirect,
        _ => panic!("set TERMCRAFT_TEST_XDMCP_MODE to direct or indirect"),
    };
    let seconds: u64 = std::env::var("TERMCRAFT_TEST_XDMCP_HOLD_SECONDS")
        .unwrap_or_else(|_| "60".into())
        .parse()
        .unwrap();
    assert!((5..=180).contains(&seconds), "hold must be 5–180 seconds");
    let mut session = packaged_manager()
        .start_xdmcp_cancellable(
            XdmcpConfig {
                selected_endpoint: Some(endpoint),
                mode,
                display: std::env::var("TERMCRAFT_TEST_XDMCP_DISPLAY")
                    .ok()
                    .map(|value| value.parse().unwrap()),
                advertise_address: std::env::var("TERMCRAFT_TEST_XDMCP_LOCAL_IPV4")
                    .ok()
                    .map(|value| value.parse().unwrap()),
                allow_network_listener: true,
                ..Default::default()
            },
            800,
            600,
            &AtomicBool::new(false),
        )
        .unwrap();
    let authority = session.authority_path().to_owned();
    let initial = session.process_id().unwrap();
    let (host, display) = session.display().rsplit_once(':').unwrap();
    let address = SocketAddr::from((
        host.parse::<Ipv4Addr>().unwrap(),
        6000 + display.parse::<u16>().unwrap(),
    ));
    eprintln!(
        "EXTERNAL READY mode={mode:?} manager={endpoint} display={} xquartz_pid={initial}",
        session.display()
    );
    let until = Instant::now() + Duration::from_secs(seconds);
    let mut active = initial;
    let mut changed = false;
    while Instant::now() < until {
        assert!(
            session.is_running().unwrap(),
            "owned manager closed before the requested hold"
        );
        if let Some(pid) = session.process_id()
            && pid != initial
            && authority.exists()
            && !changed
        {
            assert_eq!(unsafe { libc::kill(initial as i32, 0) }, -1);
            changed = true;
            active = pid;
            eprintln!(
                "EXTERNAL HANDOFF authenticated display={} xquartz_pid={pid}",
                session.display()
            );
        }
        thread::sleep(Duration::from_millis(100));
    }
    if mode == QueryMode::Indirect {
        assert!(
            changed,
            "no authenticated chooser handoff; select a target in the first manager's chooser"
        );
    }
    session.stop_owned().unwrap();
    assert_clean(&authority, active, address);
    eprintln!("EXTERNAL CLEANUP XQuartz exited and listener/authority removed");
}
