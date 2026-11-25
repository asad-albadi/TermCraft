#![cfg(unix)]
#[allow(dead_code)]
#[path = "support/ssh_server.rs"]
mod ssh_server;
use harbor_workspace::{
    remote_files::{
        RemotePath,
        editor::{CloseDecision, DocumentStatus, RemoteDocument, SavePolicy},
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
fn wait(doc: &RemoteDocument, expected: DocumentStatus) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let snapshot = doc.snapshot();
        if snapshot.status == expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "expected {expected:?}, got {:?}",
            snapshot.status
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn opened(
    server: &ssh_server::SshServer,
    h: &ConnectionHandle,
) -> (RemoteDocument, std::path::PathBuf) {
    let remote = server.root.join("document ' é\n$(literal).txt");
    std::fs::write(&remote, b"v1").unwrap();
    let doc = RemoteDocument::open(
        92,
        path(&remote),
        &h.transfers,
        server.root.join("recovery"),
    )
    .unwrap();
    wait(&doc, DocumentStatus::Ready);
    (doc, remote)
}
#[test]
fn remote_save_conflict_preserves_both_versions() {
    let server = ssh_server::SshServer::start(true);
    let h = connected(&server);
    let (mut doc, remote) = opened(&server, &h);
    doc.set_text("mine").unwrap();
    std::fs::write(&remote, b"v2").unwrap();
    doc.save(SavePolicy::VerifyUnchanged).unwrap();
    wait(&doc, DocumentStatus::Conflict);
    assert_eq!(std::fs::read(&remote).unwrap(), b"v2");
    assert_eq!(
        std::fs::read(doc.snapshot().recovery_path.unwrap()).unwrap(),
        b"mine"
    );
    assert!(doc.snapshot().dirty);
    std::fs::write(&remote, b"v3").unwrap();
    doc.save(SavePolicy::Overwrite).unwrap();
    wait(&doc, DocumentStatus::Conflict);
    assert_eq!(
        std::fs::read(&remote).unwrap(),
        b"v3",
        "A replacement confirms the observed conflict version, not an unseen later edit"
    );
    doc.save(SavePolicy::Overwrite).unwrap();
    wait(&doc, DocumentStatus::Saved);
    assert_eq!(std::fs::read(remote).unwrap(), b"mine");
    assert!(!doc.snapshot().dirty);
    assert_eq!(server.accepted_authentications(), 1);
}
#[test]
fn failed_save_keeps_recoverable_edits() {
    let server = ssh_server::SshServer::start(true);
    let mut h = connected(&server);
    let (mut doc, remote) = opened(&server, &h);
    h.cancel();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !h
        .poll()
        .iter()
        .any(|e| matches!(e, ConnectionEvent::Exited { .. }))
    {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    doc.set_text("mine").unwrap();
    doc.save(SavePolicy::VerifyUnchanged).unwrap();
    wait(&doc, DocumentStatus::Disconnected);
    assert_eq!(
        std::fs::read(doc.snapshot().recovery_path.unwrap()).unwrap(),
        b"mine"
    );
    assert_eq!(std::fs::read(remote).unwrap(), b"v1");
    assert!(doc.snapshot().dirty);
}
#[test]
fn close_cancel_retains_dirty_document() {
    let server = ssh_server::SshServer::start(true);
    let h = connected(&server);
    let (mut doc, remote) = opened(&server, &h);
    doc.set_text("mine").unwrap();
    assert!(!doc.request_close(CloseDecision::Cancel).unwrap());
    assert_eq!(doc.snapshot().text, "mine");
    assert!(doc.snapshot().dirty);
    assert_eq!(doc.snapshot().connection, 92);
    assert_eq!(doc.snapshot().path, path(&remote));
    assert!(
        RemoteDocument::open(
            93,
            path(&remote),
            &h.transfers,
            server.root.join("recovery")
        )
        .is_err()
    );
    assert!(!doc.request_close(CloseDecision::Save).unwrap());
    wait(&doc, DocumentStatus::Saved);
    assert!(doc.request_close(CloseDecision::Cancel).unwrap());
    assert_eq!(std::fs::read(remote).unwrap(), b"mine");
}
#[test]
fn a_permission_restriction_during_staging_cannot_be_reversed_by_save() {
    use std::os::unix::fs::PermissionsExt;
    let server = ssh_server::SshServer::start(true);
    let h = connected(&server);
    let remote = server.root.join("permissions.txt");
    let original = "a".repeat(2 * 1024 * 1024);
    std::fs::write(&remote, original.as_bytes()).unwrap();
    std::fs::set_permissions(&remote, std::fs::Permissions::from_mode(0o644)).unwrap();
    let mut doc = RemoteDocument::open(
        92,
        path(&remote),
        &h.transfers,
        server.root.join("recovery"),
    )
    .unwrap();
    wait(&doc, DocumentStatus::Ready);
    doc.set_text(&"b".repeat(2 * 1024 * 1024)).unwrap();
    let root = server.root.clone();
    let target = remote.clone();
    let watcher = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if std::fs::read_dir(&root).unwrap().any(|e| {
                e.unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".termcraft-")
            }) {
                std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
                return;
            }
            assert!(Instant::now() < deadline, "No staging file observed");
            std::thread::sleep(Duration::from_micros(100));
        }
    });
    doc.save(SavePolicy::VerifyUnchanged).unwrap();
    watcher.join().unwrap();
    wait(&doc, DocumentStatus::Conflict);
    assert_eq!(
        std::fs::metadata(&remote).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(std::fs::read(&remote).unwrap(), original.as_bytes());
}
#[test]
fn unsupported_atomic_replace_cleans_up_owned_remote_staging() {
    let server = ssh_server::SshServer::start_with_sftp_options(true, "-P posix-rename");
    let h = connected(&server);
    let (mut doc, remote) = opened(&server, &h);
    doc.set_text("mine").unwrap();
    doc.save(SavePolicy::VerifyUnchanged).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while doc.snapshot().status.busy() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(matches!(doc.snapshot().status, DocumentStatus::Failed(_)));
    assert_eq!(std::fs::read(remote).unwrap(), b"v1");
    assert_eq!(
        std::fs::read(doc.snapshot().recovery_path.unwrap()).unwrap(),
        b"mine"
    );
    assert!(
        !std::fs::read_dir(&server.root).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".termcraft-")),
        "Failed save leaked an unreported remote staging file"
    );
}
#[test]
fn queued_editor_operations_can_be_cancelled_without_closing_ssh() {
    use harbor_workspace::remote_files::transfer::{ConflictPolicy, TransferOperation};
    let server = ssh_server::SshServer::start(true);
    let mut h = connected(&server);
    let remote = server.root.join("large-queue-blocker");
    std::fs::File::create(&remote)
        .unwrap()
        .set_len(1024 * 1024 * 1024)
        .unwrap();
    let id = h
        .transfers
        .submit(TransferOperation::Download {
            remote: path(&remote),
            local: server.root.join("download-blocker"),
            policy: ConflictPolicy::Ask,
        })
        .unwrap();
    let mut doc = RemoteDocument::open(
        92,
        path(&remote),
        &h.transfers,
        server.root.join("recovery"),
    )
    .unwrap();
    doc.cancel_operation();
    wait(&doc, DocumentStatus::Cancelled);
    h.transfers.cancel(id);
    assert!(!h.is_finished());
    assert!(doc.request_close(CloseDecision::Discard).unwrap());
}
#[test]
fn reconnect_import_requires_a_fresh_matching_endpoint_and_never_uploads_implicitly() {
    let server = ssh_server::SshServer::start(true);
    let h = connected(&server);
    let (mut source, remote) = opened(&server, &h);
    source.set_text("mine").unwrap();
    h.cancel();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !h.is_finished() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    std::fs::write(&remote, b"v2").unwrap();
    let fresh = ConnectionHandle::connect(server.config(93, true)).unwrap();
    let mut target = RemoteDocument::open(
        93,
        path(&remote),
        &fresh.transfers,
        server.root.join("fresh-recovery"),
    )
    .unwrap();
    wait(&target, DocumentStatus::Ready);
    target.import_recovery_from(&source).unwrap();
    assert_eq!(target.snapshot().text, "mine");
    assert!(target.snapshot().dirty);
    assert_eq!(target.snapshot().connection, 93);
    assert_eq!(std::fs::read(&remote).unwrap(), b"v2");
    let other_server = ssh_server::SshServer::start(true);
    let other = ConnectionHandle::connect(other_server.config(94, true)).unwrap();
    let mut wrong = RemoteDocument::open(
        94,
        path(&remote),
        &other.transfers,
        other_server.root.join("recovery"),
    )
    .unwrap();
    wait(&wrong, DocumentStatus::Ready);
    assert!(wrong.import_recovery_from(&source).is_err());
    assert_eq!(wrong.snapshot().text, "v2");
    target.save(SavePolicy::VerifyUnchanged).unwrap();
    wait(&target, DocumentStatus::Saved);
    assert_eq!(std::fs::read(remote).unwrap(), b"mine");
    assert_eq!(server.accepted_authentications(), 2);
}
