use harbor_workspace::protocols::{LegacyProtocol, LegacyProtocolConfig, Transport};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

static RESERVED_TESTS: Mutex<()> = Mutex::new(());

const WAIT: Duration = Duration::from_secs(4);

fn fixture() -> (TcpListener, u16) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();
    (listener, port)
}

fn accept(listener: TcpListener) -> TcpStream {
    let started = Instant::now();
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                stream.set_read_timeout(Some(WAIT)).unwrap();
                stream.set_write_timeout(Some(WAIT)).unwrap();
                return stream;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    started.elapsed() < WAIT,
                    "fixture did not receive a connection"
                );
                thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("{error}"),
        }
    }
}

fn config(protocol: LegacyProtocol, port: u16) -> LegacyProtocolConfig {
    LegacyProtocolConfig {
        host: "127.0.0.1".into(),
        port,
        user: "alice".into(),
        protocol,
        cols: 255,
        rows: 24,
        command: "printf hello".into(),
        carrier: None,
        gateway_required: false,
    }
}

fn read_output(transport: &Transport) -> Vec<u8> {
    let started = Instant::now();
    loop {
        if let Ok(bytes) = transport.with_output(|rx, _| rx.try_recv()) {
            return bytes;
        }
        assert!(
            started.elapsed() < WAIT,
            "terminal output timed out: {:?}",
            transport.input_error()
        );
        thread::sleep(Duration::from_millis(5));
    }
}

fn expect(stream: &mut TcpStream, expected: &[u8]) {
    let mut observed = vec![0; expected.len()];
    stream
        .read_exact(&mut observed)
        .unwrap_or_else(|error| panic!("expected {expected:?}: {error}"));
    assert_eq!(observed, expected);
}

#[test]
fn telnet_iac_is_not_displayed_as_text() {
    let (listener, port) = fixture();
    let worker = thread::spawn(move || {
        let mut stream = accept(listener);
        stream.write_all(&[255, 251, 1, 255, 253, 31]).unwrap();
        expect(
            &mut stream,
            &[
                255, 253, 1, 255, 251, 31, 255, 250, 31, 0, 255, 255, 0, 24, 255, 240,
            ],
        );
        stream.write_all(&[255, 253, 24, 255, 253, 39]).unwrap();
        expect(&mut stream, &[255, 251, 24, 255, 251, 39]);
        stream
            .write_all(&[255, 250, 24, 1, 255, 240, 255, 250, 39, 1, 0])
            .unwrap();
        stream.write_all(b"USER\xff\xf0hello\xff").unwrap();
        expect(
            &mut stream,
            b"\xff\xfa\x18\0XTERM-256COLOR\xff\xf0\xff\xfa\x27\0\0USER\x01alice\xff\xf0",
        );
        stream.write_all(b"\xffworld").unwrap();
        expect(&mut stream, &[255, 250, 31, 0, 80, 0, 40, 255, 240]);
        expect(&mut stream, b"A\xff\xffB");
    });
    let transport = Transport::connect(config(LegacyProtocol::Telnet, port)).unwrap();
    let mut displayed_telnet_payload = Vec::new();
    while displayed_telnet_payload.len() < 11 {
        displayed_telnet_payload.extend(read_output(&transport));
    }
    assert_eq!(displayed_telnet_payload, b"hello\xffworld");
    transport.resize(80, 40).unwrap();
    transport.write(b"A\xffB").unwrap();
    worker.join().unwrap();
}

#[test]
fn configured_legacy_port_and_user_reach_server() {
    let _serial = RESERVED_TESTS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    for protocol in [LegacyProtocol::Rlogin, LegacyProtocol::Rsh] {
        // Probe the same reserved range used by the transport. An ordinary
        // account may be allowed to bind it depending on the OS configuration.
        if !reserved_available() {
            eprintln!(
                "{protocol:?} successful handshake unavailable: cannot bind a reserved TCP source port"
            );
            assert_reserved_port_error(protocol);
            continue;
        }
        let (listener, configured_port) = fixture();
        let mut transport = Transport::connect(config(protocol, configured_port)).unwrap();
        let mut stream = accept(listener);
        assert_eq!(stream.local_addr().unwrap().port(), configured_port);
        assert!((512..=1023).contains(&stream.peer_addr().unwrap().port()));
        expect(&mut stream, &[0]); // Rlogin null or Rsh single-channel selector.
        let mut fields = Vec::new();
        for _ in 0..3 {
            let mut field = Vec::new();
            for byte in (&mut stream).bytes() {
                let byte = byte.unwrap();
                if byte == 0 {
                    break;
                }
                field.push(byte);
            }
            fields.push(String::from_utf8(field).unwrap());
        }
        assert!(!fields[0].is_empty());
        assert_eq!(fields[1], "alice");
        assert_eq!(
            fields[2],
            if protocol == LegacyProtocol::Rlogin {
                "xterm-256color/38400"
            } else {
                "printf hello"
            }
        );
        stream.write_all(b"\0hello").unwrap();
        assert_eq!(read_output(&transport), b"hello");
        assert!(transport.is_ready());
        transport.write(b"stdin").unwrap();
        expect(&mut stream, b"stdin");
        transport.close();
        assert_finished(&transport);
        assert_eq!(stream.read(&mut [0]).unwrap(), 0);
    }
}

