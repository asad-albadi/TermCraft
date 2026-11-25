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

// Serve the real Endpoint protocol, including checked void requests. The recovery
// failure is injected only at its final GetSelectionOwner request, after auth,
// extension discovery, atoms, window creation and XFixes subscription succeeded.
fn endpoint_server(stream: UnixStream, close_owner: bool, ready: Option<mpsc::Sender<()>>) {
    endpoint_server_observe(stream, close_owner, ready, |_, _, _| {});
}
fn endpoint_server_observe(
    mut stream: UnixStream,
    close_owner: bool,
    ready: Option<mpsc::Sender<()>>,
    mut observe: impl FnMut(u8, &[u8], &mut [u8; 32]),
) {
    setup_reply(&mut stream);
    let mut sequence = 0u16;
    loop {
        let mut header = [0; 4];
        if stream.read_exact(&mut header).is_err() {
            return;
        }
        sequence = sequence.wrapping_add(1);
        let length = u16::from_ne_bytes([header[2], header[3]]) as usize * 4;
        let mut payload = vec![0; length - 4];
        stream.read_exact(&mut payload).unwrap();
        let mut reply = [0u8; 32];
        reply[0] = 1;
        reply[2..4].copy_from_slice(&sequence.to_ne_bytes());
        observe(header[0], &payload, &mut reply);
        match header[0] {
            98 => {
                reply[8] = 1;
                reply[9] = 200;
                reply[10] = 100;
            }
            200 if header[1] == 0 => {
                reply[8..12].copy_from_slice(&2u32.to_ne_bytes());
            }
            16 => {
                reply[8..12].copy_from_slice(&(u32::from(sequence) + 10).to_ne_bytes());
            }
            43 => {}
            23 => {
                if close_owner {
                    return;
                }
                if let Some(ref ready) = ready {
                    let _ = ready.send(());
                }
            }
            _ => continue,
        }
        if stream.write_all(&reply).is_err() {
            return;
        }
    }
}

#[test]
fn recovery_retries_initializer_eof_within_one_reset_epoch() {
    use std::os::fd::FromRawFd;
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("nested");
    let listener = UnixListener::bind(&socket).unwrap();
    let (p_client, p_server) = UnixStream::pair().unwrap();
    let p = std::thread::spawn(move || endpoint_server(p_server, false, None));
    let (n_client, n_server) = UnixStream::pair().unwrap();
    let n_control = n_server.try_clone().unwrap();
    let n = std::thread::spawn(move || endpoint_server(n_server, false, None));
    let stop = Arc::new(AtomicBool::new(false));
    let deadline = Instant::now() + Duration::from_secs(2);
    let parent = Endpoint::new(
        transport::connect(p_client, 0, (vec![], vec![]), stop.clone(), deadline).unwrap(),
        None,
        deadline,
    )
    .unwrap();
    let nested = Endpoint::new(
        transport::connect(n_client, 0, (vec![], vec![]), stop.clone(), deadline).unwrap(),
        None,
        deadline,
    )
    .unwrap();
    let (send, received) = mpsc::channel();
    let flag = stop.clone();
    let recovery = std::thread::spawn(move || {
        let (first, _) = listener.accept().unwrap();
        endpoint_server(first, true, None);
        listener.set_nonblocking(true).unwrap();
        let until = Instant::now() + Duration::from_secs(1);
        loop {
            match listener.accept() {
                Ok((second, _)) => {
                    endpoint_server(second, false, Some(send));
                    return;
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock && Instant::now() < until => {
                    std::thread::sleep(Duration::from_millis(1))
                }
                Err(_) => {
                    flag.store(true, Ordering::Relaxed);
                    return;
                }
            }
        }
    });
    let cancel = stop.clone();
    let waiter = std::thread::spawn(move || {
        let recovered = received.recv_timeout(Duration::from_millis(1500)).is_ok();
        cancel.store(true, Ordering::Relaxed);
        recovered
    });
    n_control.shutdown(std::net::Shutdown::Both).unwrap();
    let raw = unsafe { libc::syscall(libc::SYS_pidfd_open, std::process::id(), 0) as i32 };
    assert!(raw >= 0);
    let pidfd = unsafe { std::os::fd::OwnedFd::from_raw_fd(raw) };
    let result = run(
        parent,
        nested,
        &stop,
        &socket,
        &[0x51; 16],
        std::process::id(),
        &pidfd,
    );
    let recovered = waiter.join().unwrap();
    recovery.join().unwrap();
    p.join().unwrap();
    n.join().unwrap();
    assert!(
        recovered,
        "a replacement endpoint must initialize after GetSelectionOwner EOF: {result:?}"
    );
    // Stop may cancel the subsequent parent resample. The observation above is
    // emitted only by a replacement endpoint after complete initialization.
    assert!(
        result.is_ok() || result.as_ref().unwrap_err().to_string() == "operation interrupted",
        "unexpected recovery failure: {result:?}"
    );
}

fn self_pidfd() -> std::os::fd::OwnedFd {
    use std::os::fd::FromRawFd;
    let raw = unsafe { libc::syscall(libc::SYS_pidfd_open, std::process::id(), 0) as i32 };
    assert!(raw >= 0);
    unsafe { std::os::fd::OwnedFd::from_raw_fd(raw) }
}

#[test]
fn recovery_wrong_peer_never_receives_private_cookie() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("wrong-peer");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_millis(500)))
            .unwrap();
        let mut bytes = [0; 64];
        assert_eq!(stream.read(&mut bytes).unwrap(), 0);
    });
    let result = recover_endpoint(
        &socket,
        &[0x51; 16],
        std::process::id() + 1,
        &self_pidfd(),
        &Arc::new(AtomicBool::new(false)),
        Instant::now() + Duration::from_secs(2),
    );
    assert!(
        result
            .err()
            .unwrap()
            .to_string()
            .contains("peer identity mismatch")
    );
    server.join().unwrap();
}

