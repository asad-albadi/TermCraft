//! Manifest contract checks only. check-ssh-interop.sh executes each declared
//! existing runtime assertion separately and emits an EvidenceRecord per cell.
#[derive(serde::Deserialize)]
struct InteropCase {
    id: String,
    endpoint: String,
    version: String,
    auth: String,
    trust: String,
    jump: String,
    operation: String,
    expected: String,
    target: Option<String>,
    test: Option<String>,
    unavailable: Option<String>,
}
fn matrix() -> Vec<InteropCase> {
    let cells: Vec<InteropCase> =
        serde_json::from_str(include_str!("../docs/parity-evidence/V01/matrix.json")).unwrap();
    let mut ids = std::collections::HashSet::new();
    for cell in &cells {
        assert!(ids.insert(&cell.id), "duplicate cell identity");
    }
    cells
}
fn check(cases: impl Iterator<Item = InteropCase>) {
    let mut count = 0;
    for c in cases {
        count += 1;
        for field in [
            &c.id,
            &c.endpoint,
            &c.version,
            &c.auth,
            &c.trust,
            &c.jump,
            &c.operation,
            &c.expected,
        ] {
            assert!(!field.is_empty(), "empty matrix dimension for {}", c.id);
        }
        if let Some(reason) = c.unavailable {
            assert!(!reason.is_empty());
        } else {
            let source =
                std::fs::read_to_string(format!("tests/{}.rs", c.target.unwrap())).unwrap();
            assert!(
                source.contains(&format!("fn {}(", c.test.unwrap())),
                "declared assertion missing: {}",
                c.id
            );
        }
    }
    assert!(count > 0);
}
#[test]
fn ssh_interop_auth_and_host_keys() {
    check(matrix().into_iter().filter(|c| !c.id.starts_with("sftp-")));
}
#[test]
fn sftp_interop_failure_and_reconnect() {
    check(
        matrix()
            .into_iter()
            .filter(|c| c.id.starts_with("sftp-") || c.id.starts_with("reconnect-")),
    );
}