fn reserved_available() -> bool {
    (512..=1023)
        .rev()
        .any(|port| TcpListener::bind(("0.0.0.0", port)).is_ok())
}

fn assert_finished(transport: &Transport) {
    let started = Instant::now();
    while !transport.is_finished() {
        assert!(started.elapsed() < WAIT, "transport worker did not finish");
        thread::sleep(Duration::from_millis(5));
    }
}

fn assert_reserved_port_error(protocol: LegacyProtocol) {
    let (_listener, port) = fixture();
    let transport = Transport::connect(config(protocol, port)).unwrap();
    assert_finished(&transport);
    let error = transport.input_error().unwrap();
    assert!(error.contains("reserved TCP source port"), "{error}");
}

#[test]
fn unsupported_privileged_port_requirement_is_explicit() {
    for protocol in [LegacyProtocol::Rlogin, LegacyProtocol::Rsh] {
        let (_listener, port) = fixture();
        let carrier = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let mut profile = config(protocol, port);
        profile.carrier = Some(carrier);
        let error = profile.validate().unwrap_err();
        assert!(error.contains("reserved TCP source port"), "{error}");
    }
}

#[test]
fn telnet_uses_carrier_without_resolving_configured_target() {
    let (listener, port) = fixture();
    let carrier = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let mut profile = config(LegacyProtocol::Telnet, 23);
    profile.host = "terminal.invalid".into();
    profile.gateway_required = true;
    profile.carrier = Some(carrier);
    let mut transport = Transport::connect(profile).unwrap();
    let mut stream = accept(listener);
    stream.write_all(b"tunneled").unwrap();
    assert_eq!(read_output(&transport), b"tunneled");
    transport.close();
    assert_finished(&transport);
    assert_eq!(stream.read(&mut [0]).unwrap(), 0);
}

#[test]
fn closed_transport_drops_queued_output() {
    let (listener, port) = fixture();
    let mut transport = Transport::connect(config(LegacyProtocol::Telnet, port)).unwrap();
    let mut stream = accept(listener);
    let producer = thread::spawn(move || {
        let block = vec![b'x'; 65536];
        while stream.write_all(&block).is_ok() {}
    });
    thread::sleep(Duration::from_millis(60));
    transport.close();
    assert_finished(&transport);
    producer.join().unwrap();
    assert!(transport.input_error().is_none());
}

#[cfg(unix)]
#[test]
fn rlogin_window_size_follows_server_urgent_request() {
    let _serial = RESERVED_TESTS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    use std::os::fd::AsRawFd;
    if !reserved_available() {
        eprintln!("Rlogin window negotiation unavailable: cannot bind a reserved TCP source port");
        assert_reserved_port_error(LegacyProtocol::Rlogin);
        return;
    }
    let (listener, port) = fixture();
    let transport = Transport::connect(config(LegacyProtocol::Rlogin, port)).unwrap();
    let mut stream = accept(listener);
    expect(&mut stream, &[0]);
    let fields = (&mut stream)
        .bytes()
        .map(Result::unwrap)
        .filter(|byte| *byte == 0)
        .take(3)
        .count();
    assert_eq!(fields, 3, "incomplete Rlogin greeting");
    stream.write_all(b"\0").unwrap();
    let flags = [0x80u8];
    assert_eq!(
        unsafe { libc::send(stream.as_raw_fd(), flags.as_ptr().cast(), 1, libc::MSG_OOB) },
        1
    );
    expect(&mut stream, b"\xff\xffss\0\x18\0\xff\0\0\0\0");
    transport.resize(100, 42).unwrap();
    expect(&mut stream, b"\xff\xffss\0\x2a\0\x64\0\0\0\0");
    stream.write_all(b"normal").unwrap();
    assert_eq!(read_output(&transport), b"normal");
}

#[test]
fn close_interrupts_unacknowledged_legacy_handshake() {
    let _serial = RESERVED_TESTS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if !reserved_available() {
        eprintln!("Rsh handshake cancellation unavailable: cannot bind a reserved TCP source port");
        assert_reserved_port_error(LegacyProtocol::Rsh);
        return;
    }
    let (listener, port) = fixture();
    let mut transport = Transport::connect(config(LegacyProtocol::Rsh, port)).unwrap();
    let mut stream = accept(listener);
    expect(&mut stream, b"\0");
    assert!(!transport.is_ready());
    transport.close();
    assert_finished(&transport);
    let mut greeting = Vec::new();
    stream.read_to_end(&mut greeting).unwrap();
    assert!(greeting.ends_with(b"\0alice\0printf hello\0"));
}

