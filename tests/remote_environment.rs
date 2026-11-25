use harbor_workspace::{
    model::Session,
    x11::remote_environment::{DesktopPreset, RemoteEnvironment, resolve_environment},
};
#[test]
fn legacy_sessions_do_not_launch_environment() {
    let s: Session = serde_json::from_str(r#"{"host":"legacy"}"#).unwrap();
    assert_eq!(s.remote_environment, RemoteEnvironment::None);
    assert!(
        resolve_environment(&s.remote_environment)
            .unwrap()
            .is_none()
    );
    assert!(
        serde_json::to_value(s)
            .unwrap()
            .get("remote_environment")
            .is_none()
    );
}
#[test]
fn presets_and_explicit_custom_are_structured_and_round_trip() {
    let c = resolve_environment(&RemoteEnvironment::Preset(DesktopPreset::Xfce))
        .unwrap()
        .unwrap();
    assert_eq!(c.program, "dbus-run-session");
    assert_eq!(c.arguments, ["--", "xfce4-session"]);
    let custom = RemoteEnvironment::Custom("printf '%s' \"hello; world\"".into());
    let c = resolve_environment(&custom).unwrap().unwrap();
    assert_eq!(c.program, "sh");
    assert_eq!(c.arguments, ["-lc", "printf '%s' \"hello; world\""]);
    let s = Session {
        remote_environment: custom.clone(),
        ..Default::default()
    };
    let saved = serde_json::to_string(&s).unwrap();
    assert_eq!(
        serde_json::from_str::<Session>(&saved)
            .unwrap()
            .remote_environment,
        custom
    );
    assert!(resolve_environment(&RemoteEnvironment::Custom(" \n".into())).is_err());
    assert!(resolve_environment(&RemoteEnvironment::Custom("bad\0command".into())).is_err());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[allow(dead_code)]
#[path = "support/remote_environment_ssh.rs"]
mod fixture;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[allow(dead_code)]
#[path = "support/xserver.rs"]
mod xserver;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod live {
    use super::*;
    use harbor_workspace::ssh::{ConnectionCommand, ConnectionEvent, ConnectionHandle};
    use std::time::{Duration, Instant};
    use x11rb::{
        connection::Connection,
        protocol::xproto::{ConnectionExt, CreateWindowAux, MapState, WindowClass},
    };
    fn wait(ssh: &mut ConnectionHandle, needle: &str) -> String {
        let end = Instant::now() + Duration::from_secs(8);
        let mut output = String::new();
        loop {
            for event in ssh.poll() {
                match event {
                    ConnectionEvent::Output { bytes, .. } => {
                        output.push_str(&String::from_utf8_lossy(&bytes))
                    }
                    ConnectionEvent::Exited { message, .. } => panic!("shell exited: {message}"),
                    _ => {}
                }
            }
            if output.contains(needle) {
                return output;
            }
            assert!(Instant::now() < end, "missing {needle}: {output}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    #[test]
    #[ignore = "child X client helper; invoked only by isolated SSH fixture"]
    fn desktop_window_helper() {
        assert_eq!(
            std::env::var("TERMCRAFT_TEST_XSERVER_ISOLATED").as_deref(),
            Ok("1")
        );
        let (client, n) = x11rb::connect(None).unwrap();
        let screen = &client.setup().roots[n];
        let window = client.generate_id().unwrap();
        client
            .create_window(
                0,
                window,
                screen.root,
                20,
                20,
                150,
                100,
                0,
                WindowClass::INPUT_OUTPUT,
                screen.root_visual,
                &CreateWindowAux::new().background_pixel(screen.white_pixel),
            )
            .unwrap()
            .check()
            .unwrap();
        client.map_window(window).unwrap().check().unwrap();
        client.flush().unwrap();
        println!("ENVIRONMENT_WINDOW={window}");
        std::thread::sleep(Duration::from_secs(20));
    }
    #[test]
    #[ignore = "requires private run_xserver_isolated.sh"]
    fn selected_environment_launches_only_for_its_session() {
        let owned = xserver::start();
        let fixture = fixture::Fixture::start();
        let dir = tempfile::tempdir().unwrap();
        let count = dir.path().join("launches");
        let mut config = fixture.config(11);
        config.session.remote_environment = RemoteEnvironment::Custom(format!(
            "printf 'launch\\n' >> '{}'; '{}' --exact live::desktop_window_helper --ignored --nocapture",
            count.display(),
            std::env::current_exe().unwrap().display()
        ));
        let mut selected =
            ConnectionHandle::connect_with_x11(config, owned.client_display()).unwrap();
        let output = wait(&mut selected, "ENVIRONMENT_WINDOW=");
        let window: u32 = output
            .split("ENVIRONMENT_WINDOW=")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let observer = xserver::client(&owned, true).unwrap();
        assert_eq!(
            observer
                .get_window_attributes(window)
                .unwrap()
                .reply()
                .unwrap()
                .map_state,
            MapState::VIEWABLE
        );
        assert_eq!(std::fs::read_to_string(&count).unwrap().lines().count(), 1);
        assert!(
            fixture.take_request().is_some(),
            "selected shell requested X11"
        );
        assert!(
            fixture.take_request().is_some(),
            "selected desktop requested X11"
        );
        assert!(
            fixture.take_request().is_none(),
            "no duplicate desktop launch"
        );
        let (mut unrelated, request) = fixture.connect(12, &owned);
        assert!(
            fixture.take_request().is_none(),
            "unrelated session received no environment channel"
        );
        unrelated
            .try_send(ConnectionCommand::Input(
                b"printf UNRELATED_SHELL\n".to_vec(),
            ))
            .unwrap();
        wait(&mut unrelated, "UNRELATED_SHELL");
        assert_eq!(std::fs::read_to_string(&count).unwrap().lines().count(), 1);
        selected.cancel();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !selected.is_finished() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while observer
            .get_window_attributes(window)
            .unwrap()
            .reply()
            .is_ok()
        {
            assert!(
                Instant::now() < deadline,
                "cancelled desktop window remains"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(xserver::window_is_visible_on_display(
            &fixture.x_client(&request, true).unwrap()
        ));
        unrelated
            .try_send(ConnectionCommand::Input(b"printf STILL_OPEN\n".to_vec()))
            .unwrap();
        wait(&mut unrelated, "STILL_OPEN");
    }
    #[test]
    #[ignore = "requires private run_xserver_isolated.sh"]
    fn missing_desktop_reports_error_and_keeps_shell() {
        let owned = xserver::start();
        let fixture = fixture::Fixture::start();
        let mut config = fixture.config(21);
        config.session.remote_environment =
            RemoteEnvironment::Custom("termcraft-missing-desktop-938194".into());
        let mut shell = ConnectionHandle::connect_with_x11(config, owned.client_display()).unwrap();
        wait(&mut shell, "Remote environment failed");
        shell
            .try_send(ConnectionCommand::Input(
                b"printf AFTER_MISSING_DESKTOP\n".to_vec(),
            ))
            .unwrap();
        wait(&mut shell, "AFTER_MISSING_DESKTOP");
    }
    #[test]
    #[ignore = "requires private run_xserver_isolated.sh"]
    fn cancelling_pending_launch_preserves_other_session() {
        let owned = xserver::start();
        let fixture = fixture::Fixture::start();
        let (mut other, request) = fixture.connect(31, &owned);
        let mut config = fixture.config(32);
        config.session.remote_environment = RemoteEnvironment::Custom("X05_HOLD_EXEC_REPLY".into());
        let launching = ConnectionHandle::connect_with_x11(config, owned.client_display()).unwrap();
        let end = Instant::now() + Duration::from_secs(5);
        let mut requests = 0;
        while requests < 2 {
            if fixture.take_request().is_some() {
                requests += 1;
            }
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(10));
        }
        launching.cancel();
        let end = Instant::now() + Duration::from_secs(3);
        while !launching.is_finished() {
            assert!(Instant::now() < end, "launch cancellation blocked");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(xserver::window_is_visible_on_display(
            &fixture.x_client(&request, true).unwrap()
        ));
        other
            .try_send(ConnectionCommand::Input(b"printf AFTER_CANCEL\n".to_vec()))
            .unwrap();
        wait(&mut other, "AFTER_CANCEL");
    }
    #[test]
    #[ignore = "requires private run_xserver_isolated.sh"]
    fn startup_output_does_not_block_desktop_confirmation_or_cancel() {
        use std::sync::atomic::Ordering;
        const FRAMES: usize = 192;
        let owned = xserver::start();
        let fixture = fixture::Fixture::start_with_startup_frames(FRAMES);
        let mut config = fixture.config(51);
        config.session.remote_environment =
            RemoteEnvironment::Custom("printf FLOOD_DESKTOP_STARTED".into());
        let mut shell = ConnectionHandle::connect_with_x11(config, owned.client_display()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(4);
        let mut output = String::new();
        let mut connected = false;
        while !connected
            || !output.contains("FLOOD_DESKTOP_STARTED")
            || !output.contains("STARTUP_FRAME_191:")
        {
            let mut bytes_this_poll = 0;
            for event in shell.poll_with_output_budget(4096) {
                match event {
                    ConnectionEvent::Output { bytes, .. } => {
                        bytes_this_poll += bytes.len();
                        output.push_str(&String::from_utf8_lossy(&bytes));
                    }
                    ConnectionEvent::Connected { .. } => connected = true,
                    ConnectionEvent::Exited { message, .. } => panic!("shell exited: {message}"),
                    _ => {}
                }
            }
            assert!(
                bytes_this_poll <= 4096,
                "terminal output exceeded caller budget"
            );
            assert!(!output.contains("Remote environment failed"), "{output}");
            assert!(
                Instant::now() < deadline,
                "chatty shell blocked desktop confirmation: {} terminal bytes, connected={connected}",
                output.len()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(fixture.startup_frames_sent.load(Ordering::SeqCst), FRAMES);
        for index in 0..FRAMES {
            assert_eq!(
                output
                    .matches(&format!("STARTUP_FRAME_{index:03}:"))
                    .count(),
                1,
                "startup output lost or repeated"
            );
        }
        shell
            .try_send(ConnectionCommand::Input(
                b"printf AFTER_STARTUP_FLOOD\n".to_vec(),
            ))
            .unwrap();
        wait(&mut shell, "AFTER_STARTUP_FLOOD");
        shell.cancel();

        // Stop consuming output so both the bounded UI queue and terminal channel
        // back up, then cancel while the second desktop handshake is pending.
        let mut config = fixture.config(52);
        config.session.remote_environment = RemoteEnvironment::Custom("X05_HOLD_EXEC_REPLY".into());
        let blocked = ConnectionHandle::connect_with_x11(config, owned.client_display()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while fixture.startup_frames_sent.load(Ordering::SeqCst) < 2 * FRAMES {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(Duration::from_millis(100));
        let cancelled = Instant::now();
        blocked.cancel();
        while !blocked.is_finished() {
            assert!(
                cancelled.elapsed() < Duration::from_secs(2),
                "full output queue blocked cancellation"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn missing_forwarding_reports_error_and_keeps_real_shell() {
        let fixture = fixture::Fixture::start();
        let mut config = fixture.config(41);
        config.session.x11 = false;
        config.session.remote_environment = RemoteEnvironment::Preset(DesktopPreset::Xfce);
        let mut shell = ConnectionHandle::connect(config).unwrap();
        wait(&mut shell, "Enable trusted X11 forwarding");
        shell
            .try_send(ConnectionCommand::Input(b"printf REAL_SHELL_OK\n".to_vec()))
            .unwrap();
        wait(&mut shell, "REAL_SHELL_OK");
    }
}

#[test]
fn inbound_launch_cannot_supply_environment_commands() {
    use harbor_workspace::platform::launch;
    assert!(launch::open_url("termcraft://session/saved?remote_environment=evil").is_err());
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("untrusted.termcraft-link");
    std::fs::write(
        &file,
        r#"{"version":1,"session":"saved","remote_environment":{"custom":"evil"}}"#,
    )
    .unwrap();
    assert!(launch::open_file(&file).is_err());
}
#[test]
fn installed_ssh_does_not_silently_ignore_environment_selection() {
    let session = Session {
        host: "fixture.example".into(),
        remote_environment: RemoteEnvironment::Preset(DesktopPreset::Xfce),
        ssh_engine: harbor_workspace::model::SshEngine::OpenSsh,
        ..Default::default()
    };
    assert!(
        harbor_workspace::connection::launch(&session, &Default::default())
            .unwrap_err()
            .contains("Embedded SSH")
    );
}