#[cfg(unix)]
#[path = "support/dropbear_server.rs"]
mod dropbear_server;
#[cfg(unix)]
#[test]
#[ignore = "requires private signed Dropbear staging; check-ssh-interop.sh executes this fixture explicitly"]
fn independent_dropbear_key_shell_and_sftp_roundtrip() {
    use harbor_workspace::{
        remote_files::{
            RemotePath,
            transfer::{ConflictPolicy, TransferOperation, TransferOutcome},
        },
        ssh::{ConnectionEvent, ConnectionHandle},
    };
    use std::time::{Duration, Instant};
    let server = dropbear_server::DropbearServer::start();
    std::fs::write(server.root.path().join("fixture.txt"), b"DROPBEAR_SFTP_OK").unwrap();
    let mut handle = ConnectionHandle::connect(server.config()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let (mut shell, mut files) = (Vec::new(), false);
    loop {
        for event in handle.poll() {
            match event {
                ConnectionEvent::Output { bytes, .. } => shell.extend(bytes),
                ConnectionEvent::Directory(snapshot) => {
                    files |= snapshot
                        .entries
                        .iter()
                        .any(|f| f.display_name == "fixture.txt");
                }
                ConnectionEvent::Challenge { .. } => {
                    panic!("pinned key-auth endpoint unexpectedly requested credentials")
                }
                ConnectionEvent::Exited { message, .. } => panic!("{message}; {}", server.log()),
                _ => {}
            }
        }
        if files && String::from_utf8_lossy(&shell).contains("DROPBEAR_SHELL_OK") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Dropbear shell/SFTP unavailable: {}",
            server.log()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let bytes = (0..90_000).map(|n| (n % 256) as u8).collect::<Vec<_>>();
    let source = server.root.path().join("source");
    let remote = server.root.path().join("binary ' é\n$(literal)");
    let downloaded = server.root.path().join("downloaded");
    std::fs::write(&source, &bytes).unwrap();
    let path = RemotePath::from_bytes(remote.as_os_str().as_encoded_bytes().to_vec()).unwrap();
    for operation in [
        TransferOperation::Upload {
            local: source,
            remote: path.clone(),
            policy: ConflictPolicy::Ask,
        },
        TransferOperation::Download {
            remote: path,
            local: downloaded.clone(),
            policy: ConflictPolicy::Ask,
        },
    ] {
        let id = handle.transfers.submit(operation).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let completed = handle
                .transfers
                .poll()
                .into_iter()
                .find(|e| e.id == id && e.outcome.is_some());
            if let Some(event) = completed {
                assert_eq!(event.outcome, Some(TransferOutcome::Completed));
                break;
            }
            handle.poll();
            assert!(
                Instant::now() < deadline,
                "transfer timeout: {}",
                server.log()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    assert_eq!(std::fs::read(downloaded).unwrap(), bytes);
    assert_eq!(std::fs::read(remote).unwrap(), bytes);
    assert_eq!(
        server.log().matches("Pubkey auth succeeded").count(),
        1,
        "{}",
        server.log()
    );
    println!(
        "Dropbear 2026.94: pinned Ed25519 key, shell marker, SFTP 90000 exact bytes/literal filename, one authentication"
    );
}

#[cfg(unix)]
#[allow(dead_code)]
#[path = "support/ssh_server.rs"]
mod interop_ssh_server;
#[cfg(unix)]
#[test]
fn encrypted_key_agent_authenticates_pinned_openssh_then_expires() {
    use harbor_workspace::credentials::{
        Secret,
        agent::{AgentManager, AgentTarget},
        keys::{KeyManager, KeySettings},
    };
    use std::time::{Duration, Instant};
    let server = interop_ssh_server::SshServer::start(true);
    let original = KeyManager::generate(
        &server.root.join("encrypted-source"),
        KeySettings {
            passphrase: Some(Secret::new("private fixture passphrase")),
            ..Default::default()
        },
    )
    .unwrap();
    let encrypted = std::fs::read(&original.private).unwrap();
    assert!(
        KeyManager::inspect(&original.private, None)
            .unwrap()
            .encrypted
    );
    let imported = KeyManager::import(
        &original.private,
        &server.root.join("encrypted-import"),
        None,
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(std::fs::read(&imported.private).unwrap(), encrypted);
    assert_eq!(imported.fingerprint, original.fingerprint);
    std::fs::copy(&imported.public, server.root.join("authorized_keys")).unwrap();
    let mut manager = AgentManager::new(None);
    let endpoint = manager.start_owned().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(manager.add(
        AgentTarget::Owned,
        &imported.private,
        Some(Secret::new("private fixture passphrase")),
        Some(Duration::from_secs(2)),
    ))
    .unwrap();
    let authenticate = |id| {
        use harbor_workspace::ssh::{ConnectionEvent, ConnectionHandle};
        let mut config = server.config(id, true);
        config.session.key.clear();
        config.session.agent_mode = harbor_workspace::model::AgentMode::Owned;
        config.agent = Some(endpoint.clone());
        let mut handle = ConnectionHandle::connect(config).unwrap();
        let deadline = Instant::now() + Duration::from_secs(6);
        loop {
            for event in handle.poll() {
                match event {
                    ConnectionEvent::Connected { .. } => return true,
                    ConnectionEvent::Exited { .. } => return false,
                    ConnectionEvent::Challenge { .. } => {
                        panic!("strict pinned owned-agent endpoint must not request credentials")
                    }
                    _ => {}
                }
            }
            assert!(
                Instant::now() < deadline,
                "agent authentication did not finish"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    assert!(
        authenticate(9901),
        "encrypted key imported into owned agent must authenticate"
    );
    assert_eq!(
        server.accepted_authentications(),
        1,
        "encrypted identity must be observed authenticating at the pinned endpoint"
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while !rt
        .block_on(manager.list(AgentTarget::Owned))
        .unwrap()
        .identities
        .is_empty()
    {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        !authenticate(9902),
        "expired agent identity must no longer authenticate"
    );
    assert_eq!(
        server.accepted_authentications(),
        1,
        "expiry must not permit a second authentication"
    );
    manager.stop_owned().unwrap();
    println!(
        "Observed pinned OpenSSH: encrypted imported identity authenticated once; agent expiry rejected the next fresh connection"
    );
}
