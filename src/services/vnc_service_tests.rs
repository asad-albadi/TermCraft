#[cfg(target_os = "linux")]
#[path = "vnc_fixture_keyboard.rs"]
mod fixture_keyboard;
use crate::{
    credentials::Secret,
    services::{ServiceSupervisor, VncServiceConfig, VncSource},
};
#[test]
fn vnc_requires_explicit_source_password_and_transport_consent() {
    let s = ServiceSupervisor::default();
    let mut c = VncServiceConfig::local(
        VncSource::X11 {
            display: String::new(),
        },
        Secret::new("12345678"),
    );
    assert!(!c.allow_control);
    assert!(!c.allow_clipboard);
    assert!(s.start_vnc(c).is_err());
    c = VncServiceConfig::local(
        VncSource::X11 {
            display: ":99999".into(),
        },
        Secret::new("123456789"),
    );
    c.allow_unencrypted = true;
    assert!(s.start_vnc(c).is_err());
    let mut network = VncServiceConfig::local(
        VncSource::X11 {
            display: ":1".into(),
        },
        Secret::new("fixture"),
    );
    network.allow_unencrypted = true;
    network.bind = "0.0.0.0:5900".parse().unwrap();
    assert!(s.start_vnc(network).unwrap_err().contains("network access"));
}

#[test]
fn vnc_unsupported_sources_and_clipboard_fail_closed() {
    let s = ServiceSupervisor::default();
    for source in [
        #[cfg(not(target_os = "macos"))]
        VncSource::MacOsDisplay { display_id: 0 },
        #[cfg(not(windows))]
        VncSource::WindowsPrimaryDesktop,
    ] {
        let mut c = VncServiceConfig::local(source, Secret::new("fixture"));
        c.allow_unencrypted = true;
        assert!(s.start_vnc(c).is_err());
    }
    // Configuration rejection happens before touching the host portal or clipboard.
    let mut wayland = VncServiceConfig::local(VncSource::WaylandPortal, Secret::new("fixture"));
    wayland.allow_unencrypted = true;
    wayland.allow_clipboard = true;
    assert!(s.start_vnc(wayland).unwrap_err().contains("clipboard"));
    let mut c = VncServiceConfig::local(
        VncSource::X11 {
            display: ":65535".into(),
        },
        Secret::new("fixture"),
    );
    c.allow_unencrypted = true;
    c.allow_clipboard = true;
    assert!(s.start_vnc(c).is_err());
}

