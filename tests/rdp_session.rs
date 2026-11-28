#![cfg(any(unix, windows))]
#[path = "support/rdp_server.rs"]
mod rdp_server;
use harbor_workspace::desktop::{DesktopEvent, DesktopInput, DesktopSession, TrustDecision};
use std::time::{Duration, Instant};
fn until(
    session: &mut DesktopSession,
    mut accept: impl FnMut(&DesktopSession, DesktopEvent) -> bool,
) {
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        for event in session.poll() {
            if accept(session, event) {
                return;
            }
        }
        assert!(Instant::now() < end, "RDP event timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn authenticate(session: &DesktopSession, id: u64) {
    session
        .send(DesktopInput::Authenticate {
            id,
            user: "fixture-user".into(),
            password: harbor_workspace::credentials::Secret::new("fixture-password"),
            domain: String::new(),
        })
        .unwrap();
}

#[cfg(windows)]
#[test]
#[ignore = "requires the native RDP software endpoint and OpenSSL"]
fn rdp_windows_clipboard_round_trips_unicode_without_host_clipboard() {
    use harbor_workspace::desktop::rdp_options::ClipboardPolicy;

    let server = rdp_server::Server::start();
    let mut config = server.config(8110);
    config.session.rdp_options.clipboard = ClipboardPolicy::Bidirectional;
    let mut session = DesktopSession::connect(config).unwrap();
    until(&mut session, |session, event| match event {
        DesktopEvent::Authentication { id, .. } => {
            authenticate(session, id);
            false
        }
        DesktopEvent::Clipboard { text } => text == "Remote clipboard 🐧",
        DesktopEvent::Disconnected { message } => panic!("{message}"),
        _ => false,
    });
    let text = "Windows clipboard café 日本";
    session
        .send(DesktopInput::Clipboard { text: text.into() })
        .unwrap();
    let expected = format!("clipboard:{text}");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !server.observations().contains(&expected) {
        assert!(
            Instant::now() < deadline,
            "server did not receive the offered text"
        );
        for event in session.poll() {
            if let DesktopEvent::Disconnected { message } = event {
                panic!("{message}");
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    session.close();
}

#[test]
#[cfg_attr(
    windows,
    ignore = "requires the native RDP software endpoint and OpenSSL"
)]
fn rdp_frame_updates_render_inside_workspace() {
    let server = rdp_server::Server::start();
    let mut session = DesktopSession::connect(server.config(8001)).unwrap();
    until(&mut session, |session, event| match event {
        DesktopEvent::Authentication { id, .. } => {
            session.send(DesktopInput::ReleaseAll).unwrap();
            authenticate(session, id);
            false
        }
        DesktopEvent::Frame(frame) => {
            if frame.pixels.get(..4) == Some(&[255, 0, 0, 255]) {
                assert_eq!((frame.width, frame.height), (640, 480));
                true
            } else {
                false
            }
        }
        DesktopEvent::Disconnected { message } => panic!("{message}"),
        _ => false,
    });
    session
        .send(DesktopInput::Pointer {
            x: 45,
            y: 27,
            button: None,
            pressed: false,
        })
        .unwrap();
    session
        .send(DesktopInput::Key {
            code: 30,
            extended: false,
            pressed: true,
        })
        .unwrap();
    session
        .send(DesktopInput::Key {
            code: 30,
            extended: false,
            pressed: false,
        })
        .unwrap();
    let end = Instant::now() + Duration::from_secs(3);
    while !server
        .observations()
        .contains("key:Pressed { code: 30, extended: false }")
    {
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        server
            .observations()
            .contains("mouse:Move { x: 45, y: 27 }")
    );
    session.close();
}
#[test]
#[cfg_attr(
    windows,
    ignore = "requires the native RDP software endpoint and OpenSSL"
)]
fn unsuccessful_nla_reports_a_safe_sign_in_diagnostic_before_desktop_setup() {
    let server = rdp_server::Server::start();
    let mut session = DesktopSession::connect(server.config(8144)).unwrap();
    let mut submitted = false;
    until(&mut session, |session, event| match event {
        DesktopEvent::Authentication { id, .. } => {
            session
                .send(DesktopInput::Authenticate {
                    id,
                    user: "fixture-user".into(),
                    password: harbor_workspace::credentials::Secret::new(
                        "intentionally-incorrect-test-password",
                    ),
                    domain: String::new(),
                })
                .unwrap();
            submitted = true;
            false
        }
        DesktopEvent::Connected { .. } | DesktopEvent::Frame(_) => {
            panic!("Incorrect password opened a desktop")
        }
        DesktopEvent::Disconnected { message } => {
            assert!(submitted);
            assert_eq!(
                message,
                "RDP sign-in rejected (CredSSP, SSPI InvalidToken 0x80090308, NTSTATUS 0xC007030C)"
            );
            true
        }
        _ => false,
    });
}
#[test]
#[cfg_attr(
    windows,
    ignore = "requires the native RDP software endpoint and OpenSSL"
)]
fn client_resize_negotiates_and_returns_a_pane_sized_frame() {
    let server = rdp_server::Server::start();
    let mut session = DesktopSession::connect(server.config(8141)).unwrap();
    until(&mut session, |session, event| match event {
        DesktopEvent::Authentication { id, .. } => {
            authenticate(session, id);
            false
        }
        DesktopEvent::Frame(frame) => {
            (frame.width, frame.height) == (640, 480)
                && frame.pixels.get(..4) == Some(&[255, 0, 0, 255])
        }
        DesktopEvent::Disconnected { message } => panic!("{message}"),
        _ => false,
    });
    session
        .send(DesktopInput::Resize {
            width: 934,
            height: 578,
        })
        .unwrap();
    until(&mut session, |_, event| match event {
        DesktopEvent::Frame(frame) => {
            (frame.width, frame.height) == (934, 578)
                && frame.pixels.len() == 934 * 578 * 4
                && frame.pixels.get(..4) == Some(&[255, 0, 0, 255])
        }
        DesktopEvent::Disconnected { message } => panic!("{message}"),
        _ => false,
    });
    assert!(
        server
            .observations()
            .lines()
            .any(|line| line == "client_resize:934x578")
    );
    session.close();
}
#[test]
#[cfg_attr(
    windows,
    ignore = "requires the native RDP software endpoint and OpenSSL"
)]
fn input_during_client_resize_remains_ordered_across_reactivation() {
    let server = rdp_server::Server::start();
    let mut session = DesktopSession::connect(server.config(8142)).unwrap();
    until(&mut session, |session, event| match event {
        DesktopEvent::Authentication { id, .. } => {
            authenticate(session, id);
            false
        }
        DesktopEvent::Frame(frame) => frame.pixels.get(..4) == Some(&[255, 0, 0, 255]),
        DesktopEvent::Disconnected { message } => panic!("{message}"),
        _ => false,
    });
    session
        .send(DesktopInput::Resize {
            width: 822,
            height: 461,
        })
        .unwrap();
    let mut expected = Vec::new();
    for code in 30..50 {
        for (key, pressed) in [(29, true), (code, true), (code, false), (29, false)] {
            session
                .send(DesktopInput::Key {
                    code: key,
                    extended: false,
                    pressed,
                })
                .unwrap();
            expected.push(format!(
                "key:{} {{ code: {key}, extended: false }}",
                if pressed { "Pressed" } else { "Released" }
            ));
        }
        std::thread::sleep(Duration::from_millis(3));
    }
    until(&mut session, |_, event| match event {
        DesktopEvent::Frame(frame) => {
            (frame.width, frame.height) == (822, 461)
                && frame.pixels.get(..4) == Some(&[255, 0, 0, 255])
        }
        DesktopEvent::Disconnected { message } => panic!("{message}"),
        _ => false,
    });
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let observations = server.observations();
        let keys: Vec<_> = observations
            .lines()
            .filter(|line| line.starts_with("key:"))
            .collect();
        if keys.len() >= expected.len() {
            assert_eq!(keys, expected);
            break;
        }
        assert!(Instant::now() < deadline, "missing keys: {observations}");
        std::thread::sleep(Duration::from_millis(5));
    }
    session.close();
}

