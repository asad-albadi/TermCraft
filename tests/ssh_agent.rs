#![cfg(unix)]
#[allow(dead_code)]
#[path = "support/ssh_server.rs"]
mod ssh_server;
use harbor_workspace::{
    credentials::{
        agent::{AgentEndpoint, AgentManager, AgentTarget},
        keys::{KeyManager, KeySettings},
    },
    ssh::{ConnectionEvent, ConnectionHandle},
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}
fn authentication(config: harbor_workspace::ssh::ConnectionConfig) -> bool {
    let mut connection = ConnectionHandle::connect(config).unwrap();
    let end = Instant::now() + Duration::from_secs(6);
    loop {
        for event in connection.poll() {
            match event {
                ConnectionEvent::Connected { .. } => return true,
                ConnectionEvent::Exited { .. } => return false,
                ConnectionEvent::Challenge { .. } => panic!("Owned agent fixture must not prompt"),
                _ => {}
            }
        }
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn removing_identity_updates_authentication() {
    let server = ssh_server::SshServer::start(true);
    let mut manager = AgentManager::new(None);
    let endpoint = manager.start_owned().unwrap();
    let rt = runtime();
    let generated = KeyManager::generate(
        &server.root.join("managed agent key"),
        KeySettings::default(),
    )
    .unwrap();
    std::fs::write(
        server.root.join("authorized_keys"),
        std::fs::read(&generated.public).unwrap(),
    )
    .unwrap();
    let key = generated.private.to_string_lossy().into_owned();
    let info = rt
        .block_on(manager.add(AgentTarget::Owned, &PathBuf::from(key), None, None))
        .unwrap();
    let mut first = server.config(4001, true);
    first.session.key.clear();
    first.agent = Some(endpoint.clone());
    assert!(authentication(first));
    let mut disabled = server.config(4003, true);
    disabled.session.key.clear();
    disabled.session.agent_mode = harbor_workspace::model::AgentMode::Disabled;
    disabled.agent = Some(endpoint.clone());
    assert!(
        !authentication(disabled),
        "Disabled mode must ignore an otherwise usable agent"
    );
    rt.block_on(manager.remove(AgentTarget::Owned, &info.public_key))
        .unwrap();
    assert!(
        rt.block_on(manager.list(AgentTarget::Owned))
            .unwrap()
            .identities
            .is_empty()
    );
    let mut second = server.config(4002, true);
    second.session.key.clear();
    second.agent = Some(endpoint);
    assert!(!authentication(second));
    manager.stop_owned().unwrap();
}
#[test]
fn stop_owned_never_kills_system_agent() {
    let root = tempfile::tempdir().unwrap();
    let socket = root.path().join("system.sock");
    let mut process = std::process::Command::new("ssh-agent")
        .args(["-D", "-a"])
        .arg(&socket)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    struct Guard(std::process::Child);
    impl Drop for Guard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let end = Instant::now() + Duration::from_secs(3);
    while !socket.exists() {
        assert!(process.try_wait().unwrap().is_none());
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(5));
    }
    let mut process = Guard(process);
    let mut manager = AgentManager::new(Some(AgentEndpoint::Unix(socket)));
    manager.start_owned().unwrap();
    manager.stop_owned().unwrap();
    assert!(process.0.try_wait().unwrap().is_none());
    assert!(
        runtime()
            .block_on(manager.list(AgentTarget::System))
            .is_ok()
    );
    drop(manager);
    assert!(process.0.try_wait().unwrap().is_none());
}
#[test]
fn key_generation_never_overwrites_without_confirmation() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("existing key");
    std::fs::write(&path, b"ORIGINAL_PRIVATE").unwrap();
    std::fs::write(path.with_extension("pub"), b"ORIGINAL_PUBLIC").unwrap();
    assert!(KeyManager::generate(&path, KeySettings::default()).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"ORIGINAL_PRIVATE");
    assert_eq!(
        std::fs::read(path.with_extension("pub")).unwrap(),
        b"ORIGINAL_PUBLIC"
    );
    let generated =
        KeyManager::generate(&root.path().join("new key bundle"), KeySettings::default()).unwrap();
    assert!(generated.private.is_file());
    assert!(generated.public.is_file());
    let info = KeyManager::inspect(&generated.private, None).unwrap();
    assert_eq!(info.fingerprint, generated.fingerprint);
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        std::fs::metadata(generated.private)
            .unwrap()
            .permissions()
            .mode()
            & 0o077,
        0
    );
}
#[test]
fn protected_import_preserves_encryption_and_agent_lifetime_expires() {
    use harbor_workspace::credentials::Secret;
    let root = tempfile::tempdir().unwrap();
    let original = KeyManager::generate(
        &root.path().join("protected"),
        KeySettings {
            passphrase: Some(Secret::new("owned key passphrase")),
            ..Default::default()
        },
    )
    .unwrap();
    let bytes = std::fs::read(&original.private).unwrap();
    assert!(
        !bytes
            .windows(b"owned key passphrase".len())
            .any(|w| w == b"owned key passphrase")
    );
    assert!(
        KeyManager::inspect(&original.private, None)
            .unwrap()
            .encrypted
    );
    assert!(KeyManager::load_private(&original.private, Some(&Secret::new("wrong"))).is_err());
    let imported = KeyManager::import(
        &original.private,
        &root.path().join("imported"),
        None,
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(std::fs::read(&imported.private).unwrap(), bytes);
    assert_eq!(imported.fingerprint, original.fingerprint);
    let mut manager = AgentManager::new(None);
    manager.start_owned().unwrap();
    let rt = runtime();
    assert!(
        rt.block_on(manager.add(
            AgentTarget::Owned,
            &imported.private,
            Some(Secret::new("wrong")),
            None
        ))
        .is_err()
    );
    assert!(
        rt.block_on(manager.list(AgentTarget::Owned))
            .unwrap()
            .identities
            .is_empty()
    );
    rt.block_on(manager.add(
        AgentTarget::Owned,
        &imported.private,
        Some(Secret::new("owned key passphrase")),
        Some(Duration::from_secs(2)),
    ))
    .unwrap();
    assert_eq!(
        rt.block_on(manager.list(AgentTarget::Owned))
            .unwrap()
            .identities
            .len(),
        1
    );
    let end = Instant::now() + Duration::from_secs(4);
    while !rt
        .block_on(manager.list(AgentTarget::Owned))
        .unwrap()
        .identities
        .is_empty()
    {
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(20));
    }
}
#[test]
fn canceled_generation_publishes_no_key_bundle() {
    let root = tempfile::tempdir().unwrap();
    let destination = root.path().join("canceled");
    assert!(
        KeyManager::generate_cancelable(
            &destination,
            KeySettings::default(),
            &std::sync::atomic::AtomicBool::new(true)
        )
        .is_err()
    );
    assert!(!destination.exists());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}
