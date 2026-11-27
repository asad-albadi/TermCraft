#![cfg(unix)]
mod support;
use harbor_workspace::{model::Tunnel, process::Process};
use std::{
    io::{Read, Write},
    net::{IpAddr, SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use support::ssh_server::{PortReservation, SshServer};
const PAYLOAD: &[u8] = b"TUNNEL_OK";
struct Echo {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    count: Arc<AtomicUsize>,
    worker: Option<thread::JoinHandle<()>>,
    _port: PortReservation,
}
impl Echo {
    fn start(ip: IpAddr) -> Self {
        let mut reservation = PortReservation::bind(ip);
        let listener = reservation.take_listener();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let count = Arc::new(AtomicUsize::new(0));
        let (s, c) = (stop.clone(), count.clone());
        let worker = thread::spawn(move || {
            while !s.load(Ordering::Acquire) {
                if let Ok((mut socket, peer)) = listener.accept() {
                    socket.set_nonblocking(false).unwrap();
                    socket
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut bytes = [0; 9];
                    if socket.read_exact(&mut bytes).is_ok() {
                        assert_eq!(&bytes, PAYLOAD);
                        c.fetch_add(1, Ordering::SeqCst);
                        println!(
                            "echo peer={peer}, local={}, exact=TUNNEL_OK",
                            socket.local_addr().unwrap()
                        );
                        socket.write_all(&bytes).unwrap();
                    }
                } else {
                    thread::sleep(Duration::from_millis(5));
                }
            }
        });
        Self {
            address,
            stop,
            count,
            worker: Some(worker),
            _port: reservation,
        }
    }
}
impl Drop for Echo {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.worker.take().unwrap().join().unwrap();
    }
}
struct FixtureTunnel {
    tunnel: Tunnel,
    _port: PortReservation,
}
impl std::ops::Deref for FixtureTunnel {
    type Target = Tunnel;
    fn deref(&self) -> &Tunnel {
        &self.tunnel
    }
}
impl std::ops::DerefMut for FixtureTunnel {
    fn deref_mut(&mut self) -> &mut Tunnel {
        &mut self.tunnel
    }
}
fn config(server: &SshServer, mode: &str, bind: IpAddr, destination: SocketAddr) -> FixtureTunnel {
    let mut reservation = PortReservation::bind(bind);
    let local_port = reservation.port();
    reservation.release_listener();
    FixtureTunnel {
        tunnel: Tunnel {
            host: "127.0.0.1".into(),
            user: std::env::var("USER").unwrap(),
            ssh_port: server.port,
            key: server.root.join("client").display().to_string(),
            mode: mode.into(),
            bind: bind.to_string(),
            local_port,
            destination: destination.ip().to_string(),
            remote_port: destination.port(),
            ..Default::default()
        },
        _port: reservation,
    }
}
fn launch_args(server: &SshServer, tunnel: &Tunnel) -> Vec<String> {
    server.config(1, true);
    let mut args = vec![
        "-F".into(),
        "/dev/null".into(),
        "-o".into(),
        "BatchMode=yes".into(),
        "-o".into(),
        "StrictHostKeyChecking=yes".into(),
        "-o".into(),
        format!(
            "UserKnownHostsFile={}",
            server.root.join("known_hosts").display()
        ),
        "-o".into(),
        "GlobalKnownHostsFile=/dev/null".into(),
    ];
    args.extend(tunnel.args().unwrap());
    args
}
fn launch(server: &SshServer, tunnel: &Tunnel) -> Process {
    Process::spawn("ssh", &launch_args(server, tunnel), None, 80, 24).unwrap()
}
fn output(process: &Process) -> String {
    String::from_utf8_lossy(&process.rx.try_iter().flatten().collect::<Vec<_>>()).into_owned()
}
fn connect(process: &mut Process, address: SocketAddr) -> TcpStream {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        process.poll();
        assert!(
            !process.exited,
            "{}: {}",
            process.exit_message,
            output(process)
        );
        if let Ok(socket) = TcpStream::connect_timeout(&address, Duration::from_millis(100)) {
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            socket
                .set_write_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            return socket;
        }
        assert!(
            Instant::now() < deadline,
            "listener did not open: {}",
            output(process)
        );
        thread::sleep(Duration::from_millis(10));
    }
}
fn socks(socket: &mut TcpStream, destination: SocketAddr) -> std::io::Result<u8> {
    socket.write_all(&[5, 1, 0]).unwrap();
    let mut greeting = [0; 2];
    socket.read_exact(&mut greeting).unwrap();
    assert_eq!(greeting, [5, 0]);
    let mut request = vec![5, 1, 0];
    match destination.ip() {
        IpAddr::V4(ip) => {
            request.push(1);
            request.extend(ip.octets());
        }
        IpAddr::V6(ip) => {
            request.push(4);
            request.extend(ip.octets());
        }
    }
    request.extend(destination.port().to_be_bytes());
    socket.write_all(&request).unwrap();
    let mut reply = [0; 4];
    socket.read_exact(&mut reply)?;
    assert_eq!(reply[0], 5);
    assert_eq!(reply[2], 0);
    if reply[1] == 0 {
        let size = match reply[3] {
            1 => 6,
            4 => 18,
            _ => panic!("unexpected SOCKS address"),
        };
        let mut rest = vec![0; size];
        socket.read_exact(&mut rest).unwrap();
    }
    Ok(reply[1])
}
fn channel_closed(result: std::io::Result<usize>) -> bool {
    match result {
        Ok(0) => true,
        Err(error) => matches!(
            error.kind(),
            std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::ConnectionAborted
                | std::io::ErrorKind::UnexpectedEof
                | std::io::ErrorKind::BrokenPipe
        ),
        _ => false,
    }
}
fn rebind(address: SocketAddr) {
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        let socket = if address.is_ipv4() {
            tokio::net::TcpSocket::new_v4()
        } else {
            tokio::net::TcpSocket::new_v6()
        }
        .unwrap();
        socket.set_reuseaddr(true).unwrap();
        if socket.bind(address).is_ok() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "owned listener survived cancellation {address}"
        );
        thread::sleep(Duration::from_millis(10));
    }
}
fn roundtrip(mode: &str, ip: &str) {
    let ip: IpAddr = ip.parse().unwrap();
    let server = SshServer::start_with_settings(true, "", "GatewayPorts clientspecified");
    let echo = Echo::start(ip);
    let tunnel = config(&server, mode, ip, echo.address);
    let mut process = launch(&server, &tunnel);
    let address = SocketAddr::new(ip, tunnel.local_port);
    let mut socket = connect(&mut process, address);
    #[cfg(target_os = "linux")]
    for table in ["/proc/net/tcp", "/proc/net/tcp6"] {
        for line in std::fs::read_to_string(table).unwrap().lines().skip(1) {
            let columns: Vec<_> = line.split_whitespace().collect();
            let (host, port) = columns[1].split_once(':').unwrap();
            if columns[3] == "0A" && u16::from_str_radix(port, 16).unwrap() == tunnel.local_port {
                assert!(
                    !host.chars().all(|c| c == '0'),
                    "forward unexpectedly exposed wildcard listener: {line}"
                );
                println!("observed owned listener local={host}:{port}");
            }
        }
    }
    if mode == "Dynamic" {
        assert_eq!(socks(&mut socket, echo.address).unwrap(), 0);
    }
    socket.write_all(PAYLOAD).unwrap();
    let mut bytes = [0; 9];
    socket.read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes, PAYLOAD);
    drop(socket);
    assert_eq!(echo.count.load(Ordering::SeqCst), 1);
    drop(process);
    rebind(address);
}
#[test]
fn local_ipv4() {
    roundtrip("Local", "127.0.0.1");
}
#[test]
fn remote_ipv4() {
    roundtrip("Remote", "127.0.0.1");
}
#[test]
fn dynamic_ipv4() {
    roundtrip("Dynamic", "127.0.0.1");
}
#[test]
fn local_ipv6() {
    roundtrip("Local", "::1");
}
#[test]
fn remote_ipv6() {
    roundtrip("Remote", "::1");
}
#[test]
fn dynamic_ipv6() {
    roundtrip("Dynamic", "::1");
}
#[test]
fn local_remote_dynamic_tunnels_carry_exact_payload() {
    for mode in ["Local", "Remote", "Dynamic"] {
        roundtrip(mode, "127.0.0.1");
    }
}
#[test]
fn explicit_nondefault_loopback_bind() {
    // Darwin does not implicitly configure 127.0.0.2 as an interface address.
    let address = if cfg!(target_os = "macos") {
        "::1"
    } else {
        "127.0.0.2"
    };
    for mode in ["Local", "Remote", "Dynamic"] {
        roundtrip(mode, address);
    }
}
#[test]
fn tunnel_port_conflict_is_visible() {
    let server = SshServer::start(true);
    let echo = Echo::start("127.0.0.1".parse().unwrap());
    for mode in ["Local", "Remote", "Dynamic"] {
        let blocker = PortReservation::bind("127.0.0.1".parse().unwrap());
        let mut tunnel = config(&server, mode, "127.0.0.1".parse().unwrap(), echo.address);
        tunnel.local_port = blocker.port();
        let mut process = launch(&server, &tunnel);
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut text = String::new();
        loop {
            process.poll();
            text.push_str(&output(&process));
            if process.exited {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "conflicting forward never exited: {text}"
            );
            thread::sleep(Duration::from_millis(10));
        }
        text.push_str(&output(&process));
        assert!(
            text.contains("forwarding failed")
                || text.contains("Could not request local forwarding"),
            "error not visible: {text}"
        );
        println!("{mode}: {text}");
    }
}
#[test]
fn closing_tunnel_releases_listener() {
    let server = SshServer::start(true);
    let echo = Echo::start("127.0.0.1".parse().unwrap());
    for mode in ["Local", "Remote", "Dynamic"] {
        let tunnel = config(&server, mode, "127.0.0.1".parse().unwrap(), echo.address);
        let address = SocketAddr::new("127.0.0.1".parse().unwrap(), tunnel.local_port);
        for _ in 0..2 {
            let mut process = launch(&server, &tunnel);
            let mut socket = connect(&mut process, address);
            if mode == "Dynamic" {
                assert_eq!(socks(&mut socket, echo.address).unwrap(), 0);
            }
            socket.write_all(PAYLOAD).unwrap();
            let mut bytes = [0; 9];
            socket.read_exact(&mut bytes).unwrap();
            assert_eq!(&bytes, PAYLOAD);
            drop(process);
            let mut byte = [0];
            assert!(channel_closed(socket.read(&mut byte)));
            drop(socket);
            rebind(address);
        }
    }
}
#[test]
fn destination_failure_is_observable() {
    let ip = "127.0.0.1".parse().unwrap();
    let server = SshServer::start(true);
    let mut missing_reservation = PortReservation::bind(ip);
    let missing = SocketAddr::new(ip, missing_reservation.port());
    missing_reservation.release_listener();
    for mode in ["Local", "Remote", "Dynamic"] {
        let tunnel = config(&server, mode, ip, missing);
        let mut process = launch(&server, &tunnel);
        let mut socket = connect(&mut process, SocketAddr::new(ip, tunnel.local_port));
        if mode == "Dynamic" {
            match socks(&mut socket, missing) {
                Ok(code) => assert_ne!(
                    code, 0,
                    "SOCKS reported success for unavailable destination"
                ),
                Err(error) => assert!(
                    channel_closed(Err(error)),
                    "SOCKS failure must close, not time out"
                ),
            }
        } else {
            socket.write_all(PAYLOAD).unwrap();
            let mut bytes = [0; 9];
            assert!(channel_closed(socket.read(&mut bytes)));
        }
        process.poll();
        assert!(
            !process.exited,
            "destination failure killed reusable tunnel"
        );
    }
}

