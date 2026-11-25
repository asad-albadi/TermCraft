//! Shared-deadline composition tests use private sockets, never an X display.
use super::*;
use std::{
    io::{Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    sync::mpsc,
};
use x11rb::x11_utils::Serialize;
fn setup_reply(stream: &mut UnixStream) {
    let mut header = [0; 12];
    stream.read_exact(&mut header).unwrap();
    let padded = |n: u16| (usize::from(n) + 3) & !3;
    let mut auth = vec![
        0;
        padded(u16::from_ne_bytes([header[6], header[7]]))
            + padded(u16::from_ne_bytes([header[8], header[9]]))
    ];
    stream.read_exact(&mut auth).unwrap();
    let mut setup = Setup {
        status: 1,
        protocol_major_version: 11,
        resource_id_base: 0x200000,
        resource_id_mask: 0x1fffff,
        maximum_request_length: 65535,
        roots: vec![Screen {
            root: 1,
            ..Default::default()
        }],
        ..Default::default()
    };
    setup.length = ((setup.serialize().len() - 8) / 4) as u16;
    stream.write_all(&setup.serialize()).unwrap();
}
#[test]
fn delayed_listener_and_stalled_authentication_share_two_seconds() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("recovery");
    let bind = path.clone();
    let (done, wait) = mpsc::channel();
    let server = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1600));
        let listener = UnixListener::bind(bind).unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut auth = [0; 48];
        stream.read_exact(&mut auth).unwrap();
        assert!(wait.recv_timeout(Duration::from_secs(3)).is_ok());
    });
    let stop = Arc::new(AtomicBool::new(false));
    let start = Instant::now();
    let deadline = start + Duration::from_secs(2);
    let stream = super::super::xdmcp_engine::connect_ready_socket(&path, deadline, &stop).unwrap();
    assert_eq!(
        transport::peer(&stream).unwrap().pid,
        std::process::id() as i32
    );
    let result = transport::connect(
        stream,
        0,
        (b"MIT-MAGIC-COOKIE-1".to_vec(), vec![0x51; 16]),
        stop,
        deadline,
    );
    let elapsed = start.elapsed();
    done.send(()).unwrap();
    server.join().unwrap();
    println!(
        "shared recovery: delayed listener 1600ms; total={}ms budget=2000ms; authentication_error={}",
        elapsed.as_millis(),
        result.is_err()
    );
    assert!(result.is_err());
    assert!(elapsed >= Duration::from_millis(1900));
    assert!(elapsed < Duration::from_millis(2200));
}
#[test]
fn expired_or_cancelled_setup_sends_no_authentication_bytes() {
    for cancelled in [false, true] {
        let (client, mut server) = UnixStream::pair().unwrap();
        server
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let deadline = if cancelled {
            Instant::now() + Duration::from_secs(2)
        } else {
            Instant::now() - Duration::from_millis(1)
        };
        let start = Instant::now();
        assert!(
            transport::connect(
                client,
                0,
                (b"MIT-MAGIC-COOKIE-1".to_vec(), vec![0x51; 16]),
                Arc::new(AtomicBool::new(cancelled)),
                deadline
            )
            .is_err()
        );
        assert!(start.elapsed() < Duration::from_millis(100));
        let mut bytes = [0; 64];
        assert_eq!(server.read(&mut bytes).unwrap(), 0);
    }
}
#[test]
fn endpoint_initialization_keeps_remaining_budget_after_successful_handshake() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("initialization");
    let bind = path.clone();
    let (done, wait) = mpsc::channel();
    let server = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(450));
        let listener = UnixListener::bind(bind).unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        setup_reply(&mut stream);
        // The authenticated connection succeeds, but extension discovery deliberately stalls.
        let mut request = [0; 64];
        assert!(stream.read(&mut request).unwrap() > 0);
        assert!(wait.recv_timeout(Duration::from_secs(2)).is_ok());
    });
    let start = Instant::now();
    let deadline = start + Duration::from_millis(600);
    let stop = Arc::new(AtomicBool::new(false));
    let stream = super::super::xdmcp_engine::connect_ready_socket(&path, deadline, &stop).unwrap();
    assert_eq!(
        transport::peer(&stream).unwrap().pid,
        std::process::id() as i32
    );
    let conn = transport::connect(stream, 0, (Vec::new(), Vec::new()), stop, deadline).unwrap();
    assert!(Endpoint::new(conn, None, deadline).is_err());
    let elapsed = start.elapsed();
    done.send(()).unwrap();
    server.join().unwrap();
    println!(
        "successful peer/auth then stalled endpoint: total={}ms budget=600ms",
        elapsed.as_millis()
    );
    assert!(elapsed >= Duration::from_millis(550));
    assert!(elapsed < Duration::from_millis(675));
}
#[test]
fn expired_endpoint_initialization_sends_no_requests() {
    let (client, mut server) = UnixStream::pair().unwrap();
    let worker = std::thread::spawn(move || {
        setup_reply(&mut server);
        let mut b = [0; 64];
        assert_eq!(server.read(&mut b).unwrap(), 0);
    });
    let conn = transport::connect(
        client,
        0,
        (Vec::new(), Vec::new()),
        Arc::new(AtomicBool::new(false)),
        Instant::now() + Duration::from_secs(1),
    )
    .unwrap();
    assert!(Endpoint::new(conn, None, Instant::now() - Duration::from_millis(1)).is_err());
    worker.join().unwrap();
}

