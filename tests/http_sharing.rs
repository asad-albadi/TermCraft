use harbor_workspace::{
    credentials::Secret,
    http_server::{HttpShareConfig, Server},
};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{Shutdown, TcpStream},
    time::Duration,
};
use tempfile::TempDir;

struct Response {
    headers: String,
    status: u16,
    body: Vec<u8>,
}

impl Response {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .lines()
            .filter_map(|line| line.split_once(':'))
            .find_map(|(key, value)| key.eq_ignore_ascii_case(name).then(|| value.trim()))
    }
}

fn connect(server: &Server) -> TcpStream {
    let socket = TcpStream::connect(&server.address).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    socket
}

fn request(server: &Server, request: &str) -> Response {
    let mut socket = connect(server);
    socket.write_all(request.as_bytes()).unwrap();
    response(socket)
}

fn response(mut socket: TcpStream) -> Response {
    socket.shutdown(Shutdown::Write).unwrap();
    let mut raw = Vec::new();
    socket.read_to_end(&mut raw).unwrap();
    let end = raw
        .windows(4)
        .position(|bytes| bytes == b"\r\n\r\n")
        .unwrap();
    let headers = String::from_utf8(raw[..end].to_vec()).unwrap();
    let status = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
    Response {
        headers,
        status,
        body: raw[end + 4..].to_vec(),
    }
}

fn start(root: &TempDir, auth: bool, uploads: bool) -> Server {
    let mut config = HttpShareConfig::new(root.path().to_path_buf(), "127.0.0.1:0".into());
    config.auth = auth.then(|| Secret::new("fixture-token"));
    config.uploads = uploads;
    Server::start_with_config(config).unwrap()
}

#[test]
fn range_returns_correct_bytes_and_headers() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("file"), b"abcdef").unwrap();
    let server = Server::start(root.path(), "127.0.0.1:0").unwrap();
    let response = request(&server, "GET /file HTTP/1.1\r\nRange: bytes=2-4\r\n\r\n");
    assert_eq!(response.status, 206);
    assert_eq!(response.body, b"cde");
    assert_eq!(response.header("Content-Range"), Some("bytes 2-4/6"));
    assert_eq!(response.header("Content-Length"), Some("3"));
    let response = request(&server, "GET /file HTTP/1.1\r\nRange: bytes=7-\r\n\r\n");
    assert_eq!(response.status, 416);
    assert_eq!(response.header("Content-Range"), Some("bytes */6"));
    let response = request(&server, "GET /file HTTP/1.1\r\nRange: bytes=-2\r\n\r\n");
    assert_eq!(response.body, b"ef");
    let response = request(&server, "HEAD /file HTTP/1.1\r\nRange: bytes=2-4\r\n\r\n");
    assert_eq!(response.status, 200);
    assert_eq!(response.header("Content-Length"), Some("6"));
    assert!(response.body.is_empty());
    let etag = response.header("ETag").unwrap();
    let response = request(
        &server,
        &format!("GET /file HTTP/1.1\r\nIf-None-Match: {etag}\r\n\r\n"),
    );
    assert_eq!(response.status, 304);
    assert!(response.body.is_empty());
}

#[test]
fn unauthenticated_upload_cannot_modify_share() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("file"), b"keep").unwrap();
    let server = start(&root, true, true);
    let response = request(
        &server,
        "PUT /file HTTP/1.1\r\nContent-Length: 4\r\n\r\nlose",
    );
    assert_eq!(response.status, 401);
    let response = request(
        &server,
        "GET /file HTTP/1.1\r\nAuthorization: Bearer incorrect\r\n\r\n",
    );
    assert_eq!(response.status, 401);
    assert!(!response.body.windows(4).any(|s| s == b"keep"));
    let response = request(
        &server,
        "GET /file HTTP/1.1\r\nAuthorization: Bearer fixture-token\r\n\r\n",
    );
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"keep");
    assert_eq!(std::fs::read(root.path().join("file")).unwrap(), b"keep");
}

#[test]
fn cancelled_upload_keeps_existing_file() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("existing"), b"keep").unwrap();
    let server = start(&root, false, true);
    assert_eq!(
        request(
            &server,
            "PUT /existing HTTP/1.1\r\nContent-Length: 4\r\n\r\nlose"
        )
        .status,
        409
    );
    assert_eq!(
        request(&server, "PUT /new HTTP/1.1\r\nContent-Length: 5\r\n\r\nabc").status,
        400
    );
    assert_eq!(
        request(
            &server,
            "PUT /huge HTTP/1.1\r\nContent-Length: 999999999\r\n\r\n"
        )
        .status,
        413
    );
    assert_eq!(
        std::fs::read(root.path().join("existing")).unwrap(),
        b"keep"
    );
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
}

#[test]
fn upload_is_opt_in_and_creates_a_new_file() {
    let root = tempfile::tempdir().unwrap();
    let read_only = Server::start(root.path(), "127.0.0.1:0").unwrap();
    let put = "PUT /new HTTP/1.1\r\nContent-Length: 3\r\n\r\nabc";
    assert_eq!(request(&read_only, put).status, 405);
    assert!(!root.path().join("new").exists());
    let server = start(&root, false, true);
    assert_eq!(request(&server, put).status, 201);
    assert_eq!(request(&server, put).status, 409);
    assert_eq!(std::fs::read(root.path().join("new")).unwrap(), b"abc");
}

