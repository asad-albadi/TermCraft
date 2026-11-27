//! Owned loopback-only HTTP CONNECT relay with byte counters; never contacts public endpoints.
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
pub struct Proxy {
    pub address: SocketAddr,
    pub requested: Arc<Mutex<Option<String>>>,
    pub client_bytes: Arc<AtomicUsize>,
    pub authenticated: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl Proxy {
    pub fn start(target: SocketAddr) -> Self {
        Self::with_mode(target, 0)
    }
    pub fn http_authenticated(target: SocketAddr) -> Self {
        Self::with_mode(target, 1)
    }
    pub fn socks_authenticated(target: SocketAddr) -> Self {
        Self::with_mode(target, 2)
    }
    fn with_mode(target: SocketAddr, mode: u8) -> Self {
        assert!(target.ip().is_loopback());
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let requested = Arc::new(Mutex::new(None));
        let request = requested.clone();
        let client_bytes = Arc::new(AtomicUsize::new(0));
        let bytes = client_bytes.clone();
        let authenticated = Arc::new(AtomicBool::new(false));
        let auth = authenticated.clone();
        let worker = std::thread::spawn(move || {
            let mut client = loop {
                if stopped.load(Ordering::Acquire) {
                    return;
                }
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2))
                    }
                    Err(_) => return,
                }
            };
            client.set_nonblocking(false).unwrap();
            client
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            client
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            if mode == 2 {
                let mut greeting = [0; 2];
                if client.read_exact(&mut greeting).is_err() {
                    return;
                }
                assert_eq!(greeting[0], 5);
                let mut methods = vec![0; greeting[1] as usize];
                client.read_exact(&mut methods).unwrap();
                assert!(methods.contains(&2));
                client.write_all(&[5, 2]).unwrap();
                let mut head = [0; 2];
                if client.read_exact(&mut head).is_err() {
                    return;
                }
                assert_eq!(head[0], 1);
                let mut user = vec![0; head[1] as usize];
                client.read_exact(&mut user).unwrap();
                let mut len = [0; 1];
                client.read_exact(&mut len).unwrap();
                let mut pass = vec![0; len[0] as usize];
                client.read_exact(&mut pass).unwrap();
                assert_eq!(user, b"proxy-user");
                assert_eq!(pass, b"SENTINEL_PROXY_PASSWORD");
                auth.store(true, Ordering::Release);
                client.write_all(&[1, 0]).unwrap();
                let mut head = [0; 4];
                client.read_exact(&mut head).unwrap();
                assert_eq!(&head[..3], &[5, 1, 0]);
                let host = match head[3] {
                    1 => {
                        let mut ip = [0; 4];
                        client.read_exact(&mut ip).unwrap();
                        std::net::Ipv4Addr::from(ip).to_string()
                    }
                    3 => {
                        client.read_exact(&mut len).unwrap();
                        let mut host = vec![0; len[0] as usize];
                        client.read_exact(&mut host).unwrap();
                        String::from_utf8(host).unwrap()
                    }
                    _ => panic!("Unexpected proxy address"),
                };
                let mut port = [0; 2];
                client.read_exact(&mut port).unwrap();
                let authority = format!("{host}:{}", u16::from_be_bytes(port));
                assert_eq!(authority, target.to_string());
                *request.lock().unwrap() = Some(authority);
                client.write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0]).unwrap();
            } else {
                let end = Instant::now() + Duration::from_secs(5);
                let mut header = Vec::new();
                while !header.ends_with(b"\r\n\r\n") {
                    if stopped.load(Ordering::Acquire)
                        || Instant::now() > end
                        || header.len() > 16384
                    {
                        return;
                    }
                    let mut b = [0; 1];
                    if client.read_exact(&mut b).is_err() {
                        return;
                    }
                    header.push(b[0]);
                }
                let text = String::from_utf8(header).unwrap();
                let authority = text
                    .lines()
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .nth(1)
                    .unwrap()
                    .to_owned();
                *request.lock().unwrap() = Some(authority.clone());
                assert_eq!(authority, target.to_string());
                if mode == 1 {
                    use base64::Engine;
                    let expected = base64::engine::general_purpose::STANDARD
                        .encode("proxy-user:SENTINEL_PROXY_PASSWORD");
                    assert!(
                        text.lines()
                            .any(|l| l == format!("Proxy-Authorization: Basic {expected}"))
                    );
                    auth.store(true, Ordering::Release);
                }
                client
                    .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                    .unwrap();
            }
            let mut upstream = TcpStream::connect(target).unwrap();
            upstream.set_nonblocking(true).unwrap();
            client.set_nonblocking(true).unwrap();
            let (mut to_upstream, mut to_client) = (Vec::new(), Vec::new());
            let mut block = [0; 32768];
            while !stopped.load(Ordering::Acquire) {
                match client.read(&mut block) {
                    Ok(0) => break,
                    Ok(n) => {
                        bytes.fetch_add(n, Ordering::AcqRel);
                        to_upstream.extend_from_slice(&block[..n]);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(_) => break,
                }
                match upstream.read(&mut block) {
                    Ok(0) => break,
                    Ok(n) => to_client.extend_from_slice(&block[..n]),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(_) => break,
                }
                if !to_upstream.is_empty() {
                    match upstream.write(&to_upstream) {
                        Ok(n) => {
                            to_upstream.drain(..n);
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                        Err(_) => break,
                    }
                }
                if !to_client.is_empty() {
                    match client.write(&to_client) {
                        Ok(n) => {
                            to_client.drain(..n);
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                        Err(_) => break,
                    }
                }
                assert!(to_client.len() < 1024 * 1024 && to_upstream.len() < 1024 * 1024);
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        Self {
            address,
            requested,
            client_bytes,
            authenticated,
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for Proxy {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}
