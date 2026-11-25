use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[derive(Clone)]
pub(super) struct Request {
    pub target: String,
    pub headers: Vec<(String, String)>,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(header, _)| header.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

pub(super) struct Fixture {
    pub origin: String,
    pub requests: Arc<Mutex<Vec<Request>>>,
    done: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Fixture {
    pub fn http(reply: fn(&Request) -> Response) -> Self {
        Self::start("http", move |stream, requests| {
            if let Ok(request) = parse_request(&mut *stream) {
                let response = reply(&request);
                requests.lock().unwrap().push(request);
                let _ = response.write(stream);
            }
        })
    }

    pub fn untrusted_https() -> Self {
        let certificate =
            rustls::pki_types::CertificateDer::from(include_bytes!("ftp_tls/server.der").to_vec());
        let key = rustls::pki_types::PrivateKeyDer::try_from(
            include_bytes!("ftp_tls/server.key.der").to_vec(),
        )
        .unwrap();
        let config = rustls::ServerConfig::builder_with_provider(
            rustls::crypto::ring::default_provider().into(),
        )
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![certificate], key)
        .unwrap();
        let config = Arc::new(config);
        Self::start("https", move |stream, requests| {
            let mut tls = rustls::StreamOwned::new(
                rustls::ServerConnection::new(config.clone()).unwrap(),
                stream,
            );
            if let Ok(request) = parse_request(&mut tls) {
                requests.lock().unwrap().push(request);
                let _ = Response::html("<title>Untrusted</title>").write(&mut tls);
            }
        })
    }

    fn start(
        scheme: &str,
        mut serve: impl FnMut(&mut TcpStream, &Mutex<Vec<Request>>) + Send + 'static,
    ) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("{scheme}://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let done = Arc::new(AtomicBool::new(false));
        let worker = {
            let done = done.clone();
            let requests = requests.clone();
            thread::spawn(move || {
                while !done.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            let timeout = Some(Duration::from_millis(500));
                            stream.set_read_timeout(timeout).unwrap();
                            stream.set_write_timeout(timeout).unwrap();
                            serve(&mut stream, &requests);
                        }
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("Fixture listener failed: {error}"),
                    }
                }
            })
        };
        Self {
            origin,
            requests,
            done,
            worker: Some(worker),
        }
    }

    pub fn saw(&self, target: &str) -> bool {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .any(|request| request.target == target)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn parse_request(stream: &mut impl Read) -> io::Result<Request> {
    let mut stream = BufReader::new(stream.take(16 * 1024));
    let mut line = String::new();
    stream.read_line(&mut line)?;
    let target = line
        .split_whitespace()
        .nth(1)
        .ok_or(io::ErrorKind::InvalidData)?
        .to_owned();
    let mut headers = Vec::new();
    loop {
        line.clear();
        if stream.read_line(&mut line)? == 0 || line.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.into(), value.trim().into()));
        }
    }
    Ok(Request { target, headers })
}

pub(super) struct Response {
    pub status: &'static str,
    pub header: &'static str,
    pub body: &'static str,
}

impl Response {
    pub fn html(body: &'static str) -> Self {
        Self {
            status: "200 OK",
            header: "",
            body,
        }
    }

    fn write(self, stream: &mut impl Write) -> io::Result<()> {
        write!(
            stream,
            "HTTP/1.1 {}\r\n{}Content-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            self.status,
            self.header,
            self.body.len(),
            self.body
        )
    }
}