#[test]
fn default_agent_mode_preserves_existing_vault_profile_bindings() {
    let mut session = harbor_workspace::model::Session {
        host: "example.test".into(),
        user: "alice".into(),
        key: "/keys/demo".into(),
        port: 22,
        jump: String::new(),
        ssh_engine: harbor_workspace::model::SshEngine::Embedded,
        ..Default::default()
    };
    assert_eq!(
        harbor_workspace::ssh::auth_profile_binding(&session),
        "144af1288c230b72da697822ab3f88b60a9be31b8f14c44959de8f960823a9de"
    );
    session.agent_mode = harbor_workspace::model::AgentMode::Disabled;
    assert_ne!(
        harbor_workspace::ssh::auth_profile_binding(&session),
        "144af1288c230b72da697822ab3f88b60a9be31b8f14c44959de8f960823a9de"
    );
}
#[test]
fn ppk_import_preserves_identity_and_encryption_in_openssh_format() {
    use harbor_workspace::credentials::Secret;
    let root = tempfile::tempdir().unwrap();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/keys/id_ed25519_enc.ppk");
    let imported = KeyManager::import(
        &source,
        &root.path().join("converted"),
        Some(Secret::new("123")),
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    assert!(
        KeyManager::inspect(&imported.private, None)
            .unwrap()
            .encrypted
    );
    let output = std::process::Command::new("ssh-keygen")
        .args(["-y", "-P", "123", "-f"])
        .arg(&imported.private)
        .output()
        .unwrap();
    assert!(output.status.success());
    let public =
        russh::keys::ssh_key::PublicKey::from_openssh(std::str::from_utf8(&output.stdout).unwrap())
            .unwrap();
    assert_eq!(
        public
            .fingerprint(russh::keys::ssh_key::HashAlg::Sha256)
            .to_string(),
        imported.fingerprint
    );
    let hostile = std::fs::read_to_string(&source)
        .unwrap()
        .replace("Argon2-Parallelism: 1", "Argon2-Parallelism: 17");
    let path = root.path().join("high-cost.ppk");
    std::fs::write(&path, hostile).unwrap();
    let result = KeyManager::import(
        &path,
        &root.path().join("rejected"),
        Some(Secret::new("123")),
        &std::sync::atomic::AtomicBool::new(false),
    );
    assert!(result.unwrap_err().contains("parallelism"));
    assert!(!root.path().join("rejected").exists());
}
#[test]
fn generated_ecdsa_and_rsa_keys_authenticate_with_openssh() {
    use harbor_workspace::credentials::keys::KeyAlgorithm;
    for (algorithm, name) in [
        (KeyAlgorithm::EcdsaP256, "p256"),
        (KeyAlgorithm::Rsa3072, "rsa3072"),
    ] {
        let server = ssh_server::SshServer::start(true);
        let pair = KeyManager::generate(
            &server.root.join(name),
            KeySettings {
                algorithm,
                ..Default::default()
            },
        )
        .unwrap();
        std::fs::write(
            server.root.join("authorized_keys"),
            std::fs::read(&pair.public).unwrap(),
        )
        .unwrap();
        let mut config = server.config(4010, true);
        config.session.key = pair.private.to_str().unwrap().into();
        assert!(authentication(config));
    }
}
