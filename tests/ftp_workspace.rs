#[allow(dead_code)]
#[path = "support/ftp_server.rs"]
mod fixture;

use fixture::{Endpoint, ServerOptions};
use harbor_workspace::{
    protocols::{
        ftp::{FtpTlsMode, FtpTransferType},
        ftp_actor::Workspace,
    },
    remote_files::{
        EntryKind, RemotePath,
        editor::{DocumentStatus, RemoteDocument, SavePolicy},
        transfer::{ConflictPolicy, TransferEvent, TransferOperation, TransferOutcome},
    },
};
use std::{path::PathBuf, time::Duration};

fn path(value: &str) -> RemotePath {
    RemotePath::from_bytes(value.as_bytes().to_vec()).unwrap()
}

async fn wait_until<T>(mut poll: impl FnMut() -> Option<T>) -> T {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            if let Some(value) = poll() {
                return value;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("FTP workspace operation never completed")
}

async fn ready(workspace: &mut Workspace) {
    wait_until(|| {
        workspace.poll();
        assert!(!workspace.closed(), "{}", workspace.status());
        (workspace.ready() && !workspace.browser.loading).then_some(())
    })
    .await;
    assert!(workspace.browser.error.is_none());
}

async fn operation(workspace: &mut Workspace, operation: TransferOperation) -> TransferEvent {
    let id = workspace.submit(operation).unwrap();
    wait_until(|| {
        workspace
            .poll()
            .into_iter()
            .find(|event| event.id == id && event.outcome.is_some())
    })
    .await
}

async fn document_status(workspace: &mut Workspace, document: &RemoteDocument) -> DocumentStatus {
    wait_until(|| {
        workspace.poll();
        let (status, _, _) = document.details();
        (!status.busy()).then_some(status)
    })
    .await
}

#[tokio::test]
async fn ftp_workspace_browses_stages_transfers_and_checks_conflicts() {
    let server = Endpoint::start(ServerOptions {
        tls: FtpTlsMode::Explicit,
        ..ServerOptions::default()
    })
    .await;
    server.write_file("/binary", b"\xff\0original");
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("local.bin");
    std::fs::write(&target, b"user contents").unwrap();
    let mut workspace = Workspace::start(84, server.options(), FtpTransferType::Binary).unwrap();
    ready(&mut workspace).await;
    assert_eq!(workspace.browser.connection, 84);
    assert_eq!(workspace.browser.path, path("/"));
    assert_eq!(workspace.certificate_fingerprint().unwrap().len(), 64);
    assert_eq!(workspace.browser.entries[0].kind, EntryKind::File);

    let download = |policy| TransferOperation::Download {
        remote: path("/binary"),
        local: target.clone(),
        policy,
    };
    assert_eq!(
        operation(&mut workspace, download(ConflictPolicy::Ask))
            .await
            .outcome,
        Some(TransferOutcome::Conflict)
    );
    assert_eq!(
        operation(&mut workspace, download(ConflictPolicy::Skip))
            .await
            .outcome,
        Some(TransferOutcome::Skipped)
    );
    assert_eq!(std::fs::read(&target).unwrap(), b"user contents");
    let transferred = operation(&mut workspace, download(ConflictPolicy::Overwrite)).await;
    assert_eq!(transferred.outcome, Some(TransferOutcome::Completed));
    assert_eq!(transferred.bytes, 10);
    assert!(transferred.recovery.is_none());
    assert_eq!(std::fs::read(&target).unwrap(), b"\xff\0original");

    std::fs::write(&target, b"\0new\xff\n").unwrap();
    let upload = |remote, policy| TransferOperation::Upload {
        local: target.clone(),
        remote,
        policy,
    };
    assert_eq!(
        operation(&mut workspace, upload(path("/binary"), ConflictPolicy::Ask))
            .await
            .outcome,
        Some(TransferOutcome::Conflict)
    );
    assert_eq!(server.read_file("/binary").unwrap(), b"\xff\0original");
    let staged = operation(&mut workspace, upload(path("/new"), ConflictPolicy::Ask)).await;
    assert!(matches!(
        staged.outcome,
        Some(TransferOutcome::CompletedWithWarning(_))
    ));
    assert_eq!(staged.bytes, 6);
    assert!(staged.recovery.is_none());
    assert_eq!(server.read_file("/new").unwrap(), b"\0new\xff\n");
    let evidence = server.evidence();
    let reserve = evidence
        .commands
        .iter()
        .position(|verb| verb == "MKD")
        .unwrap();
    let store = evidence
        .commands
        .iter()
        .position(|verb| verb == "STOR")
        .unwrap();
    let publish = evidence
        .commands
        .iter()
        .position(|verb| verb == "RNTO")
        .unwrap();
    assert!(reserve < store && store < publish);
    assert_eq!(evidence.commands[publish + 1], "RMD");

    let directory = path("/folder");
    assert_eq!(
        operation(
            &mut workspace,
            TransferOperation::Mkdir {
                remote: directory.clone()
            }
        )
        .await
        .outcome,
        Some(TransferOutcome::Completed)
    );
    workspace.navigate(directory.clone()).unwrap();
    ready(&mut workspace).await;
    assert_eq!(workspace.browser.path, directory);
    assert!(workspace.browser.entries.is_empty());
    let refused = operation(
        &mut workspace,
        TransferOperation::Remove {
            remote: directory.clone(),
            confirmed: path("/other"),
        },
    )
    .await;
    assert!(matches!(refused.outcome, Some(TransferOutcome::Failed(_))));
    assert_eq!(
        operation(
            &mut workspace,
            TransferOperation::Remove {
                remote: directory.clone(),
                confirmed: directory
            }
        )
        .await
        .outcome,
        Some(TransferOutcome::Completed)
    );
}

#[tokio::test]
async fn ftp_document_conflicts_preserve_edits_and_require_an_observed_replacement() {
    let server = Endpoint::start(ServerOptions {
        tls: FtpTlsMode::Explicit,
        ..ServerOptions::default()
    })
    .await;
    server.write_file("/notes", b"original\r\n");
    let root = tempfile::tempdir().unwrap();
    let recovery_root: PathBuf = root.path().join("recovery");
    let mut workspace = Workspace::start(85, server.options(), FtpTransferType::Ascii).unwrap();
    ready(&mut workspace).await;
    let mut document = workspace
        .open_document(path("/notes"), recovery_root.clone())
        .unwrap();
    assert_eq!(
        document_status(&mut workspace, &document).await,
        DocumentStatus::Ready
    );
    assert_eq!(document.snapshot().text, "original\r\n");

    document.set_text("local changes\n").unwrap();
    server.write_file("/notes", b"remote changes\r\n");
    document.save(SavePolicy::VerifyUnchanged).unwrap();
    assert_eq!(
        document_status(&mut workspace, &document).await,
        DocumentStatus::Conflict
    );
    assert_eq!(server.read_file("/notes").unwrap(), b"remote changes\r\n");
    assert_eq!(
        std::fs::read(document.snapshot().recovery_path.unwrap()).unwrap(),
        b"local changes\n"
    );
    assert!(document.snapshot().dirty);

    document.save(SavePolicy::Overwrite).unwrap();
    assert_eq!(
        document_status(&mut workspace, &document).await,
        DocumentStatus::Saved
    );
    assert_eq!(server.read_file("/notes").unwrap(), b"local changes\n");
    assert!(!document.snapshot().dirty);
    assert!(document.snapshot().warning.is_some());

    workspace.disconnect();
    wait_until(|| {
        workspace.poll();
        workspace.closed().then_some(())
    })
    .await;
    let mut fresh = Workspace::start(86, server.options(), FtpTransferType::Binary).unwrap();
    ready(&mut fresh).await;
    let target = fresh.open_document(path("/notes"), recovery_root).unwrap();
    assert_eq!(
        document_status(&mut fresh, &target).await,
        DocumentStatus::Ready
    );
    assert!(target.can_import_from(&document));
}

#[tokio::test]
async fn ftp_workspace_cancels_a_queued_operation_before_it_reaches_the_server() {
    let server = Endpoint::start(ServerOptions {
        stall_transfer: true,
        ..ServerOptions::default()
    })
    .await;
    let mut workspace = Workspace::start(87, server.options(), FtpTransferType::Binary).unwrap();
    wait_until(|| {
        workspace.poll();
        (workspace.ready() && server.evidence().data_connections == 1).then_some(())
    })
    .await;
    assert!(workspace.browser.loading);
    let id = workspace
        .submit(TransferOperation::Mkdir {
            remote: path("/cancel-this"),
        })
        .unwrap();
    workspace.cancel_transfer(id);
    let outcome = wait_until(|| {
        workspace
            .poll()
            .into_iter()
            .find(|event| event.id == id)
            .and_then(|event| event.outcome)
    })
    .await;
    assert_eq!(outcome, TransferOutcome::Cancelled);
    workspace.disconnect();
    wait_until(|| {
        workspace.poll();
        workspace.closed().then_some(())
    })
    .await;
    assert!(!server.evidence().commands.iter().any(|verb| verb == "MKD"));
}

#[tokio::test]
async fn ftp_workspace_retains_short_binary_download_without_replacing_local_file() {
    use harbor_workspace::remote_files::transfer::RecoveryPath;
    let server = Endpoint::start(ServerOptions {
        short_retr: Some(4096),
        ..ServerOptions::default()
    })
    .await;
    let full = vec![0x7f; 8192];
    server.write_file("/binary", &full);
    let local = tempfile::tempdir().unwrap();
    let target = local.path().join("existing.bin");
    std::fs::write(&target, b"keep this complete file").unwrap();
    let mut workspace = Workspace::start(88, server.options(), FtpTransferType::Binary).unwrap();
    ready(&mut workspace).await;
    let event = operation(
        &mut workspace,
        TransferOperation::Download {
            remote: path("/binary"),
            local: target.clone(),
            policy: ConflictPolicy::Overwrite,
        },
    )
    .await;
    assert!(matches!(event.outcome, Some(TransferOutcome::Failed(_))));
    assert_eq!(std::fs::read(&target).unwrap(), b"keep this complete file");
    assert_eq!(event.bytes, 4096);
    let Some(RecoveryPath::Local(recovery)) = event.recovery else {
        panic!("short download did not expose its recovery file");
    };
    assert_eq!(std::fs::read(recovery).unwrap(), full[..4096]);
}

#[tokio::test]
async fn ftp_folder_round_trip_merges_entries_and_requires_exact_delete_confirmation() {
    let server = Endpoint::start(ServerOptions {
        tls: FtpTlsMode::Explicit,
        ..Default::default()
    })
    .await;
    let local = tempfile::tempdir().unwrap();
    let source = local.path().join("source");
    std::fs::create_dir_all(source.join("日本語/empty")).unwrap();
    std::fs::write(source.join("日本語/naïve.txt"), b"original\n").unwrap();
    let mut workspace = Workspace::start(89, server.options(), FtpTransferType::Binary).unwrap();
    ready(&mut workspace).await;
    let remote = path("/tree");
    let upload = |policy| TransferOperation::UploadFolder {
        local: source.clone(),
        remote: remote.clone(),
        policy,
    };
    let fresh = operation(&mut workspace, upload(ConflictPolicy::Ask)).await;
    assert!(matches!(
        fresh.outcome,
        Some(TransferOutcome::CompletedWithWarning(_))
    ));
    assert!(fresh.recovery.is_none());
    assert!(server.has_directory("/tree/日本語/empty"));
    assert_eq!(
        server.read_file("/tree/日本語/naïve.txt").unwrap(),
        b"original\n"
    );

    server.write_file("/tree/keep.txt", b"unrelated remote");
    std::fs::write(source.join("日本語/naïve.txt"), b"local update").unwrap();
    std::fs::write(source.join("added.txt"), b"added").unwrap();
    assert_eq!(
        operation(&mut workspace, upload(ConflictPolicy::Ask))
            .await
            .outcome,
        Some(TransferOutcome::Conflict)
    );
    let skipped = operation(&mut workspace, upload(ConflictPolicy::Skip)).await;
    assert!(matches!(
        skipped.outcome,
        Some(TransferOutcome::CompletedWithWarning(_))
    ));
    assert!(skipped.recovery.is_some());
    assert_eq!(server.read_file("/tree/added.txt").unwrap(), b"added");
    assert_eq!(
        server.read_file("/tree/日本語/naïve.txt").unwrap(),
        b"original\n"
    );
    let overwrite = operation(&mut workspace, upload(ConflictPolicy::Overwrite)).await;
    assert!(matches!(
        overwrite.outcome,
        Some(TransferOutcome::CompletedWithWarning(_))
    ));
    assert!(overwrite.recovery.is_none());
    assert_eq!(
        server.read_file("/tree/日本語/naïve.txt").unwrap(),
        b"local update"
    );
    assert_eq!(
        server.read_file("/tree/keep.txt").unwrap(),
        b"unrelated remote"
    );

    let destination = local.path().join("download");
    let download = |policy| TransferOperation::DownloadFolder {
        remote: remote.clone(),
        local: destination.clone(),
        policy,
    };
    assert_eq!(
        operation(&mut workspace, download(ConflictPolicy::Ask))
            .await
            .outcome,
        Some(TransferOutcome::Completed)
    );
    assert!(destination.join("日本語/empty").is_dir());
    std::fs::write(destination.join("unrelated.txt"), b"keep local").unwrap();
    std::fs::write(destination.join("日本語/naïve.txt"), b"local only").unwrap();
    assert_eq!(
        operation(&mut workspace, download(ConflictPolicy::Ask))
            .await
            .outcome,
        Some(TransferOutcome::Conflict)
    );
    let skipped = operation(&mut workspace, download(ConflictPolicy::Skip)).await;
    assert!(matches!(
        skipped.outcome,
        Some(TransferOutcome::CompletedWithWarning(_))
    ));
    assert!(skipped.recovery.is_some());
    assert_eq!(
        std::fs::read(destination.join("日本語/naïve.txt")).unwrap(),
        b"local only"
    );
    assert_eq!(
        operation(&mut workspace, download(ConflictPolicy::Overwrite))
            .await
            .outcome,
        Some(TransferOutcome::Completed)
    );
    assert_eq!(
        std::fs::read(destination.join("日本語/naïve.txt")).unwrap(),
        b"local update"
    );
    assert_eq!(
        std::fs::read(destination.join("unrelated.txt")).unwrap(),
        b"keep local"
    );

    let refused = operation(
        &mut workspace,
        TransferOperation::RemoveTree {
            remote: remote.clone(),
            confirmed: path("/other"),
        },
    )
    .await;
    assert!(matches!(refused.outcome, Some(TransferOutcome::Failed(_))));
    assert!(server.has_directory("/tree/日本語"));
    let removed = operation(
        &mut workspace,
        TransferOperation::RemoveTree {
            remote: remote.clone(),
            confirmed: remote,
        },
    )
    .await;
    assert_eq!(removed.outcome, Some(TransferOutcome::Completed));
    assert_eq!(removed.completed_entries, 6);
    assert!(!server.has_directory("/tree"));
    assert!(server.read_file("/tree/日本語/naïve.txt").is_none());
}

#[tokio::test]
async fn ftp_folder_download_cancellation_preserves_the_existing_destination_and_staging() {
    use harbor_workspace::remote_files::transfer::RecoveryPath;
    let server = Endpoint::start(ServerOptions {
        tls: FtpTlsMode::Explicit,
        stall_retr: Some("/tree/stalled".into()),
        ..Default::default()
    })
    .await;
    let local = tempfile::tempdir().unwrap();
    let target = local.path().join("current");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("stalled"), b"complete local").unwrap();
    let mut workspace = Workspace::start(90, server.options(), FtpTransferType::Binary).unwrap();
    ready(&mut workspace).await;
    assert_eq!(
        operation(
            &mut workspace,
            TransferOperation::Mkdir {
                remote: path("/tree")
            }
        )
        .await
        .outcome,
        Some(TransferOutcome::Completed)
    );
    server.write_file("/tree/stalled", &[0xf3; 8192]);
    let id = workspace
        .submit(TransferOperation::DownloadFolder {
            remote: path("/tree"),
            local: target.clone(),
            policy: ConflictPolicy::Overwrite,
        })
        .unwrap();
    wait_until(|| {
        workspace
            .poll()
            .into_iter()
            .find(|event| event.id == id && event.bytes >= 4096)
    })
    .await;
    workspace.cancel_transfer(id);
    let event = wait_until(|| {
        workspace
            .poll()
            .into_iter()
            .find(|event| event.id == id && event.outcome.is_some())
    })
    .await;
    assert_eq!(event.outcome, Some(TransferOutcome::Cancelled));
    assert_eq!(
        std::fs::read(target.join("stalled")).unwrap(),
        b"complete local"
    );
    let Some(RecoveryPath::Local(staging)) = event.recovery else {
        panic!("cancelled folder download lost its recovery location");
    };
    assert_eq!(
        std::fs::read(staging.join("stalled")).unwrap(),
        [0xf3; 4096]
    );
}