#[test]
#[cfg_attr(
    windows,
    ignore = "requires the native RDP software endpoint and OpenSSL"
)]
fn ignored_client_resize_resumes_queued_input_on_original_desktop() {
    let server = rdp_server::Server::ignoring_client_resize();
    let mut session = DesktopSession::connect(server.config(8143)).unwrap();
    until(&mut session, |session, event| match event {
        DesktopEvent::Authentication { id, .. } => {
            authenticate(session, id);
            false
        }
        DesktopEvent::Frame(frame) => frame.pixels.get(..4) == Some(&[255, 0, 0, 255]),
        DesktopEvent::Disconnected { message } => panic!("{message}"),
        _ => false,
    });
    session
        .send(DesktopInput::Resize {
            width: 822,
            height: 461,
        })
        .unwrap();
    session
        .send(DesktopInput::Text { value: "q".into() })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut local_fit = false;
    loop {
        for event in session.poll() {
            match event {
                DesktopEvent::Status { message } => {
                    local_fit |= message.contains("fitting the view locally")
                }
                DesktopEvent::Frame(frame) => assert_eq!((frame.width, frame.height), (640, 480)),
                DesktopEvent::Disconnected { message } => panic!("{message}"),
                _ => {}
            }
        }
        let seen = server.observations();
        if local_fit
            && seen.contains("client_resize:822x461")
            && seen.contains("key:UnicodeReleased(113)")
        {
            break;
        }
        assert!(Instant::now() < deadline, "input did not resume: {seen}");
        std::thread::sleep(Duration::from_millis(5));
    }
    session
        .send(DesktopInput::Resize {
            width: 924,
            height: 540,
        })
        .unwrap();
    session
        .send(DesktopInput::Text { value: "r".into() })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let seen = server.observations();
        assert!(
            !seen.contains("client_resize:924x540"),
            "ignored endpoint received a second resize"
        );
        if seen.contains("key:UnicodeReleased(114)") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "local fitting interrupted later input"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    session.close();
}