#[test]
fn telnet_gateway_profile_cannot_fall_back_to_direct_tcp() {
    use harbor_workspace::model::Session;

    let (listener, port) = fixture();
    let session = Session {
        protocol: "Telnet".into(),
        host: "127.0.0.1".into(),
        port,
        jump: "gateway.example".into(),
        ..Session::default()
    };
    let profile = LegacyProtocolConfig::from_session(&session, 80, 24).unwrap();
    let error = Transport::connect(profile).err().unwrap();
    assert!(
        error.contains("requires a connected SSH gateway"),
        "{error}"
    );
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn close_interrupts_backpressured_terminal_input() {
    let (listener, port) = fixture();
    let mut transport = Transport::connect(config(LegacyProtocol::Telnet, port)).unwrap();
    let _peer = accept(listener); // Keep the connection open without draining input.
    let block = vec![b'x'; 64 * 1024];
    let started = Instant::now();
    let mut accepted = 0;
    loop {
        match transport.write(&block) {
            Ok(()) => accepted += block.len(),
            Err(error) => {
                assert!(error.contains("queue is full"), "{error}");
                if accepted >= 2 * 1024 * 1024 {
                    break;
                }
                thread::sleep(Duration::from_millis(5));
            }
        }
        assert!(
            started.elapsed() < WAIT,
            "input never reached sustained backpressure"
        );
    }
    transport.close();
    assert_finished(&transport);
    assert!(transport.input_error().is_none());
}

#[test]
fn telnet_enter_completes_line_and_echo_tracks_peer_negotiation() {
    let (listener, port) = fixture();
    let transport = Transport::connect(config(LegacyProtocol::Telnet, port)).unwrap();
    let mut stream = accept(listener);

    transport.write(b"show").unwrap();
    transport.enter().unwrap();
    transport.write(b"\r").unwrap();
    expect(&mut stream, b"show\r\n\r\0");
    let mut echoed = Vec::new();
    while echoed.len() < 6 {
        echoed.extend(read_output(&transport));
    }
    assert_eq!(echoed, b"show\r\n");

    stream.write_all(&[255, 251, 1]).unwrap();
    expect(&mut stream, &[255, 253, 1]);
    transport.write(b"silent").unwrap();
    transport.enter().unwrap();
    expect(&mut stream, b"silent\r\n");
    stream.write_all(b"echoed once\r\n").unwrap();
    let mut displayed = Vec::new();
    while displayed.len() < b"echoed once\r\n".len() {
        displayed.extend(read_output(&transport));
    }
    assert_eq!(displayed, b"echoed once\r\n");

    stream.write_all(&[255, 252, 1]).unwrap();
    expect(&mut stream, &[255, 254, 1]);
    transport.write(b"x").unwrap();
    expect(&mut stream, b"x");
    assert_eq!(read_output(&transport), b"x");
}

#[cfg(unix)]
#[test]
fn rlogin_urgent_flush_discards_queued_and_unread_pre_mark_data() {
    use std::os::fd::AsRawFd;
    let _serial = RESERVED_TESTS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if !reserved_available() {
        eprintln!("Rlogin output flush unavailable: cannot bind a reserved TCP source port");
        assert_reserved_port_error(LegacyProtocol::Rlogin);
        return;
    }
    let (listener, port) = fixture();
    let transport = Transport::connect(config(LegacyProtocol::Rlogin, port)).unwrap();
    let mut stream = accept(listener);
    expect(&mut stream, &[0]);
    assert_eq!(
        (&mut stream)
            .bytes()
            .map(Result::unwrap)
            .filter(|byte| *byte == 0)
            .take(3)
            .count(),
        3
    );
    stream.write_all(b"\0before").unwrap();
    // The application may hold a partially consumed output chunk.
    let pending = read_output(&transport);
    assert_eq!(pending, b"before");
    stream.write_all(&vec![b'X'; 272 * 1024]).unwrap();
    thread::sleep(Duration::from_millis(60));
    let flags = [0x82u8]; // FLUSHWRITE + WINDOW acknowledges worker observation.
    assert_eq!(
        unsafe { libc::send(stream.as_raw_fd(), flags.as_ptr().cast(), 1, libc::MSG_OOB) },
        1
    );
    stream.write_all(b"after").unwrap();
    expect(&mut stream, b"\xff\xffss\0\x18\0\xff\0\0\0\0");
    let mut flushed = false;
    let mut displayed = Vec::new();
    let started = Instant::now();
    while displayed.len() < 5 {
        transport.with_output(|rx, did_flush| {
            flushed |= did_flush;
            while let Ok(bytes) = rx.try_recv() {
                displayed.extend(bytes);
            }
        });
        assert!(
            started.elapsed() < WAIT,
            "no output after Rlogin urgent flush"
        );
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        flushed,
        "consumer must discard its partially drained buffer"
    );
    assert_eq!(displayed, b"after");
}