#[cfg(unix)]
#[tokio::test]
async fn ftp_folder_upload_rejects_symlink_children_instead_of_reading_outside() {
    use std::os::unix::fs::symlink;
    let server = Endpoint::start(ServerOptions {
        tls: FtpTlsMode::Explicit,
        ..Default::default()
    })
    .await;
    let local = tempfile::tempdir().unwrap();
    let outside = local.path().join("outside");
    std::fs::write(&outside, b"private outside bytes").unwrap();
    let root = local.path().join("tree");
    std::fs::create_dir(&root).unwrap();
    symlink(&outside, root.join("outside-link")).unwrap();
    let mut workspace = Workspace::start(91, server.options(), FtpTransferType::Binary).unwrap();
    ready(&mut workspace).await;
    let event = operation(
        &mut workspace,
        TransferOperation::UploadFolder {
            local: root,
            remote: path("/tree"),
            policy: ConflictPolicy::Ask,
        },
    )
    .await;
    assert!(matches!(event.outcome, Some(TransferOutcome::Failed(_))));
    assert!(event.recovery.is_some());
    assert!(!server.has_directory("/tree"));
    assert!(server.evidence().uploads.is_empty());
}

#[tokio::test]
async fn ftp_folder_rejects_an_ambiguous_server_child_before_recursive_delete() {
    let server = Endpoint::start(ServerOptions {
        tls: FtpTlsMode::Explicit,
        listing: Some(b"type=file;size=4; ..\\outside\r\n".to_vec()),
        listing_path: Some("/tree".into()),
        ..Default::default()
    })
    .await;
    server.write_file("/outside", b"keep");
    let mut workspace = Workspace::start(92, server.options(), FtpTransferType::Binary).unwrap();
    ready(&mut workspace).await;
    assert_eq!(
        operation(
            &mut workspace,
            TransferOperation::Mkdir {
                remote: path("/tree")
            }
        )
        .await
        .outcome,
        Some(TransferOutcome::Completed)
    );
    let refused = operation(
        &mut workspace,
        TransferOperation::RemoveTree {
            remote: path("/tree"),
            confirmed: path("/tree"),
        },
    )
    .await;
    assert!(matches!(refused.outcome, Some(TransferOutcome::Failed(_))));
    assert!(server.has_directory("/tree"));
    assert_eq!(server.read_file("/outside").unwrap(), b"keep");
    assert!(
        !server
            .evidence()
            .commands
            .iter()
            .any(|command| command == "DELE" || command == "RMD")
    );
}

