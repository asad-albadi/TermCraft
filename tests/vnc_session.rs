#[path = "support/vnc_server.rs"]
mod fixture;

use harbor_workspace::desktop::{
    DesktopEvent, DesktopInput, DesktopSession, rdp_options::ClipboardPolicy,
};
use std::{
    io::{Read, Write},
    time::{Duration, Instant},
};

fn until(session: &mut DesktopSession, mut accept: impl FnMut(DesktopEvent) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if session.poll().into_iter().any(&mut accept) {
            return;
        }
        assert!(Instant::now() < deadline, "VNC event timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn vnc_known_frame_and_pointer_coordinates_match() {
    let (listener, config) = fixture::endpoint(false, ClipboardPolicy::Disabled);
    let mut session = DesktopSession::connect(config).unwrap();
    let mut server = fixture::accept(listener);
    let enc = fixture::encodings(&mut server);
    assert!(
        enc.contains(&5),
        "Hextile is missing from the advertised encodings"
    );
    assert_eq!(
        fixture::read(&mut server, 10),
        [3, 0, 0, 0, 0, 0, 0, 20, 0, 4]
    );
    // Hextile background blue, subrectangle at (1,1) green, then a second
    // tile that inherits both colours and has a green pixel at its edge.
    fixture::update(
        &mut server,
        (0, 0, 20, 4),
        5,
        &[14, 255, 0, 0, 0, 0, 255, 0, 0, 1, 0x11, 0, 8, 1, 0x30, 0],
    );
    assert_eq!(fixture::read(&mut server, 10)[1], 1);
    until(&mut session, |event| match event {
        DesktopEvent::Frame(frame) => {
            assert_eq!(frame.pixels[84..88], [0, 255, 0, 255]);
            assert_eq!(frame.pixels[76..80], [0, 255, 0, 255]);
            assert_eq!(frame.pixels[..4], [0, 0, 255, 255]);
            true
        }
        DesktopEvent::Disconnected { message } => panic!("{message}"),
        _ => false,
    });
    session
        .send(DesktopInput::Pointer {
            x: 11,
            y: 3,
            button: Some(1),
            pressed: true,
        })
        .unwrap();
    assert_eq!(fixture::read(&mut server, 6), [5, 1, 0, 11, 0, 3]);
    session
        .send(DesktopInput::Key {
            code: 30,
            extended: false,
            pressed: true,
        })
        .unwrap();
    assert_eq!(fixture::read(&mut server, 8), [4, 1, 0, 0, 0, 0, 0, b'a']);
    session.send(DesktopInput::ReleaseAll).unwrap();
    assert_eq!(
        fixture::read(&mut server, 14),
        [4, 0, 0, 0, 0, 0, 0, b'a', 5, 0, 0, 11, 0, 3]
    );
    session.close();
    assert_eq!(server.read(&mut [0]).unwrap(), 0);
}

#[test]
fn view_only_sends_no_keyboard_or_pointer() {
    let (listener, config) = fixture::endpoint(true, ClipboardPolicy::FromRemote);
    let mut session = DesktopSession::connect(config).unwrap();
    let mut server = fixture::accept(listener);
    fixture::encodings(&mut server);
    fixture::read(&mut server, 10);
    until(&mut session, |event| {
        matches!(event, DesktopEvent::Connected { .. })
    });
    assert!(
        session
            .send(DesktopInput::Pointer {
                x: 2,
                y: 1,
                button: Some(1),
                pressed: true
            })
            .is_err()
    );
    assert!(
        session
            .send(DesktopInput::Key {
                code: 30,
                extended: false,
                pressed: true
            })
            .is_err()
    );
    assert!(
        session
            .send(DesktopInput::Clipboard {
                text: "local".into()
            })
            .is_err()
    );

    server
        .write_all(&[3, 0, 0, 0, 0, 0, 0, 2, b'a', 0xe9])
        .unwrap();
    until(
        &mut session,
        |event| matches!(event, DesktopEvent::Clipboard { text } if text == "aé"),
    );
    server
        .set_read_timeout(Some(Duration::from_millis(120)))
        .unwrap();
    let error = server.read(&mut [0]).unwrap_err();
    assert!(matches!(
        error.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    ));
}

#[test]
fn vnc_resize_reinitializes_framebuffer() {
    let (listener, config) = fixture::endpoint(false, ClipboardPolicy::Disabled);
    let mut session = DesktopSession::connect(config).unwrap();
    let mut server = fixture::accept(listener);
    fixture::encodings(&mut server);
    fixture::read(&mut server, 10);
    fixture::update(&mut server, (0, 0, 800, 600), -223, &[]);
    assert_eq!(
        fixture::read(&mut server, 10),
        [3, 0, 0, 0, 0, 0, 3, 32, 2, 88]
    );
    until(&mut session, |event| match event {
        DesktopEvent::Frame(frame) => {
            assert_eq!((frame.width, frame.height), (800, 600));
            assert_eq!(frame.pixels.len(), 800 * 600 * 4);
            true
        }
        DesktopEvent::Disconnected { message } => panic!("{message}"),
        _ => false,
    });
}

#[test]
fn extended_clipboard_is_negotiated_before_transfer() {
    let (listener, config) = fixture::endpoint(false, ClipboardPolicy::Bidirectional);
    let _session = DesktopSession::connect(config).unwrap();
    let mut server = fixture::accept(listener);
    assert!(fixture::encodings(&mut server).contains(&(0xc0a1e5ceu32 as i32)));
    fixture::read(&mut server, 10);
    let capabilities = 0x1f000001;
    fixture::extended_clipboard(&mut server, capabilities, &[0; 4]);
    assert_eq!(
        fixture::read_extended_clipboard(&mut server),
        (capabilities, vec![0; 4])
    );
}

#[test]
fn unicode_clipboard_round_trips_over_negotiated_tcp_session() {
    const CAPS: u32 = 0x1f000001;
    const REQUEST: u32 = 0x02000001;
    const NOTIFY: u32 = 0x08000001;
    const PROVIDE: u32 = 0x10000001;
    let (listener, config) = fixture::endpoint(false, ClipboardPolicy::Bidirectional);
    let mut session = DesktopSession::connect(config).unwrap();
    let mut server = fixture::accept(listener);
    fixture::encodings(&mut server);
    fixture::read(&mut server, 10);
    fixture::extended_clipboard(&mut server, CAPS, &[0; 4]);
    assert_eq!(fixture::read_extended_clipboard(&mut server).0, CAPS);

    fixture::extended_clipboard(&mut server, NOTIFY, &[]);
    assert_eq!(
        fixture::read_extended_clipboard(&mut server),
        (REQUEST, vec![])
    );
    // Independent Python zlib vector, compressing big-endian UTF-8 length,
    // Japanese text, CRLF, emoji and the required NUL terminator.
    fixture::extended_clipboard(
        &mut server,
        PROVIDE,
        &[
            120, 156, 99, 96, 96, 16, 120, 54, 125, 233, 179, 57, 107, 94, 172, 154, 199, 203, 245,
            97, 254, 204, 38, 6, 0, 90, 81, 9, 82,
        ],
    );
    until(&mut session, |event| {
        matches!(
            event, DesktopEvent::Clipboard { text } if text == "日本語\n🙂"
        )
    });

    session
        .send(DesktopInput::Clipboard {
            text: "مرحبا\n🙂".into(),
        })
        .unwrap();
    assert_eq!(
        fixture::read_extended_clipboard(&mut server),
        (NOTIFY, vec![])
    );
    fixture::extended_clipboard(&mut server, REQUEST, &[]);
    let (flags, compressed) = fixture::read_extended_clipboard(&mut server);
    assert_eq!(flags, PROVIDE);
    let mut decoded = Vec::new();
    flate2::read::ZlibDecoder::new(compressed.as_slice())
        .read_to_end(&mut decoded)
        .unwrap();
    let text = "مرحبا\r\n🙂\0".as_bytes();
    assert_eq!(decoded[..4], (text.len() as u32).to_be_bytes());
    assert_eq!(&decoded[4..], text);
}

#[test]
fn classic_clipboard_reports_unicode_without_ending_session() {
    let (listener, config) = fixture::endpoint(false, ClipboardPolicy::ToRemote);
    let mut session = DesktopSession::connect(config).unwrap();
    let mut server = fixture::accept(listener);
    fixture::encodings(&mut server);
    fixture::read(&mut server, 10);
    session
        .send(DesktopInput::Clipboard {
            text: "🙂".into()
        })
        .unwrap();
    until(&mut session, |event| {
        matches!(
            event, DesktopEvent::Status { message } if message.contains("Latin-1")
        )
    });
    session
        .send(DesktopInput::Clipboard {
            text: "café".into(),
        })
        .unwrap();
    assert_eq!(
        fixture::read(&mut server, 12),
        [6, 0, 0, 0, 0, 0, 0, 4, b'c', b'a', b'f', 0xe9]
    );
}

#[test]
fn rich_cursor_shape_is_separate_from_framebuffer() {
    let (listener, config) = fixture::endpoint(false, ClipboardPolicy::Disabled);
    let mut session = DesktopSession::connect(config).unwrap();
    let mut server = fixture::accept(listener);
    assert!(fixture::encodings(&mut server).contains(&-239));
    fixture::read(&mut server, 10);
    fixture::update(
        &mut server,
        (1, 0, 2, 1),
        -239,
        &[0, 255, 0, 0, 0, 0, 255, 0, 0x80],
    );
    fixture::read(&mut server, 10);
    until(&mut session, |event| match event {
        DesktopEvent::Frame(_) => panic!("cursor replaced framebuffer pixels"),
        DesktopEvent::CursorShape {
            width,
            height,
            hotspot,
            pixels,
        } => {
            assert_eq!((width, height, hotspot), (2, 1, (1, 0)));
            assert_eq!(pixels, [0, 255, 0, 255, 255, 0, 0, 0]);
            true
        }
        DesktopEvent::Disconnected { message } => panic!("{message}"),
        _ => false,
    });
    fixture::update(&mut server, (0, 0, 0, 0), -239, &[]);
    fixture::read(&mut server, 10);
    until(&mut session, |event| {
        matches!(event, DesktopEvent::CursorShape {
        width: 0, height: 0, pixels, ..
    } if pixels.is_empty())
    });
}

#[test]
fn vnc_rejects_bad_resize_rectangle_and_encoding_before_publish() {
    for (geometry, encoding, payload, error) in [
        ((0, 0, u16::MAX, u16::MAX), -223, &[][..], "dimensions"),
        ((0, 0, 1, 1), 0, &[255][..], "ended during a message"),
        ((0, 0, 1, 1), 7, &[][..], "unrequested encoding 7"),
        (
            (16, 0, 4, 1),
            5,
            &[14, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0x30, 0x10][..],
            "outside tile",
        ),
    ] {
        let (listener, config) = fixture::endpoint(false, ClipboardPolicy::Disabled);
        let mut session = DesktopSession::connect(config).unwrap();
        let mut server = fixture::accept(listener);
        fixture::encodings(&mut server);
        fixture::read(&mut server, 10);
        fixture::update(&mut server, geometry, encoding, payload);
        server.shutdown(std::net::Shutdown::Write).unwrap();
        until(&mut session, |event| match event {
            DesktopEvent::Frame(_) => panic!("published a partial framebuffer"),
            DesktopEvent::Disconnected { message } => {
                assert!(message.contains(error), "{message}");
                true
            }
            _ => false,
        });
    }
}

#[test]
fn vnc_cancel_closes_supplied_tcp_carrier_during_incomplete_rectangle() {
    let (listener, mut config) = fixture::endpoint(false, ClipboardPolicy::Disabled);
    config.stream = Some(std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap());
    config.session.host = "unresolvable.invalid".into();
    let mut session = DesktopSession::connect(config).unwrap();
    let mut server = fixture::accept(listener);
    fixture::encodings(&mut server);
    fixture::read(&mut server, 10);
    fixture::update(&mut server, (0, 0, 1, 1), 0, &[0, 255]);
    session.close();
    assert_eq!(server.read(&mut [0]).unwrap(), 0);
    let deadline = Instant::now() + Duration::from_secs(2);
    while !session.is_finished() {
        assert!(Instant::now() < deadline, "VNC cancel did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn extended_clipboard_respects_view_only_and_bounds_decompression() {
    const CAPS: u32 = 0x1f000001;
    const REQUEST: u32 = 0x02000001;
    const NOTIFY: u32 = 0x08000001;
    const PROVIDE: u32 = 0x10000001;
    let (listener, config) = fixture::endpoint(true, ClipboardPolicy::Bidirectional);
    let mut session = DesktopSession::connect(config).unwrap();
    let mut server = fixture::accept(listener);
    let enc = fixture::encodings(&mut server);
    assert!(!enc.contains(&-239));
    fixture::read(&mut server, 10);
    fixture::extended_clipboard(&mut server, CAPS, &[0; 4]);
    assert_eq!(
        fixture::read_extended_clipboard(&mut server),
        (0x19000001, vec![0; 4])
    );
    assert!(
        session
            .send(DesktopInput::Clipboard {
                text: "private".into()
            })
            .is_err()
    );
    fixture::extended_clipboard(&mut server, REQUEST, &[]);
    fixture::extended_clipboard(&mut server, NOTIFY, &[]);
    // The prohibited upload contributes no frame before our download request.
    assert_eq!(
        fixture::read_extended_clipboard(&mut server),
        (REQUEST, vec![])
    );
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(&[0; 32 * 1024]).unwrap();
    fixture::extended_clipboard(&mut server, PROVIDE, &encoder.finish().unwrap());
    until(&mut session, |event| match event {
        DesktopEvent::Clipboard { .. } => panic!("oversize clipboard escapes its bounds"),
        DesktopEvent::Disconnected { message } => {
            assert!(message.contains("clipboard compression"), "{message}");
            true
        }
        _ => false,
    });
}

#[test]
fn to_remote_policy_does_not_request_remote_clipboard() {
    let (listener, config) = fixture::endpoint(false, ClipboardPolicy::ToRemote);
    let mut session = DesktopSession::connect(config).unwrap();
    let mut server = fixture::accept(listener);
    fixture::encodings(&mut server);
    fixture::read(&mut server, 10);
    fixture::extended_clipboard(&mut server, 0x1f000001, &[0; 4]);
    assert_eq!(
        fixture::read_extended_clipboard(&mut server),
        (0x07000001, vec![0; 4])
    );
    fixture::extended_clipboard(&mut server, 0x08000001, &[]);
    fixture::update(&mut server, (0, 0, 1, 1), 0, &[0, 255, 0, 0]);
    // An illegal remote clipboard request would precede the incremental update.
    assert_eq!(
        fixture::read(&mut server, 10),
        [3, 1, 0, 0, 0, 0, 0, 20, 0, 4]
    );
    until(&mut session, |event| match event {
        DesktopEvent::Clipboard { .. } => panic!("upload-only relies on remote clipboard"),
        DesktopEvent::Frame(_) => true,
        _ => false,
    });
}

#[test]
fn unicode_upload_works_with_receive_only_peer() {
    const RECEIVE_ONLY_CAPS: u32 = 0x19000001;
    const NOTIFY: u32 = 0x08000001;
    const REQUEST: u32 = 0x02000001;
    const PROVIDE: u32 = 0x10000001;
    let (listener, config) = fixture::endpoint(false, ClipboardPolicy::ToRemote);
    let session = DesktopSession::connect(config).unwrap();
    let mut server = fixture::accept(listener);
    fixture::encodings(&mut server);
    fixture::read(&mut server, 10);
    fixture::extended_clipboard(&mut server, RECEIVE_ONLY_CAPS, &[0; 4]);
    assert_eq!(fixture::read_extended_clipboard(&mut server).0, 0x07000001);
    session
        .send(DesktopInput::Clipboard {
            text: "🙂".into()
        })
        .unwrap();
    assert_eq!(
        fixture::read_extended_clipboard(&mut server),
        (NOTIFY, vec![])
    );
    fixture::extended_clipboard(&mut server, REQUEST, &[]);
    let (flags, compressed) = fixture::read_extended_clipboard(&mut server);
    assert_eq!(flags, PROVIDE);
    let mut decoded = Vec::new();
    flate2::read::ZlibDecoder::new(compressed.as_slice())
        .read_to_end(&mut decoded)
        .unwrap();
    assert_eq!(&decoded[4..], "🙂\0".as_bytes());
}
