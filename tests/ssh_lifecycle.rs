#![cfg(unix)]
#[allow(dead_code)]
#[path = "support/auth_server.rs"]
mod auth_server;
#[allow(dead_code)]
#[path = "support/ssh_server.rs"]
mod ssh_server;
use harbor_workspace::{
    remote_files::{
        RemotePath,
        editor::{DocumentStatus, RemoteDocument},
    },
    ssh::{ChallengeAnswer, ConnectionCommand, ConnectionEvent, ConnectionHandle},
};
use std::time::{Duration, Instant};
fn path(path: &std::path::Path) -> RemotePath {
    RemotePath::from_bytes(path.as_os_str().as_encoded_bytes().to_vec()).unwrap()
}
fn until(
    handle: &mut ConnectionHandle,
    mut expected: impl FnMut(&ConnectionEvent) -> bool,
) -> ConnectionEvent {
    let deadline = Instant::now() + Duration::from_secs(6);
    loop {
        for event in handle.poll() {
            if expected(&event) {
                return event;
            }
            if let ConnectionEvent::Exited { message, .. } = event {
                panic!("Unexpected exit: {message}");
            }
        }
        assert!(
            Instant::now() < deadline,
            "Expected lifecycle event did not arrive"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[cfg(target_os = "linux")]
#[test]
fn browser_retry_does_not_reauthenticate() {
    let server = ssh_server::SshServer::start(true);
    let mut handle = ConnectionHandle::connect(server.config(501, true)).unwrap();
    until(&mut handle, |e| matches!(e, ConnectionEvent::Directory(_)));
    server.terminate_owned_sftp();
    std::thread::sleep(Duration::from_millis(100));
    handle
        .try_send(ConnectionCommand::List {
            request: 1,
            path: path(&server.root),
        })
        .unwrap();
    until(&mut handle, |e| {
        matches!(e, ConnectionEvent::FilesError { request: 1, .. })
    });
    handle
        .try_send(ConnectionCommand::List {
            request: 2,
            path: path(&server.root),
        })
        .unwrap();
    until(
        &mut handle,
        |e| matches!(e, ConnectionEvent::Directory(d) if d.request==2),
    );
    assert_eq!(server.accepted_authentications(), 1);
}
#[test]
fn reconnect_discards_old_challenges_and_listings() {
    let server = auth_server::AuthServer::start(auth_server::Mode::Password);
    let mut old = ConnectionHandle::connect(server.config()).unwrap();
    let event = until(&mut old, |e| matches!(e, ConnectionEvent::Challenge { .. }));
    let ConnectionEvent::Challenge { id: old_id, .. } = event else {
        unreachable!()
    };
    old.cancel();
    let mut config = server.config();
    config.id += 1;
    let mut fresh = ConnectionHandle::connect(config).unwrap();
    let event = until(&mut fresh, |e| {
        matches!(e, ConnectionEvent::Challenge { .. })
    });
    let ConnectionEvent::Challenge { id: fresh_id, .. } = event else {
        unreachable!()
    };
    assert_ne!(
        old_id, fresh_id,
        "Challenge identities must survive connection-generation changes"
    );
    assert!(
        fresh
            .answer(ChallengeAnswer {
                id: old_id,
                values: vec!["fixture-password".to_owned().into()],
                trust: None
            })
            .is_err()
    );
    fresh
        .answer(ChallengeAnswer {
            id: fresh_id,
            values: vec!["fixture-password".to_owned().into()],
            trust: None,
        })
        .unwrap();
    until(&mut fresh, |e| {
        matches!(e, ConnectionEvent::Connected { .. })
    });
    assert_eq!(server.count(), 1);
}
#[test]
fn cancel_auth_stops_the_entire_attempt() {
    for mode in [
        auth_server::Mode::Password,
        auth_server::Mode::Mfa,
        auth_server::Mode::KeyThenMfa,
    ] {
        let server = auth_server::AuthServer::start(mode);
        let mut handle = ConnectionHandle::connect(server.config()).unwrap();
        let event = until(&mut handle, |e| {
            matches!(e, ConnectionEvent::Challenge { .. })
        });
        let ConnectionEvent::Challenge { id, .. } = event else {
            unreachable!()
        };
        let attempts_before_cancel = server.attempts();
        handle.cancel();
        assert!(
            handle
                .answer(ChallengeAnswer {
                    id,
                    values: vec![
                        "fixture-password".to_owned().into(),
                        "123456".to_owned().into()
                    ],
                    trust: None
                })
                .is_err(),
            "Cancelled authentication still accepted credentials"
        );
        let deadline = Instant::now() + Duration::from_secs(3);
        while !handle.is_finished() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!handle.poll().iter().any(|event| matches!(
            event,
            ConnectionEvent::Challenge { .. } | ConnectionEvent::Connected { .. }
        )));
        assert_eq!(server.count(), 0);
        assert_eq!(
            server.attempts(),
            attempts_before_cancel,
            "Cancellation must not submit another credential or begin another method"
        );
    }
}
#[test]
fn prompted_username_identity_survives_fresh_editor_recovery_without_implicit_upload() {
    let server = ssh_server::SshServer::start(true);
    let remote = server.root.join("prompted-user-document");
    std::fs::write(&remote, b"original").unwrap();
    fn opened(
        server: &ssh_server::SshServer,
        remote: &std::path::Path,
        id: u64,
    ) -> (ConnectionHandle, RemoteDocument) {
        let user = std::env::var("USER").unwrap();
        let mut config = server.config(id, true);
        config.session.user.clear();
        let mut handle = ConnectionHandle::connect(config).unwrap();
        let document = RemoteDocument::open(
            id,
            path(remote),
            &handle.transfers,
            server.root.join(format!("recovery-{id}")),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(6);
        loop {
            for event in handle.poll() {
                match event {
                    ConnectionEvent::Challenge { id, prompts, .. } => {
                        assert_eq!(prompts[0].label, "Username");
                        assert!(prompts[0].echo);
                        handle
                            .answer(ChallengeAnswer {
                                id,
                                values: vec![user.clone().into()],
                                trust: None,
                            })
                            .unwrap();
                    }
                    ConnectionEvent::Exited { message, .. } => panic!("{message}"),
                    _ => {}
                }
            }
            if document.snapshot().status == DocumentStatus::Ready {
                return (handle, document);
            }
            assert!(
                Instant::now() < deadline,
                "{:?}",
                document.snapshot().status
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    let (old, mut source) = opened(&server, &remote, 601);
    source.set_text("recovered edit").unwrap();
    old.cancel();
    let (fresh, mut target) = opened(&server, &remote, 602);
    assert!(
        target.can_import_from(&source),
        "An authenticated prompted username was lost from endpoint identity"
    );
    target.import_recovery_from(&source).unwrap();
    assert_eq!(target.snapshot().text, "recovered edit");
    assert!(target.snapshot().dirty);
    assert_eq!(std::fs::read(remote).unwrap(), b"original");
    assert_eq!(server.accepted_authentications(), 2);
    drop(fresh);
}
#[test]
fn cancelling_encrypted_key_unlock_rejects_late_passphrases() {
    let server = ssh_server::SshServer::start(true);
    let locked = server.root.join("locked-key");
    assert!(
        std::process::Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "fixture-passphrase", "-f"])
            .arg(&locked)
            .status()
            .unwrap()
            .success()
    );
    let mut config = server.config(701, true);
    config.session.key = locked.to_string_lossy().into();
    let mut handle = ConnectionHandle::connect(config).unwrap();
    let event = until(&mut handle, |e| {
        matches!(e, ConnectionEvent::Challenge { .. })
    });
    let ConnectionEvent::Challenge { id, prompts, .. } = event else {
        unreachable!()
    };
    assert_eq!(prompts[0].label, "Private-key passphrase");
    assert!(!prompts[0].echo);
    handle.cancel();
    assert!(
        handle
            .answer(ChallengeAnswer {
                id,
                values: vec!["fixture-passphrase".to_owned().into()],
                trust: None
            })
            .is_err()
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    while !handle.is_finished() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(server.accepted_authentications(), 0);
    assert!(!handle.poll().iter().any(|e| matches!(
        e,
        ConnectionEvent::Challenge { .. } | ConnectionEvent::Connected { .. }
    )));
}
#[test]
fn reconnect_rejects_real_old_directory_snapshots() {
    use harbor_workspace::remote_files::BrowserState;
    let server = ssh_server::SshServer::start(true);
    let mut old = ConnectionHandle::connect(server.config(801, true)).unwrap();
    let event = until(&mut old, |e| matches!(e, ConnectionEvent::Directory(_)));
    let ConnectionEvent::Directory(old_snapshot) = event else {
        unreachable!()
    };
    old.cancel();
    let mut fresh = ConnectionHandle::connect(server.config(802, true)).unwrap();
    let event = until(&mut fresh, |e| matches!(e, ConnectionEvent::Directory(_)));
    let ConnectionEvent::Directory(fresh_snapshot) = event else {
        unreachable!()
    };
    assert_ne!(old_snapshot.connection, fresh_snapshot.connection);
    let mut browser = BrowserState::new(fresh_snapshot.connection);
    browser.begin(fresh_snapshot.request);
    assert!(!browser.apply_snapshot(old_snapshot));
    assert!(browser.apply_snapshot(fresh_snapshot));
    assert_eq!(server.accepted_authentications(), 2);
}
#[test]
fn password_then_mfa_uses_the_new_methods_without_repeating_password() {
    let server = auth_server::AuthServer::start(auth_server::Mode::PasswordThenMfa);
    let mut handle = ConnectionHandle::connect(server.config()).unwrap();
    let mut prompts = 0;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        for event in handle.poll() {
            match event {
                ConnectionEvent::Challenge {
                    id,
                    prompts: fields,
                    credential,
                    ..
                } => {
                    prompts += 1;
                    if prompts == 1 {
                        assert!(credential.is_some());
                        assert_eq!(fields.len(), 1);
                    } else {
                        assert_eq!(prompts, 2);
                        assert!(
                            credential.is_none(),
                            "Keyboard-interactive and OTP responses must not offer password-vault save/use"
                        );
                        assert_eq!(fields.len(), 2, "Password acceptance must advance to MFA");
                    }
                    let values = if prompts == 1 {
                        vec!["fixture-password".to_owned().into()]
                    } else {
                        vec![
                            "fixture-password".to_owned().into(),
                            "123456".to_owned().into(),
                        ]
                    };
                    handle
                        .answer(ChallengeAnswer {
                            id,
                            values,
                            trust: None,
                        })
                        .unwrap();
                }
                ConnectionEvent::Connected { .. } => {
                    assert_eq!(prompts, 2);
                    assert_eq!(server.count(), 1);
                    return;
                }
                ConnectionEvent::Exited { message, .. } => panic!("{message}"),
                _ => {}
            }
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn credential_titles_identify_the_role_host_and_port() {
    let server = auth_server::AuthServer::start(auth_server::Mode::Password);
    let config = server.config();
    let endpoint = format!("{}:{}", config.session.host, config.session.port);
    let mut handle = ConnectionHandle::connect(config).unwrap();
    let event = until(&mut handle, |e| {
        matches!(e, ConnectionEvent::Challenge { .. })
    });
    let ConnectionEvent::Challenge { title, .. } = event else {
        unreachable!()
    };
    assert!(
        title.starts_with("Target SSH") && title.contains(&endpoint),
        "Ambiguous endpoint: {title}"
    );
    handle.cancel();
}
fn wait_document(document: &RemoteDocument, failed: bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let status = document.snapshot().status;
        if (!failed && status == DocumentStatus::Ready)
            || (failed && matches!(status, DocumentStatus::Failed(_)))
        {
            return;
        }
        assert!(Instant::now() < deadline, "{status:?}");
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn recovery_from_an_unsuccessful_open_cannot_replace_a_fresh_document() {
    let server = ssh_server::SshServer::start(true);
    let remote = server.root.join("previously-missing");
    let old = ConnectionHandle::connect(server.config(911, true)).unwrap();
    let failed = RemoteDocument::open(
        911,
        path(&remote),
        &old.transfers,
        server.root.join("old-recovery"),
    )
    .unwrap();
    wait_document(&failed, true);
    std::fs::write(&remote, b"valid").unwrap();
    let fresh = ConnectionHandle::connect(server.config(912, true)).unwrap();
    let mut target = RemoteDocument::open(
        912,
        path(&remote),
        &fresh.transfers,
        server.root.join("fresh-recovery"),
    )
    .unwrap();
    wait_document(&target, false);
    assert!(!target.can_import_from(&failed));
    assert!(target.import_recovery_from(&failed).is_err());
    assert_eq!(target.snapshot().text, "valid");
    assert_eq!(std::fs::read(remote).unwrap(), b"valid");
}
#[test]
fn recovery_import_preserves_unsaved_destination_edits() {
    let server = ssh_server::SshServer::start(true);
    let remote = server.root.join("dirty-recovery-target");
    std::fs::write(&remote, b"original").unwrap();
    let old = ConnectionHandle::connect(server.config(921, true)).unwrap();
    let mut source = RemoteDocument::open(
        921,
        path(&remote),
        &old.transfers,
        server.root.join("old-recovery"),
    )
    .unwrap();
    wait_document(&source, false);
    source.set_text("source edit").unwrap();
    let fresh = ConnectionHandle::connect(server.config(922, true)).unwrap();
    let mut target = RemoteDocument::open(
        922,
        path(&remote),
        &fresh.transfers,
        server.root.join("fresh-recovery"),
    )
    .unwrap();
    wait_document(&target, false);
    target.set_text("destination edit").unwrap();
    assert!(target.import_recovery_from(&source).is_err());
    assert_eq!(target.snapshot().text, "destination edit");
    assert_eq!(std::fs::read(remote).unwrap(), b"original");
}
#[test]
fn server_disconnect_retires_a_visible_password_prompt_without_an_answer() {
    use std::{
        io,
        net::{Shutdown, TcpListener, TcpStream},
    };
    let server = auth_server::AuthServer::start(auth_server::Mode::Password);
    let mut config = server.config();
    let upstream = config.session.port;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    config.session.port = listener.local_addr().unwrap().port();
    config.app_known_hosts = config.app_known_hosts.with_file_name("proxy-known-hosts");
    let (send, receive) = std::sync::mpsc::channel();
    let relay = std::thread::spawn(move || {
        let (mut client, _) = listener.accept().unwrap();
        let mut remote = TcpStream::connect(("127.0.0.1", upstream)).unwrap();
        let mut client_reader = client.try_clone().unwrap();
        let mut remote_writer = remote.try_clone().unwrap();
        send.send(remote.try_clone().unwrap()).unwrap();
        let copy = std::thread::spawn(move || {
            let _ = io::copy(&mut client_reader, &mut remote_writer);
            let _ = remote_writer.shutdown(Shutdown::Both);
        });
        let _ = io::copy(&mut remote, &mut client);
        let _ = client.shutdown(Shutdown::Both);
        copy.join().unwrap();
    });
    let mut handle = ConnectionHandle::connect(config).unwrap();
    let event = until(&mut handle, |e| {
        matches!(e, ConnectionEvent::Challenge { .. })
    });
    let ConnectionEvent::Challenge { id, host_key, .. } = event else {
        unreachable!()
    };
    assert!(host_key);
    handle
        .answer(ChallengeAnswer {
            id,
            values: vec![],
            trust: Some(harbor_workspace::ssh::TrustDecision::Once),
        })
        .unwrap();
    let event = until(&mut handle, |e| {
        matches!(e, ConnectionEvent::Challenge { .. })
    });
    let ConnectionEvent::Challenge {
        id: password_id,
        host_key,
        ..
    } = event
    else {
        unreachable!()
    };
    assert!(!host_key);
    assert!(!handle.is_finished());
    let remote = receive.recv_timeout(Duration::from_secs(2)).unwrap();
    // Disconnect upstream. The relay owns propagation back to the client.
    remote.shutdown(Shutdown::Both).unwrap();
    relay.join().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !handle.is_finished() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        handle.is_finished(),
        "A disconnected password prompt waited forever for local input"
    );
    assert!(
        handle
            .answer(ChallengeAnswer {
                id: password_id,
                values: vec!["fixture-password".to_owned().into()],
                trust: None
            })
            .is_err()
    );
    assert_eq!(server.count(), 0);
}
#[test]
fn recovery_never_crosses_a_changed_trust_once_server_key() {
    fn trusted_document(
        server: &ssh_server::SshServer,
        remote: &std::path::Path,
        id: u64,
    ) -> (ConnectionHandle, RemoteDocument) {
        let mut handle = ConnectionHandle::connect(server.config(id, false)).unwrap();
        let doc = RemoteDocument::open(
            id,
            path(remote),
            &handle.transfers,
            server.root.join(format!("recovery-{id}")),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            for event in handle.poll() {
                match event {
                    ConnectionEvent::Challenge { id, host_key, .. } => {
                        assert!(host_key);
                        handle
                            .answer(ChallengeAnswer {
                                id,
                                values: vec![],
                                trust: Some(harbor_workspace::ssh::TrustDecision::Once),
                            })
                            .unwrap();
                    }
                    ConnectionEvent::Exited { message, .. } => panic!("{message}"),
                    _ => {}
                }
            }
            if doc.snapshot().status == DocumentStatus::Ready {
                return (handle, doc);
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    let mut server = ssh_server::SshServer::start(true);
    let remote = server.root.join("key-bound-recovery");
    std::fs::write(&remote, b"original").unwrap();
    let (old, mut source) = trusted_document(&server, &remote, 941);
    source.set_text("private recovered text").unwrap();
    old.cancel();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !old.is_finished() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    server.restart_with_new_host_key();
    let (_fresh, mut target) = trusted_document(&server, &remote, 942);
    assert!(
        !target.can_import_from(&source),
        "Network address reuse must not equate different verified host keys"
    );
    assert!(target.import_recovery_from(&source).is_err());
    assert_eq!(target.snapshot().text, "original");
    assert_eq!(std::fs::read(remote).unwrap(), b"original");
}
#[test]
fn gateway_credential_title_identifies_the_gateway_before_target_authentication() {
    let gateway = auth_server::AuthServer::start(auth_server::Mode::Password);
    let gateway_port = gateway.config().session.port;
    let target = ssh_server::SshServer::start(true);
    let mut config = target.config(961, true);
    config.session.key.clear();
    config.session.jump = format!("{}@127.0.0.1:{gateway_port}", config.session.user);
    let mut handle = ConnectionHandle::connect(config).unwrap();
    let event = until(&mut handle, |e| {
        matches!(e, ConnectionEvent::Challenge { .. })
    });
    let ConnectionEvent::Challenge { id, host_key, .. } = event else {
        unreachable!()
    };
    assert!(host_key);
    handle
        .answer(ChallengeAnswer {
            id,
            values: vec![],
            trust: Some(harbor_workspace::ssh::TrustDecision::Once),
        })
        .unwrap();
    let event = until(&mut handle, |e| {
        matches!(e, ConnectionEvent::Challenge { .. })
    });
    let ConnectionEvent::Challenge {
        title,
        host_key,
        credential,
        ..
    } = event
    else {
        unreachable!()
    };
    assert!(!host_key);
    assert!(
        matches!(credential.unwrap().binding,harbor_workspace::credentials::CredentialBinding::Ssh{role:harbor_workspace::credentials::CredentialRole::Gateway,port,..}if port==gateway_port)
    );
    assert!(
        title.starts_with("Gateway SSH") && title.contains(&format!("127.0.0.1:{gateway_port}")),
        "{title}"
    );
    handle.cancel();
    assert_eq!(gateway.count(), 0);
    assert_eq!(target.accepted_authentications(), 0);
}
#[test]
fn recovery_never_crosses_a_changed_trust_once_gateway_key() {
    fn open(
        target: &ssh_server::SshServer,
        gateway: &ssh_server::SshServer,
        remote: &std::path::Path,
        id: u64,
    ) -> (ConnectionHandle, RemoteDocument) {
        let mut config = target.config(id, false);
        config.session.jump = format!("{}@127.0.0.1:{}", config.session.user, gateway.port);
        let mut h = ConnectionHandle::connect(config).unwrap();
        let d = RemoteDocument::open(
            id,
            path(remote),
            &h.transfers,
            target.root.join(format!("gateway-recovery-{id}")),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            for e in h.poll() {
                match e {
                    ConnectionEvent::Challenge { id, host_key, .. } => {
                        assert!(host_key);
                        h.answer(ChallengeAnswer {
                            id,
                            values: vec![],
                            trust: Some(harbor_workspace::ssh::TrustDecision::Once),
                        })
                        .unwrap();
                    }
                    ConnectionEvent::Exited { message, .. } => panic!("{message}"),
                    _ => {}
                }
            }
            if d.snapshot().status == DocumentStatus::Ready {
                return (h, d);
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    let target = ssh_server::SshServer::start(true);
    let mut gateway = ssh_server::SshServer::start(true);
    std::fs::copy(
        target.root.join("client.pub"),
        gateway.root.join("authorized_keys"),
    )
    .unwrap();
    let remote = target.root.join("gateway-key-document");
    std::fs::write(&remote, b"original").unwrap();
    let (old, mut source) = open(&target, &gateway, &remote, 971);
    source.set_text("recovered text").unwrap();
    old.cancel();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !old.is_finished() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    gateway.restart_with_new_host_key();
    let (_fresh, mut destination) = open(&target, &gateway, &remote, 972);
    assert!(!destination.can_import_from(&source));
    assert!(destination.import_recovery_from(&source).is_err());
    assert_eq!(destination.snapshot().text, "original");
    assert_eq!(std::fs::read(remote).unwrap(), b"original");
}
#[test]
fn password_credential_binding_uses_verified_endpoint_and_typed_method() {
    use harbor_workspace::credentials::{CredentialBinding, CredentialRole, SshSecretKind};
    let server = auth_server::AuthServer::start(auth_server::Mode::Password);
    let config = server.config();
    let expected = harbor_workspace::ssh::auth_profile_binding(&config.session);
    let expected_session = config.session.clone();
    let mut handle = ConnectionHandle::connect(config).unwrap();
    let event = until(&mut handle, |e| {
        matches!(
            e,
            ConnectionEvent::Challenge {
                host_key: false,
                ..
            }
        )
    });
    let ConnectionEvent::Challenge {
        credential: Some(credential),
        ..
    } = event
    else {
        panic!("Password challenge must carry verified binding")
    };
    let CredentialBinding::Ssh {
        role,
        host,
        port,
        user,
        host_key,
        profile,
        kind,
    } = credential.binding
    else {
        panic!()
    };
    assert_eq!(role, CredentialRole::Target);
    assert_eq!(host, expected_session.host);
    assert_eq!(port, expected_session.port);
    assert_eq!(user, expected_session.user);
    assert!(!host_key.is_empty());
    assert_eq!(profile, expected);
    assert_eq!(kind, SshSecretKind::Password);
    handle.cancel();
}
