use harbor_workspace::{desktop::rdp_options::*, model::Session};
#[test]
fn rdp_username_reaches_windows_profile() {
    let session = Session {
        protocol: "RDP".into(),
        host: "server.example".into(),
        port: 3390,
        user: "alice".into(),
        ..Default::default()
    };
    let bytes = windows_profile(&session).unwrap();
    assert_eq!(&bytes[..2], &[0xff, 0xfe]);
    let text = String::from_utf16(
        &bytes[2..]
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let get = |key: &str| text.lines().find_map(|line| line.strip_prefix(key));
    assert_eq!(get("username:s:"), Some("alice"));
    assert_eq!(get("full address:s:"), Some("server.example:3390"));
    assert_eq!(get("redirectclipboard:i:"), Some("0"));
    assert_eq!(get("drivestoredirect:s:"), Some(""));
    assert!(!text.to_lowercase().contains("password"));
}
#[test]
fn unsupported_smartcard_is_not_silently_enabled() {
    let options = RdpOptions {
        smartcard: true,
        ..Default::default()
    };
    let error = options.negotiate(&RdpCapabilities::embedded()).unwrap_err();
    assert!(error.to_lowercase().contains("smartcard"), "{error}");
}
#[test]
fn selected_redirection_requires_a_provider_and_legacy_security_is_explicit() {
    for options in [
        RdpOptions {
            printers: true,
            ..Default::default()
        },
        RdpOptions {
            serial_ports: true,
            ..Default::default()
        },
        RdpOptions {
            security: RdpSecurity::Tls,
            ..Default::default()
        },
    ] {
        assert!(options.negotiate(&RdpCapabilities::embedded()).is_err());
    }
    let mut session = Session {
        protocol: "RDP".into(),
        host: "server".into(),
        user: "alice\r\nredirectclipboard:i:1".into(),
        ..Default::default()
    };
    assert!(windows_profile(&session).is_err());
    session.user = "alice".into();
    session.rdp_options.security = RdpSecurity::Legacy;
    assert!(windows_profile(&session).is_err());
}
#[test]
fn old_sessions_keep_safe_defaults_and_options_roundtrip_without_secrets() {
    let session: Session =
        serde_json::from_str(r#"{"protocol":"RDP","host":"example","user":"alice"}"#).unwrap();
    assert_eq!(session.rdp_options, RdpOptions::default());
    assert_eq!(session.rdp_options.clipboard, ClipboardPolicy::Disabled);
    assert_eq!(session.rdp_options.security, RdpSecurity::Nla);
    let mut custom = session;
    custom.rdp_options.clipboard = ClipboardPolicy::Bidirectional;
    custom.rdp_options.keyboard = KeyboardRouting::Local;
    let encoded = serde_json::to_string(&custom).unwrap();
    let decoded: Session = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded.rdp_options, custom.rdp_options);
}
#[cfg(unix)]
#[path = "support/rdp_server.rs"]
mod rdp_server;
#[cfg(unix)]
#[test]
fn redirection_options_match_negotiated_channels() {
    use harbor_workspace::desktop::{DesktopEvent, DesktopInput, DesktopSession};
    use std::time::{Duration, Instant};
    for policy in [
        ClipboardPolicy::Disabled,
        ClipboardPolicy::ToRemote,
        ClipboardPolicy::FromRemote,
        ClipboardPolicy::Bidirectional,
    ] {
        let server = rdp_server::Server::start();
        let mut config = server.config(8300);
        config.session.rdp_options.clipboard = policy;
        let mut session = DesktopSession::connect(config).unwrap();
        let end = Instant::now() + Duration::from_secs(10);
        let mut painted = false;
        let mut clipboard = false;
        let mut ready = false;
        while !painted
            || (policy != ClipboardPolicy::Disabled && !ready)
            || (policy.from_remote() && !clipboard)
        {
            for event in session.poll() {
                match event {
                    DesktopEvent::Authentication { id, .. } => session
                        .send(DesktopInput::Authenticate {
                            id,
                            user: "fixture-user".into(),
                            password: harbor_workspace::credentials::Secret::new(
                                "fixture-password",
                            ),
                            domain: String::new(),
                        })
                        .unwrap(),
                    DesktopEvent::Frame(frame) => {
                        painted = frame.pixels.get(..4) == Some(&[255, 0, 0, 255])
                    }
                    DesktopEvent::ClipboardReady => ready = true,
                    DesktopEvent::Clipboard { text } => {
                        assert!(policy.from_remote());
                        assert_eq!(text, "Remote clipboard 🐧");
                        clipboard = true;
                    }
                    DesktopEvent::Disconnected { message } => panic!("{message}"),
                    _ => {}
                }
            }
            assert!(
                Instant::now() < end,
                "Clipboard negotiation timed out: {policy:?}, painted={painted}, ready={ready}, clipboard={clipboard}; {}",
                server.observations()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let sent = session.send(DesktopInput::Clipboard {
            text: "Local clipboard 🐧".into(),
        });
        assert_eq!(sent.is_ok(), policy.to_remote());
        session
            .send(DesktopInput::Key {
                code: 88,
                extended: false,
                pressed: true,
            })
            .unwrap();
        let end = Instant::now() + Duration::from_secs(3);
        while !server.observations().contains("code: 88")
            || (policy.to_remote()
                && !server
                    .observations()
                    .contains("clipboard:Local clipboard 🐧"))
        {
            assert!(Instant::now() < end, "{}", server.observations());
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            server.observations().contains("clipboard:ready"),
            policy != ClipboardPolicy::Disabled
        );
        assert_eq!(
            server
                .observations()
                .contains("clipboard:Local clipboard 🐧"),
            policy.to_remote()
        );
        session.close();
    }
}

#[test]
fn windows_launcher_owns_a_private_profile_until_last_launch_is_dropped() {
    let session = Session {
        protocol: "RDP".into(),
        host: "server.example".into(),
        user: "alice".into(),
        ..Default::default()
    };
    let launch = harbor_workspace::connection::windows_rdp_launch(&session).unwrap();
    assert_eq!(launch.program, "mstsc.exe");
    assert_eq!(launch.args.len(), 1);
    let path = std::path::PathBuf::from(&launch.args[0]);
    assert_eq!(path.extension().unwrap(), "rdp");
    assert_eq!(
        std::fs::read(&path).unwrap(),
        windows_profile(&session).unwrap()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let cloned = launch.clone();
    drop(launch);
    assert!(path.exists());
    drop(cloned);
    assert!(
        !path.exists(),
        "Temporary RDP profile leaked after launch ended"
    );
}

#[test]
fn installed_freerdp_respects_clipboard_and_domain_and_rejects_unavailable_options() {
    let mut session = Session {
        protocol: "RDP".into(),
        host: "server.example".into(),
        user: "alice".into(),
        desktop_engine: harbor_workspace::model::DesktopEngine::External,
        ..Default::default()
    };
    session.rdp_options.domain = "EXAMPLE".into();
    let args = harbor_workspace::connection::freerdp_arguments(&session).unwrap();
    #[cfg(not(windows))]
    {
        assert!(args.iter().any(|a| a == "-clipboard"));
        assert!(args.iter().any(|a| a == "/sec:nla"));
        assert!(args.iter().any(|a| a == "/d:EXAMPLE"));
        assert!(!args.iter().any(|a| a.contains("cert:ignore")));
        session.rdp_options.clipboard = ClipboardPolicy::ToRemote;
        assert!(harbor_workspace::connection::freerdp_arguments(&session).is_err());
    }
}

#[test]
fn installed_adapters_reject_options_they_cannot_enforce() {
    let mut session = Session {
        protocol: "RDP".into(),
        host: "example".into(),
        ..Default::default()
    };
    session.view_only = true;
    assert!(
        windows_profile(&session).is_err(),
        "mstsc cannot enforce view-only"
    );
    assert!(harbor_workspace::connection::freerdp_arguments(&session).is_err());
    session.view_only = false;
    session.rdp_options.certificate_authority = Some("custom.pem".into());
    assert!(
        windows_profile(&session).is_err(),
        "custom CA must not be silently ignored"
    );
    session.rdp_options.certificate_authority = None;
    session.rdp_options.keyboard_layout = 0x0409;
    assert!(
        windows_profile(&session).is_err(),
        "custom layout must not be silently ignored"
    );
}

#[test]
fn embedded_drive_and_administrative_options_have_providers() {
    let options = RdpOptions {
        administrative: true,
        drives: vec![DriveMapping {
            name: "Work".into(),
            path: std::env::temp_dir(),
            read_only: true,
        }],
        ..Default::default()
    };
    assert!(options.negotiate(&RdpCapabilities::embedded()).is_ok());
}

#[test]
fn redirected_drive_names_cannot_collide_after_protocol_truncation() {
    let options = RdpOptions {
        drives: vec![DriveMapping {
            name: "WorkDrive".into(),
            path: std::env::temp_dir(),
            read_only: true,
        }],
        ..Default::default()
    };
    assert!(options.validate().is_err());
}

#[test]
fn printer_mapping_is_explicit_and_rejected_by_installed_adapters() {
    let mut session = Session {
        protocol: "RDP".into(),
        host: "server.example".into(),
        ..Default::default()
    };
    assert!(session.rdp_options.printer_mapping.is_none());
    session.rdp_options.printer_mapping = Some(PrinterMapping {
        queue: "Office_Laser".into(),
        driver: "Example PostScript".into(),
    });
    assert_eq!(
        session
            .rdp_options
            .negotiate(&RdpCapabilities::embedded())
            .is_ok(),
        cfg!(any(target_os = "linux", target_os = "macos", windows))
    );
    let encoded = serde_json::to_string(&session).unwrap();
    let roundtrip: Session = serde_json::from_str(&encoded).unwrap();
    assert_eq!(roundtrip.rdp_options, session.rdp_options);
    assert!(windows_profile(&session).is_err());
    assert!(harbor_workspace::connection::freerdp_arguments(&session).is_err());
    session.view_only = true;
    let attempt = harbor_workspace::desktop::DesktopSession::connect(
        harbor_workspace::desktop::DesktopConfig::new(1, session.clone(), "unused-engine".into()),
    );
    assert!(attempt.err().unwrap().contains("View-only"));
    for queue in ["", "bad/name", "bad\\name", "bad\r\nHost"]
        .into_iter()
        .chain(
            (!cfg!(windows))
                .then_some(["..", "bad%2f"])
                .into_iter()
                .flatten(),
        )
    {
        session.rdp_options.printer_mapping.as_mut().unwrap().queue = queue.into();
        assert!(session.rdp_options.validate().is_err());
    }
    let mapping = session.rdp_options.printer_mapping.as_mut().unwrap();
    mapping.queue = "Office".into();
    mapping.driver = "Injected\0Driver".into();
    assert!(session.rdp_options.validate().is_err());
}

#[test]
fn explicit_pcsc_reader_is_required_and_platform_bounded() {
    use harbor_workspace::desktop::rdp_options::{RdpCapabilities, RdpOptions};
    let capability = RdpCapabilities::embedded();
    let mut options: RdpOptions =
        serde_json::from_str(r#"{"smartcard_reader":"Fixture Reader 00 00"}"#).unwrap();
    assert_eq!(
        options.negotiate(&capability).is_ok(),
        cfg!(any(target_os = "linux", target_os = "macos", windows))
    );
    options.smartcard = true;
    assert!(options.negotiate(&capability).is_err());
    let invalid: RdpOptions =
        serde_json::from_str(r#"{"smartcard_reader":"invalid\u0000reader"}"#).unwrap();
    assert!(invalid.validate().is_err());
}
