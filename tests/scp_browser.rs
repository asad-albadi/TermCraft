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
        capabilities::RemoteBackendKind,
        transfer::{ConflictPolicy, TransferOperation, TransferOutcome},
    },
    ssh::{ConnectionEvent, ConnectionHandle},
};
use std::{
    path::Path,
    time::{Duration, Instant},
};
fn path(p: &Path) -> RemotePath {
    use std::os::unix::ffi::OsStrExt;
    RemotePath::from_bytes(p.as_os_str().as_bytes().to_vec()).unwrap()
}
fn connect(server: &ssh_server::SshServer) -> ConnectionHandle {
    let mut config = server.config(50, true);
    config.session.remote_backend = RemoteBackendKind::Scp;
    ConnectionHandle::connect(config).unwrap()
}
fn listing(
    h: &mut ConnectionHandle,
) -> Result<harbor_workspace::remote_files::DirectorySnapshot, String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        for e in h.poll() {
            match e {
                ConnectionEvent::Directory(s) => return Ok(s),
                ConnectionEvent::FilesError { message, .. }
                | ConnectionEvent::Exited { message, .. } => return Err(message),
                _ => {}
            }
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn finish_event(
    h: &mut ConnectionHandle,
    id: u64,
) -> harbor_workspace::remote_files::transfer::TransferEvent {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        for e in h.transfers.poll() {
            if e.id == id && e.outcome.is_some() {
                return e;
            }
        }
        h.poll();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn finish(h: &mut ConnectionHandle, id: u64) -> TransferOutcome {
    finish_event(h, id).outcome.unwrap()
}

#[test]
fn sftp_disabled_endpoint_supports_explicit_scp() {
    let server = ssh_server::SshServer::start(false);
    let mut h = connect(&server);
    assert!(!listing(&mut h).unwrap().path.as_bytes().is_empty());
    let source = server.root.join("source");
    let remote = server.root.join("remote");
    let local = server.root.join("download");
    std::fs::write(&source, b"SCP_OK").unwrap();
    let id = h
        .transfers
        .submit(TransferOperation::Upload {
            local: source,
            remote: path(&remote),
            policy: ConflictPolicy::Ask,
        })
        .unwrap();
    assert_eq!(finish(&mut h, id), TransferOutcome::Completed);
    let id = h
        .transfers
        .submit(TransferOperation::Download {
            remote: path(&remote),
            local: local.clone(),
            policy: ConflictPolicy::Ask,
        })
        .unwrap();
    assert_eq!(finish(&mut h, id), TransferOutcome::Completed);
    assert_eq!(std::fs::read(local).unwrap(), b"SCP_OK");
    assert_eq!(server.accepted_authentications(), 1);
}
#[test]
fn scp_names_cannot_be_interpreted_as_commands() {
    let server = ssh_server::SshServer::start(false);
    let mut h = connect(&server);
    listing(&mut h).unwrap();
    let source = server.root.join("source");
    let side_effect = server.root.join("pwn");
    // Octal separators keep this a single filename; an accidental shell evaluation would touch only our owned fixture.
    let escaped = side_effect.to_str().unwrap().replace('/', "\\057");
    let remote = server
        .root
        .join(format!("literal ' é\n$(touch \"$(printf '{escaped}')\")"));
    std::fs::write(&source, b"SCP_OK").unwrap();
    let id = h
        .transfers
        .submit(TransferOperation::Upload {
            local: source,
            remote: path(&remote),
            policy: ConflictPolicy::Ask,
        })
        .unwrap();
    assert_eq!(finish(&mut h, id), TransferOutcome::Completed);
    let local = server.root.join("downloaded");
    let id = h
        .transfers
        .submit(TransferOperation::Download {
            remote: path(&remote),
            local: local.clone(),
            policy: ConflictPolicy::Ask,
        })
        .unwrap();
    assert_eq!(finish(&mut h, id), TransferOutcome::Completed);
    assert_eq!(std::fs::read(local).unwrap(), b"SCP_OK");
    assert!(!side_effect.exists());
    h.try_send(harbor_workspace::ssh::ConnectionCommand::List {
        request: 1,
        path: path(&server.root),
    })
    .unwrap();
    assert!(
        listing(&mut h)
            .unwrap()
            .entries
            .iter()
            .any(|e| e.path == path(&remote))
    );
}
#[test]
fn unsupported_enumerator_reports_capability_error() {
    let server =
        ssh_server::SshServer::start_with_settings(false, "", "SetEnv PATH=/termcraft-no-helpers");
    let mut h = connect(&server);
    let error =
        listing(&mut h).expect_err("A missing helper cannot produce a fake empty directory");
    assert!(error.contains("SCP") && error.contains("helper"), "{error}");
}
#[test]
fn scp_enumeration_failure_retains_the_remote_reason() {
    let server = ssh_server::SshServer::start(false);
    let mut h = connect(&server);
    listing(&mut h).unwrap();
    let file = server.root.join("not-a-directory");
    std::fs::write(&file, b"file").unwrap();
    h.try_send(harbor_workspace::ssh::ConnectionCommand::List {
        request: 21,
        path: path(&file),
    })
    .unwrap();
    let error = listing(&mut h).unwrap_err();
    assert!(
        error.contains("Not a directory"),
        "Helper diagnostic was discarded: {error}"
    );
}
#[test]
fn special_entry_download_is_rejected_before_local_staging() {
    let server = ssh_server::SshServer::start(false);
    let mut h = connect(&server);
    listing(&mut h).unwrap();
    let socket = server.root.join("unsupported-socket");
    let _listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    let target = server.root.join("special-download");
    let id = h
        .transfers
        .submit(TransferOperation::Download {
            remote: path(&socket),
            local: target,
            policy: ConflictPolicy::Ask,
        })
        .unwrap();
    assert!(matches!(finish(&mut h, id), TransferOutcome::Failed(_)));
    assert!(
        !std::fs::read_dir(&server.root).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".termcraft-")),
        "Unsupported special entry download created a misleading recovery file"
    );
}
#[test]
fn explicit_scp_reports_negotiated_endpoint_capabilities() {
    let server = ssh_server::SshServer::start(false);
    let mut h = connect(&server);
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut capabilities = None;
    loop {
        let mut listed = false;
        for event in h.poll() {
            match event {
                ConnectionEvent::FilesCapabilities {
                    connection,
                    capabilities: value,
                } => {
                    assert_eq!(connection, 50);
                    capabilities = Some(value);
                }
                ConnectionEvent::Directory(_) => listed = true,
                ConnectionEvent::Exited { message, .. }
                | ConnectionEvent::FilesError { message, .. } => panic!("{message}"),
                _ => {}
            }
        }
        if listed {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    let capabilities =
        capabilities.expect("A valid listing must identify actual endpoint capabilities");
    assert!(capabilities.browse && capabilities.upload_files && capabilities.download_files);
    assert!(capabilities.exclusive_create);
    assert!(capabilities.upload_folders && capabilities.download_folders);
    assert!(capabilities.recursive_remove);
    assert!(capabilities.edit);
    assert!(capabilities.mutate_entries);
}
#[test]
fn silent_terminal_does_not_delay_initial_scp_listing() {
    let server = ssh_server::SshServer::start(false);
    let mut config = server.config(51, true);
    config.session.remote_backend = RemoteBackendKind::Scp;
    config.session.command = "exec sleep 5".into();
    let mut h = ConnectionHandle::connect(config).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if h.poll()
            .iter()
            .any(|e| matches!(e, ConnectionEvent::Directory(_)))
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Initial listing was waiting on terminal output"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!h.is_finished());
}
#[cfg(target_os = "linux")]
#[test]
fn virtual_file_sizes_cannot_silently_publish_empty_content() {
    let virtual_file = Path::new("/proc/version");
    assert_eq!(std::fs::metadata(virtual_file).unwrap().len(), 0);
    assert!(!std::fs::read(virtual_file).unwrap().is_empty());
    let server = ssh_server::SshServer::start(false);
    let mut h = connect(&server);
    listing(&mut h).unwrap();
    let local = server.root.join("keep-local");
    let remote = server.root.join("keep-remote");
    std::fs::write(&local, b"keep").unwrap();
    std::fs::write(&remote, b"keep").unwrap();
    let id = h
        .transfers
        .submit(TransferOperation::Upload {
            local: virtual_file.to_owned(),
            remote: path(&remote),
            policy: ConflictPolicy::Overwrite,
        })
        .unwrap();
    let upload = finish(&mut h, id);
    let id = h
        .transfers
        .submit(TransferOperation::Download {
            remote: path(virtual_file),
            local: local.clone(),
            policy: ConflictPolicy::Overwrite,
        })
        .unwrap();
    let download = finish(&mut h, id);
    assert_ne!(
        upload,
        TransferOutcome::Completed,
        "Zero-sized readable virtual source was uploaded as empty"
    );
    assert_ne!(
        download,
        TransferOutcome::Completed,
        "Zero-sized readable virtual source was downloaded as empty"
    );
    assert_eq!(std::fs::read(local).unwrap(), b"keep");
    assert_eq!(std::fs::read(remote).unwrap(), b"keep");
}
#[test]
fn completed_scp_staging_does_not_authorize_remote_publication_on_eof() {
    use std::{
        io::{Read, Write},
        process::{Command, Stdio},
    };
    let server = ssh_server::SshServer::start(false);
    let target = server.root.join("keep");
    std::fs::write(&target, b"keep").unwrap();
    use std::os::unix::ffi::OsStrExt;
    let hex = target
        .as_os_str()
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let mut child = Command::new("python3")
        .args([
            "-c",
            include_str!("../src/remote_files/scp/helper.py"),
            "put",
            &hex,
            "overwrite",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = child.stdout.take().unwrap();
    let mut ack = [0];
    output.read_exact(&mut ack).unwrap();
    assert_eq!(ack, [0]);
    let mut staging_line = Vec::new();
    loop {
        let mut byte = [0];
        output.read_exact(&mut byte).unwrap();
        if byte == [b'\n'] {
            break;
        }
        staging_line.push(byte[0]);
    }
    assert!(staging_line.starts_with(b"TCSTAGING:"));
    input.write_all(b"C0600 4 payload\n").unwrap();
    input.flush().unwrap();
    output.read_exact(&mut ack).unwrap();
    assert_eq!(ack, [0]);
    input.write_all(b"mine\0").unwrap();
    input.flush().unwrap();
    output.read_exact(&mut ack).unwrap();
    assert_eq!(ack, [0]);
    drop(input);
    child.wait().unwrap();
    assert_eq!(
        std::fs::read(target).unwrap(),
        b"keep",
        "EOF after staging is cancellation, not commit authority"
    );
}
#[test]
fn moving_a_remote_upload_parent_cannot_publish_or_clean_up_unrelated_objects() {
    let server = ssh_server::SshServer::start(false);
    let mut h = connect(&server);
    listing(&mut h).unwrap();
    let parent = server.root.join("remote-parent");
    let moved = server.root.join("remote-moved");
    std::fs::create_dir(&parent).unwrap();
    let target = parent.join("target");
    std::fs::write(&target, b"original").unwrap();
    let source = server.root.join("source-large");
    std::fs::write(&source, vec![b'm'; 8 * 1024 * 1024]).unwrap();
    let watched = parent.clone();
    let destination = moved.clone();
    let watcher = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            for entry in std::fs::read_dir(&watched).unwrap() {
                let entry = entry.unwrap();
                if entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".termcraft-scp-")
                    && std::fs::metadata(entry.path().join("payload")).is_ok_and(|m| m.len() > 0)
                {
                    let name = entry.file_name();
                    std::fs::rename(&watched, &destination).unwrap();
                    std::fs::create_dir(&watched).unwrap();
                    std::fs::create_dir(watched.join(&name)).unwrap();
                    std::fs::write(watched.join(&name).join("payload"), b"attacker").unwrap();
                    std::fs::write(watched.join(&name).join("sentinel"), b"unrelated-stage")
                        .unwrap();
                    std::fs::write(watched.join("target"), b"unrelated-target").unwrap();
                    return name;
                }
            }
            assert!(
                Instant::now() < deadline,
                "No remote staging bytes observed"
            );
            std::thread::sleep(Duration::from_micros(100));
        }
    });
    let id = h
        .transfers
        .submit(TransferOperation::Upload {
            local: source,
            remote: path(&target),
            policy: ConflictPolicy::Overwrite,
        })
        .unwrap();
    let outcome = finish(&mut h, id);
    let stage = watcher.join().unwrap();
    assert_ne!(outcome, TransferOutcome::Completed);
    assert_eq!(
        std::fs::read(parent.join("target")).unwrap(),
        b"unrelated-target"
    );
    assert_eq!(
        std::fs::read(parent.join(&stage).join("sentinel")).unwrap(),
        b"unrelated-stage"
    );
    assert_eq!(std::fs::read(moved.join("target")).unwrap(), b"original");
}
#[test]
fn moving_a_local_download_parent_cannot_publish_a_replaced_staging_name() {
    let server = ssh_server::SshServer::start(false);
    let mut h = connect(&server);
    listing(&mut h).unwrap();
    let remote = server.root.join("remote-large");
    std::fs::write(&remote, vec![b'm'; 8 * 1024 * 1024]).unwrap();
    let parent = server.root.join("local-parent");
    let moved = server.root.join("local-moved");
    std::fs::create_dir(&parent).unwrap();
    let target = parent.join("target");
    std::fs::write(&target, b"original").unwrap();
    let watched = parent.clone();
    let destination = moved.clone();
    let watcher = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            for entry in std::fs::read_dir(&watched).unwrap() {
                let entry = entry.unwrap();
                if entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".termcraft-")
                    && entry.metadata().is_ok_and(|m| m.len() > 0)
                {
                    let name = entry.file_name();
                    std::fs::rename(&watched, &destination).unwrap();
                    std::fs::create_dir(&watched).unwrap();
                    std::fs::write(watched.join(&name), b"attacker").unwrap();
                    std::fs::write(watched.join("target"), b"unrelated-target").unwrap();
                    return name;
                }
            }
            assert!(Instant::now() < deadline, "No local staging bytes observed");
            std::thread::sleep(Duration::from_micros(100));
        }
    });
    let id = h
        .transfers
        .submit(TransferOperation::Download {
            remote: path(&remote),
            local: target.clone(),
            policy: ConflictPolicy::Overwrite,
        })
        .unwrap();
    let outcome = finish(&mut h, id);
    let stage = watcher.join().unwrap();
    assert_ne!(outcome, TransferOutcome::Completed);
    assert_eq!(std::fs::read(target).unwrap(), b"unrelated-target");
    assert_eq!(std::fs::read(parent.join(stage)).unwrap(), b"attacker");
    assert_eq!(std::fs::read(moved.join("target")).unwrap(), b"original");
}