#[test]
fn recovery_cancelled_or_expired_epoch_opens_no_connection() {
    for cancelled in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("inactive");
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        let deadline = Instant::now()
            + if cancelled {
                Duration::from_secs(2)
            } else {
                Duration::ZERO
            };
        let start = Instant::now();
        assert!(
            recover_endpoint(
                &socket,
                &[0x51; 16],
                std::process::id(),
                &self_pidfd(),
                &Arc::new(AtomicBool::new(cancelled)),
                deadline
            )
            .is_err()
        );
        assert!(start.elapsed() < Duration::from_millis(100));
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
    }
}

#[test]
fn recovery_does_not_retry_authentication_rejection() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("rejected");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut header = [0; 12];
        stream.read_exact(&mut header).unwrap();
        let padded = |n: u16| (n as usize + 3) & !3;
        let mut auth = vec![
            0;
            padded(u16::from_ne_bytes([header[6], header[7]]))
                + padded(u16::from_ne_bytes([header[8], header[9]]))
        ];
        stream.read_exact(&mut auth).unwrap();
        let mut rejected = [0; 12];
        rejected[1] = 4;
        rejected[2..4].copy_from_slice(&11u16.to_ne_bytes());
        rejected[6..8].copy_from_slice(&1u16.to_ne_bytes());
        rejected[8..].copy_from_slice(b"Nope");
        stream.write_all(&rejected).unwrap();
        listener.set_nonblocking(true).unwrap();
        listener
    });
    let result = recover_endpoint(
        &socket,
        &[0x51; 16],
        std::process::id(),
        &self_pidfd(),
        &Arc::new(AtomicBool::new(false)),
        Instant::now() + Duration::from_secs(2),
    );
    assert_eq!(result.err().unwrap().kind(), io::ErrorKind::Other);
    let listener = server.join().unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
}

#[test]
fn recovery_initializer_retries_share_original_deadline() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("budget");
    let listener = UnixListener::bind(&socket).unwrap();
    let flag = Arc::new(AtomicBool::new(false));
    let server_flag = flag.clone();
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        let mut attempts = 0;
        while !server_flag.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((stream, _)) => {
                    attempts += 1;
                    endpoint_server(stream, true, None);
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => std::thread::yield_now(),
                Err(e) => panic!("{e}"),
            }
        }
        attempts
    });
    let start = Instant::now();
    let deadline = start + Duration::from_millis(150);
    let result = recover_endpoint(
        &socket,
        &[0x51; 16],
        std::process::id(),
        &self_pidfd(),
        &Arc::new(AtomicBool::new(false)),
        deadline,
    );
    let elapsed = start.elapsed();
    flag.store(true, Ordering::Relaxed);
    let attempts = server.join().unwrap();
    assert!(result.is_err());
    assert!(attempts > 1);
    assert!(elapsed >= Duration::from_millis(140));
    assert!(elapsed < Duration::from_millis(300));
    println!(
        "{attempts} complete initializer EOFs shared original 150ms deadline: {}ms",
        elapsed.as_millis()
    );
}

