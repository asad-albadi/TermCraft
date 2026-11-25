use harbor_workspace::{
    connection,
    model::{Preferences, Session, SshEngine},
    ssh::config::{SshDefaults, SshOptions},
};
#[test]
fn unsupported_directive_keeps_visible_compatibility_mode() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("ssh config");
    std::fs::write(
        &path,
        "Host work\n HostName actual.test\n ProxyCommand /usr/bin/false\n",
    )
    .unwrap();
    let mut session = Session {
        host: "work".into(),
        ..Default::default()
    };
    session.ssh_options.config_file = Some(path.clone());
    let resolved = SshOptions::resolve(&session, &SshDefaults::default()).unwrap();
    assert!(!resolved.report.unsupported.is_empty());
    assert!(resolved.report.requires_compatibility());
    assert_eq!(resolved.session.host, "actual.test");
    session.ssh_engine = SshEngine::OpenSsh;
    let launch = connection::launch(&session, &Preferences::default()).unwrap();
    assert!(
        launch
            .args
            .windows(2)
            .any(|args| args[0] == "-F" && args[1] == path.to_str().unwrap())
    );
    assert!(SshOptions::parse_timeout("").is_err());
    assert!(SshOptions::parse_timeout("0").is_err());
    assert!(SshOptions::parse_timeout("invalid").is_err());
}
#[test]
fn proxy_secret_is_not_in_launch_args() {
    use harbor_workspace::ssh::config::{ProxyKind, ProxySettings};
    let mut session = Session {
        host: "example.test".into(),
        ssh_engine: SshEngine::OpenSsh,
        ..Default::default()
    };
    session.ssh_options.proxy = Some(ProxySettings {
        kind: ProxyKind::HttpConnect,
        host: "127.0.0.1".into(),
        port: 3128,
        credential_ref: Some("opaque-proxy-ref".into()),
        authenticate: true,
    });
    let serialized = serde_json::to_string(&session).unwrap();
    assert!(!serialized.contains("SENTINEL_PROXY_PASSWORD"));
    match connection::launch(&session, &Preferences::default()) {
        Ok(launch) => assert!(
            !launch
                .args
                .iter()
                .any(|a| a.contains("SENTINEL_PROXY_PASSWORD"))
        ),
        Err(error) => assert!(error.to_lowercase().contains("embedded")),
    }
}
#[cfg(unix)]
#[path = "support/ssh_proxy.rs"]
mod ssh_proxy;
#[cfg(unix)]
#[allow(dead_code)]
#[path = "support/ssh_server.rs"]
mod ssh_server;
#[cfg(unix)]
#[derive(Clone, Default)]
struct ManualClock(std::sync::Arc<std::sync::Mutex<ClockState>>);
#[cfg(unix)]
#[derive(Default)]
struct ClockState {
    now: u64,
    requested: Option<u64>,
    sleepers: Vec<(u64, tokio::sync::oneshot::Sender<()>)>,
}
#[cfg(unix)]
impl ManualClock {
    fn advance(&self, seconds: u64) {
        let mut state = self.0.lock().unwrap();
        state.now += seconds;
        let now = state.now;
        let mut pending = vec![];
        for (deadline, sender) in state.sleepers.drain(..) {
            if deadline <= now {
                let _ = sender.send(());
            } else {
                pending.push((deadline, sender));
            }
        }
        state.sleepers = pending;
    }
}
#[cfg(unix)]
impl harbor_workspace::ssh::clock::Clock for ManualClock {
    fn sleep(
        &self,
        duration: std::time::Duration,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let mut state = self.0.lock().unwrap();
        state.requested = Some(duration.as_secs());
        let deadline = state.now + duration.as_secs();
        state.sleepers.push((deadline, tx));
        Box::pin(async move {
            let _ = rx.await;
        })
    }
}
#[cfg(unix)]
#[test]
fn keepalive_and_proxy_reach_the_transport() {
    use harbor_workspace::{
        ssh::config::{ProxyKind, ProxySettings},
        ssh::{ConnectionEvent, ConnectionHandle},
    };
    use std::{
        sync::{Arc, atomic::Ordering},
        time::{Duration, Instant},
    };
    let server = ssh_server::SshServer::start(true);
    let target = format!("127.0.0.1:{}", server.port).parse().unwrap();
    let proxy = ssh_proxy::Proxy::start(target);
    let clock = ManualClock::default();
    let mut cfg = server.config(7001, true);
    cfg.session.ssh_options.keepalive_secs = Some(60);
    cfg.session.ssh_options.proxy = Some(ProxySettings {
        kind: ProxyKind::HttpConnect,
        host: "127.0.0.1".into(),
        port: proxy.address.port(),
        credential_ref: None,
        authenticate: false,
    });
    cfg.clock = Some(Arc::new(clock.clone()));
    let mut handle = ConnectionHandle::connect(cfg).unwrap();
    let end = Instant::now() + Duration::from_secs(6);
    let mut ready = false;
    while !ready || clock.0.lock().unwrap().requested.is_none() {
        for event in handle.poll() {
            match event {
                ConnectionEvent::Directory(_) => ready = true,
                ConnectionEvent::Exited { message, .. } => panic!("{message}"),
                ConnectionEvent::Challenge { .. } => panic!("Owned fixture must not prompt"),
                _ => {}
            }
        }
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        proxy.requested.lock().unwrap().as_deref(),
        Some(target.to_string().as_str())
    );
    assert_eq!(clock.0.lock().unwrap().requested, Some(60));
    std::thread::sleep(Duration::from_millis(100));
    let before = proxy.client_bytes.load(Ordering::Acquire);
    clock.advance(59);
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(proxy.client_bytes.load(Ordering::Acquire), before);
    clock.advance(1);
    let end = Instant::now() + Duration::from_secs(2);
    while proxy.client_bytes.load(Ordering::Acquire) == before {
        assert!(
            Instant::now() < end,
            "Configured keepalive did not reach proxy transport"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    handle.cancel();
}
#[cfg(unix)]
#[test]
fn proxy_authentication_uses_private_typed_prompts_for_http_and_socks() {
    use harbor_workspace::{
        credentials::CredentialBinding,
        model::{ProxyKind, ProxySettings},
        ssh::{ChallengeAnswer, ConnectionEvent, ConnectionHandle},
    };
    use std::time::{Duration, Instant};
    for kind in [ProxyKind::HttpConnect, ProxyKind::Socks5] {
        let server = ssh_server::SshServer::start(true);
        let target = format!("127.0.0.1:{}", server.port).parse().unwrap();
        let proxy = if kind == ProxyKind::HttpConnect {
            ssh_proxy::Proxy::http_authenticated(target)
        } else {
            ssh_proxy::Proxy::socks_authenticated(target)
        };
        let mut cfg = server.config(7010, true);
        cfg.session.ssh_options.proxy = Some(ProxySettings {
            kind,
            host: "127.0.0.1".into(),
            port: proxy.address.port(),
            credential_ref: Some("opaque-ref".into()),
            authenticate: true,
        });
        let mut handle = ConnectionHandle::connect(cfg).unwrap();
        let end = Instant::now() + Duration::from_secs(7);
        let mut ready = false;
        let mut prompts = 0;
        while !ready {
            for event in handle.poll() {
                match event {
                    ConnectionEvent::Challenge {
                        id,
                        credential: Some(credential),
                        prompts: fields,
                        ..
                    } => {
                        assert_eq!(fields.len(), 2);
                        assert_eq!(credential.reference.as_deref(), Some("opaque-ref"));
                        assert!(
                            matches!(credential.binding,CredentialBinding::Proxy{kind:bound,..}if bound==kind)
                        );
                        prompts += 1;
                        handle
                            .answer(ChallengeAnswer {
                                id,
                                values: vec![
                                    zeroize::Zeroizing::new("proxy-user".into()),
                                    zeroize::Zeroizing::new("SENTINEL_PROXY_PASSWORD".into()),
                                ],
                                trust: None,
                            })
                            .unwrap();
                    }
                    ConnectionEvent::Directory(_) => ready = true,
                    ConnectionEvent::Exited { message, .. } => panic!("{message}"),
                    ConnectionEvent::Challenge { .. } => panic!("Unexpected SSH prompt"),
                    _ => {}
                }
            }
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(prompts, 1);
        assert!(
            proxy
                .authenticated
                .load(std::sync::atomic::Ordering::Acquire)
        );
        assert_eq!(
            proxy.requested.lock().unwrap().as_deref(),
            Some(target.to_string().as_str())
        );
        handle.cancel();
    }
}
#[test]
fn config_precedence_paths_patterns_and_shared_secret_stripping() {
    let mut session = Session {
        host: "work".into(),
        user: "explicit".into(),
        ..Default::default()
    };
    let text = "Host w* !wrong\n HostName Actual.test\n User inherited\n Port 2202\n IdentityFile=\"/Tmp/Case Sensitive\"\n ServerAliveInterval 17\n ConnectTimeout 23\nHost *\n User fallback\n";
    let resolved =
        SshOptions::resolve_text(&session, &SshDefaults::default(), text, "owned").unwrap();
    assert_eq!(resolved.session.host, "Actual.test");
    assert_eq!(resolved.session.user, "explicit");
    assert_eq!(resolved.session.key, "/Tmp/Case Sensitive");
    assert_eq!(resolved.session.port, 22);
    assert_eq!(resolved.keepalive_secs, Some(17));
    assert_eq!(resolved.connect_timeout_secs, 23);
    session.ssh_options.prefer_config = true;
    session.ssh_options.keepalive_disabled = true;
    session.ssh_options.connect_timeout_secs = Some(30);
    let resolved =
        SshOptions::resolve_text(&session, &SshDefaults::default(), text, "owned").unwrap();
    assert_eq!(resolved.session.port, 2202);
    assert_eq!(resolved.session.user, "inherited");
    assert_eq!(resolved.keepalive_secs, None);
    assert_eq!(resolved.connect_timeout_secs, 30);
    session
        .gateway_credential_refs
        .insert("gateway".into(), "SECRET_REFERENCE".into());
    session.ssh_options.proxy = Some(harbor_workspace::model::ProxySettings {
        kind: Default::default(),
        host: "proxy.test".into(),
        port: 80,
        credential_ref: Some("SECRET_REFERENCE".into()),
        authenticate: true,
    });
    session.clear_credential_refs();
    assert!(
        !serde_json::to_string(&session)
            .unwrap()
            .contains("SECRET_REFERENCE")
    );
    let report = SshOptions::resolve_text(
        &Session {
            host: "work".into(),
            ..Default::default()
        },
        &SshDefaults::default(),
        "Host work\n SENTINEL_PASSWORD hello\n",
        "owned",
    )
    .unwrap()
    .report;
    assert_eq!(report.unsupported[0].field, "Unrecognized directive");
}
#[cfg(unix)]
#[test]
fn two_configured_gateways_authenticate_in_order_before_target() {
    use harbor_workspace::ssh::{ConnectionEvent, ConnectionHandle};
    use std::{
        fs,
        time::{Duration, Instant},
    };
    let first = ssh_server::SshServer::start(true);
    let second = ssh_server::SshServer::start(true);
    let target = ssh_server::SshServer::start(true);
    let mut cfg = target.config(7020, true);
    let user = cfg.session.user.clone();
    let path = target.root.join("chain config");
    fs::write(&path,format!("Host first\n HostName 127.0.0.1\n Port {}\n User {user}\n IdentityFile \"{}\"\nHost second\n HostName 127.0.0.1\n Port {}\n User {user}\n IdentityFile \"{}\"\n",first.port,first.root.join("client").display(),second.port,second.root.join("client").display())).unwrap();
    let known = format!(
        "[127.0.0.1]:{} {}[127.0.0.1]:{} {}[127.0.0.1]:{} {}",
        first.port,
        fs::read_to_string(first.root.join("host.pub")).unwrap(),
        second.port,
        fs::read_to_string(second.root.join("host.pub")).unwrap(),
        target.port,
        fs::read_to_string(target.root.join("host.pub")).unwrap()
    );
    fs::write(&cfg.app_known_hosts, known).unwrap();
    cfg.session.jump = "first,second".into();
    cfg.session.ssh_options.config_file = Some(path);
    let mut handle = ConnectionHandle::connect(cfg).unwrap();
    let end = Instant::now() + Duration::from_secs(8);
    let mut ready = false;
    while !ready {
        for event in handle.poll() {
            match event {
                ConnectionEvent::Directory(_) => ready = true,
                ConnectionEvent::Exited { message, .. } => panic!("{message}"),
                ConnectionEvent::Challenge { .. } => {
                    panic!("Unexpected prompt in trusted key-only chain")
                }
                _ => {}
            }
        }
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(first.accepted_authentications(), 1);
    assert_eq!(second.accepted_authentications(), 1);
    assert_eq!(target.accepted_authentications(), 1);
    handle.cancel();
}
#[cfg(unix)]
#[test]
fn unsupported_configuration_exits_before_opening_network_connection() {
    use harbor_workspace::ssh::{ConnectionEvent, ConnectionHandle};
    use std::time::{Duration, Instant};
    let server = ssh_server::SshServer::start(true);
    let mut cfg = server.config(7030, true);
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    cfg.session.port = listener.local_addr().unwrap().port();
    let path = server.root.join("unsupported config");
    std::fs::write(
        &path,
        "Host *\n ProxyCommand secret-command-that-must-not-run\n",
    )
    .unwrap();
    cfg.session.ssh_options.config_file = Some(path);
    let mut handle = ConnectionHandle::connect(cfg).unwrap();
    let end = Instant::now() + Duration::from_secs(3);
    loop {
        let exited = handle.poll().into_iter().find_map(|e| {
            if let ConnectionEvent::Exited { message, .. } = e {
                Some(message)
            } else {
                None
            }
        });
        if let Some(message) = exited {
            assert!(message.contains("compatibility"));
            assert!(message.contains("proxycommand"));
            assert!(!message.contains("secret-command"));
            break;
        }
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(matches!(listener.accept(),Err(e)if e.kind()==std::io::ErrorKind::WouldBlock));
}
#[test]
fn forwarding_enablement_is_separate_from_trusted_policy() {
    let mut session = Session {
        host: "work".into(),
        ..Default::default()
    };
    session.ssh_options.prefer_config = true;
    let disabled = SshOptions::resolve_text(
        &session,
        &SshDefaults::default(),
        "Host work\n ForwardX11 no\n ForwardX11Trusted yes\n",
        "owned",
    )
    .unwrap();
    assert!(
        !disabled.session.x11,
        "Trusted policy cannot enable forwarding"
    );
    let untrusted = SshOptions::resolve_text(
        &session,
        &SshDefaults::default(),
        "Host work\n ForwardX11 yes\n ForwardX11Trusted no\n",
        "owned",
    )
    .unwrap();
    assert!(
        untrusted.report.requires_compatibility(),
        "Untrusted X11 is not implemented by the trusted forwarder"
    );
}
#[cfg(unix)]
#[test]
fn timeout_covers_silent_target_and_gateway_handshakes() {
    use harbor_workspace::ssh::{ConnectionEvent, ConnectionHandle};
    use std::time::{Duration, Instant};
    for gateway in [false, true] {
        let server = ssh_server::SshServer::start(true);
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut cfg = server.config(7050, true);
        cfg.session.ssh_options.connect_timeout_secs = Some(1);
        if gateway {
            cfg.session.jump = format!("127.0.0.1:{port}");
        } else {
            cfg.session.port = port;
        }
        let mut handle = ConnectionHandle::connect(cfg).unwrap();
        let (_silent_peer, _) = listener.accept().unwrap();
        let end = Instant::now() + Duration::from_millis(1900);
        let mut exited = false;
        while Instant::now() < end {
            for event in handle.poll() {
                if let ConnectionEvent::Exited { message, .. } = event {
                    assert!(message.to_lowercase().contains("timed out"), "{message}");
                    exited = true;
                }
            }
            if exited {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            exited,
            "Silent SSH peer exceeded configured one-second handshake timeout (gateway={gateway})"
        );
        handle.cancel();
    }
}
#[cfg(unix)]
#[test]
fn waiting_for_host_key_answer_pauses_handshake_deadline() {
    use harbor_workspace::ssh::{
        ChallengeAnswer, ConnectionEvent, ConnectionHandle, TrustDecision,
    };
    use std::time::{Duration, Instant};
    let server = ssh_server::SshServer::start(true);
    let mut cfg = server.config(7060, false);
    cfg.session.ssh_options.connect_timeout_secs = Some(1);
    let mut handle = ConnectionHandle::connect(cfg).unwrap();
    let end = Instant::now() + Duration::from_secs(5);
    let mut answered = false;
    let mut ready = false;
    while !ready {
        for event in handle.poll() {
            match event {
                ConnectionEvent::Challenge {
                    id, host_key: true, ..
                } => {
                    std::thread::sleep(Duration::from_millis(1250));
                    handle
                        .answer(ChallengeAnswer {
                            id,
                            values: vec![],
                            trust: Some(TrustDecision::Once),
                        })
                        .unwrap();
                    answered = true;
                }
                ConnectionEvent::Directory(_) => ready = true,
                ConnectionEvent::Exited { message, .. } => panic!("{message}"),
                _ => {}
            }
        }
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(answered);
    handle.cancel();
}
#[cfg(unix)]
#[test]
fn config_cannot_enable_x11_on_source_only_sftp_connection() {
    const CHILD: &str = "TERMCRAFT_SOURCE_OPTIONS_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "config_cannot_enable_x11_on_source_only_sftp_connection",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env_remove("DISPLAY")
            .env_remove("XAUTHORITY")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    use harbor_workspace::{
        remote_files::RemotePath,
        ssh::{ConnectionEvent, ConnectionHandle},
    };
    use std::time::{Duration, Instant};
    let server = ssh_server::SshServer::start(true);
    let mut cfg = server.config(7070, true);
    let path = server.root.join("source");
    std::fs::write(&path, b"owned source bytes").unwrap();
    let config = server.root.join("source-config");
    std::fs::write(&config, "Host *\n ForwardX11 yes\n ForwardX11Trusted yes\n").unwrap();
    cfg.session.ssh_options.prefer_config = true;
    cfg.session.ssh_options.config_file = Some(config);
    let mut handle = ConnectionHandle::read_file(
        cfg,
        1,
        RemotePath::from_bytes(path.to_str().unwrap().as_bytes().to_vec()).unwrap(),
        1024,
    )
    .unwrap();
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        for event in handle.poll() {
            match event {
                ConnectionEvent::FileContents { bytes, .. } => {
                    assert_eq!(bytes, b"owned source bytes");
                    return;
                }
                ConnectionEvent::Exited { message, .. } => panic!("{message}"),
                _ => {}
            }
        }
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(5));
    }
}
