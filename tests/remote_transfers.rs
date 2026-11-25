#![cfg(unix)]
#[allow(dead_code)]
#[path = "support/ssh_server.rs"]
mod ssh_server;
use harbor_workspace::{
    remote_files::{
        RemotePath,
        transfer::{ConflictPolicy, TransferEvent, TransferOperation, TransferOutcome},
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
fn connected(server: &ssh_server::SshServer) -> ConnectionHandle {
    let mut h = ConnectionHandle::connect(server.config(92, true)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        for e in h.poll() {
            match e {
                ConnectionEvent::Directory(_) => return h,
                ConnectionEvent::Exited { message, .. } => panic!("{message}"),
                _ => {}
            }
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn finish(h: &mut ConnectionHandle, id: u64) -> TransferEvent {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        for e in h.transfers.poll() {
            if e.id == id && e.outcome.is_some() {
                return e;
            }
        }
        h.poll();
        assert!(Instant::now() < deadline, "transfer timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn literal_binary_file_round_trip_uses_sftp() {
    let server = ssh_server::SshServer::start(true);
    let mut h = connected(&server);
    let source = server.root.join("local-binary");
    let remote = server.root.join("remote ' é\n$(literal)");
    let dest = server.root.join("downloaded");
    let bytes = (0..150_000).map(|n| (n % 256) as u8).collect::<Vec<_>>();
    std::fs::write(&source, &bytes).unwrap();
    let id = h
        .transfers
        .submit(TransferOperation::Upload {
            local: source,
            remote: path(&remote),
            policy: ConflictPolicy::Ask,
        })
        .unwrap();
    assert_eq!(finish(&mut h, id).outcome, Some(TransferOutcome::Completed));
    let id = h
        .transfers
        .submit(TransferOperation::Download {
            remote: path(&remote),
            local: dest.clone(),
            policy: ConflictPolicy::Ask,
        })
        .unwrap();
    assert_eq!(finish(&mut h, id).outcome, Some(TransferOutcome::Completed));
    assert_eq!(std::fs::read(dest).unwrap(), bytes);
    assert_eq!(server.accepted_authentications(), 1);
}
#[test]
fn cancellation_and_conflicts_preserve_existing_destination() {
    let server = ssh_server::SshServer::start(true);
    let mut h = connected(&server);
    let remote = server.root.join("large");
    let dest = server.root.join("keep");
    std::fs::write(&remote, vec![3; 2 * 1024 * 1024]).unwrap();
    std::fs::write(&dest, b"keep").unwrap();
    let id = h
        .transfers
        .submit(TransferOperation::Download {
            remote: path(&remote),
            local: dest.clone(),
            policy: ConflictPolicy::Ask,
        })
        .unwrap();
    assert_eq!(finish(&mut h, id).outcome, Some(TransferOutcome::Conflict));
    assert_eq!(std::fs::read(&dest).unwrap(), b"keep");
    let id = h
        .transfers
        .submit(TransferOperation::Download {
            remote: path(&remote),
            local: dest.clone(),
            policy: ConflictPolicy::Overwrite,
        })
        .unwrap();
    h.transfers.cancel(id);
    assert_eq!(finish(&mut h, id).outcome, Some(TransferOutcome::Cancelled));
    assert_eq!(std::fs::read(dest).unwrap(), b"keep");
}
#[test]
fn remote_delete_requires_confirmed_target() {
    let server = ssh_server::SshServer::start(true);
    let mut h = connected(&server);
    let original = server.root.join("dir ' é\n$(literal)");
    let renamed = server.root.join("renamed");
    let other = server.root.join("other");
    std::fs::write(&other, b"keep").unwrap();
    let id = h
        .transfers
        .submit(TransferOperation::Mkdir {
            remote: path(&original),
        })
        .unwrap();
    assert_eq!(finish(&mut h, id).outcome, Some(TransferOutcome::Completed));
    assert!(original.is_dir());
    let id = h
        .transfers
        .submit(TransferOperation::Rename {
            source: path(&original),
            destination: path(&renamed),
            policy: ConflictPolicy::Ask,
        })
        .unwrap();
    assert_eq!(finish(&mut h, id).outcome, Some(TransferOutcome::Completed));
    assert!(renamed.is_dir());
    let id = h
        .transfers
        .submit(TransferOperation::Remove {
            remote: path(&other),
            confirmed: path(&renamed),
        })
        .unwrap();
    assert!(matches!(
        finish(&mut h, id).outcome,
        Some(TransferOutcome::Failed(_))
    ));
    assert_eq!(std::fs::read(&other).unwrap(), b"keep");
    let id = h
        .transfers
        .submit(TransferOperation::Remove {
            remote: path(&renamed),
            confirmed: path(&renamed),
        })
        .unwrap();
    assert_eq!(finish(&mut h, id).outcome, Some(TransferOutcome::Completed));
    assert!(!renamed.exists());
    assert!(other.exists());
}
#[test]
fn cancelled_download_does_not_overwrite_destination() {
    let server = ssh_server::SshServer::start(true);
    let mut h = connected(&server);
    let remote = server.root.join("large-active");
    std::fs::File::create(&remote)
        .unwrap()
        .set_len(128 * 1024 * 1024)
        .unwrap();
    let local = server.root.join("keep-active");
    std::fs::write(&local, b"keep").unwrap();
    let id = h
        .transfers
        .submit(TransferOperation::Download {
            remote: path(&remote),
            local: local.clone(),
            policy: ConflictPolicy::Overwrite,
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if h.transfers
            .poll()
            .iter()
            .any(|e| e.id == id && e.recovery.is_some())
        {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    h.transfers.cancel(id);
    let event = finish(&mut h, id);
    assert_eq!(event.outcome, Some(TransferOutcome::Cancelled));
    assert_eq!(std::fs::read(local).unwrap(), b"keep");
    match event.recovery.unwrap() {
        harbor_workspace::remote_files::transfer::RecoveryPath::Local(path) => {
            assert!(path.is_file())
        }
        _ => panic!("wrong recovery side"),
    }
}
#[test]
fn recursive_transfer_preserves_bytes_and_names() {
    let server = ssh_server::SshServer::start(true);
    let mut h = connected(&server);
    let source = server.root.join("tree-local");
    let remote = server.root.join("tree-remote");
    let dest = server.root.join("tree-downloaded");
    std::fs::create_dir_all(source.join("sub é\n$(literal)")).unwrap();
    for name in ["empty", "sub é\n$(literal)/binary '"] {
        std::fs::write(
            source.join(name),
            if name == "empty" {
                Vec::new()
            } else {
                vec![0, 255, 1, 2, 0]
            },
        )
        .unwrap();
    }
    std::os::unix::fs::symlink(".", source.join("loop")).unwrap();
    let id = h
        .transfers
        .submit(TransferOperation::Upload {
            local: source.clone(),
            remote: path(&remote),
            policy: ConflictPolicy::Ask,
        })
        .unwrap();
    let event = finish(&mut h, id);
    assert_eq!(event.outcome, Some(TransferOutcome::Completed), "{event:?}");
    let id = h
        .transfers
        .submit(TransferOperation::Download {
            remote: path(&remote),
            local: dest.clone(),
            policy: ConflictPolicy::Ask,
        })
        .unwrap();
    let event = finish(&mut h, id);
    assert_eq!(event.outcome, Some(TransferOutcome::Completed), "{event:?}");
    for name in ["empty", "sub é\n$(literal)/binary '"] {
        assert_eq!(
            std::fs::read(source.join(name)).unwrap(),
            std::fs::read(dest.join(name)).unwrap()
        );
    }
    assert_eq!(
        std::fs::read_link(dest.join("loop")).unwrap(),
        std::path::PathBuf::from(".")
    );
    assert_eq!(std::fs::read_dir(&dest).unwrap().count(), 3);
    assert_eq!(server.accepted_authentications(), 1);
}
#[test]
fn repeated_tree_cancellation_releases_bounded_sftp_handles() {
    let server = ssh_server::SshServer::start(true);
    let mut h = connected(&server);
    let source = server.root.join("tree-cancel");
    std::fs::create_dir(&source).unwrap();
    std::fs::File::create(source.join("large"))
        .unwrap()
        .set_len(128 * 1024 * 1024)
        .unwrap();
    for n in 0..6 {
        let id = h
            .transfers
            .submit(TransferOperation::Download {
                remote: path(&source),
                local: server.root.join(format!("tree-partial-{n}")),
                policy: ConflictPolicy::Ask,
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let events = h.transfers.poll();
            assert!(
                !events.iter().any(|e| e.id == id && e.outcome.is_some()),
                "{events:?}"
            );
            if events.iter().any(|e| e.id == id && e.bytes > 0) {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        h.transfers.cancel(id);
        assert_eq!(finish(&mut h, id).outcome, Some(TransferOutcome::Cancelled));
    }
    std::fs::write(source.join("small"), b"still-usable").unwrap();
    let id = h
        .transfers
        .submit(TransferOperation::Download {
            remote: path(&source.join("small")),
            local: server.root.join("after-cancel"),
            policy: ConflictPolicy::Ask,
        })
        .unwrap();
    assert_eq!(finish(&mut h, id).outcome, Some(TransferOutcome::Completed));
}
#[test]
fn missing_sftp_rejects_file_jobs_without_stranding_the_queue() {
    let server = ssh_server::SshServer::start(false);
    let mut h = ConnectionHandle::connect(server.config(92, true)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(6);
    loop {
        if h.poll()
            .iter()
            .any(|e| matches!(e, ConnectionEvent::FilesError { .. }))
        {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let id = h
        .transfers
        .submit(TransferOperation::Download {
            remote: path(&server.root.join("not-used")),
            local: server.root.join("not-created"),
            policy: ConflictPolicy::Ask,
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let events = h.transfers.poll();
        if let Some(event) = events.iter().find(|e| e.id == id && e.outcome.is_some()) {
            assert!(matches!(event.outcome, Some(TransferOutcome::Failed(_))));
            break;
        }
        h.poll();
        assert!(
            Instant::now() < deadline,
            "Missing SFTP stranded the file job"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn failed_local_and_remote_io_preserve_existing_data() {
    use std::os::unix::fs::PermissionsExt;
    let server = ssh_server::SshServer::start(true);
    let mut h = connected(&server);
    let remote = server.root.join("denied-remote");
    let local = server.root.join("keep-on-error");
    std::fs::write(&remote, b"remote").unwrap();
    std::fs::write(&local, b"keep").unwrap();
    std::fs::set_permissions(&remote, std::fs::Permissions::from_mode(0)).unwrap();
    let id = h
        .transfers
        .submit(TransferOperation::Download {
            remote: path(&remote),
            local: local.clone(),
            policy: ConflictPolicy::Overwrite,
        })
        .unwrap();
    let event = finish(&mut h, id);
    std::fs::set_permissions(&remote, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(matches!(event.outcome, Some(TransferOutcome::Failed(_))));
    assert_eq!(std::fs::read(&local).unwrap(), b"keep");
    assert!(event.recovery.is_some());
    let directory = server.root.join("readonly-local");
    std::fs::create_dir(&directory).unwrap();
    let dest = directory.join("keep");
    std::fs::write(&dest, b"keep").unwrap();
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o500)).unwrap();
    let id = h
        .transfers
        .submit(TransferOperation::Download {
            remote: path(&remote),
            local: dest.clone(),
            policy: ConflictPolicy::Overwrite,
        })
        .unwrap();
    let event = finish(&mut h, id);
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(matches!(event.outcome, Some(TransferOutcome::Failed(_))));
    assert_eq!(std::fs::read(dest).unwrap(), b"keep");
}
#[test]
fn disconnect_keeps_original_destination_and_recoverable_partial() {
    let server = ssh_server::SshServer::start(true);
    let mut h = connected(&server);
    let remote = server.root.join("disconnect-large");
    std::fs::File::create(&remote)
        .unwrap()
        .set_len(128 * 1024 * 1024)
        .unwrap();
    let local = server.root.join("disconnect-keep");
    std::fs::write(&local, b"keep").unwrap();
    let id = h
        .transfers
        .submit(TransferOperation::Download {
            remote: path(&remote),
            local: local.clone(),
            policy: ConflictPolicy::Overwrite,
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if h.transfers.poll().iter().any(|e| e.id == id && e.bytes > 0) {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    h.cancel();
    let event = finish(&mut h, id);
    assert!(matches!(event.outcome, Some(TransferOutcome::Failed(_))));
    assert_eq!(std::fs::read(local).unwrap(), b"keep");
    assert!(event.recovery.is_some());
}
#[test]
fn composed_symlinks_cannot_publish_an_escape_from_the_selected_tree() {
    let server = ssh_server::SshServer::start(true);
    let mut h = connected(&server);
    let source = server.root.join("composed-links");
    std::fs::create_dir(&source).unwrap();
    std::os::unix::fs::symlink(".", source.join("alias")).unwrap();
    std::os::unix::fs::symlink("alias/..", source.join("escape")).unwrap();
    let remote = server.root.join("composed-upload");
    let id = h
        .transfers
        .submit(TransferOperation::Upload {
            local: source,
            remote: path(&remote),
            policy: ConflictPolicy::Ask,
        })
        .unwrap();
    let event = finish(&mut h, id);
    assert!(
        matches!(event.outcome, Some(TransferOutcome::Failed(_))),
        "{event:?}"
    );
    assert!(!remote.exists());
}
#[test]
fn overlapping_overwrite_jobs_commit_in_submission_order() {
    let server = ssh_server::SshServer::start(true);
    let mut h = connected(&server);
    let large = server.root.join("first-large");
    std::fs::File::create(&large)
        .unwrap()
        .set_len(8 * 1024 * 1024)
        .unwrap();
    let small = server.root.join("second-small");
    std::fs::write(&small, b"new").unwrap();
    let remote = server.root.join("ordered-result");
    let a = h
        .transfers
        .submit(TransferOperation::Upload {
            local: large,
            remote: path(&remote),
            policy: ConflictPolicy::Overwrite,
        })
        .unwrap();
    let b = h
        .transfers
        .submit(TransferOperation::Upload {
            local: small,
            remote: path(&remote),
            policy: ConflictPolicy::Overwrite,
        })
        .unwrap();
    let mut completed = std::collections::BTreeSet::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    while completed.len() < 2 {
        for e in h.transfers.poll() {
            if [a, b].contains(&e.id) && e.outcome.is_some() {
                assert_eq!(e.outcome, Some(TransferOutcome::Completed));
                completed.insert(e.id);
            }
        }
        h.poll();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        std::fs::metadata(&remote).unwrap().len(),
        3,
        "An earlier job overwrote the newer destination"
    );
    assert_eq!(std::fs::read(remote).unwrap(), b"new");
}
#[test]
fn transfer_preserves_executable_modes_after_staging() {
    use std::os::unix::fs::PermissionsExt;
    let server = ssh_server::SshServer::start(true);
    let mut h = connected(&server);
    let source = server.root.join("executable-src");
    std::fs::write(&source, b"#!/bin/sh\ntrue\n").unwrap();
    std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o751)).unwrap();
    let remote = server.root.join("executable-remote");
    let id = h
        .transfers
        .submit(TransferOperation::Upload {
            local: source,
            remote: path(&remote),
            policy: ConflictPolicy::Ask,
        })
        .unwrap();
    assert_eq!(finish(&mut h, id).outcome, Some(TransferOutcome::Completed));
    assert_eq!(
        std::fs::metadata(&remote).unwrap().permissions().mode() & 0o777,
        0o751
    );
    let local = server.root.join("executable-download");
    let id = h
        .transfers
        .submit(TransferOperation::Download {
            remote: path(&remote),
            local: local.clone(),
            policy: ConflictPolicy::Ask,
        })
        .unwrap();
    assert_eq!(finish(&mut h, id).outcome, Some(TransferOutcome::Completed));
    assert_eq!(
        std::fs::metadata(local).unwrap().permissions().mode() & 0o777,
        0o751
    );
}
#[cfg(target_os = "linux")]
#[test]
fn injected_disk_full_sync_and_close_errors_preserve_destination_and_partial() {
    const MODE: &str = "TERMCRAFT_IO_FAULT_MODE";
    if std::env::var_os(MODE).is_none() {
        let root = std::env::temp_dir().join(format!("termcraft-fault-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let library = root.join("io_fault.so");
        assert!(
            std::process::Command::new("cc")
                .args(["-shared", "-fPIC", "-O2"])
                .arg(
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/support/io_fault.c")
                )
                .arg("-ldl")
                .arg("-o")
                .arg(&library)
                .status()
                .unwrap()
                .success()
        );
        for mode in ["write", "sync", "close"] {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "injected_disk_full_sync_and_close_errors_preserve_destination_and_partial",
                    "--nocapture",
                ])
                .env("LD_PRELOAD", &library)
                .env(MODE, mode)
                .status()
                .unwrap();
            assert!(
                status.success(),
                "Injected {mode} failure was not safely handled"
            );
        }
        std::fs::remove_dir_all(root).unwrap();
        return;
    }
    let mode = std::env::var(MODE).unwrap();
    let server = ssh_server::SshServer::start(true);
    let mut h = connected(&server);
    let remote = server.root.join("fault-source");
    std::fs::write(&remote, vec![7; 256 * 1024]).unwrap();
    let local = server.root.join("fault-destination");
    std::fs::write(&local, b"keep").unwrap();
    let id = h
        .transfers
        .submit(TransferOperation::Download {
            remote: path(&remote),
            local: local.clone(),
            policy: ConflictPolicy::Overwrite,
        })
        .unwrap();
    let event = finish(&mut h, id);
    assert!(
        matches!(&event.outcome,Some(TransferOutcome::Failed(message)) if message.contains(if mode=="close"{"os error 5"}else{"os error 28"})),
        "{mode}: {event:?}"
    );
    assert_eq!(std::fs::read(local).unwrap(), b"keep");
    match event.recovery.unwrap() {
        harbor_workspace::remote_files::transfer::RecoveryPath::Local(path) => {
            assert!(std::fs::metadata(path).unwrap().len() > 0)
        }
        _ => panic!("wrong recovery side"),
    }
}
#[test]
fn explicit_folder_overwrite_merges_files_and_preserves_unrelated_entries() {
    let server = ssh_server::SshServer::start(true);
    let mut h = connected(&server);
    let source = server.root.join("merge-source");
    let remote = server.root.join("merge-remote");
    let local = server.root.join("merge-local");
    for root in [&source, &remote, &local] {
        std::fs::create_dir_all(root.join("sub")).unwrap();
    }
    std::fs::write(source.join("sub/update"), b"new").unwrap();
    std::fs::create_dir(source.join("empty")).unwrap();
    std::os::unix::fs::symlink(".", source.join("loop")).unwrap();
    for root in [&remote, &local] {
        std::fs::write(root.join("sub/update"), b"old").unwrap();
        std::fs::write(root.join("unrelated"), b"keep").unwrap();
    }
    let id = h
        .transfers
        .submit(TransferOperation::Upload {
            local: source.clone(),
            remote: path(&remote),
            policy: ConflictPolicy::Ask,
        })
        .unwrap();
    assert_eq!(finish(&mut h, id).outcome, Some(TransferOutcome::Conflict));
    let id = h
        .transfers
        .submit(TransferOperation::Upload {
            local: source,
            remote: path(&remote),
            policy: ConflictPolicy::Overwrite,
        })
        .unwrap();
    let event = finish(&mut h, id);
    assert_eq!(event.outcome, Some(TransferOutcome::Completed), "{event:?}");
    assert_eq!(std::fs::read(remote.join("unrelated")).unwrap(), b"keep");
    assert_eq!(std::fs::read(remote.join("sub/update")).unwrap(), b"new");
    let id = h
        .transfers
        .submit(TransferOperation::Download {
            remote: path(&remote),
            local: local.clone(),
            policy: ConflictPolicy::Overwrite,
        })
        .unwrap();
    let event = finish(&mut h, id);
    assert_eq!(event.outcome, Some(TransferOutcome::Completed), "{event:?}");
    assert_eq!(std::fs::read(local.join("unrelated")).unwrap(), b"keep");
    assert_eq!(std::fs::read(local.join("sub/update")).unwrap(), b"new");
    assert!(local.join("empty").is_dir());
    assert_eq!(
        std::fs::read_link(local.join("loop")).unwrap(),
        Path::new(".")
    );
}
#[test]
fn folder_merge_never_follows_existing_destination_directory_symlinks() {
    let server = ssh_server::SshServer::start(true);
    let mut h = connected(&server);
    let source = server.root.join("merge-safe-source");
    std::fs::create_dir_all(source.join("sub")).unwrap();
    std::fs::write(source.join("sub/file"), b"new").unwrap();
    let outside = server.root.join("merge-outside");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("file"), b"keep").unwrap();
    for (upload, name) in [(true, "remote-link-target"), (false, "local-link-target")] {
        let dest = server.root.join(name);
        std::fs::create_dir(&dest).unwrap();
        std::os::unix::fs::symlink(&outside, dest.join("sub")).unwrap();
        let operation = if upload {
            TransferOperation::Upload {
                local: source.clone(),
                remote: path(&dest),
                policy: ConflictPolicy::Overwrite,
            }
        } else {
            TransferOperation::Download {
                remote: path(&source),
                local: dest,
                policy: ConflictPolicy::Overwrite,
            }
        };
        let id = h.transfers.submit(operation).unwrap();
        assert!(matches!(
            finish(&mut h, id).outcome,
            Some(TransferOutcome::Failed(_))
        ));
        assert_eq!(std::fs::read(outside.join("file")).unwrap(), b"keep");
    }
}
#[test]
fn folder_merge_retains_modes_without_making_staging_read_only_too_early() {
    use std::os::unix::fs::PermissionsExt;
    struct Restore(Vec<std::path::PathBuf>);
    impl Drop for Restore {
        fn drop(&mut self) {
            for path in &self.0 {
                let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755));
            }
        }
    }
    let server = ssh_server::SshServer::start(true);
    let mut h = connected(&server);
    let source = server.root.join("readonly-merge-source");
    let remote = server.root.join("readonly-merge-remote");
    let local = server.root.join("readonly-merge-local");
    for root in [&source, &remote, &local] {
        std::fs::create_dir(root).unwrap();
    }
    std::fs::create_dir(source.join("sub")).unwrap();
    std::fs::write(source.join("sub/run"), b"#!/bin/sh\ntrue\n").unwrap();
    std::fs::set_permissions(
        source.join("sub/run"),
        std::fs::Permissions::from_mode(0o751),
    )
    .unwrap();
    std::fs::set_permissions(source.join("sub"), std::fs::Permissions::from_mode(0o555)).unwrap();
    std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o555)).unwrap();
    let _restore = Restore(vec![
        source.clone(),
        source.join("sub"),
        remote.join("sub"),
        local.join("sub"),
    ]);
    let id = h
        .transfers
        .submit(TransferOperation::Upload {
            local: source,
            remote: path(&remote),
            policy: ConflictPolicy::Overwrite,
        })
        .unwrap();
    let event = finish(&mut h, id);
    assert_eq!(event.outcome, Some(TransferOutcome::Completed), "{event:?}");
    let id = h
        .transfers
        .submit(TransferOperation::Download {
            remote: path(&remote),
            local: local.clone(),
            policy: ConflictPolicy::Overwrite,
        })
        .unwrap();
    let event = finish(&mut h, id);
    assert_eq!(event.outcome, Some(TransferOutcome::Completed), "{event:?}");
    assert_eq!(
        std::fs::metadata(local.join("sub"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o555
    );
    assert_eq!(
        std::fs::metadata(local.join("sub/run"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o751
    );
}
#[test]
fn recursive_delete_is_confirmed_bounded_and_never_follows_symlinks() {
    let server = ssh_server::SshServer::start(true);
    let mut h = connected(&server);
    let root = server.root.join("delete tree ' é\n$(literal)");
    let outside = server.root.join("delete-outside");
    std::fs::create_dir_all(root.join("sub")).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(root.join("sub/file"), b"remove").unwrap();
    std::fs::write(outside.join("keep"), b"keep").unwrap();
    std::os::unix::fs::symlink("../delete-outside", root.join("external-link")).unwrap();
    let id = h
        .transfers
        .submit(TransferOperation::RemoveTree {
            remote: path(&root),
            confirmed: path(&outside),
        })
        .unwrap();
    assert!(matches!(
        finish(&mut h, id).outcome,
        Some(TransferOutcome::Failed(_))
    ));
    assert!(root.join("sub/file").exists());
    let id = h
        .transfers
        .submit(TransferOperation::RemoveTree {
            remote: path(&root),
            confirmed: path(&root),
        })
        .unwrap();
    let event = finish(&mut h, id);
    assert_eq!(event.outcome, Some(TransferOutcome::Completed), "{event:?}");
    assert!(!root.exists());
    assert_eq!(std::fs::read(outside.join("keep")).unwrap(), b"keep");
}
#[test]
fn symlink_cycle_is_not_followed() {
    let server = ssh_server::SshServer::start(true);
    let mut h = connected(&server);
    let source = server.root.join("two-link-cycle");
    std::fs::create_dir(&source).unwrap();
    std::os::unix::fs::symlink("b", source.join("a")).unwrap();
    std::os::unix::fs::symlink("a", source.join("b")).unwrap();
    let remote = server.root.join("cycle-remote");
    let local = server.root.join("cycle-local");
    let id = h
        .transfers
        .submit(TransferOperation::Upload {
            local: source,
            remote: path(&remote),
            policy: ConflictPolicy::Ask,
        })
        .unwrap();
    let event = finish(&mut h, id);
    assert_eq!(event.outcome, Some(TransferOutcome::Completed));
    assert_eq!(event.bytes, 0);
    let id = h
        .transfers
        .submit(TransferOperation::Download {
            remote: path(&remote),
            local: local.clone(),
            policy: ConflictPolicy::Ask,
        })
        .unwrap();
    let event = finish(&mut h, id);
    assert_eq!(event.outcome, Some(TransferOutcome::Completed));
    assert_eq!(event.bytes, 0);
    assert_eq!(std::fs::read_link(local.join("a")).unwrap(), Path::new("b"));
    assert_eq!(std::fs::read_link(local.join("b")).unwrap(), Path::new("a"));
}
#[test]
fn oversized_recursive_delete_stops_before_removing_root_entries() {
    let server = ssh_server::SshServer::start(true);
    let mut h = connected(&server);
    let root = server.root.join("delete-over-limit");
    std::fs::create_dir(&root).unwrap();
    for n in 0..10_001 {
        std::fs::write(root.join(format!("file-{n:05}")), b"keep").unwrap();
    }
    let id = h
        .transfers
        .submit(TransferOperation::RemoveTree {
            remote: path(&root),
            confirmed: path(&root),
        })
        .unwrap();
    let event = finish(&mut h, id);
    assert!(matches!(event.outcome, Some(TransferOutcome::Failed(_))));
    assert_eq!(event.completed_entries, 0);
    assert_eq!(std::fs::read_dir(root).unwrap().count(), 10_001);
}
