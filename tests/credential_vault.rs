use harbor_workspace::{
    credentials::{Secret, vault::Vault},
    model::{Session, Store},
};
#[test]
fn wrong_master_and_tamper_cannot_decrypt() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("vault.bin");
    let mut vault = Vault::create(&path, Secret::new("owned master password")).unwrap();
    vault
        .put("fixture-credential".into(), Secret::new("SENTINEL_SECRET"))
        .unwrap();
    assert_eq!(
        vault.get(&"fixture-credential".into()).unwrap().expose(),
        "SENTINEL_SECRET"
    );
    let bytes = std::fs::read(&path).unwrap();
    assert!(
        !bytes
            .windows(b"SENTINEL_SECRET".len())
            .any(|w| w == b"SENTINEL_SECRET")
    );
    assert!(Vault::unlock(&path, Secret::new("wrong master password")).is_err());
    vault.lock();
    assert!(vault.get(&"fixture-credential".into()).is_err());
    let mut tampered = bytes.clone();
    let n = tampered.len() - 1;
    tampered[n] ^= 1;
    std::fs::write(&path, &tampered).unwrap();
    assert!(Vault::unlock(&path, Secret::new("owned master password")).is_err());
    std::fs::write(&path, &bytes).unwrap();
    let mut unlocked = Vault::unlock(&path, Secret::new("owned master password")).unwrap();
    unlocked.delete(&"fixture-credential".into()).unwrap();
    assert!(unlocked.get(&"fixture-credential".into()).is_err());
}
#[test]
fn session_export_contains_reference_not_password() {
    let root = tempfile::tempdir().unwrap();
    let mut vault = Vault::create(
        &root.path().join("vault.bin"),
        Secret::new("owned master password"),
    )
    .unwrap();
    vault
        .put("opaque-vault-entry".into(), Secret::new("SENTINEL_SECRET"))
        .unwrap();
    let mut store = Store::default();
    store.sessions.push(Session {
        credential_ref: Some("opaque-vault-entry".into()),
        ..Default::default()
    });
    let json = serde_json::to_string(&store).unwrap();
    assert!(json.contains("opaque-vault-entry"));
    assert!(!json.contains("SENTINEL_SECRET"));
    let reloaded: Store = serde_json::from_str(&json).unwrap();
    assert_eq!(
        reloaded.sessions[0].credential_ref.as_deref(),
        Some("opaque-vault-entry")
    );
}
#[test]
fn failed_master_change_keeps_old_vault_usable() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("secured");
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("vault.bin");
    let mut vault = Vault::create(&path, Secret::new("old owned master")).unwrap();
    vault
        .put("entry".into(), Secret::new("SENTINEL_SECRET"))
        .unwrap();
    let original = std::fs::read(&path).unwrap();
    let displaced = root.path().join("displaced");
    std::fs::rename(&directory, &displaced).unwrap();
    std::fs::write(&directory, b"owned filesystem commit blocker").unwrap();
    assert!(
        vault
            .change_master(Secret::new("new owned master"))
            .is_err()
    );
    std::fs::remove_file(&directory).unwrap();
    std::fs::rename(&displaced, &directory).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert_eq!(
        vault.get(&"entry".into()).unwrap().expose(),
        "SENTINEL_SECRET"
    );
    assert!(Vault::unlock(&path, Secret::new("old owned master")).is_ok());
    assert!(Vault::unlock(&path, Secret::new("new owned master")).is_err());
    vault
        .change_master(Secret::new("new owned master"))
        .unwrap();
    vault.lock();
    assert!(Vault::unlock(&path, Secret::new("old owned master")).is_err());
    assert_eq!(
        Vault::unlock(&path, Secret::new("new owned master"))
            .unwrap()
            .get(&"entry".into())
            .unwrap()
            .expose(),
        "SENTINEL_SECRET"
    );
}
#[test]
fn bound_credentials_reject_other_origins_and_unbound_entries() {
    use harbor_workspace::credentials::CredentialBinding;
    let root = tempfile::tempdir().unwrap();
    let mut vault = Vault::create(
        &root.path().join("vault"),
        Secret::new("owned master password"),
    )
    .unwrap();
    let binding = CredentialBinding::HttpsBasic {
        source_id: "owned-source".into(),
        url: "https://team.test/private/sessions".into(),
    };
    vault
        .put_bound(
            "bound".into(),
            Secret::new("SENTINEL_SECRET"),
            binding.clone(),
        )
        .unwrap();
    assert_eq!(
        vault.get_bound(&"bound".into(), &binding).unwrap().expose(),
        "SENTINEL_SECRET"
    );
    for changed in [
        CredentialBinding::HttpsBasic {
            source_id: "other-source".into(),
            url: "https://team.test/private/sessions".into(),
        },
        CredentialBinding::HttpsBasic {
            source_id: "owned-source".into(),
            url: "https://other.test/private/sessions".into(),
        },
    ] {
        assert!(vault.get_bound(&"bound".into(), &changed).is_err());
    }
    vault
        .put("unbound".into(), Secret::new("manual entry"))
        .unwrap();
    assert!(vault.get_bound(&"unbound".into(), &binding).is_err());
}
#[test]
fn stale_vault_writer_preserves_committed_data() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("vault");
    let mut first = Vault::create(&path, Secret::new("owned master password")).unwrap();
    let mut stale = Vault::unlock(&path, Secret::new("owned master password")).unwrap();
    first
        .put("committed".into(), Secret::new("KEEP_SECRET"))
        .unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert!(
        stale
            .put("stale".into(), Secret::new("DROP_SECRET"))
            .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert!(stale.get(&"stale".into()).is_err());
}
#[cfg(unix)]
#[test]
fn vault_replacements_keep_private_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("vault");
    let mut vault = Vault::create(&path, Secret::new("owned master password")).unwrap();
    vault
        .put("owned".into(), Secret::new("SENTINEL_SECRET"))
        .unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o077,
        0
    );
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(Vault::unlock(&path, Secret::new("owned master password")).is_err());
}