// Reproduce the old selection→bind gap without relying on the OS to randomly
// give a second fixture the port. This is an owned banner fixture, not a service.
#[test]
fn released_forward_port_can_connect_to_wrong_ssh_endpoint() {
    let server = SshServer::start(true);
    let echo = Echo::start("127.0.0.1".parse().unwrap());
    let tunnel = config(
        &server,
        "Remote",
        "127.0.0.1".parse().unwrap(),
        echo.address,
    );
    let address = SocketAddr::new("127.0.0.1".parse().unwrap(), tunnel.local_port);
    let intruder = TcpListener::bind(address).unwrap();
    let banner = thread::spawn(move || {
        let (mut socket, _) = intruder.accept().unwrap();
        socket.write_all(b"SSH-2.0-OwnedFixture\r\n").unwrap();
        // Keep the conflicting listener until the real SSH process reports failure.
        (intruder, socket)
    });
    // Hold the subprocess before exec so readiness definitely runs in the
    // selection→bind interval, independent of scheduling/SSH handshake speed.
    let mut args = vec![
        "-c".into(),
        "IFS= read -r ready; exec ssh \"$@\"".into(),
        "owned-forward-collision-gate".into(),
    ];
    args.extend(launch_args(&server, &tunnel));
    let mut process = Process::spawn("/bin/sh", &args, None, 80, 24).unwrap();
    let mut socket = connect(&mut process, address);
    let mut observed = [0; 9];
    socket.read_exact(&mut observed).unwrap();
    assert_eq!(&observed, b"SSH-2.0-O");
    assert_ne!(&observed, PAYLOAD);
    println!(
        "deterministic wrong endpoint {address}: {:?}",
        String::from_utf8_lossy(&observed)
    );
    let _intruder = banner.join().unwrap();
    process.write(b"continue\n").unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut text = String::new();
    loop {
        process.poll();
        text.push_str(&output(&process));
        if process.exited {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "forward collision did not exit: {text}"
        );
        thread::sleep(Duration::from_millis(10));
    }
    text.push_str(&output(&process));
    assert!(text.contains("forwarding failed"), "{text}");
    assert_eq!(echo.count.load(Ordering::SeqCst), 0);
    println!("real SSH process subsequently rejected occupied requested port: {text}");
}

