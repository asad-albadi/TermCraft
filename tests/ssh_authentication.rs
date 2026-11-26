#![cfg(unix)]
#[path = "support/auth_server.rs"]
mod auth_server;
#[allow(dead_code)]
#[path = "support/ssh_server.rs"]
mod ssh_server;
use auth_server::{AuthServer, Mode};
use harbor_workspace::ssh::{
    ChallengeAnswer, ConnectionCommand, ConnectionEvent, ConnectionHandle,
};
use std::time::{Duration, Instant};
fn authenticate(mode: Mode) {
    let server = AuthServer::start(mode);
    let mut handle = ConnectionHandle::connect(server.config()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(6);
    let mut connected = false;
    let mut prompts_seen = 0;
    let mut output = Vec::new();
    while Instant::now() < deadline {
        for event in handle.poll() {
            match event {
                ConnectionEvent::Challenge {
                    id,
                    prompts,
                    host_key,
                    ..
                } => {
                    assert!(!host_key);
                    assert!(!prompts[0].echo);
                    prompts_seen += 1;
                    let values = if prompts.len() == 1 {
                        vec!["fixture-password"]
                    } else {
                        assert!(prompts[1].echo);
                        vec!["fixture-password", "123456"]
                    };
                    handle
                        .answer(ChallengeAnswer {
                            id,
                            values: values
                                .into_iter()
                                .map(|s| zeroize::Zeroizing::new(s.to_owned()))
                                .collect(),
                            trust: None,
                        })
                        .unwrap();
                }
                ConnectionEvent::Connected { .. } => {
                    connected = true;
                    handle
                        .try_send(ConnectionCommand::Input(b"test".to_vec()))
                        .unwrap();
                }
                ConnectionEvent::Output { bytes, .. } => output.extend(bytes),
                ConnectionEvent::Exited { message, .. } => {
                    panic!("Unexpected auth failure: {message}")
                }
                _ => {}
            }
        }
        if String::from_utf8_lossy(&output).contains("TERMINAL_OK") {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(connected);
    assert_eq!(prompts_seen, 1);
    assert_eq!(server.count(), 1);
    assert_eq!(output, b"TERMINAL_OK");
    // SFTP deliberately never answers VERSION, yet shell I/O remains usable.
    handle.cancel();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !handle.is_finished() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(handle.is_finished());
}
#[test]
fn password_auth_and_stalled_sftp_preserve_shell() {
    authenticate(Mode::Password);
}
#[test]
fn keyboard_interactive_preserves_prompt_echo_flags() {
    authenticate(Mode::Mfa);
}
#[test]
fn public_key_then_mfa_uses_updated_methods() {
    authenticate(Mode::KeyThenMfa);
}

#[test]
fn x11_cookie_forwarding_shares_the_authenticated_connection() {
    const CHILD: &str = "TERMCRAFT_X11_FIXTURE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        // DISPLAY's TCP port is 6000 + display number.
        assert!(port > 6000);
        let path = std::env::temp_dir().join(format!("termcraft-xauth-{}", std::process::id()));
        let number = (port - 6000).to_string();
        let mut record = 65535u16.to_be_bytes().to_vec();
        for field in [
            b"".as_slice(),
            number.as_bytes(),
            b"MIT-MAGIC-COOKIE-1",
            &[42; 16],
        ] {
            record.extend_from_slice(&(field.len() as u16).to_be_bytes());
            record.extend_from_slice(field);
        }
        std::fs::write(&path, record).unwrap();
        let thread = std::thread::spawn(move || {
            listener.set_nonblocking(true).unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        socket
                            .set_read_timeout(Some(Duration::from_secs(3)))
                            .unwrap();
                        let mut setup = [0; 48];
                        socket.read_exact(&mut setup).unwrap();
                        assert_eq!(&setup[32..], &[42; 16]);
                        socket.write_all(b"X11_OK").unwrap();
                        return;
                    }
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(10))
                    }
                    Err(e) => panic!("No authenticated local X11 connection: {e}"),
                }
            }
        });
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "x11_cookie_forwarding_shares_the_authenticated_connection",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("DISPLAY", format!("localhost:{number}.0"))
            .env("XAUTHORITY", &path)
            .status()
            .unwrap();
        let _ = std::fs::remove_file(path);
        assert!(status.success());
        thread.join().unwrap();
        return;
    }
    let server = AuthServer::start(Mode::X11);
    let mut config = server.config();
    config.session.x11 = true;
    let mut handle = ConnectionHandle::connect(config).expect("Embedded X11 must be available");
    verify_forwarded_connection(&server, &mut handle, None);
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "requires an explicitly staged Apple Silicon package and a macOS desktop"]
fn managed_ssh_forwards_to_packaged_xquartz() {
    use harbor_workspace::x11::{XDisplaySource, XServerManager, XServerOptions};
    let package = std::env::var_os("TERMCRAFT_TEST_XQUARTZ_ROOT")
        .expect("set the explicit fixture resource root");
    let lease = XServerManager::for_installation(package.into())
        .start(XServerOptions::default())
        .unwrap();
    let server = AuthServer::start(Mode::NativeX11);
    let mut config = server.config();
    config.session.x11 = true;
    config.session.x11_source = XDisplaySource::Owned;
    let mut handle = ConnectionHandle::connect_with_managed_x11(config).unwrap();
    verify_forwarded_connection(&server, &mut handle, Some(lease.client_display()));
    let socket = std::path::PathBuf::from(lease.display());
    lease.stop_owned().unwrap();
    assert!(!socket.exists());
}

fn verify_forwarded_connection(
    server: &AuthServer,
    handle: &mut ConnectionHandle,
    mut managed_display: Option<harbor_workspace::x11::XClientDisplay>,
) {
    let deadline = Instant::now() + Duration::from_secs(6);
    while Instant::now() < deadline && !server.x11_verified() {
        for event in handle.poll() {
            match event {
                ConnectionEvent::NeedsXDisplay { .. } => {
                    handle.supply_x_display(Ok(managed_display.take().unwrap()));
                }
                ConnectionEvent::Challenge { id, host_key, .. } => {
                    assert!(!host_key);
                    handle
                        .answer(ChallengeAnswer {
                            id,
                            values: vec![zeroize::Zeroizing::new("fixture-password".into())],
                            trust: None,
                        })
                        .unwrap();
                }
                ConnectionEvent::Exited { message, .. } => {
                    panic!("X11 connection failed: {message}")
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(server.x11_verified());
    assert_eq!(server.count(), 1);
    handle.cancel();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !handle.is_finished() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(handle.is_finished());
}