#[test]
fn three_recovered_manager_reset_episodes_still_enforce_thirty_second_limit() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("episodes");
    let listener = UnixListener::bind(&socket).unwrap();
    let (p_client, p_server) = UnixStream::pair().unwrap();
    let (sampled, samples) = mpsc::channel();
    let parent_server = std::thread::spawn(move || {
        let mut owners = 0;
        endpoint_server_observe(p_server, false, None, move |opcode, _, _| {
            if opcode == 23 {
                owners += 1;
                if owners >= 3 && owners % 2 == 1 {
                    sampled.send(()).unwrap();
                }
            }
        });
    });
    let (n_client, n_server) = UnixStream::pair().unwrap();
    let n_control = n_server.try_clone().unwrap();
    let initial_server = std::thread::spawn(move || endpoint_server(n_server, false, None));
    let stop = Arc::new(AtomicBool::new(false));
    let deadline = Instant::now() + Duration::from_secs(2);
    let parent = Endpoint::new(
        transport::connect(p_client, 0, (vec![], vec![]), stop.clone(), deadline).unwrap(),
        None,
        deadline,
    )
    .unwrap();
    let nested = Endpoint::new(
        transport::connect(n_client, 0, (vec![], vec![]), stop.clone(), deadline).unwrap(),
        None,
        deadline,
    )
    .unwrap();
    let replacements = std::thread::spawn(move || {
        for _ in 0..3 {
            let (stream, _) = listener.accept().unwrap();
            let control = stream.try_clone().unwrap();
            let server = std::thread::spawn(move || endpoint_server(stream, false, None));
            samples.recv_timeout(Duration::from_secs(2)).unwrap();
            control.shutdown(std::net::Shutdown::Both).unwrap();
            server.join().unwrap();
        }
        listener.set_nonblocking(true).unwrap();
        listener
    });
    n_control.shutdown(std::net::Shutdown::Both).unwrap();
    let result = run(
        parent,
        nested,
        &stop,
        &socket,
        &[0x51; 16],
        std::process::id(),
        &self_pidfd(),
    );
    let listener = replacements.join().unwrap();
    parent_server.join().unwrap();
    initial_server.join().unwrap();
    assert!(
        result
            .err()
            .unwrap()
            .to_string()
            .contains("Nested clipboard reset limit reached")
    );
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
}

#[test]
fn recovery_dead_owned_pidfd_opens_no_connection() {
    use std::os::fd::FromRawFd;
    let mut child = std::process::Command::new("/usr/bin/true").spawn().unwrap();
    let raw = unsafe { libc::syscall(libc::SYS_pidfd_open, child.id(), 0) as i32 };
    assert!(raw >= 0);
    let pidfd = unsafe { std::os::fd::OwnedFd::from_raw_fd(raw) };
    child.wait().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("dead");
    let listener = UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let result = recover_endpoint(
        &socket,
        &[0x51; 16],
        std::process::id(),
        &pidfd,
        &Arc::new(AtomicBool::new(false)),
        Instant::now() + Duration::from_secs(2),
    );
    assert_eq!(
        result.err().unwrap().to_string(),
        "Owned clipboard engine exited"
    );
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
}

#[test]
#[ignore = "requires the isolated test compositor and private parent display"]
fn recovery_preserves_foreign_parent_selection() {
    assert_eq!(
        std::env::var("TERMCRAFT_TEST_XSERVER_ISOLATED").as_deref(),
        Ok("1")
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    let stop = Arc::new(AtomicBool::new(false));
    let (conn, _) = transport::parent_until(":0", None, stop.clone(), deadline).unwrap();
    let mut parent = Endpoint::new(conn, None, deadline).unwrap();
    let (foreign, _) = transport::parent_until(":0", None, stop, deadline).unwrap();
    let owner = create_window(&foreign).unwrap();
    foreign
        .set_selection_owner(owner, parent.atoms.selection, 0u32)
        .unwrap()
        .check()
        .unwrap();
    parent.clear_for_recovery(deadline).unwrap();
    assert_eq!(
        foreign
            .get_selection_owner(parent.atoms.selection)
            .unwrap()
            .reply()
            .unwrap()
            .owner,
        owner
    );
    assert!(!parent.clearing);
}

#[test]
fn endpoint_initial_owner_conversion_eof_keeps_transport_kind() {
    let (client, server) = UnixStream::pair().unwrap();
    let control = server.try_clone().unwrap();
    let worker = std::thread::spawn(move || {
        endpoint_server_observe(server, false, None, move |opcode, _, reply| {
            if opcode == 23 {
                reply[8..12].copy_from_slice(&42u32.to_ne_bytes());
            }
            if opcode == 24 {
                control.shutdown(std::net::Shutdown::Both).unwrap();
            }
        })
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    let stop = Arc::new(AtomicBool::new(false));
    let conn = transport::connect(client, 0, (vec![], vec![]), stop, deadline).unwrap();
    let e = Endpoint::new(conn, None, deadline).err().unwrap();
    worker.join().unwrap();
    assert_eq!(
        e.kind(),
        io::ErrorKind::UnexpectedEof,
        "an initializer's owner conversion must remain eligible for the same bounded recovery: {e}"
    );
}