#[cfg(target_os = "linux")]
mod isolated {
    use super::fixture_keyboard::{next_character, send_key};
    use super::*;
    use crate::{
        desktop::{
            DesktopConfig, DesktopEvent, DesktopInput, DesktopSession,
            rdp_options::ClipboardPolicy,
            vnc::{Security, VncOptions},
        },
        model::Session,
        x11::{XServerManager, XServerOptions},
    };
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        thread,
        time::{Duration, Instant},
    };
    use x11rb::{connection::Connection, protocol::xproto::*};
    fn read(s: &mut TcpStream, n: usize) -> Vec<u8> {
        let mut b = vec![0; n];
        s.read_exact(&mut b).unwrap();
        b
    }
    fn wire(address: std::net::SocketAddr, password: &str) -> Result<TcpStream, ()> {
        let mut s = TcpStream::connect(address).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        s.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
        assert_eq!(read(&mut s, 12), b"RFB 003.008\n");
        s.write_all(b"RFB 003.008\n").unwrap();
        assert_eq!(read(&mut s, 2), [1, 2]);
        s.write_all(&[2]).unwrap();
        let challenge: [u8; 16] = read(&mut s, 16).try_into().unwrap();
        let response = super::super::vnc::auth_response(&Secret::new(password), &challenge);
        s.write_all(&*response).unwrap();
        let result = read(&mut s, 4);
        if result != [0, 0, 0, 0] {
            return Err(());
        }
        s.write_all(&[1]).unwrap();
        let init = read(&mut s, 24);
        let n = u32::from_be_bytes(init[20..24].try_into().unwrap()) as usize;
        read(&mut s, n);
        Ok(s)
    }
    fn wait_state(
        s: &ServiceSupervisor,
        id: crate::services::ServiceId,
        expected: crate::services::ServiceState,
    ) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while s.status(id).unwrap().state != expected {
            assert!(Instant::now() < deadline, "state {:?}", s.status(id));
            thread::sleep(Duration::from_millis(20));
        }
    }
    fn frame(s: &mut TcpStream) -> Vec<u8> {
        s.write_all(&[3, 0, 0, 24, 0, 24, 0, 4, 0, 4]).unwrap();
        assert_eq!(read(s, 4), [0, 0, 0, 1]);
        let h = read(s, 12);
        assert_eq!(h[8..], [0, 0, 0, 0]);
        let w = u16::from_be_bytes(h[4..6].try_into().unwrap()) as usize;
        let height = u16::from_be_bytes(h[6..8].try_into().unwrap()) as usize;
        read(s, w * height * 4)
    }
    #[test]
    #[ignore = "requires private bwrap/Cage display; never run on a host display"]
    fn owned_x11_vnc_dynamic_auth_control_stop_and_app_client() {
        assert_eq!(
            std::env::var("TERMCRAFT_TEST_XSERVER_ISOLATED").as_deref(),
            Ok("1")
        );
        assert_eq!(std::env::var("DISPLAY").as_deref(), Ok(":0"));
        let root =
            std::path::PathBuf::from(std::env::var_os("TERMCRAFT_TEST_XSERVER_PACKAGE").unwrap());
        let manager = XServerManager::for_installation(root);
        let mut lease = manager.start(XServerOptions::default()).unwrap();
        let display = lease.display().to_string();
        let authority = lease.authority_path().to_path_buf();
        // Authenticate to only the owned fixture display, without mutating process environment.
        let record = std::fs::read(&authority).unwrap();
        let cookie = record[record.len() - 16..].to_vec();
        let socket = std::os::unix::net::UnixStream::connect(format!(
            "/tmp/.X11-unix/X{}",
            display.trim_start_matches(':')
        ))
        .unwrap();
        let (stream, _) = x11rb::rust_connection::DefaultStream::from_unix_stream(socket).unwrap();
        let x = x11rb::rust_connection::RustConnection::connect_to_stream_with_auth_info(
            stream,
            0,
            b"MIT-MAGIC-COOKIE-1".to_vec(),
            cookie,
        )
        .unwrap();
        let screen = &x.setup().roots[0];
        let window = x.generate_id().unwrap();
        x.create_window(
            x11rb::COPY_DEPTH_FROM_PARENT,
            window,
            screen.root,
            24,
            24,
            120,
            100,
            0,
            WindowClass::INPUT_OUTPUT,
            screen.root_visual,
            &CreateWindowAux::new()
                .background_pixel(0xff0000)
                .event_mask(
                    EventMask::KEY_PRESS
                        | EventMask::KEY_RELEASE
                        | EventMask::BUTTON_PRESS
                        | EventMask::BUTTON_RELEASE,
                ),
        )
        .unwrap()
        .check()
        .unwrap();
        x.map_window(window).unwrap().check().unwrap();
        x.set_input_focus(InputFocus::PARENT, window, 0u32)
            .unwrap()
            .check()
            .unwrap();
        x.flush().unwrap();
        let supervisor = ServiceSupervisor::default();
        let make = |control| {
            let mut c = VncServiceConfig::local(
                VncSource::X11 {
                    display: display.clone(),
                },
                Secret::new("fixture"),
            );
            c.x11_authority = Some(authority.clone());
            c.allow_unencrypted = true;
            c.allow_control = control;
            c
        };
        let id = supervisor.start_vnc(make(false)).unwrap();
        let address = supervisor.status(id).unwrap().address.unwrap();
        assert!(wire(address, "wrong").is_err());
        let occupied = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut occupied_config = make(false);
        occupied_config.bind = occupied.local_addr().unwrap();
        assert!(supervisor.start_vnc(occupied_config).is_err());
        assert!(TcpStream::connect(occupied.local_addr().unwrap()).is_ok());
        for message in [&[2, 0, 1, 1][..], &[6, 0, 0, 0, 0, 1, 0, 1][..]] {
            let mut malformed = wire(address, "fixture").unwrap();
            malformed.write_all(message).unwrap();
            match malformed.read(&mut [0]) {
                Ok(n) => assert_eq!(n, 0),
                Err(e) => assert_eq!(e.kind(), std::io::ErrorKind::ConnectionReset),
            }
        }

        let mut viewer = wire(address, "fixture").unwrap();
        thread::sleep(Duration::from_millis(160));
        let red = frame(&mut viewer);
        assert_eq!(red[..4], [0, 0, 255, 0]);
        x.change_window_attributes(
            window,
            &ChangeWindowAttributesAux::new().background_pixel(0x00ff00),
        )
        .unwrap()
        .check()
        .unwrap();
        x.clear_area(false, window, 0, 0, 0, 0)
            .unwrap()
            .check()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let green = loop {
            let pixels = frame(&mut viewer);
            if pixels[..4] == [0, 255, 0, 0] {
                break pixels;
            }
            assert!(
                Instant::now() < deadline,
                "live source did not capture the changed owned window"
            );
            thread::sleep(Duration::from_millis(30));
        };
        assert_eq!(green[..4], [0, 255, 0, 0]);
        assert_ne!(red, green);
        // Server-side read-only enforcement even for a client that sends input anyway.
        while x.poll_for_event().unwrap().is_some() {}
        viewer
            .write_all(&[
                4, 1, 0, 0, 0, 0, 0, b'a', 4, 0, 0, 0, 0, 0, 0, b'a', 5, 1, 0, 30, 0, 30, 5, 0, 0,
                30, 0, 30,
            ])
            .unwrap();
        thread::sleep(Duration::from_millis(150));
        assert!(x.poll_for_event().unwrap().is_none());
        let session = Session {
            protocol: "VNC".into(),
            host: "127.0.0.1".into(),
            port: address.port(),
            vnc_options: VncOptions {
                embedded: true,
                allow_unencrypted: true,
                security: Security::Password,
                clipboard: ClipboardPolicy::Disabled,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut client =
            DesktopSession::connect(DesktopConfig::new(909, session, Default::default())).unwrap();
        let deadline = Instant::now() + Duration::from_secs(6);
        let mut got_frame = false;
        while !got_frame {
            for e in client.poll() {
                match e {
                    DesktopEvent::Authentication { id, .. } => {
                        client
                            .send(DesktopInput::Authenticate {
                                id,
                                user: "vnc".into(),
                                password: Secret::new("fixture"),
                                domain: String::new(),
                            })
                            .unwrap();
                    }
                    DesktopEvent::Frame(f) => {
                        let index = (24 * usize::from(f.width) + 24) * 4;
                        assert_eq!(f.pixels[index..index + 4], [0, 255, 0, 255]);
                        got_frame = true
                    }
                    DesktopEvent::Disconnected { message } => panic!("{message}"),
                    _ => {}
                }
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        client.close();
        // Independent maintained vncdotool client: captures the owned live framebuffer in memory.
        {
            let python=std::env::var_os("TERMCRAFT_TEST_VNCDOTOOL_PYTHON").expect("Set the private vncdotool Python path for independent interoperability qualification");
            let code = "from vncdotool import api; import sys; c=api.connect(sys.argv[1],password='fixture'); c.refreshScreen(); assert c.screen.getpixel((24,24))[:3] == (0,255,0); c.disconnect(); api.shutdown()";
            let output = std::process::Command::new(python)
                .args(["-c", code, &format!("127.0.0.1::{}", address.port())])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            eprintln!(
                "Independent vncdotool 1.3.0 authenticated and observed the live green owned-window pixels"
            );
        }
        let unrelated = TcpListener::bind("127.0.0.1:0").unwrap();
        let unrelated_addr = unrelated.local_addr().unwrap();
        let sibling = supervisor.start_vnc(make(false)).unwrap();
        let sibling_address = supervisor.status(sibling).unwrap().address.unwrap();
        let mut sibling_viewer = wire(sibling_address, "fixture").unwrap();
        let _unfinished_auth = TcpStream::connect(address).unwrap();
        let mut slow = wire(address, "fixture").unwrap();
        slow.write_all(&[3, 0, 0, 0, 0, 0, 255, 255, 255, 255])
            .unwrap();
        let started = Instant::now();
        supervisor.stop(id).unwrap();
        wait_state(&supervisor, id, crate::services::ServiceState::Stopped);
        assert!(started.elapsed() < Duration::from_secs(3));
        assert!(TcpStream::connect(address).is_err());
        assert!(TcpStream::connect(unrelated_addr).is_ok());
        assert!(lease.is_running().unwrap());
        assert_eq!(frame(&mut sibling_viewer)[..4], [0, 255, 0, 0]);
        supervisor.stop(sibling).unwrap();
        wait_state(&supervisor, sibling, crate::services::ServiceState::Stopped);
        let mut timed = make(false);
        timed.lifetime = Some(Duration::from_secs(1));
        let timed = supervisor.start_vnc(timed).unwrap();
        wait_state(&supervisor, timed, crate::services::ServiceState::Stopped);
        let original_keys = x.query_keymap().unwrap().reply().unwrap().keys;
        let controlled = supervisor.start_vnc(make(true)).unwrap();
        let controlled_address = supervisor.status(controlled).unwrap().address.unwrap();
        let mut controller = wire(controlled_address, "fixture").unwrap();
        while x.poll_for_event().unwrap().is_some() {}
        controller
            .write_all(&[
                4, 1, 0, 0, 0, 0, 0, b'a', 4, 0, 0, 0, 0, 0, 0, b'a', 5, 1, 0, 30, 0, 30, 5, 0, 0,
                30, 0, 30,
            ])
            .unwrap();
        let mut key = false;
        let mut button = false;
        let deadline = Instant::now() + Duration::from_secs(5);
        while !(key && button) {
            while let Some(event) = x.poll_for_event().unwrap() {
                match event {
                    x11rb::protocol::Event::KeyPress(_) => key = true,
                    x11rb::protocol::Event::ButtonPress(_) => button = true,
                    _ => {}
                }
            }
            assert!(
                Instant::now() < deadline,
                "owned fixture did not receive optional input"
            );
            thread::sleep(Duration::from_millis(10));
        }
        send_key(&mut controller, u32::from(b'A'), true);
        send_key(&mut controller, u32::from(b'A'), false);
        assert_eq!(
            next_character(&x, &display, &authority).0,
            u32::from(b'A'),
            "RFB uppercase A must not require a client Shift event"
        );
        send_key(&mut controller, u32::from(b'#'), true);
        send_key(&mut controller, u32::from(b'#'), false);
        assert_eq!(
            next_character(&x, &display, &authority).0,
            u32::from(b'#'),
            "RFB shifted punctuation must retain its character"
        );
        // Explicit client Shift is restored after literal lowercase translation.
        send_key(&mut controller, 0xffe1, true);
        send_key(&mut controller, u32::from(b'a'), true);
        send_key(&mut controller, u32::from(b'a'), false);
        assert_eq!(next_character(&x, &display, &authority).0, u32::from(b'a'));
        send_key(&mut controller, u32::from(b'A'), true);
        send_key(&mut controller, u32::from(b'A'), false);
        let (symbol, shifted) = next_character(&x, &display, &authority);
        assert_eq!(symbol, u32::from(b'A'));
        assert_ne!(
            u16::from(shifted.state) & 1,
            0,
            "explicit Shift was not restored"
        );
        send_key(&mut controller, 0xffe1, false);
        for (modifier, mask) in [(0xffe3, 4), (0xffe9, 8)] {
            send_key(&mut controller, modifier, true);
            send_key(&mut controller, u32::from(b'c'), true);
            send_key(&mut controller, u32::from(b'c'), false);
            let (symbol, event) = next_character(&x, &display, &authority);
            assert_eq!(symbol, u32::from(b'c'));
            assert_ne!(
                u16::from(event.state) & mask,
                0,
                "shortcut modifier was lost"
            );
            send_key(&mut controller, modifier, false);
        }
        // Lock state is respected without silently toggling the user's CapsLock.
        send_key(&mut controller, 0xffe5, true);
        send_key(&mut controller, 0xffe5, false);
        for symbol in [u32::from(b'a'), u32::from(b'A')] {
            send_key(&mut controller, symbol, true);
            send_key(&mut controller, symbol, false);
            assert_eq!(next_character(&x, &display, &authority).0, symbol);
        }
        send_key(&mut controller, 0xffe5, true);
        send_key(&mut controller, 0xffe5, false);
        // Change only the explicitly owned fixture server to a different real layout.
        let layout = std::process::Command::new("/usr/bin/setxkbmap")
            .args(["-display", &display, "-layout", "de"])
            .env("XAUTHORITY", &authority)
            .output()
            .unwrap();
        assert!(
            layout.status.success(),
            "{}",
            String::from_utf8_lossy(&layout.stderr)
        );
        for symbol in [u32::from(b'#'), u32::from(b'A'), 0x20ac, 0x010020ac] {
            send_key(&mut controller, symbol, true);
            send_key(&mut controller, symbol, false);
            assert_eq!(
                next_character(&x, &display, &authority).0,
                if symbol == 0x010020ac { 0x20ac } else { symbol },
                "cross-layout / AltGr translation failed"
            );
        }
        // Explicit AltGr must also remain held through a level-three character.
        send_key(&mut controller, 0xfe03, true);
        send_key(&mut controller, 0x20ac, true);
        send_key(&mut controller, 0x20ac, false);
        let (symbol, event) = next_character(&x, &display, &authority);
        assert_eq!(symbol, 0x20ac);
        assert_ne!(u16::from(event.state) & 128, 0);
        send_key(&mut controller, 0xfe03, false);
        let layout = std::process::Command::new("/usr/bin/setxkbmap")
            .args(["-display", &display, "-layout", "gr"])
            .env("XAUTHORITY", &authority)
            .output()
            .unwrap();
        assert!(
            layout.status.success(),
            "{}",
            String::from_utf8_lossy(&layout.stderr)
        );
        for (press, release) in [(0x07e1, 0x010003b1), (0x010003b1, 0x07e1)] {
            send_key(&mut controller, press, true);
            send_key(&mut controller, release, false);
            assert_eq!(
                next_character(&x, &display, &authority).0,
                0x07e1,
                "Greek legacy/Unicode aliases must reach the same target character"
            );
        }
        // A valid Unicode scalar absent from this map must not stop the managed source.
        send_key(&mut controller, 0x0101f680, true);
        send_key(&mut controller, 0x0101f680, false);
        send_key(&mut controller, 0x010003b1, true);
        send_key(&mut controller, 0x010003b1, false);
        assert_eq!(next_character(&x, &display, &authority).0, 0x07e1);
        assert_eq!(frame(&mut controller)[..4], [0, 255, 0, 0]);
        assert_eq!(
            supervisor.status(controlled).unwrap().state,
            crate::services::ServiceState::Running
        );
        let layout = std::process::Command::new("/usr/bin/setxkbmap")
            .args(["-display", &display, "-layout", "de"])
            .env("XAUTHORITY", &authority)
            .output()
            .unwrap();
        assert!(
            layout.status.success(),
            "{}",
            String::from_utf8_lossy(&layout.stderr)
        );

        send_key(&mut controller, 0xffe1, true);
        send_key(&mut controller, u32::from(b'b'), true);
        let (symbol, event) = next_character(&x, &display, &authority);
        assert_eq!(symbol, u32::from(b'b'));
        let held_code = event.detail;
        drop(controller);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(x11rb::protocol::Event::KeyRelease(event)) = x.poll_for_event().unwrap() {
                if event.detail == held_code {
                    break;
                }
            }
            assert!(
                Instant::now() < deadline,
                "disconnect did not release owned remote key"
            );
            thread::sleep(Duration::from_millis(10));
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while x.query_keymap().unwrap().reply().unwrap().keys != original_keys {
            assert!(
                Instant::now() < deadline,
                "disconnect left owned modifier or character pressed"
            );
            thread::sleep(Duration::from_millis(10));
        }
        supervisor.stop(controlled).unwrap();
        wait_state(
            &supervisor,
            controlled,
            crate::services::ServiceState::Stopped,
        );
        assert!(lease.is_running().unwrap());
    }
    #[test]
    #[ignore = "requires private bwrap/Cage display; never host clipboard"]
    fn owned_x11_vnc_clipboard_selection_roundtrip() {
        assert_eq!(
            std::env::var("TERMCRAFT_TEST_XSERVER_ISOLATED").as_deref(),
            Ok("1")
        );
        assert_eq!(std::env::var("DISPLAY").as_deref(), Ok(":0"));
        let root =
            std::path::PathBuf::from(std::env::var_os("TERMCRAFT_TEST_XSERVER_PACKAGE").unwrap());
        let manager = XServerManager::for_installation(root);
        let lease = manager.start(XServerOptions::default()).unwrap();
        let display = lease.display().to_string();
        let authority = lease.authority_path().to_path_buf();
        // Authenticate to only the owned fixture display, without mutating process environment.
        let record = std::fs::read(&authority).unwrap();
        let cookie = record[record.len() - 16..].to_vec();
        let socket = std::os::unix::net::UnixStream::connect(format!(
            "/tmp/.X11-unix/X{}",
            display.trim_start_matches(':')
        ))
        .unwrap();
        let (stream, _) = x11rb::rust_connection::DefaultStream::from_unix_stream(socket).unwrap();
        let x = x11rb::rust_connection::RustConnection::connect_to_stream_with_auth_info(
            stream,
            0,
            b"MIT-MAGIC-COOKIE-1".to_vec(),
            cookie,
        )
        .unwrap();
        let screen = &x.setup().roots[0];
        let window = x.generate_id().unwrap();
        x.create_window(
            x11rb::COPY_DEPTH_FROM_PARENT,
            window,
            screen.root,
            24,
            24,
            120,
            100,
            0,
            WindowClass::INPUT_OUTPUT,
            screen.root_visual,
            &CreateWindowAux::new()
                .background_pixel(0xff0000)
                .event_mask(
                    EventMask::KEY_PRESS
                        | EventMask::KEY_RELEASE
                        | EventMask::BUTTON_PRESS
                        | EventMask::BUTTON_RELEASE,
                ),
        )
        .unwrap()
        .check()
        .unwrap();
        x.map_window(window).unwrap().check().unwrap();
        x.set_input_focus(InputFocus::PARENT, window, 0u32)
            .unwrap()
            .check()
            .unwrap();
        x.flush().unwrap();

        use x11rb::{protocol::Event, wrapper::ConnectionExt as _};
        let atom = |name: &[u8]| x.intern_atom(false, name).unwrap().reply().unwrap().atom;
        let selection = atom(b"CLIPBOARD");
        let utf8 = atom(b"UTF8_STRING");
        let property = atom(b"_OWNED_TEST_CLIPBOARD");
        x.set_selection_owner(window, selection, 0u32)
            .unwrap()
            .check()
            .unwrap();
        let supervisor = ServiceSupervisor::default();
        let mut config =
            VncServiceConfig::local(VncSource::X11 { display }, Secret::new("fixture"));
        config.x11_authority = Some(authority);
        config.allow_unencrypted = true;
        config.allow_clipboard = true;
        config.allow_control = true;
        let id = supervisor.start_vnc(config).unwrap();
        let address = supervisor.status(id).unwrap().address.unwrap();
        let mut viewer = wire(address, "fixture").unwrap();
        viewer
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        // The owned window is a real CLIPBOARD selection owner, responding to ConvertSelection.
        loop {
            while let Some(event) = x.poll_for_event().unwrap() {
                if let Event::SelectionRequest(e) = event {
                    x.change_property8(
                        PropMode::REPLACE,
                        e.requestor,
                        e.property,
                        utf8,
                        b"owned local text",
                    )
                    .unwrap()
                    .check()
                    .unwrap();
                    x.send_event(
                        false,
                        e.requestor,
                        EventMask::NO_EVENT,
                        SelectionNotifyEvent {
                            response_type: SELECTION_NOTIFY_EVENT,
                            sequence: 0,
                            time: e.time,
                            requestor: e.requestor,
                            selection: e.selection,
                            target: e.target,
                            property: e.property,
                        },
                    )
                    .unwrap()
                    .check()
                    .unwrap();
                    x.flush().unwrap();
                }
            }
            let mut ty = [0];
            match viewer.read(&mut ty) {
                Ok(1) => {
                    assert_eq!(ty, [3]);
                    break;
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                other => panic!("clipboard message: {other:?}"),
            }
            assert!(
                Instant::now() < deadline,
                "selected clipboard never reached remote viewer"
            );
        }
        viewer
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let h = read(&mut viewer, 7);
        let n = u32::from_be_bytes(h[3..].try_into().unwrap()) as usize;
        assert_eq!(read(&mut viewer, n), b"owned local text");
        let remote = b"remote Latin-1: \xe9";
        let mut packet = vec![6, 0, 0, 0];
        packet.extend((remote.len() as u32).to_be_bytes());
        packet.extend(remote);
        viewer.write_all(&packet).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while x
            .get_selection_owner(selection)
            .unwrap()
            .reply()
            .unwrap()
            .owner
            == window
        {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(20));
        }
        x.convert_selection(window, selection, utf8, property, 0u32)
            .unwrap()
            .check()
            .unwrap();
        x.flush().unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(Event::SelectionNotify(e)) = x.poll_for_event().unwrap() {
                if e.property == property {
                    break;
                }
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        let value = x
            .get_property(true, window, property, utf8, 0, 4096)
            .unwrap()
            .reply()
            .unwrap();
        assert_eq!(
            std::str::from_utf8(&value.value).unwrap(),
            "remote Latin-1: é"
        );
        viewer
            .set_read_timeout(Some(Duration::from_millis(250)))
            .unwrap();
        assert!(
            viewer.read(&mut [0]).is_err(),
            "remote clipboard echoed back"
        );
        drop(viewer);
        thread::sleep(Duration::from_millis(200));
        x.set_selection_owner(window, selection, 0u32)
            .unwrap()
            .check()
            .unwrap();
        let session = Session {
            protocol: "VNC".into(),
            host: "127.0.0.1".into(),
            port: address.port(),
            vnc_options: VncOptions {
                embedded: true,
                allow_unencrypted: true,
                security: Security::Password,
                clipboard: ClipboardPolicy::Bidirectional,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut client =
            DesktopSession::connect(DesktopConfig::new(919, session, Default::default())).unwrap();
        let deadline = Instant::now() + Duration::from_secs(6);
        let mut received = false;
        while !received {
            while let Some(event) = x.poll_for_event().unwrap() {
                if let Event::SelectionRequest(e) = event {
                    x.change_property8(
                        PropMode::REPLACE,
                        e.requestor,
                        e.property,
                        utf8,
                        "owned α UTF-8".as_bytes(),
                    )
                    .unwrap()
                    .check()
                    .unwrap();
                    x.send_event(
                        false,
                        e.requestor,
                        EventMask::NO_EVENT,
                        SelectionNotifyEvent {
                            response_type: SELECTION_NOTIFY_EVENT,
                            sequence: 0,
                            time: e.time,
                            requestor: e.requestor,
                            selection: e.selection,
                            target: e.target,
                            property: e.property,
                        },
                    )
                    .unwrap()
                    .check()
                    .unwrap();
                    x.flush().unwrap();
                }
            }
            for event in client.poll() {
                match event {
                    DesktopEvent::Authentication { id, .. } => client
                        .send(DesktopInput::Authenticate {
                            id,
                            user: "vnc".into(),
                            password: Secret::new("fixture"),
                            domain: String::new(),
                        })
                        .unwrap(),
                    DesktopEvent::Clipboard { text } => {
                        assert_eq!(text, "owned α UTF-8");
                        received = true;
                    }
                    DesktopEvent::Disconnected { message } => panic!("{message}"),
                    _ => {}
                }
            }
            assert!(
                Instant::now() < deadline,
                "Unicode selected clipboard did not reach actual embedded client"
            );
            thread::sleep(Duration::from_millis(10));
        }
        client
            .send(DesktopInput::Clipboard {
                text: "remote α UTF-8\nline".into(),
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while x
            .get_selection_owner(selection)
            .unwrap()
            .reply()
            .unwrap()
            .owner
            == window
        {
            for event in client.poll() {
                if let DesktopEvent::Disconnected { message } = event {
                    panic!("{message}");
                }
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        x.convert_selection(window, selection, utf8, property, 0u32)
            .unwrap()
            .check()
            .unwrap();
        x.flush().unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(Event::SelectionNotify(e)) = x.poll_for_event().unwrap() {
                if e.property == property {
                    break;
                }
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        let value = x
            .get_property(true, window, property, utf8, 0, 4096)
            .unwrap()
            .reply()
            .unwrap();
        assert_eq!(
            std::str::from_utf8(&value.value).unwrap(),
            "remote α UTF-8\nline"
        );
        client.close();
        supervisor.stop(id).unwrap();
        wait_state(&supervisor, id, crate::services::ServiceState::Stopped);
        assert_eq!(
            x.get_selection_owner(selection)
                .unwrap()
                .reply()
                .unwrap()
                .owner,
            0,
            "Stop must release only owned selection"
        );
        // A read-only server may export selected text but must ignore clipboard writes.
        x.set_selection_owner(window, selection, 0u32)
            .unwrap()
            .check()
            .unwrap();
        let mut config = VncServiceConfig::local(
            VncSource::X11 {
                display: lease.display().to_string(),
            },
            Secret::new("fixture"),
        );
        config.x11_authority = Some(lease.authority_path().to_path_buf());
        config.allow_unencrypted = true;
        config.allow_clipboard = true;
        let readonly = supervisor.start_vnc(config).unwrap();
        let mut viewer = wire(
            supervisor.status(readonly).unwrap().address.unwrap(),
            "fixture",
        )
        .unwrap();
        viewer.write_all(&packet).unwrap();
        thread::sleep(Duration::from_millis(300));
        assert_eq!(
            x.get_selection_owner(selection)
                .unwrap()
                .reply()
                .unwrap()
                .owner,
            window
        );
        viewer
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            while let Some(event) = x.poll_for_event().unwrap() {
                if let Event::SelectionRequest(e) = event {
                    let accepted = if e.target == u32::from(AtomEnum::STRING) {
                        x.change_property8(
                            PropMode::REPLACE,
                            e.requestor,
                            e.property,
                            AtomEnum::STRING,
                            b"fallback \xe9",
                        )
                        .unwrap()
                        .check()
                        .unwrap();
                        e.property
                    } else {
                        0
                    };
                    x.send_event(
                        false,
                        e.requestor,
                        EventMask::NO_EVENT,
                        SelectionNotifyEvent {
                            response_type: SELECTION_NOTIFY_EVENT,
                            sequence: 0,
                            time: e.time,
                            requestor: e.requestor,
                            selection: e.selection,
                            target: e.target,
                            property: accepted,
                        },
                    )
                    .unwrap()
                    .check()
                    .unwrap();
                    x.flush().unwrap();
                }
            }
            match viewer.read(&mut [0]) {
                Ok(1) => break,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                v => panic!("readonly selection export {v:?}"),
            }
            assert!(
                Instant::now() < deadline,
                "readonly source failed STRING fallback"
            );
        }
        viewer
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let h = read(&mut viewer, 7);
        let n = u32::from_be_bytes(h[3..].try_into().unwrap()) as usize;
        assert_eq!(read(&mut viewer, n), b"fallback \xe9");
        supervisor.stop(readonly).unwrap();
        wait_state(
            &supervisor,
            readonly,
            crate::services::ServiceState::Stopped,
        );
        assert_eq!(
            x.get_selection_owner(selection)
                .unwrap()
                .reply()
                .unwrap()
                .owner,
            window,
            "Stop preserved another app's selection"
        );
        assert!(x.get_geometry(window).unwrap().reply().is_ok());
        drop(lease);
    }
}