#[test]
#[cfg_attr(
    windows,
    ignore = "requires the native RDP software endpoint and OpenSSL"
)]
fn rdp_certificate_change_requires_resolution() {
    let server = rdp_server::Server::start();
    let mut cfg = server.config(8002);
    cfg.certificate_authority = None;
    let mut session = DesktopSession::connect(cfg).unwrap();
    let mut found = None;
    until(&mut session, |_session, event| match event {
        DesktopEvent::Certificate {
            id, fingerprint, ..
        } => {
            found = Some((id, fingerprint));
            true
        }
        DesktopEvent::Authentication { .. } => {
            panic!("Credentials requested before certificate resolution")
        }
        DesktopEvent::Disconnected { message } => panic!("{message}"),
        _ => false,
    });
    let (id, fingerprint) = found.unwrap();
    session
        .send(DesktopInput::Trust {
            id,
            decision: TrustDecision::Once,
        })
        .unwrap();
    until(&mut session, |session, event| match event {
        DesktopEvent::Authentication { id, .. } => {
            authenticate(session, id);
            false
        }
        DesktopEvent::Frame(frame) => frame.pixels.get(..4) == Some(&[255, 0, 0, 255]),
        DesktopEvent::Disconnected { message } => panic!("{message}"),
        _ => false,
    });
    session.close();
    let end = Instant::now() + Duration::from_secs(3);
    while !session.is_finished() {
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(5));
    }
    let address = server.address;
    drop(server);
    let replacement = rdp_server::Server::at(address);
    let mut cfg = replacement.config(8004);
    cfg.certificate_authority = None;
    cfg.remembered_certificate = Some(fingerprint.clone());
    let mut session = DesktopSession::connect(cfg).unwrap();
    until(&mut session, |session, event| match event {
        DesktopEvent::Certificate {
            id,
            fingerprint: changed,
            previous,
            changed: requires_resolution,
            ..
        } => {
            assert!(requires_resolution);
            assert_ne!(changed, fingerprint);
            assert_eq!(previous.as_deref(), Some(fingerprint.as_str()));
            session
                .send(DesktopInput::Trust {
                    id,
                    decision: TrustDecision::Reject,
                })
                .unwrap();
            true
        }
        DesktopEvent::Authentication { .. } | DesktopEvent::Connected { .. } => {
            panic!("Changed certificate reached sign-in without resolution")
        }
        DesktopEvent::Disconnected { message } => panic!("{message}"),
        _ => false,
    });
    until(&mut session, |_session, event| {
        matches!(event, DesktopEvent::Disconnected { .. })
    });
    assert!(!replacement.observations().contains("key:"));
}
#[test]
#[cfg_attr(
    windows,
    ignore = "requires the native RDP software endpoint and OpenSSL"
)]
fn closing_rdp_releases_transport() {
    let server = rdp_server::Server::start();
    let mut session = DesktopSession::connect(server.config(8003)).unwrap();
    until(&mut session, |session, event| match event {
        DesktopEvent::Authentication { id, .. } => {
            authenticate(session, id);
            false
        }
        DesktopEvent::Frame(frame) => frame.pixels.get(..4) == Some(&[255, 0, 0, 255]),
        DesktopEvent::Disconnected { message } => panic!("{message}"),
        _ => false,
    });
    assert!(!server.observations().contains("CLOSED"));
    session.close();
    let end = Instant::now() + Duration::from_secs(3);
    while !session.is_finished() || !server.observations().contains("CLOSED") {
        assert!(
            Instant::now() < end,
            "Closing RDP did not release its active TLS transport and helper"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(session.poll().is_empty());
}

#[test]
#[cfg_attr(
    windows,
    ignore = "requires the native RDP software endpoint and OpenSSL"
)]
fn remote_cursor_and_focus_release_reach_real_connection() {
    let server = rdp_server::Server::with_cursor();
    let mut session = DesktopSession::connect(server.config(8010)).unwrap();
    let mut hidden = false;
    until(&mut session, |session, event| match event {
        DesktopEvent::Authentication { id, .. } => {
            authenticate(session, id);
            false
        }
        DesktopEvent::Cursor { visible: false, .. } => {
            hidden = true;
            false
        }
        DesktopEvent::Frame(frame) => {
            hidden
                && frame
                    .pixels
                    .get((20 * 640 + 20) * 4..(20 * 640 + 20) * 4 + 4)
                    .is_some_and(|p| p[0] == 0 && p[1] >= 250 && p[2] == 0 && p[3] == 255)
        }
        DesktopEvent::Disconnected { message } => panic!("{message}"),
        _ => false,
    });
    session
        .send(DesktopInput::Key {
            code: 30,
            extended: false,
            pressed: true,
        })
        .unwrap();
    session
        .send(DesktopInput::Pointer {
            x: 45,
            y: 27,
            button: Some(1),
            pressed: true,
        })
        .unwrap();
    // The release barrier discards queued input from the old focus epoch.
    // Establish real held state on the peer before testing its release.
    let end = Instant::now() + Duration::from_secs(3);
    while !server.observations().contains("mouse:LeftPressed")
        || !server
            .observations()
            .contains("key:Pressed { code: 30, extended: false }")
    {
        assert!(Instant::now() < end, "Input never reached the server");
        std::thread::sleep(Duration::from_millis(5));
    }
    session.send(DesktopInput::ReleaseAll).unwrap();
    let end = Instant::now() + Duration::from_secs(3);
    while !server.observations().contains("mouse:LeftReleased") {
        assert!(
            Instant::now() < end,
            "Focus release left a mouse button held: {}",
            server.observations()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        server
            .observations()
            .contains("key:Released { code: 30, extended: false }")
    );
    session.close();
}
#[test]
#[cfg_attr(
    windows,
    ignore = "requires the native RDP software endpoint and OpenSSL"
)]
fn server_display_resize_renegotiates_the_framebuffer() {
    let server = rdp_server::Server::with_resize();
    let mut session = DesktopSession::connect(server.config(8011)).unwrap();
    let mut initial = false;
    until(&mut session, |session, event| match event {
        DesktopEvent::Authentication { id, .. } => {
            authenticate(session, id);
            false
        }
        DesktopEvent::Frame(frame) => {
            if frame.width == 640 {
                initial = true;
            }
            if frame.width == 800
                && frame.height == 600
                && frame.pixels.get(..4) == Some(&[255, 0, 0, 255])
            {
                assert!(initial);
                true
            } else {
                false
            }
        }
        DesktopEvent::Disconnected { message } => panic!("{message}"),
        _ => false,
    });
    session.close();
}
#[test]
#[cfg_attr(
    windows,
    ignore = "requires the native RDP software endpoint and OpenSSL"
)]
fn wheel_input_reaches_remote_desktop() {
    let server = rdp_server::Server::start();
    let mut session = DesktopSession::connect(server.config(8012)).unwrap();
    until(&mut session, |session, event| match event {
        DesktopEvent::Authentication { id, .. } => {
            authenticate(session, id);
            false
        }
        DesktopEvent::Frame(frame) => frame.pixels.get(..4) == Some(&[255, 0, 0, 255]),
        DesktopEvent::Disconnected { message } => panic!("{message}"),
        _ => false,
    });
    session
        .send(DesktopInput::Scroll {
            x: 45,
            y: 27,
            delta: 120,
            horizontal: false,
        })
        .unwrap();
    let end = Instant::now() + Duration::from_secs(3);
    while !server
        .observations()
        .contains("mouse:VerticalScroll { value: 120 }")
    {
        assert!(Instant::now() < end, "{}", server.observations());
        std::thread::sleep(Duration::from_millis(5));
    }
    session.close();
}