#[test]
fn fixture_port_claim_survives_socket_handoff_and_releases_on_drop() {
    let ip = "127.0.0.1".parse().unwrap();
    let mut reservation = PortReservation::bind(ip);
    let address = SocketAddr::new(ip, reservation.port());
    let candidate = reservation.take_listener();
    // Keep the underlying socket bound while exercising the logical claim.
    // A duplicate presents the exact same port to the claim check without a rebind.
    let rejected = PortReservation::claim(candidate.try_clone().unwrap());
    assert!(
        matches!(rejected, Err(ref e) if e.kind() == std::io::ErrorKind::AddrInUse),
        "a second fixture claimed the handed-off port {address}"
    );
    drop(rejected);
    drop(reservation);
    let reclaimed = PortReservation::claim(candidate).unwrap();
    assert_eq!(reclaimed.port(), address.port());
    println!("fixture candidate {address} rejected while claimed, available after drop");
}

#[test]
fn parallel_forward_handoffs_keep_distinct_claims_and_real_roundtrips() {
    // Each worker pauses after selection and socket release. The controller
    // checks all live claims before allowing any of the SSH forwards to bind.
    // Bounded channels also unwind if a worker cannot start its fixture.
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    thread::scope(|scope| {
        let mut gates = Vec::new();
        for (mode, ip) in [
            ("Local", "127.0.0.1"),
            ("Remote", "127.0.0.1"),
            ("Dynamic", "127.0.0.1"),
            ("Local", "::1"),
            ("Remote", "::1"),
            ("Dynamic", "::1"),
        ] {
            let ready_tx = ready_tx.clone();
            let (gate_tx, gate_rx) = std::sync::mpsc::channel();
            gates.push(gate_tx);
            scope.spawn(move || {
                let ip: IpAddr = ip.parse().unwrap();
                let server = SshServer::start_with_settings(true, "", "GatewayPorts clientspecified");
                let echo = Echo::start(ip);
                let tunnel = config(&server, mode, ip, echo.address);
                ready_tx.send([server.port, echo.address.port(), tunnel.local_port]).unwrap();
                gate_rx.recv_timeout(Duration::from_secs(8)).expect("forward handoff gate");
                let mut process = launch(&server, &tunnel);
                let address = SocketAddr::new(ip, tunnel.local_port);
                let mut socket = connect(&mut process, address);
                if mode == "Dynamic" { assert_eq!(socks(&mut socket, echo.address).unwrap(), 0); }
                socket.write_all(PAYLOAD).unwrap();
                let mut bytes = [0; 9];
                socket.read_exact(&mut bytes).unwrap();
                assert_eq!(&bytes, PAYLOAD);
                assert_eq!(echo.count.load(Ordering::SeqCst), 1);
                drop(socket);
                drop(process);
                rebind(address);
                println!("parallel roundtrip {mode} {address}: exact=TUNNEL_OK; count=1; cancelled; rebound");
            });
        }
        drop(ready_tx);
        let mut ports = Vec::new();
        for _ in 0..6 {
            ports.extend(
                ready_rx
                    .recv_timeout(Duration::from_secs(8))
                    .expect("fixture selection"),
            );
        }
        let unique: std::collections::HashSet<_> = ports.iter().collect();
        assert_eq!(ports.len(), 18);
        assert_eq!(
            unique.len(),
            ports.len(),
            "live fixture claims overlapped: {ports:?}"
        );
        println!("all 18 fixture claims distinct before concurrent forward handoff: {ports:?}");
        for gate in gates {
            gate.send(()).unwrap();
        }
    });
}
