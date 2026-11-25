use super::{request, value};
use crate::devices::{BoundPrinterMapping, BoundSerialMappings, Channel};
use crate::rdp_options::{PrinterMapping, RdpOptions};
use ironrdp_svc::SvcProcessor;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::{Duration, Instant};

fn mapping() -> PrinterMapping {
    PrinterMapping {
        queue: "Office".into(),
        driver: "Example PS".into(),
    }
}

fn completion(channel: &mut Channel, file: u32, id: u32, major: u32, body: &[u8]) -> Vec<u8> {
    assert!(
        channel
            .process(&request(1, file, id, major, 0, body))
            .unwrap()
            .is_empty()
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let replies = channel.poll().unwrap();
        if let Some(reply) = replies.first() {
            let reply = reply.encode_unframed_pdu().unwrap();
            assert_eq!(&reply[..4], b"rDCI");
            assert_eq!(value(&reply, 4), 1);
            assert_eq!(value(&reply, 8), id);
            return reply;
        }
        assert!(Instant::now() < deadline, "Missing printer completion");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn start(binding: &BoundPrinterMapping) -> Channel {
    let mut channel =
        Channel::with_bound_devices(&[], &BoundSerialMappings::new(&[]).unwrap(), binding).unwrap();
    let caps = channel.process(b"rDPS\0\0\0\0").unwrap()[0]
        .encode_unframed_pdu()
        .unwrap();
    assert_eq!(caps[4..6], [2, 0]);
    assert_eq!(caps[52..], [2, 0, 8, 0, 1, 0, 0, 0]);
    channel.process(b"rDCC\x01\0\x0c\0\x01\0\0\0").unwrap();
    let announce = channel.process(b"rDLU").unwrap()[0]
        .encode_unframed_pdu()
        .unwrap();
    assert_eq!(value(&announce, 4), 1);
    assert_eq!(value(&announce, 8), 4);
    assert_eq!(value(&announce, 12), 1);
    assert_eq!(&announce[16..24], b"PRN1\0\0\0\0");
    assert_eq!(value(&announce, 28), 0);
    assert_eq!(value(&announce, 36), 0);
    assert_eq!(value(&announce, 48), 0);
    let names: Vec<u8> = "Example PS\0Office\0"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    assert_eq!(&announce[52..], names);
    channel
}

fn fixture() -> (
    tempfile::TempDir,
    std::path::PathBuf,
    UnixListener,
    BoundPrinterMapping,
) {
    let tmp = tempfile::tempdir().unwrap();
    let socket = tmp.path().join("cups.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let probe = listener.try_clone().unwrap();
    let readiness = std::thread::spawn(move || answer_readiness(&probe, &ready_queue()));
    let binding = BoundPrinterMapping::at(&mapping(), &socket).unwrap();
    readiness.join().unwrap();
    (tmp, socket, listener, binding)
}

fn reply(stream: &mut UnixStream, body: &[u8]) {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/ipp\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )
    .unwrap();
    stream.write_all(body).unwrap();
}

fn attribute(bytes: &mut Vec<u8>, tag: u8, name: &str, value: &[u8]) {
    bytes.push(tag);
    for field in [name.as_bytes(), value] {
        bytes.extend((field.len() as u16).to_be_bytes());
        bytes.extend(field);
    }
}

fn response(operation: u8, status: u16) -> Vec<u8> {
    let mut body = vec![2, 0];
    body.extend(status.to_be_bytes());
    body.extend([0, 0, 0, operation, 1]);
    attribute(&mut body, 0x47, "attributes-charset", b"utf-8");
    attribute(&mut body, 0x48, "attributes-natural-language", b"en");
    body
}

fn ready_queue() -> Vec<u8> {
    let mut body = response(11, 0);
    body.push(4);
    attribute(
        &mut body,
        0x45,
        "printer-uri-supported",
        b"ipp://localhost:631/printers/Office",
    );
    attribute(&mut body, 0x22, "printer-is-accepting-jobs", &[1]);
    attribute(
        &mut body,
        0x49,
        "document-format-supported",
        b"application/octet-stream",
    );
    attribute(&mut body, 0x49, "", b"application/vnd.cups-raw");
    body.push(3);
    body
}

fn accepted_job() -> Vec<u8> {
    let mut body = response(2, 0);
    body.push(2);
    attribute(&mut body, 0x21, "job-id", &7i32.to_be_bytes());
    attribute(&mut body, 0x45, "job-uri", b"ipp://localhost:631/jobs/7");
    attribute(&mut body, 0x23, "job-state", &3i32.to_be_bytes());
    attribute(&mut body, 0x44, "job-state-reasons", b"none");
    body.push(3);
    body
}

fn accept_job(stream: &mut UnixStream) {
    reply(stream, &accepted_job());
}

fn answer_readiness(listener: &UnixListener, response: &[u8]) {
    let (mut stream, request) = receive_job(listener);
    assert_eq!(&request[..8], &[2, 0, 0, 11, 0, 0, 0, 11]);
    assert_eq!(request.last(), Some(&3));
    let raw = b"application/vnd.cups-raw";
    assert!(request.windows(raw.len()).any(|window| window == raw));
    reply(&mut stream, response);
}

fn write_body(bytes: &[u8]) -> Vec<u8> {
    let mut write = vec![0; 32];
    write[..4].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
    write.extend(bytes);
    write
}

fn connect_from_client(listener: &UnixListener) -> UnixStream {
    listener.set_nonblocking(true).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match listener.accept() {
            Ok((stream, _)) => return stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "Printer never contacted the fixture"
                );
                std::thread::sleep(Duration::from_millis(2));
            }
            Err(error) => panic!("{error}"),
        }
    }
}