#[test]
#[cfg_attr(
    windows,
    ignore = "requires the native RDP software endpoint and OpenSSL"
)]
fn cancelling_certificate_prompt_reaps_the_helper() {
    let server = rdp_server::Server::start();
    let mut cfg = server.config(8003);
    cfg.certificate_authority = None;
    let mut session = DesktopSession::connect(cfg).unwrap();
    until(&mut session, |_session, e| {
        matches!(e, DesktopEvent::Certificate { .. })
    });
    session.close();
    let end = Instant::now() + Duration::from_secs(3);
    while !session.is_finished() {
        assert!(Instant::now() < end, "Closing RDP did not reap its helper");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(session.poll().is_empty());
}
#[test]
#[cfg_attr(
    windows,
    ignore = "requires the native RDP software endpoint and OpenSSL"
)]
fn disconnect_retires_sign_in_without_waiting_for_a_password() {
    let mut server = rdp_server::Server::start();
    let mut session = DesktopSession::connect(server.config(8013)).unwrap();
    until(&mut session, |_session, event| {
        matches!(event, DesktopEvent::Authentication { .. })
    });
    server.stop();
    let end = Instant::now() + Duration::from_secs(3);
    let mut retired = false;
    while Instant::now() < end {
        for event in session.poll() {
            if matches!(event, DesktopEvent::Disconnected { .. }) {
                retired = true;
            }
        }
        if retired {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(retired, "A closed TLS peer left its sign-in prompt active");
}
#[path = "support/rdp_wire_client.rs"]
mod rdp_wire_client;
#[test]
#[cfg_attr(
    windows,
    ignore = "requires the native RDP software endpoint and OpenSSL"
)]
fn queued_pointer_input_survives_a_server_shrink() {
    use rdp_wire_client::wire::Input;
    let server = rdp_server::Server::with_shrink();
    let mut client = rdp_wire_client::Client::connect(&server);
    client.frame(800, 600);
    client.send(Input::Pointer {
        x: 700,
        y: 27,
        button: Some(1),
        pressed: true,
    });
    let end = Instant::now() + Duration::from_secs(3);
    while !server.observations().contains("mouse:LeftPressed") {
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(5));
    }
    std::fs::write(server.root.path().join("resize-now"), b"owned resize gate").unwrap();
    client.frame(640, 480);
    client.send(Input::Pointer {
        x: 700,
        y: 27,
        button: None,
        pressed: false,
    });
    client.send(Input::Pointer {
        x: 700,
        y: 27,
        button: Some(1),
        pressed: false,
    });
    client.send(Input::Scroll {
        x: 700,
        y: 27,
        delta: 120,
        horizontal: false,
    });
    client.frame(640, 480);
    let end = Instant::now() + Duration::from_secs(3);
    while !server.observations().contains("mouse:LeftReleased") {
        assert!(Instant::now() < end, "{}", server.observations());
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        server
            .observations()
            .contains("mouse:Move { x: 639, y: 27 }")
    );
}

#[test]
#[cfg_attr(
    windows,
    ignore = "requires the native RDP software endpoint and OpenSSL"
)]
fn supplied_rdp_carrier_preserves_logical_tls_and_auth_endpoint() {
    let server = rdp_server::Server::start();
    let mut config = server.config(8411);
    config.stream = Some(std::net::TcpStream::connect(server.address).unwrap());
    config.session.port = 9; // No direct listener here: transport must use the supplied carrier.
    let mut session = DesktopSession::connect(config).unwrap();
    let mut authenticated = false;
    until(&mut session, |session, event| match event {
        DesktopEvent::Authentication { id, host, port, .. } => {
            assert_eq!(host, "localhost");
            assert_eq!(port, 9);
            authenticated = true;
            authenticate(session, id);
            false
        }
        DesktopEvent::Frame(frame) => frame.pixels.get(..4) == Some(&[255, 0, 0, 255]),
        DesktopEvent::Disconnected { message } => panic!("{message}"),
        _ => false,
    });
    assert!(authenticated);
    session.close();
    let end = Instant::now() + Duration::from_secs(3);
    while !session.is_finished() {
        assert!(Instant::now() < end, "Supplied carrier retained its helper");
        std::thread::sleep(Duration::from_millis(5));
    }
}