#[tokio::test]
async fn ftp_confirmed_upload_does_not_recurse_into_a_file_replaced_by_a_directory() {
    let server = Endpoint::start(ServerOptions {
        tls: FtpTlsMode::Explicit,
        ..Default::default()
    })
    .await;
    server.write_file("/target", b"fixture placeholder");
    server.replace_file_with_directory("/target");
    server.write_file("/target/outside-selection", b"existing remote");
    let local = tempfile::tempdir().unwrap();
    let selected = local.path().join("selected");
    std::fs::write(&selected, b"selected file").unwrap();
    let approved = TransferOperation::Upload {
        local: selected.clone(),
        remote: path("/target"),
        policy: ConflictPolicy::Overwrite,
    };
    std::fs::rename(&selected, local.path().join("original")).unwrap();
    std::fs::create_dir(&selected).unwrap();
    std::fs::write(selected.join("outside-selection"), b"must not be sent").unwrap();
    let mut workspace = Workspace::start(93, server.options(), FtpTransferType::Binary).unwrap();
    ready(&mut workspace).await;
    let rejected = operation(&mut workspace, approved).await;
    assert!(matches!(rejected.outcome, Some(TransferOutcome::Failed(_))));
    assert!(rejected.recovery.is_none());
    assert!(server.evidence().uploads.is_empty());
    assert_eq!(
        server.read_file("/target/outside-selection").unwrap(),
        b"existing remote"
    );
    assert_eq!(
        std::fs::read(local.path().join("original")).unwrap(),
        b"selected file"
    );
}