fn receive_job(listener: &UnixListener) -> (UnixStream, Vec<u8>) {
    let stream = connect_from_client(listener);
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut reader = BufReader::new(stream);
    let mut header = String::new();
    while !header.ends_with("\r\n\r\n") {
        assert_ne!(reader.read_line(&mut header).unwrap(), 0);
    }
    assert!(
        header.starts_with("POST /printers/Office HTTP/1.0\r\n"),
        "{header}"
    );
    let length: usize = header
        .lines()
        .find_map(|line| line.strip_prefix("Content-Length: "))
        .unwrap()
        .parse()
        .unwrap();
    let mut payload = vec![0; length];
    reader.read_exact(&mut payload).unwrap();
    (reader.into_inner(), payload)
}

#[test]
fn selected_printer_announces_and_submits_complete_ordered_job_to_ipp() {
    let (_tmp, _, listener, binding) = fixture();
    let receiver = std::thread::spawn(move || {
        let (mut stream, payload) = receive_job(&listener);
        assert_eq!(&payload[..8], &[2, 0, 0, 2, 0, 0, 0, 2]);
        assert!(
            payload
                .windows(b"ipp://localhost/printers/Office".len())
                .any(|w| w == b"ipp://localhost/printers/Office")
        );
        assert!(payload.ends_with(b"\x03%!PS\nshowpage\n"));
        accept_job(&mut stream);
    });
    let mut channel = start(&binding);
    let open = completion(&mut channel, 0, 1, 0, &[0; 32]);
    assert_eq!(value(&open, 12), 0);
    assert_eq!(open.len(), 20);
    let handle = value(&open, 16);
    for (id, bytes) in [(2, b"%!PS\n".as_slice()), (3, b"showpage\n".as_slice())] {
        let written = completion(&mut channel, handle, id, 4, &write_body(bytes));
        assert_eq!(value(&written, 12), 0);
        assert_eq!(value(&written, 16) as usize, bytes.len());
        assert_eq!(written.len(), 21);
    }
    let closed = completion(&mut channel, handle, 4, 2, &[0; 32]);
    assert_eq!(value(&closed, 12), 0);
    assert_eq!(closed.len(), 20);
    assert_ne!(
        value(&completion(&mut channel, handle, 5, 2, &[0; 32]), 12),
        0
    );
    receiver.join().unwrap();
}

