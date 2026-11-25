use harbor_workspace::{
    model::{Session, Store},
    sessions::shared::{SharedFormat, SharedLocation, SharedSource, SharedSourceConfig},
};
fn write(path: &std::path::Path, session: Session) {
    let mut store = Store::default();
    store.sessions.push(session);
    store.save(path).unwrap();
}
#[test]
fn shared_refresh_never_overwrites_local_session() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("shared.json");
    let original = Session {
        name: "Office".into(),
        host: "local.test".into(),
        ..Default::default()
    };
    let mut local = Store::default();
    local.sessions.push(original.clone());
    let before = serde_json::to_value(&local).unwrap();
    let mut upstream = Session {
        name: "Office".into(),
        host: "shared.test".into(),
        ..Default::default()
    };
    let upstream_id = upstream.id.clone();
    write(&path, upstream.clone());
    let config = SharedSourceConfig::new(
        "Team",
        SharedLocation::File(path.clone()),
        SharedFormat::Workspace,
    )
    .unwrap();
    let mut source = SharedSource::new(config);
    let first = source.refresh().unwrap();
    let first_id = first.entries[0].id.clone();
    assert_ne!(first_id, original.id);
    upstream.name = "Renamed upstream".into();
    write(&path, upstream);
    let next = source.refresh().unwrap();
    assert_eq!(next.entries[0].id, first_id);
    assert_eq!(next.entries[0].session.name, "Renamed upstream");
    assert_ne!(next.revision, first.revision);
    assert_eq!(serde_json::to_value(&local).unwrap(), before);
    let copy = source.copy_to_local(&first_id).unwrap();
    assert_ne!(copy.id, first_id);
    assert_ne!(copy.id, upstream_id);
    assert_eq!(copy.host, "shared.test");
}
#[test]
fn offline_source_keeps_labeled_cached_snapshot() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("shared.json");
    write(
        &path,
        Session {
            name: "Saved".into(),
            host: "shared.test".into(),
            ..Default::default()
        },
    );
    let config = SharedSourceConfig::new(
        "Team",
        SharedLocation::File(path.clone()),
        SharedFormat::Workspace,
    )
    .unwrap();
    let mut source = SharedSource::new(config);
    let snapshot = source.refresh().unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(source.refresh().is_err());
    let cached = source.snapshot().unwrap();
    assert!(cached.stale);
    assert_eq!(cached.entries[0].id, snapshot.entries[0].id);
    assert!(cached.last_error.is_some());
    let reloaded: SharedSourceConfig =
        serde_json::from_str(&serde_json::to_string(source.config()).unwrap()).unwrap();
    assert!(reloaded.cached.unwrap().stale);
}
#[test]
fn unsupported_or_credential_bearing_source_urls_are_rejected() {
    for uri in [
        "http://example.test/sessions",
        "ftp://example.test/sessions",
        "https://user:password@example.test/sessions",
    ] {
        assert!(SharedSourceConfig::from_url("Team", uri, SharedFormat::Workspace).is_err());
    }
}
#[test]
fn editing_source_metadata_preserves_identity_but_does_not_retarget_credentials() {
    let mut old = SharedSourceConfig::from_url(
        "Original",
        "https://one.test/sessions",
        SharedFormat::Workspace,
    )
    .unwrap();
    old.credential_ref = Some("vault-ref-1".into());
    let mut renamed = SharedSourceConfig::from_url(
        "Renamed",
        "https://one.test/sessions",
        SharedFormat::Workspace,
    )
    .unwrap();
    renamed.inherit_metadata(&old);
    assert_eq!(renamed.id, old.id);
    assert_eq!(renamed.credential_ref, old.credential_ref);
    let mut changed = SharedSourceConfig::from_url(
        "Changed",
        "https://two.test/sessions",
        SharedFormat::Workspace,
    )
    .unwrap();
    changed.inherit_metadata(&old);
    assert!(changed.credential_ref.is_none());
}
#[test]
fn shared_definitions_cannot_reference_local_vault_credentials() {
    use harbor_workspace::{
        model::{Session, Store},
        sessions::shared::{SharedFormat, SharedLocation, SharedSource, SharedSourceConfig},
    };
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("shared.json");
    let mut store = Store::default();
    store.sessions.push(Session {
        credential_ref: Some("LOCAL_VAULT_SENTINEL".into()),
        gateway_credential_ref: Some("LOCAL_GATEWAY_SENTINEL".into()),
        host: "example.test".into(),
        ..Default::default()
    });
    store.save(&path).unwrap();
    let cfg = SharedSourceConfig::new(
        "untrusted",
        SharedLocation::File(path),
        SharedFormat::Workspace,
    )
    .unwrap();
    let mut source = SharedSource::new(cfg);
    let snapshot = source.refresh().unwrap();
    assert!(snapshot.entries[0].session.credential_ref.is_none());
    assert!(snapshot.entries[0].session.gateway_credential_ref.is_none());
    assert!(
        !serde_json::to_string(&snapshot)
            .unwrap()
            .contains("LOCAL_VAULT_SENTINEL")
    );
}