#[tokio::test]
async fn ftp_confirmed_download_does_not_recurse_into_a_file_replaced_by_a_directory() {
    let server = Endpoint::start(ServerOptions {
        tls: FtpTlsMode::Explicit,
        ..Default::default()
    })
    .await;
    server.write_file("/selected", b"selected file");
    let local = tempfile::tempdir().unwrap();
    let selected = local.path().join("existing");
    std::fs::create_dir(&selected).unwrap();
    std::fs::write(selected.join("keep"), b"existing local").unwrap();
    let mut workspace = Workspace::start(94, server.options(), FtpTransferType::Binary).unwrap();
    ready(&mut workspace).await;
    assert_eq!(workspace.browser.entries[0].kind, EntryKind::File);
    let approved = TransferOperation::Download {
        remote: path("/selected"),
        local: selected.clone(),
        policy: ConflictPolicy::Overwrite,
    };
    server.replace_file_with_directory("/selected");
    server.write_file("/selected/keep", b"must not be transferred");
    let rejected = operation(&mut workspace, approved).await;
    assert!(matches!(rejected.outcome, Some(TransferOutcome::Failed(_))));
    assert!(rejected.recovery.is_none());
    assert!(
        !server
            .evidence()
            .commands
            .iter()
            .any(|command| command == "RETR")
    );
    assert_eq!(
        std::fs::read(selected.join("keep")).unwrap(),
        b"existing local"
    );
}
