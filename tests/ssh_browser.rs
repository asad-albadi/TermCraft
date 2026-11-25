#![cfg(unix)]
mod support;
use harbor_workspace::{
    remote_files::RemotePath,
    ssh::{ChallengeAnswer, ConnectionCommand, ConnectionEvent, ConnectionHandle, TrustDecision},
};
use std::time::{Duration, Instant};
use support::ssh_server::SshServer;
fn collect_until(handle: &mut ConnectionHandle, mut check: impl FnMut(&ConnectionEvent) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        for event in handle.poll() {
            if let ConnectionEvent::Exited { ref message, .. } = event {
                panic!("unexpected exit: {message}");
            }
            if check(&event) {
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("Timed out waiting for SSH event");
}
#[test]
fn embedded_shell_and_browser_share_one_authentication() {
    let server = SshServer::start(true);
    std::fs::write(server.root.join("fixture.txt"), b"hello").unwrap();
    std::fs::create_dir(server.root.join("a 'quote' é\n$(literal)")).unwrap();
    let mut handle = ConnectionHandle::connect(server.config(41, true)).unwrap();
    collect_until(
        &mut handle,
        |e| matches!(e,ConnectionEvent::Directory(snapshot) if snapshot.entries.iter().any(|f|f.display_name=="fixture.txt")),
    );
    handle
        .try_send(ConnectionCommand::Input(
            b"printf '%s%s\\n' 'SSH_' 'OK'\n".to_vec(),
        ))
        .unwrap();
    let mut output = Vec::new();
    collect_until(&mut handle, |e| {
        if let ConnectionEvent::Output { bytes, .. } = e {
            output.extend(bytes);
        }
        String::from_utf8_lossy(&output).contains("SSH_OK")
    });
    assert_eq!(server.accepted_authentications(), 1);
    let literal = RemotePath::from_bytes(
        server
            .root
            .join("a 'quote' é\n$(literal)")
            .as_os_str()
            .as_encoded_bytes()
            .to_vec(),
    )
    .unwrap();
    handle
        .try_send(ConnectionCommand::List {
            request: 1,
            path: literal.clone(),
        })
        .unwrap();
    collect_until(
        &mut handle,
        |e| matches!(e,ConnectionEvent::Directory(s) if s.request==1 && s.path==literal),
    );
}
#[test]
fn unknown_host_can_be_rejected_before_authentication() {
    let server = SshServer::start(true);
    let mut handle = ConnectionHandle::connect(server.config(42, false)).unwrap();
    let mut challenge = None;
    collect_until(&mut handle, |e| {
        if let ConnectionEvent::Challenge { id, .. } = e {
            challenge = Some(*id);
            true
        } else {
            false
        }
    });
    handle
        .answer(ChallengeAnswer {
            id: challenge.unwrap(),
            values: vec![],
            trust: Some(TrustDecision::Reject),
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !handle.is_finished() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(handle.is_finished());
    assert_eq!(server.accepted_authentications(), 0);
}
#[test]
fn cancelling_a_host_prompt_stops_the_connection() {
    let server = SshServer::start(true);
    let mut handle = ConnectionHandle::connect(server.config(43, false)).unwrap();
    collect_until(&mut handle, |e| {
        matches!(e, ConnectionEvent::Challenge { .. })
    });
    handle.cancel();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !handle.is_finished() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(handle.is_finished());
    assert_eq!(server.accepted_authentications(), 0);
}
#[test]
fn changed_key_prevents_authentication() {
    let server = SshServer::start(true);
    let mut config = server.config(44, true);
    let wrong = std::fs::read_to_string(server.root.join("client.pub")).unwrap();
    std::fs::write(
        &config.app_known_hosts,
        format!("[127.0.0.1]:{} {wrong}", server.port),
    )
    .unwrap();
    config.session.command = "printf SHOULD_NOT_RUN".into();
    let mut handle = ConnectionHandle::connect(config).unwrap();
    let deadline = Instant::now() + Duration::from_secs(4);
    let mut message = String::new();
    while Instant::now() < deadline && !handle.is_finished() {
        for e in handle.poll() {
            if let ConnectionEvent::Exited { message: m, .. } = e {
                message = m;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    for e in handle.poll() {
        if let ConnectionEvent::Exited { message: m, .. } = e {
            message = m;
        }
    }
    assert!(message.to_lowercase().contains("key"), "{message}");
    assert_eq!(server.accepted_authentications(), 0);
}
#[test]
fn missing_sftp_keeps_shell_usable() {
    let server = SshServer::start(false);
    let mut handle = ConnectionHandle::connect(server.config(45, true)).unwrap();
    collect_until(&mut handle, |e| {
        matches!(e, ConnectionEvent::FilesError { .. })
    });
    handle
        .try_send(ConnectionCommand::Input(
            b"printf '%s%s\\n' 'SHELL_' 'STILL_OK'\n".to_vec(),
        ))
        .unwrap();
    let mut output = Vec::new();
    collect_until(&mut handle, |e| {
        if let ConnectionEvent::Output { bytes, .. } = e {
            output.extend(bytes);
        }
        String::from_utf8_lossy(&output).contains("SHELL_STILL_OK")
    });
    assert_eq!(server.accepted_authentications(), 1);
}

#[test]
fn single_jump_host_authenticates_each_hop_and_opens_files() {
    let gateway = SshServer::start(true);
    let target = SshServer::start(true);
    std::fs::write(
        gateway.root.join("authorized_keys"),
        std::fs::read(target.root.join("client.pub")).unwrap(),
    )
    .unwrap();
    let mut config = target.config(51, true);
    let gateway_pub = std::fs::read_to_string(gateway.root.join("host.pub")).unwrap();
    use std::io::Write;
    writeln!(
        std::fs::OpenOptions::new()
            .append(true)
            .open(&config.app_known_hosts)
            .unwrap(),
        "[127.0.0.1]:{} {gateway_pub}",
        gateway.port
    )
    .unwrap();
    config.session.jump = format!("{}@127.0.0.1:{}", config.session.user, gateway.port);
    let mut handle = ConnectionHandle::connect(config).unwrap();
    collect_until(&mut handle, |e| matches!(e, ConnectionEvent::Directory(_)));
    assert_eq!(gateway.accepted_authentications(), 1);
    assert_eq!(target.accepted_authentications(), 1);
}

#[test]
fn full_output_queue_cannot_prevent_cancellation() {
    let server = SshServer::start(true);
    let mut config = server.config(52, true);
    config.session.command = "yes".into();
    let handle = ConnectionHandle::connect(config).unwrap();
    std::thread::sleep(Duration::from_millis(400));
    assert!(
        handle
            .try_send(ConnectionCommand::Input(vec![0; 1024 * 1024 + 1]))
            .is_err()
    );
    handle.cancel();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !handle.is_finished() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(handle.is_finished());
}

fn assert_host_policy_rejected(
    config: harbor_workspace::ssh::ConnectionConfig,
    server: &SshServer,
) {
    let mut handle = ConnectionHandle::connect(config).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut rejected = false;
    while Instant::now() < deadline {
        for event in handle.poll() {
            match event {
                ConnectionEvent::Connected { .. } => panic!("Host policy was bypassed"),
                ConnectionEvent::Challenge { .. } => {
                    panic!("Existing host policy was incorrectly treated as unknown")
                }
                ConnectionEvent::Exited { .. } => {
                    rejected = true;
                    break;
                }
                _ => {}
            }
        }
        if rejected {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(rejected);
    assert_eq!(server.accepted_authentications(), 0);
}
#[test]
fn revoked_host_key_overrides_a_matching_app_pin() {
    let server = SshServer::start(true);
    let mut config = server.config(61, true);
    let revoked = server.root.join("revoked");
    let key = std::fs::read_to_string(server.root.join("host.pub")).unwrap();
    std::fs::write(
        &revoked,
        format!("@revoked [127.0.0.1]:{} {key}", server.port),
    )
    .unwrap();
    config.user_known_hosts.push(revoked);
    assert_host_policy_rejected(config, &server);
}
#[test]
fn tab_separated_wildcard_host_pin_is_enforced() {
    let server = SshServer::start(true);
    let config = server.config(62, false);
    let wrong = std::fs::read_to_string(server.root.join("client.pub")).unwrap();
    std::fs::write(
        &config.app_known_hosts,
        format!("[127.*]:{}\t{}", server.port, wrong.replace(' ', "\t")),
    )
    .unwrap();
    assert_host_policy_rejected(config, &server);
}
#[test]
fn exit_reason_survives_a_saturated_event_queue() {
    let server = SshServer::start(true);
    let mut config = server.config(63, true);
    config.session.command = "yes".into();
    let mut handle = ConnectionHandle::connect(config).unwrap();
    std::thread::sleep(Duration::from_millis(600));
    handle.cancel();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !handle.is_finished() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(handle.is_finished());
    assert!(
        handle.poll().iter().any(
            |e| matches!(e,ConnectionEvent::Exited{message,..} if message.contains("cancelled"))
        )
    );
}

#[test]
fn stalled_shell_write_does_not_block_sftp_navigation() {
    let server = SshServer::start(true);
    let mut config = server.config(64, true);
    config.session.command = "stty -echo; sleep 30".into();
    let path = RemotePath::from_bytes(server.root.as_os_str().as_encoded_bytes().to_vec()).unwrap();
    let mut handle = ConnectionHandle::connect(config).unwrap();
    collect_until(&mut handle, |e| matches!(e, ConnectionEvent::Directory(_)));
    let deadline = Instant::now() + Duration::from_millis(800);
    while Instant::now() < deadline {
        let _ = handle.poll(); // Drain output as a live UI does while entering input.
        let _ = handle.try_send(ConnectionCommand::Input(vec![b'x'; 64 * 1024]));
        std::thread::sleep(Duration::from_millis(2));
    }
    handle
        .try_send(ConnectionCommand::List { request: 9, path })
        .unwrap();
    let start = Instant::now();
    collect_until(
        &mut handle,
        |e| matches!(e,ConnectionEvent::Directory(s) if s.request==9),
    );
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[test]
#[cfg(target_os = "linux")]
fn closed_sftp_channel_can_reopen_without_reauthenticating() {
    let server = SshServer::start(true);
    let config = server.config(65, true);
    let path = RemotePath::from_bytes(server.root.as_os_str().as_encoded_bytes().to_vec()).unwrap();
    let mut handle = ConnectionHandle::connect(config).unwrap();
    collect_until(&mut handle, |e| matches!(e, ConnectionEvent::Directory(_)));
    server.terminate_owned_sftp();
    std::thread::sleep(Duration::from_millis(100));
    handle
        .try_send(ConnectionCommand::List {
            request: 1,
            path: path.clone(),
        })
        .unwrap();
    collect_until(&mut handle, |e| {
        matches!(e, ConnectionEvent::FilesError { request: 1, .. })
    });
    handle
        .try_send(ConnectionCommand::List { request: 2, path })
        .unwrap();
    collect_until(
        &mut handle,
        |e| matches!(e,ConnectionEvent::Directory(s) if s.request==2),
    );
    assert_eq!(server.accepted_authentications(), 1);
}
#[test]
fn bounded_output_poll_preserves_complete_ssh_tail_and_exit() {
    let server = SshServer::start(true);
    let mut config = server.config(611, true);
    config.session.command =
        r#"python3 -c 'import os; os.write(1, b"BUDGET_START" + b"x"*65536 + b"BUDGET_END")'"#
            .into();
    let mut handle = ConnectionHandle::connect(config).unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut output = Vec::new();
    let mut exited = false;
    while !(exited && handle.output_drained()) {
        let mut bytes_this_tick = 0;
        for event in handle.poll_with_output_budget(257) {
            match event {
                ConnectionEvent::Output { bytes, .. } => {
                    bytes_this_tick += bytes.len();
                    output.extend(bytes);
                }
                ConnectionEvent::Exited { .. } => exited = true,
                ConnectionEvent::Challenge { .. } => {
                    panic!("trusted owned key fixture must not prompt")
                }
                _ => {}
            }
        }
        assert!(bytes_this_tick <= 257);
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    let expected = [
        b"BUDGET_START".as_slice(),
        &vec![b'x'; 65536],
        b"BUDGET_END",
    ]
    .concat();
    assert!(
        output
            .windows(expected.len())
            .any(|bytes| bytes == expected)
    );
    assert_eq!(server.accepted_authentications(), 1);
}