#[test]
fn default_session_requests_no_device_channel() {
    let mut config = crate::wire::Config {
        version: crate::wire::VERSION,
        options: RdpOptions::default(),
        connect_to: None,
        carrier_token: None,
        host: "localhost".into(),
        port: 3389,
        user: "fixture".into(),
        width: 800,
        height: 600,
        timeout_secs: 10,
        certificate_authority: None,
        remembered_certificate: None,
    };
    let mut connector = ironrdp_connector::ClientConnector::new(
        crate::config::connector(&config),
        "127.0.0.1:0".parse().unwrap(),
    );
    let serial = BoundSerialMappings::new(&[]).unwrap();
    Channel::attach(
        &mut connector,
        &config.options,
        &serial,
        &BoundPrinterMapping::default(),
    )
    .unwrap();
    assert!(
        connector
            .get_static_channel_processor::<Channel>()
            .is_none()
    );

    let (_tmp, _, _listener, binding) = fixture();
    config.options.printer_mapping = Some(mapping());
    Channel::attach(&mut connector, &config.options, &serial, &binding).unwrap();
    let channel = connector.get_static_channel_processor::<Channel>().unwrap();
    assert_eq!(channel.channel_name().as_str(), Some("rdpdr"));
}

#[test]
fn truncated_write_retires_job_without_submitting_a_partial_document() {
    let (_tmp, _, listener, binding) = fixture();
    let mut channel = start(&binding);
    let handle = value(&completion(&mut channel, 0, 1, 0, &[0; 32]), 16);
    assert_eq!(
        value(
            &completion(&mut channel, handle, 2, 4, &write_body(b"partial")),
            12
        ),
        0
    );
    assert_eq!(
        value(&completion(&mut channel, handle, 3, 4, &[7]), 12),
        0xc000000d
    );
    let close = completion(&mut channel, handle, 4, 2, &[0; 32]);
    assert_eq!(value(&close, 12), 0xc0000008);
    listener.set_nonblocking(true).unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn abandoned_job_and_certificate_retry_keep_the_original_queue_binding() {
    let (_tmp, socket, original, binding) = fixture();
    let mut first = start(&binding);
    let handle = value(&completion(&mut first, 0, 1, 0, &[0; 32]), 16);
    assert_eq!(
        value(
            &completion(&mut first, handle, 2, 4, &write_body(b"abandoned")),
            12
        ),
        0
    );
    drop(first);
    // No close arrived, so even a complete-looking document is discarded.
    original.set_nonblocking(true).unwrap();
    assert_eq!(
        original.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    std::fs::remove_file(&socket).unwrap();
    let replacement = UnixListener::bind(&socket).unwrap();
    let receiver = std::thread::spawn(move || {
        answer_readiness(&original, &ready_queue());
        let (mut stream, payload) = receive_job(&original);
        assert!(payload.ends_with(b"\x03retry"));
        accept_job(&mut stream);
    });
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(binding.check_ready())
        .unwrap();
    let mut retry = start(&binding);
    assert_eq!(
        value(&completion(&mut retry, handle, 1, 2, &[0; 32]), 12),
        0xc0000008
    );
    let handle = value(&completion(&mut retry, 0, 2, 0, &[0; 32]), 16);
    assert_eq!(
        value(
            &completion(&mut retry, handle, 3, 4, &write_body(b"retry")),
            12
        ),
        0
    );
    assert_eq!(
        value(&completion(&mut retry, handle, 4, 2, &[0; 32]), 12),
        0
    );
    receiver.join().unwrap();
    replacement.set_nonblocking(true).unwrap();
    assert_eq!(
        replacement.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn stalled_cups_response_times_out_and_retires_the_document() {
    let (_tmp, _, listener, binding) = fixture();
    let receiver = std::thread::spawn(move || {
        let (mut stream, payload) = receive_job(&listener);
        assert!(payload.ends_with(b"\x03delayed"));
        let mut byte = [0];
        assert_eq!(stream.read(&mut byte).unwrap(), 0);
    });
    let mut channel = start(&binding);
    let handle = value(&completion(&mut channel, 0, 1, 0, &[0; 32]), 16);
    completion(&mut channel, handle, 2, 4, &write_body(b"delayed"));
    assert_eq!(
        value(&completion(&mut channel, handle, 3, 2, &[0; 32]), 12),
        0xc00000b5
    );
    assert_eq!(
        value(&completion(&mut channel, handle, 4, 2, &[0; 32]), 12),
        0xc0000008
    );
    receiver.join().unwrap();
}

#[test]
fn disconnect_cancels_an_in_flight_cups_upload_without_finishing_it() {
    let (_tmp, _, listener, binding) = fixture();
    let mut channel = start(&binding);
    let handle = value(&completion(&mut channel, 0, 1, 0, &[0; 32]), 16);
    let chunk = write_body(&vec![b'a'; 64 * 1024]);
    for id in 2..18 {
        assert_eq!(
            value(&completion(&mut channel, handle, id, 4, &chunk), 12),
            0
        );
    }
    channel
        .process(&request(1, handle, 18, 2, 0, &[0; 32]))
        .unwrap();
    let mut stream = connect_from_client(&listener);
    drop(channel);
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut transmitted = Vec::new();
    stream.read_to_end(&mut transmitted).unwrap();
    assert!(transmitted.len() < 16 * 64 * 1024);
}

#[test]
fn malformed_pdu_and_job_limits_preserve_the_channel() {
    let (_tmp, _, listener, binding) = fixture();
    let mut channel = start(&binding);
    assert!(channel.process(b"rDRI\x01\0\0\0").is_err());
    let mut malicious_path = [0; 33];
    malicious_path[28] = 1;
    assert_eq!(
        value(&completion(&mut channel, 0, 1, 0, &malicious_path), 12),
        0xc000000d
    );
    for id in 0..4 {
        let response = completion(&mut channel, 0, 2 + id, 0, &[0; 32]);
        assert_eq!(value(&response, 12), 0);
        assert_eq!(value(&response, 16), id);
    }
    assert_eq!(
        value(&completion(&mut channel, 0, 6, 0, &[0; 32]), 12),
        0xc000009a
    );
    assert_eq!(
        value(&completion(&mut channel, 0, 7, 2, &[0; 31]), 12),
        0xc000000d
    );
    let chunk = write_body(&vec![b'a'; 64 * 1024]);
    for id in 0..512 {
        assert_eq!(
            value(&completion(&mut channel, 1, 10 + id, 4, &chunk), 12),
            0
        );
    }
    assert_eq!(
        value(
            &completion(&mut channel, 1, 522, 4, &write_body(b"overflow")),
            12
        ),
        0xc000007f
    );
    assert_eq!(
        value(&completion(&mut channel, 1, 523, 2, &[0; 32]), 12),
        0xc0000008
    );
    listener.set_nonblocking(true).unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn cups_ipp_rejection_is_reported_even_when_http_succeeds() {
    let (_tmp, _, listener, binding) = fixture();
    let receiver = std::thread::spawn(move || {
        let (mut stream, _) = receive_job(&listener);
        let mut body = response(2, 0x0406);
        body.push(3);
        reply(&mut stream, &body);
    });
    let mut channel = start(&binding);
    let handle = value(&completion(&mut channel, 0, 1, 0, &[0; 32]), 16);
    completion(&mut channel, handle, 2, 4, &write_body(b"rejected"));
    assert_ne!(
        value(&completion(&mut channel, handle, 3, 2, &[0; 32]), 12),
        0
    );
    receiver.join().unwrap();
}

#[test]
fn delayed_close_for_failed_job_cannot_submit_a_new_open_job() {
    let (_tmp, _, listener, binding) = fixture();
    let mut channel = start(&binding);
    let first = value(&completion(&mut channel, 0, 1, 0, &[0; 32]), 16);
    assert_eq!(
        value(&completion(&mut channel, first, 2, 4, &[0]), 12),
        0xc000000d
    );
    let second = value(&completion(&mut channel, 0, 3, 0, &[0; 32]), 16);
    assert_eq!(
        value(
            &completion(&mut channel, second, 4, 4, &write_body(b"second")),
            12
        ),
        0
    );
    assert_eq!(
        value(&completion(&mut channel, first, 5, 2, &[0; 32]), 12),
        0xc0000008
    );
    assert_ne!(first, second);
    assert_eq!(
        value(
            &completion(&mut channel, second, 6, 4, &write_body(b" complete")),
            12
        ),
        0
    );
    let receiver = std::thread::spawn(move || {
        let (mut stream, payload) = receive_job(&listener);
        assert!(payload.ends_with(b"\x03second complete"));
        accept_job(&mut stream);
    });
    assert_eq!(
        value(&completion(&mut channel, second, 7, 2, &[0; 32]), 12),
        0
    );
    receiver.join().unwrap();
}

#[test]
fn stale_owned_socket_is_refused_before_announcement() {
    let tmp = tempfile::tempdir().unwrap();
    let socket = tmp.path().join("cups.sock");
    drop(UnixListener::bind(&socket).unwrap());
    let error = BoundPrinterMapping::at(&mapping(), &socket)
        .err()
        .expect("Accepted a stale CUPS socket");
    assert!(
        error.contains("CUPS") && error.contains("Office"),
        "{error}"
    );
}

#[test]
fn malformed_ipp_success_is_an_unconfirmed_submission() {
    let mut missing_job = response(2, 0);
    missing_job.push(3);
    let mut truncated = accepted_job();
    truncated.truncate(truncated.len() - 3);
    truncated.push(3);
    let mut mismatched = accepted_job();
    mismatched[7] = 123;
    let mut wrong_job = accepted_job();
    let uri = b"/jobs/7";
    let offset = wrong_job.windows(uri.len()).position(|s| s == uri).unwrap();
    wrong_job[offset + uri.len() - 1] = b'9';
    let mut malformed_uri = accepted_job();
    let authority = b"localhost:631";
    let offset = malformed_uri
        .windows(authority.len())
        .position(|s| s == authority)
        .unwrap();
    malformed_uri[offset] = b'@';
    let mut wrong_version = accepted_job();
    wrong_version[0] = 1;
    let mut wrong_queue = accepted_job();
    wrong_queue.pop();
    attribute(
        &mut wrong_queue,
        0x45,
        "job-printer-uri",
        b"ipp://localhost/printers/Other",
    );
    wrong_queue.push(3);
    for malformed in [
        vec![2, 0, 0, 0, 0, 0, 0, 2, 3],
        missing_job,
        truncated,
        mismatched,
        wrong_job,
        malformed_uri,
        wrong_version,
        wrong_queue,
    ] {
        let (_tmp, _, listener, binding) = fixture();
        let receiver = std::thread::spawn(move || {
            let (mut stream, payload) = receive_job(&listener);
            assert!(payload.ends_with(b"\x03unconfirmed"));
            reply(&mut stream, &malformed);
            listener
        });
        let mut channel = start(&binding);
        let handle = value(&completion(&mut channel, 0, 1, 0, &[0; 32]), 16);
        completion(&mut channel, handle, 2, 4, &write_body(b"unconfirmed"));
        assert_ne!(
            value(&completion(&mut channel, handle, 3, 2, &[0; 32]), 12),
            0
        );
        assert_eq!(
            value(&completion(&mut channel, handle, 4, 2, &[0; 32]), 12),
            0xc0000008
        );
        assert!(channel.take_notice().unwrap().contains("did not confirm"));
        let listener = receiver.join().unwrap();
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
}

#[test]
fn missing_wrong_stopped_or_incompatible_queue_is_refused_before_announcement() {
    let mut missing = response(11, 0x0406);
    missing.push(3);
    let mut wrong = ready_queue();
    let offset = wrong.windows(6).position(|s| s == b"Office").unwrap();
    wrong[offset..offset + 6].copy_from_slice(b"Other1");
    let mut stopped = ready_queue();
    let attribute_name = b"printer-is-accepting-jobs";
    let offset = stopped
        .windows(attribute_name.len())
        .position(|s| s == attribute_name)
        .unwrap();
    stopped[offset + attribute_name.len() + 2] = 0;
    let mut incompatible = ready_queue();
    let offset = incompatible.windows(4).position(|s| s == b"-raw").unwrap();
    incompatible[offset..offset + 4].copy_from_slice(b"-pdf");
    for (response, expected) in [
        (missing, "does not exist"),
        (wrong, "did not identify the selected queue"),
        (stopped, "is not accepting print jobs"),
        (incompatible, "does not support raw print data"),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let socket = tmp.path().join("cups.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let worker = std::thread::spawn(move || answer_readiness(&listener, &response));
        let error = BoundPrinterMapping::at(&mapping(), &socket)
            .err()
            .expect("Accepted an unavailable queue");
        assert!(
            error.contains("Office") && error.contains(expected),
            "{error}"
        );
        worker.join().unwrap();
    }
}
