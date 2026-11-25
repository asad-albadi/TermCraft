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
use support::ssh_server::SshServer;
const PAYLOAD: &[u8] = b"TUNNEL_OK";
struct Echo {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    count: Arc<AtomicUsize>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Echo {
    fn start(ip: IpAddr) -> Self {
        let listener = TcpListener::bind((ip, 0)).expect("required address family unavailable");
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let count = Arc::new(AtomicUsize::new(0));
        let (s, c) = (stop.clone(), count.clone());
        let worker = thread::spawn(move || {
            while !s.load(Ordering::Acquire) {
                if let Ok((mut socket, peer)) = listener.accept() {
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
        }
    }
}
impl Drop for Echo {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.worker.take().unwrap().join().unwrap();
    }
}
fn port(ip: IpAddr) -> u16 {
    TcpListener::bind((ip, 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}
fn config(server: &SshServer, mode: &str, bind: IpAddr, destination: SocketAddr) -> Tunnel {
    Tunnel {
        host: "127.0.0.1".into(),
        user: std::env::var("USER").unwrap(),
        ssh_port: server.port,
        key: server.root.join("client").display().to_string(),
        mode: mode.into(),
        bind: bind.to_string(),
        local_port: port(bind),
        destination: destination.ip().to_string(),
        remote_port: destination.port(),
        ..Default::default()
    }
}
fn launch(server: &SshServer, tunnel: &Tunnel) -> Process {
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
    Process::spawn("ssh", &args, None, 80, 24).unwrap()
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
    for mode in ["Local", "Remote", "Dynamic"] {
        roundtrip(mode, "127.0.0.2");
    }
}
#[test]
fn tunnel_port_conflict_is_visible() {
    let server = SshServer::start(true);
    let echo = Echo::start("127.0.0.1".parse().unwrap());
    for mode in ["Local", "Remote", "Dynamic"] {
        let blocker = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut tunnel = config(&server, mode, "127.0.0.1".parse().unwrap(), echo.address);
        tunnel.local_port = blocker.local_addr().unwrap().port();
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
    let missing = SocketAddr::new(ip, port(ip));
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