#[test]
fn cancelling_scp_exec_startup_does_not_exhaust_session_channels() {
    let server = auth_server::AuthServer::start(auth_server::Mode::ScpStartup);
    let config = server.config();
    let source = config.app_known_hosts.with_file_name("startup-source");
    std::fs::write(&source, b"source").unwrap();
    let mut h = ConnectionHandle::connect(config).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while server.startup_requests() < 1 {
        h.poll();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    for _ in 0..4 {
        let expected = server.startup_requests() + 1;
        let id = h
            .transfers
            .submit(TransferOperation::Upload {
                local: source.clone(),
                remote: RemotePath::from_bytes(b"/fixture-destination".to_vec()).unwrap(),
                policy: ConflictPolicy::Ask,
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while server.startup_requests() < expected {
            h.poll();
            assert!(Instant::now() < deadline, "Session capacity was exhausted");
            std::thread::sleep(Duration::from_millis(5));
        }
        h.transfers.cancel(id);
        assert_eq!(finish(&mut h, id), TransferOutcome::Cancelled);
        let deadline = Instant::now() + Duration::from_secs(2);
        while server.active_startup_helpers() > 1 {
            h.poll();
            assert!(
                Instant::now() < deadline,
                "Cancelled exec startup leaked its raw SSH channel"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    server.allow_startup_helpers();
    assert!(!listing(&mut h).unwrap().path.as_bytes().is_empty());
    assert!(!h.is_finished());
    assert_eq!(server.count(), 1);
}

#[test]
fn late_scp_conflicts_are_typed_and_keep_complete_remote_recovery() {
    use std::os::unix::ffi::OsStrExt;
    let server = ssh_server::SshServer::start(false);
    let mut h = connect(&server);
    listing(&mut h).unwrap();
    let source = server.root.join("late-conflict-source");
    std::fs::write(&source, vec![b'm'; 8 * 1024 * 1024]).unwrap();
    for policy in [ConflictPolicy::Ask, ConflictPolicy::Skip] {
        let target = server.root.join(format!("late-{policy:?}"));
        let known = std::fs::read_dir(&server.root)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect::<std::collections::HashSet<_>>();
        let parent = server.root.clone();
        let racing = target.clone();
        let watcher = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if std::fs::read_dir(&parent).unwrap().any(|e| {
                    let e = e.unwrap();
                    !known.contains(&e.file_name())
                        && e.file_name()
                            .to_string_lossy()
                            .starts_with(".termcraft-scp-")
                        && std::fs::metadata(e.path().join("payload")).is_ok_and(|m| m.len() > 0)
                }) {
                    std::fs::write(&racing, b"racing-version").unwrap();
                    return;
                }
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_micros(100));
            }
        });
        let id = h
            .transfers
            .submit(TransferOperation::Upload {
                local: source.clone(),
                remote: path(&target),
                policy,
            })
            .unwrap();
        let event = finish_event(&mut h, id);
        watcher.join().unwrap();
        assert_eq!(
            event.outcome,
            Some(if policy == ConflictPolicy::Skip {
                TransferOutcome::Skipped
            } else {
                TransferOutcome::Conflict
            })
        );
        assert_eq!(std::fs::read(target).unwrap(), b"racing-version");
        let Some(harbor_workspace::remote_files::transfer::RecoveryPath::Remote(recovery)) =
            event.recovery
        else {
            panic!("Complete conflicting upload had no recovery path");
        };
        assert_eq!(
            std::fs::metadata(Path::new(std::ffi::OsStr::from_bytes(recovery.as_bytes())))
                .unwrap()
                .len(),
            8 * 1024 * 1024
        );
    }
}
#[test]
fn scp_upload_preserves_ordinary_new_and_existing_modes() {
    use std::os::unix::fs::PermissionsExt;
    let server = ssh_server::SshServer::start(false);
    let mut h = connect(&server);
    listing(&mut h).unwrap();
    let source = server.root.join("mode-source");
    std::fs::write(&source, b"SCP_OK").unwrap();
    std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o777)).unwrap();
    let remote = server.root.join("mode-remote");
    let id = h
        .transfers
        .submit(TransferOperation::Upload {
            local: source.clone(),
            remote: path(&remote),
            policy: ConflictPolicy::Ask,
        })
        .unwrap();
    assert_eq!(finish(&mut h, id), TransferOutcome::Completed);
    assert_eq!(
        std::fs::metadata(&remote).unwrap().permissions().mode() & 0o777,
        0o777
    );
    std::fs::set_permissions(&remote, std::fs::Permissions::from_mode(0o640)).unwrap();
    let id = h
        .transfers
        .submit(TransferOperation::Upload {
            local: source,
            remote: path(&remote),
            policy: ConflictPolicy::Overwrite,
        })
        .unwrap();
    assert_eq!(finish(&mut h, id), TransferOutcome::Completed);
    assert_eq!(
        std::fs::metadata(&remote).unwrap().permissions().mode() & 0o777,
        0o640
    );
}
#[cfg(target_os = "linux")]
#[test]
fn lost_remote_commit_acknowledgement_is_indeterminate_after_real_publication() {
    let temporary = std::env::temp_dir().join(format!(
        "termcraft-scp-ack-fault-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&temporary).unwrap();
    let library = temporary.join("scp-ack-fault.so");
    let status = std::process::Command::new("cc")
        .args(["-shared", "-fPIC", "-O2"])
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/scp_ack_fault.c"))
        .arg("-o")
        .arg(&library)
        .arg("-ldl")
        .status()
        .unwrap();
    assert!(status.success());
    let server = ssh_server::SshServer::start_with_settings(
        false,
        "",
        &format!("SetEnv LD_PRELOAD={}", library.display()),
    );
    let mut h = connect(&server);
    listing(&mut h).unwrap();
    let source = server.root.join("ack-source");
    let target = server.root.join("lost-ack-target");
    std::fs::write(&source, b"COMMITTED").unwrap();
    std::fs::write(&target, b"original").unwrap();
    let id = h
        .transfers
        .submit(TransferOperation::Upload {
            local: source.clone(),
            remote: path(&target),
            policy: ConflictPolicy::Overwrite,
        })
        .unwrap();
    let outcome = finish(&mut h, id);
    assert!(
        matches!(outcome, TransferOutcome::Indeterminate(_)),
        "A lost reply cannot claim rollback or failure after publication: {outcome:?}"
    );
    assert_eq!(std::fs::read(target).unwrap(), b"COMMITTED");
    assert_eq!(std::fs::read(source).unwrap(), b"COMMITTED");
    assert!(!h.is_finished());
    assert_eq!(server.accepted_authentications(), 1);
    drop(h);
    drop(server);
    std::fs::remove_dir_all(temporary).unwrap();
}
#[test]
fn scp_remote_editor_uses_versioned_conflicts_and_local_recovery() {
    use harbor_workspace::remote_files::editor::{
        CloseDecision, DocumentStatus, RemoteDocument, SavePolicy,
    };
    fn wait(document: &RemoteDocument, expected: DocumentStatus) {
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            let state = document.snapshot();
            if state.status == expected {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "Expected {expected:?}, got {:?}",
                state.status
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    let server = ssh_server::SshServer::start(false);
    let h = connect(&server);
    let remote = server.root.join("edit ' é\n$(literal).txt");
    std::fs::write(&remote, b"v1").unwrap();
    let mut document = RemoteDocument::open(
        50,
        path(&remote),
        &h.transfers,
        server.root.join("editor-recovery"),
    )
    .unwrap();
    wait(&document, DocumentStatus::Ready);
    document.set_text("mine").unwrap();
    assert!(!document.request_close(CloseDecision::Cancel).unwrap());
    std::fs::write(&remote, b"v2").unwrap();
    document.save(SavePolicy::VerifyUnchanged).unwrap();
    wait(&document, DocumentStatus::Conflict);
    assert_eq!(std::fs::read(&remote).unwrap(), b"v2");
    assert_eq!(
        std::fs::read(document.snapshot().recovery_path.unwrap()).unwrap(),
        b"mine"
    );
    std::fs::write(&remote, b"v3").unwrap();
    document.save(SavePolicy::Overwrite).unwrap();
    wait(&document, DocumentStatus::Conflict);
    assert_eq!(std::fs::read(&remote).unwrap(), b"v3");
    document.save(SavePolicy::Overwrite).unwrap();
    wait(&document, DocumentStatus::Saved);
    assert_eq!(std::fs::read(&remote).unwrap(), b"mine");
    document.set_text("next revision").unwrap();
    document.save(SavePolicy::VerifyUnchanged).unwrap();
    wait(&document, DocumentStatus::Saved);
    assert_eq!(std::fs::read(&remote).unwrap(), b"next revision");
    assert!(!document.snapshot().dirty);
    assert_eq!(server.accepted_authentications(), 1);
}
#[test]
fn scp_entry_management_keeps_confirmation_and_no_replace_semantics() {
    let server = ssh_server::SshServer::start(false);
    let mut h = connect(&server);
    listing(&mut h).unwrap();
    let directory = server.root.join("new ' é\nfolder");
    let moved = server.root.join("renamed folder");
    let id = h
        .transfers
        .submit(TransferOperation::Mkdir {
            remote: path(&directory),
        })
        .unwrap();
    assert_eq!(finish(&mut h, id), TransferOutcome::Completed);
    let id = h
        .transfers
        .submit(TransferOperation::Rename {
            source: path(&directory),
            destination: path(&moved),
            policy: ConflictPolicy::Ask,
        })
        .unwrap();
    assert_eq!(finish(&mut h, id), TransferOutcome::Completed);
    assert!(!directory.exists());
    let file = moved.join("keep");
    let other = moved.join("other");
    std::fs::write(&file, b"keep").unwrap();
    std::fs::write(&other, b"other").unwrap();
    let id = h
        .transfers
        .submit(TransferOperation::Rename {
            source: path(&file),
            destination: path(&other),
            policy: ConflictPolicy::Ask,
        })
        .unwrap();
    assert_eq!(finish(&mut h, id), TransferOutcome::Conflict);
    assert_eq!(std::fs::read(&other).unwrap(), b"other");
    let id = h
        .transfers
        .submit(TransferOperation::Remove {
            remote: path(&file),
            confirmed: path(&other),
        })
        .unwrap();
    assert!(matches!(finish(&mut h, id), TransferOutcome::Failed(_)));
    assert_eq!(std::fs::read(&file).unwrap(), b"keep");
    for file in [&file, &other] {
        let id = h
            .transfers
            .submit(TransferOperation::Remove {
                remote: path(file),
                confirmed: path(file),
            })
            .unwrap();
        assert_eq!(finish(&mut h, id), TransferOutcome::Completed);
    }
    let id = h
        .transfers
        .submit(TransferOperation::Remove {
            remote: path(&moved),
            confirmed: path(&moved),
        })
        .unwrap();
    assert_eq!(finish(&mut h, id), TransferOutcome::Completed);
    assert!(!moved.exists());
    assert_eq!(server.accepted_authentications(), 1);
}
#[test]
fn scp_folder_round_trip_preserves_binary_names_empty_directories_and_cycles() {
    let server = ssh_server::SshServer::start(false);
    let mut h = connect(&server);
    listing(&mut h).unwrap();
    let source = server.root.join("source-folder");
    let remote = server.root.join("remote-folder");
    let local = server.root.join("download-folder");
    std::fs::create_dir_all(source.join("empty")).unwrap();
    std::fs::create_dir(source.join("sub ' é\n$(literal)")).unwrap();
    let bytes = (0..100_000).map(|n| (n % 256) as u8).collect::<Vec<_>>();
    std::fs::write(source.join("sub ' é\n$(literal)/binary"), &bytes).unwrap();
    std::fs::write(source.join("empty-file"), b"").unwrap();
    std::os::unix::fs::symlink("cycle", source.join("cycle")).unwrap();
    let id = h
        .transfers
        .submit(TransferOperation::Upload {
            local: source,
            remote: path(&remote),
            policy: ConflictPolicy::Ask,
        })
        .unwrap();
    assert_eq!(finish(&mut h, id), TransferOutcome::Completed);
    let id = h
        .transfers
        .submit(TransferOperation::Download {
            remote: path(&remote),
            local: local.clone(),
            policy: ConflictPolicy::Ask,
        })
        .unwrap();
    assert_eq!(finish(&mut h, id), TransferOutcome::Completed);
    assert_eq!(
        std::fs::read(local.join("sub ' é\n$(literal)/binary")).unwrap(),
        bytes
    );
    assert!(local.join("empty").is_dir());
    assert!(local.join("empty-file").is_file());
    assert_eq!(
        std::fs::read_link(local.join("cycle")).unwrap(),
        Path::new("cycle")
    );
    assert_eq!(server.accepted_authentications(), 1);
}

#[cfg(target_os = "linux")]
#[test]
fn malformed_remote_editor_commit_reply_is_indeterminate() {
    use harbor_workspace::remote_files::editor::{DocumentStatus, RemoteDocument, SavePolicy};
    let temporary = std::env::temp_dir().join(format!(
        "termcraft-scp-malformed-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&temporary).unwrap();
    let library = temporary.join("fault.so");
    assert!(
        std::process::Command::new("cc")
            .args(["-shared", "-fPIC", "-O2"])
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/scp_ack_fault.c"))
            .arg("-o")
            .arg(&library)
            .arg("-ldl")
            .status()
            .unwrap()
            .success()
    );
    let server = ssh_server::SshServer::start_with_settings(
        false,
        "",
        &format!("SetEnv LD_PRELOAD={}", library.display()),
    );
    let h = connect(&server);
    let remote = server.root.join("malformed-ack-target");
    std::fs::write(&remote, b"before").unwrap();
    let mut document = RemoteDocument::open(
        50,
        path(&remote),
        &h.transfers,
        server.root.join("recovery"),
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while document.snapshot().status != DocumentStatus::Ready {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    document.set_text("committed").unwrap();
    document.save(SavePolicy::VerifyUnchanged).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while document.snapshot().status.busy() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(std::fs::read(&remote).unwrap(), b"committed");
    assert!(
        matches!(document.snapshot().status, DocumentStatus::Indeterminate(_)),
        "{:?}",
        document.snapshot().status
    );
    assert!(document.snapshot().dirty);
    drop(h);
    drop(server);
    std::fs::remove_dir_all(temporary).unwrap();
}
#[test]
fn scp_recursive_delete_does_not_follow_links_and_requires_exact_confirmation() {
    let server = ssh_server::SshServer::start(false);
    let mut h = connect(&server);
    listing(&mut h).unwrap();
    let root = server.root.join("delete-tree");
    let outside = server.root.join("outside-delete");
    std::fs::create_dir_all(root.join("child/empty")).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("sentinel"), b"keep").unwrap();
    std::fs::write(root.join("child/file"), b"remove").unwrap();
    std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
    let id = h
        .transfers
        .submit(TransferOperation::RemoveTree {
            remote: path(&root),
            confirmed: path(&outside),
        })
        .unwrap();
    assert!(matches!(finish(&mut h, id), TransferOutcome::Failed(_)));
    assert!(root.exists());
    let id = h
        .transfers
        .submit(TransferOperation::RemoveTree {
            remote: path(&root),
            confirmed: path(&root),
        })
        .unwrap();
    let event = finish_event(&mut h, id);
    assert_eq!(event.outcome, Some(TransferOutcome::Completed));
    assert!(!root.exists());
    assert_eq!(std::fs::read(outside.join("sentinel")).unwrap(), b"keep");
    assert_eq!(event.completed_entries, 5);
}
#[test]
fn scp_folder_merge_preserves_unrelated_entries_and_existing_modes() {
    use std::os::unix::fs::PermissionsExt;
    let server = ssh_server::SshServer::start(false);
    let mut h = connect(&server);
    listing(&mut h).unwrap();
    let source = server.root.join("merge-source");
    let remote = server.root.join("merge-remote");
    let local = server.root.join("merge-local");
    for root in [&source, &remote, &local] {
        std::fs::create_dir_all(root.join("child")).unwrap();
    }
    std::fs::write(source.join("child/file"), b"new").unwrap();
    std::fs::create_dir(source.join("empty")).unwrap();
    for root in [&remote, &local] {
        std::fs::write(root.join("child/file"), b"old").unwrap();
        std::fs::set_permissions(
            root.join("child/file"),
            std::fs::Permissions::from_mode(0o640),
        )
        .unwrap();
        std::fs::write(root.join("unrelated"), b"keep").unwrap();
    }
    let id = h
        .transfers
        .submit(TransferOperation::Upload {
            local: source,
            remote: path(&remote),
            policy: ConflictPolicy::Overwrite,
        })
        .unwrap();
    assert_eq!(finish(&mut h, id), TransferOutcome::Completed);
    let id = h
        .transfers
        .submit(TransferOperation::Download {
            remote: path(&remote),
            local: local.clone(),
            policy: ConflictPolicy::Overwrite,
        })
        .unwrap();
    assert_eq!(finish(&mut h, id), TransferOutcome::Completed);
    for root in [&remote, &local] {
        assert_eq!(std::fs::read(root.join("child/file")).unwrap(), b"new");
        assert_eq!(
            std::fs::metadata(root.join("child/file"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o640
        );
        assert_eq!(std::fs::read(root.join("unrelated")).unwrap(), b"keep");
        assert!(root.join("empty").is_dir());
    }
}
#[cfg(target_os = "linux")]
#[test]
fn scp_bulk_cancel_finishes_the_current_entry_and_preserves_the_rest() {
    let temporary = std::env::temp_dir().join(format!(
        "termcraft-scp-bulk-fault-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&temporary).unwrap();
    let library = temporary.join("fault.so");
    assert!(
        std::process::Command::new("cc")
            .args(["-shared", "-fPIC", "-O2"])
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/scp_ack_fault.c"))
            .arg("-o")
            .arg(&library)
            .arg("-ldl")
            .status()
            .unwrap()
            .success()
    );
    let server = ssh_server::SshServer::start_with_settings(
        false,
        "",
        &format!("SetEnv LD_PRELOAD={}", library.display()),
    );
    let mut h = connect(&server);
    listing(&mut h).unwrap();
    for deleting in [false, true] {
        let source = server.root.join(if deleting {
            "delete-source"
        } else {
            "merge-source"
        });
        let target = server.root.join(if deleting {
            "delete-target"
        } else {
            "merge-target"
        });
        std::fs::create_dir(&source).unwrap();
        std::fs::create_dir(&target).unwrap();
        for n in 0..100 {
            let name = format!("{}-step-{n}", if deleting { "delete" } else { "cancel" });
            std::fs::write(source.join(&name), b"new").unwrap();
            std::fs::write(target.join(name), b"old").unwrap();
        }
        let operation = if deleting {
            TransferOperation::RemoveTree {
                remote: path(&target),
                confirmed: path(&target),
            }
        } else {
            TransferOperation::Upload {
                local: source,
                remote: path(&target),
                policy: ConflictPolicy::Overwrite,
            }
        };
        let id = h.transfers.submit(operation).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !target.join(".termcraft-paused").exists() {
            let _ = h.transfers.poll();
            assert!(
                Instant::now() < deadline,
                "The owned helper did not reach the pause"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        h.transfers.cancel(id);
        std::fs::write(target.join(".termcraft-release"), b"continue").unwrap();
        let event = finish_event(&mut h, id);
        assert_eq!(event.outcome, Some(TransferOutcome::Cancelled));
        assert_eq!(event.completed_entries, 1);
        let entries = std::fs::read_dir(&target)
            .unwrap()
            .map(|e| e.unwrap())
            .filter(|e| !e.file_name().to_string_lossy().starts_with(".termcraft-"))
            .collect::<Vec<_>>();
        if deleting {
            assert_eq!(entries.len(), 99);
        } else {
            assert_eq!(
                entries
                    .iter()
                    .filter(|e| std::fs::read(e.path()).unwrap() == b"new")
                    .count(),
                1
            );
            assert_eq!(
                entries
                    .iter()
                    .filter(|e| std::fs::read(e.path()).unwrap() == b"old")
                    .count(),
                99
            );
        }
    }
    assert_eq!(server.accepted_authentications(), 1);
    drop(h);
    drop(server);
    std::fs::remove_dir_all(temporary).unwrap();
}
#[test]
fn scp_folder_policies_raw_names_and_modes_survive_round_trip() {
    use std::os::unix::{ffi::OsStringExt, fs::PermissionsExt};
    let server = ssh_server::SshServer::start(false);
    let mut h = connect(&server);
    listing(&mut h).unwrap();
    let source = server.root.join("policy-source");
    let remote = server.root.join("policy-remote");
    let local = server.root.join("policy-local");
    std::fs::create_dir_all(source.join("child")).unwrap();
    let raw = std::ffi::OsString::from_vec(b"raw-\xff-name".to_vec());
    std::fs::write(source.join("child").join(&raw), b"content").unwrap();
    for (path, mode) in [
        (source.clone(), 0o751),
        (source.join("child"), 0o711),
        (source.join("child").join(&raw), 0o444),
    ] {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    }
    let id = h
        .transfers
        .submit(TransferOperation::Upload {
            local: source.clone(),
            remote: path(&remote),
            policy: ConflictPolicy::Ask,
        })
        .unwrap();
    assert_eq!(finish(&mut h, id), TransferOutcome::Completed);
    let id = h
        .transfers
        .submit(TransferOperation::Download {
            remote: path(&remote),
            local: local.clone(),
            policy: ConflictPolicy::Ask,
        })
        .unwrap();
    assert_eq!(finish(&mut h, id), TransferOutcome::Completed);
    for root in [&remote, &local] {
        assert_eq!(
            std::fs::read(root.join("child").join(&raw)).unwrap(),
            b"content"
        );
        for (path, mode) in [
            (root.to_path_buf(), 0o751),
            (root.join("child"), 0o711),
            (root.join("child").join(&raw), 0o444),
        ] {
            assert_eq!(
                std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
                mode
            );
        }
    }
    for policy in [ConflictPolicy::Ask, ConflictPolicy::Skip] {
        let expected = if policy == ConflictPolicy::Ask {
            TransferOutcome::Conflict
        } else {
            TransferOutcome::Skipped
        };
        let id = h
            .transfers
            .submit(TransferOperation::Upload {
                local: source.clone(),
                remote: path(&remote),
                policy,
            })
            .unwrap();
        assert_eq!(finish(&mut h, id), expected);
        let id = h
            .transfers
            .submit(TransferOperation::Download {
                remote: path(&remote),
                local: local.clone(),
                policy,
            })
            .unwrap();
        assert_eq!(finish(&mut h, id), expected);
    }
}
#[test]
fn scp_folder_merges_never_follow_existing_destination_directory_links() {
    let server = ssh_server::SshServer::start(false);
    let mut h = connect(&server);
    listing(&mut h).unwrap();
    let source = server.root.join("alias-source");
    let remote = server.root.join("alias-remote");
    let local = server.root.join("alias-local");
    let outside = server.root.join("alias-outside");
    std::fs::create_dir_all(source.join("child")).unwrap();
    std::fs::write(source.join("child/sentinel"), b"replacement").unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("sentinel"), b"keep").unwrap();
    for target in [&remote, &local] {
        std::fs::create_dir(target).unwrap();
        std::os::unix::fs::symlink(&outside, target.join("child")).unwrap();
    }
    let id = h
        .transfers
        .submit(TransferOperation::Upload {
            local: source.clone(),
            remote: path(&remote),
            policy: ConflictPolicy::Overwrite,
        })
        .unwrap();
    assert!(matches!(finish(&mut h, id), TransferOutcome::Failed(_)));
    assert_eq!(std::fs::read(outside.join("sentinel")).unwrap(), b"keep");
    let id = h
        .transfers
        .submit(TransferOperation::Download {
            remote: path(&source),
            local,
            policy: ConflictPolicy::Overwrite,
        })
        .unwrap();
    assert!(matches!(finish(&mut h, id), TransferOutcome::Failed(_)));
    assert_eq!(std::fs::read(outside.join("sentinel")).unwrap(), b"keep");
}