#[test]
fn rejects_traversal_and_symlink_escape() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("existing"), b"outside").unwrap();
    let server = start(&root, false, true);
    let response = request(
        &server,
        "PUT /%2e%2e/new HTTP/1.1\r\nContent-Length: 3\r\n\r\nabc",
    );
    assert_eq!(response.status, 404);
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outside.path(), root.path().join("link")).unwrap();
        assert_eq!(
            request(&server, "GET /link/existing HTTP/1.1\r\n\r\n").status,
            404
        );
        assert_eq!(
            request(
                &server,
                "PUT /link/new HTTP/1.1\r\nContent-Length: 3\r\n\r\nabc"
            )
            .status,
            404
        );
    }
    assert!(!outside.path().join("new").exists());
}

#[test]
fn dropping_server_cancels_an_in_progress_upload() {
    let root = tempfile::tempdir().unwrap();
    let server = start(&root, false, true);
    let mut socket = connect(&server);
    socket
        .write_all(b"PUT /new HTTP/1.1\r\nContent-Length: 20\r\n\r\nabc")
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while std::fs::read_dir(root.path()).unwrap().next().is_none() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(server);
    let mut response = Vec::new();
    if let Err(err) = socket.read_to_end(&mut response) {
        assert_eq!(err.kind(), std::io::ErrorKind::ConnectionReset);
    }
    while std::fs::read_dir(root.path()).unwrap().next().is_some() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn concurrent_uploads_publish_one_complete_file() {
    let root = tempfile::tempdir().unwrap();
    let server = start(&root, false, true);
    let stage = || {
        let mut socket = BufReader::new(connect(&server));
        socket
            .get_mut()
            .write_all(b"PUT /new HTTP/1.1\r\nContent-Length: 3\r\nExpect: 100-continue\r\n\r\n")
            .unwrap();
        let mut intermediate = String::new();
        socket.read_line(&mut intermediate).unwrap();
        socket.read_line(&mut intermediate).unwrap();
        assert_eq!(intermediate, "HTTP/1.1 100 Continue\r\n\r\n");
        socket.into_inner()
    };
    let mut first = stage();
    let mut second = stage();

    first.write_all(b"abc").unwrap();
    second.write_all(b"xyz").unwrap();
    let statuses = (response(first).status, response(second).status);
    let expected: &[u8] = match statuses {
        (201, 409) => b"abc",
        (409, 201) => b"xyz",
        _ => panic!("expected one upload winner, got {statuses:?}"),
    };
    assert_eq!(std::fs::read(root.path().join("new")).unwrap(), expected);
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
}

#[test]
fn deep_directory_listing_uses_one_base_path() {
    let root = tempfile::tempdir().unwrap();
    let mut directory = root.path().to_path_buf();
    // Keep the full fixture path within Darwin's PATH_MAX while making a
    // repeated absolute base prohibitively large at 240 links.
    let segment = "a".repeat(48);
    let mut target = String::new();
    for _ in 0..16 {
        directory.push(&segment);
        std::fs::create_dir(&directory).unwrap();
        target.push('/');
        target.push_str(&segment);
    }
    for index in 0..240 {
        std::fs::write(directory.join(format!("entry-{index}")), b"").unwrap();
    }
    let server = start(&root, false, false);
    let listing = request(&server, &format!("GET {target} HTTP/1.1\r\n\r\n"));
    assert_eq!(listing.status, 200);
    assert!(listing.body.len() < 32 * 1024);
    let body = String::from_utf8(listing.body).unwrap();
    assert!(body.contains(&format!("<base href=\"{target}/\">")));
    assert!(body.contains("href=\"./entry-239\""));
}

#[cfg(unix)]
#[test]
fn directory_listing_rejects_bodies_over_the_render_limit() {
    let root = tempfile::tempdir().unwrap();
    let long_name = "&".repeat(220);
    for index in 0..580 {
        std::fs::write(root.path().join(format!("{long_name}{index}")), b"").unwrap();
    }
    let server = start(&root, false, false);
    let listing = request(&server, "GET / HTTP/1.1\r\n\r\n");
    assert_eq!(listing.status, 200);
    assert!(
        (1000 * 1024..=1024 * 1024).contains(&listing.body.len()),
        "rendered listing was {} bytes",
        listing.body.len()
    );

    for index in 580..590 {
        std::fs::write(root.path().join(format!("{long_name}{index}")), b"").unwrap();
    }
    let over_limit = request(&server, "GET / HTTP/1.1\r\n\r\n");
    assert_eq!(over_limit.status, 503);
    assert!(over_limit.body.len() < 1024);
}

#[test]
fn directory_listing_reports_when_hidden_entries_exceed_the_scan_limit() {
    let root = tempfile::tempdir().unwrap();
    for index in 0..4097 {
        std::fs::write(
            root.path().join(format!(".harbor-upload-fixture-{index}")),
            b"",
        )
        .unwrap();
    }
    let server = start(&root, false, false);
    let response = request(&server, "GET / HTTP/1.1\r\n\r\n");
    assert_eq!(response.status, 503);
}
