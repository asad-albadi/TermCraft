use harbor_workspace::{
    credentials::{Secret, vault::Vault},
    desktop::rdp_options::*,
    model::Session,
};

#[test]
fn desktop_password_is_bound_to_endpoint_certificate_user_domain_and_profile() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("desktop.vault");
    let mut vault = Vault::create(&path, Secret::new("owned desktop master")).unwrap();
    let session = Session {
        protocol: "RDP".into(),
        host: "server.example".into(),
        user: "alice".into(),
        ..Default::default()
    };
    let certificate = "ab".repeat(32);
    let binding = credential_binding(
        &session,
        "server.example",
        session.port,
        &certificate,
        "alice",
        "WORK",
    )
    .unwrap();
    vault
        .put_bound(
            "desktop".into(),
            Secret::new("private RDP password"),
            binding.clone(),
        )
        .unwrap();
    assert_eq!(
        vault
            .get_bound(&"desktop".into(), &binding)
            .unwrap()
            .expose(),
        "private RDP password"
    );
    for other in [
        credential_binding(
            &session,
            "server.example",
            session.port,
            &"cd".repeat(32),
            "alice",
            "WORK",
        )
        .unwrap(),
        credential_binding(
            &session,
            "server.example",
            session.port,
            &certificate,
            "bob",
            "WORK",
        )
        .unwrap(),
        credential_binding(
            &session,
            "server.example",
            session.port,
            &certificate,
            "alice",
            "OTHER",
        )
        .unwrap(),
    ] {
        assert!(vault.get_bound(&"desktop".into(), &other).is_err());
    }
    let mut changed = session.clone();
    changed.rdp_options.security = RdpSecurity::Tls;
    let other = credential_binding(
        &changed,
        "server.example",
        changed.port,
        &certificate,
        "alice",
        "WORK",
    )
    .unwrap();
    assert!(vault.get_bound(&"desktop".into(), &other).is_err());
    assert!(
        credential_binding(
            &session,
            "attacker.example",
            session.port,
            &certificate,
            "alice",
            "WORK"
        )
        .is_err()
    );
    assert!(
        credential_binding(&session, "server.example", 1, &certificate, "alice", "WORK").is_err()
    );
    assert!(
        credential_binding(
            &session,
            "server.example",
            session.port,
            "",
            "alice",
            "WORK"
        )
        .is_err()
    );
    assert!(
        credential_binding(
            &session,
            "server.example",
            session.port,
            &certificate,
            "",
            "WORK"
        )
        .is_err()
    );
    let bytes = std::fs::read(path).unwrap();
    assert!(
        !bytes
            .windows(b"private RDP password".len())
            .any(|w| w == b"private RDP password")
    );
    assert!(
        !serde_json::to_string(&session)
            .unwrap()
            .contains("private RDP password")
    );
    vault.lock();
    assert!(vault.get_bound(&"desktop".into(), &binding).is_err());
}