#[test]
#[ignore = "private owned X11 display namespace only; no display manager"]
fn owned_endpoint_reauthenticates_and_initializes_with_one_deadline_per_epoch() {
    assert_eq!(
        std::env::var("TERMCRAFT_TEST_XSERVER_ISOLATED").as_deref(),
        Ok("1")
    );
    let manager = crate::x11::XServerManager::for_installation(std::path::PathBuf::from(
        std::env::var("TERMCRAFT_TEST_XDMCP_PACKAGE").unwrap(),
    ));
    let mut session = manager
        .start_xdmcp_cancellable(
            super::super::xdmcp::XdmcpConfig {
                selected_endpoint: Some("127.0.0.1:1177".parse().unwrap()),
                display: Some(181),
                advertise_address: Some("192.0.2.1".parse().unwrap()),
                allow_network_listener: true,
                ..Default::default()
            },
            640,
            480,
            &AtomicBool::new(false),
        )
        .unwrap();
    let pid = session.process_id().unwrap();
    let authority = session.authority_path().to_owned();
    for epoch in 0..2 {
        let start = Instant::now();
        let deadline = start + Duration::from_secs(2);
        let (conn, peer) = transport::parent_until(
            session.display(),
            Some(&authority),
            Arc::new(AtomicBool::new(false)),
            deadline,
        )
        .unwrap();
        assert_eq!(peer, pid as i32);
        let endpoint = Endpoint::new(conn, None, deadline).unwrap();
        assert!(start.elapsed() < Duration::from_secs(2));
        println!(
            "owned endpoint epoch {epoch}: same PID {pid}, private-cookie authentication and XFixes/window initialization succeeded within shared budget ({}ms)",
            start.elapsed().as_millis()
        );
        drop(endpoint);
        assert!(session.is_running().unwrap());
    }
    session.stop_owned().unwrap();
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    assert!(!authority.exists());
}

#[test]
fn resumed_parent_reads_selection_within_remaining_reset_budget() {
    let (client, mut server) = UnixStream::pair().unwrap();
    let (done, wait) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        setup_reply(&mut server);
        let mut request = [0; 8];
        server.read_exact(&mut request).unwrap();
        assert_eq!(request[0], 23); // GetSelectionOwner
        let mut reply = [0u8; 32];
        reply[0] = 1;
        reply[2..4].copy_from_slice(&1u16.to_ne_bytes());
        reply[8..12].copy_from_slice(&42u32.to_ne_bytes());
        server.write_all(&reply).unwrap();
        wait.recv_timeout(Duration::from_secs(2)).unwrap();
    });
    let recovery_deadline = Instant::now() + Duration::from_secs(2);
    let conn = transport::connect(
        client,
        0,
        (Vec::new(), Vec::new()),
        Arc::new(AtomicBool::new(false)),
        recovery_deadline,
    )
    .unwrap();
    transport::renew(&conn);
    std::thread::sleep(Duration::from_millis(310));
    assert!(transport::begin_recovery_request(&conn, Instant::now()).is_err());
    transport::begin_recovery_request(&conn, recovery_deadline).unwrap();
    assert_eq!(
        conn.get_selection_owner(1u32)
            .unwrap()
            .reply()
            .unwrap()
            .owner,
        42
    );
    done.send(()).unwrap();
    worker.join().unwrap();
}

#[test]
#[ignore = "requires the isolated test compositor and private parent display"]
fn recovery_clears_owned_parent_selection_after_previous_budget_expires() {
    assert_eq!(
        std::env::var("TERMCRAFT_TEST_XSERVER_ISOLATED").as_deref(),
        Ok("1")
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    let (conn, _) =
        transport::parent_until(":0", None, Arc::new(AtomicBool::new(false)), deadline).unwrap();
    let mut parent = Endpoint::new(conn, None, deadline).unwrap();
    let selection = parent.atoms.selection;
    parent
        .conn
        .set_selection_owner(parent.window, selection, 0u32)
        .unwrap()
        .check()
        .unwrap();
    assert_eq!(
        parent
            .conn
            .get_selection_owner(selection)
            .unwrap()
            .reply()
            .unwrap()
            .owner,
        parent.window
    );
    transport::renew(&parent.conn);
    std::thread::sleep(Duration::from_millis(310));
    parent.clear_for_recovery(deadline).unwrap();
    assert!(parent.clearing);
    assert_eq!(
        parent
            .conn
            .get_selection_owner(selection)
            .unwrap()
            .reply()
            .unwrap()
            .owner,
        0
    );
    assert!(Instant::now() < deadline);
}
