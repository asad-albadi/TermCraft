use harbor_workspace::{
    model::{Session, Store},
    sessions::{
        import_moba, import_putty,
        report::{DuplicatePolicy, apply},
    },
};
#[test]
fn moba_import_preserves_host_port_key_and_folder() {
    let report = import_moba(include_bytes!("fixtures/imports/moba-26.3-derived.ini")).unwrap();
    assert_eq!(report.candidates.len(), 1);
    let session = &report.candidates[0].session;
    assert_eq!(session.host, "example.test");
    assert_eq!(session.port, 2222);
    assert_eq!(session.key, "/keys/with spaces");
    assert_eq!(session.folder, "Team/Linux");
}
#[test]
fn unsupported_putty_option_is_reported() {
    let report = import_putty(include_bytes!("fixtures/imports/putty-0.85-derived.reg")).unwrap();
    assert!(
        report
            .unsupported
            .iter()
            .any(|field| field.field == "ProxyTelnetCommand")
    );
    assert_eq!(report.candidates[0].session.port, 2222);
    let serialized = serde_json::to_string(&report.candidates[0].session).unwrap();
    assert!(!serialized.contains("NEVER-PERSIST"));
    assert!(
        report
            .unsupported
            .iter()
            .all(|field| !field.message.contains("NEVER-PERSIST"))
    );
}
#[test]
fn malformed_import_does_not_mutate_store() {
    let mut store = Store::default();
    store.sessions.push(Session::default());
    let before = serde_json::to_value(&store).unwrap();
    assert!(import_moba(b"[Bookmarks]\nBad=#109#0%host%not-a-port%user").is_err());
    assert!(import_putty(b"Windows Registry Editor Version 5.00\n[broken").is_err());
    assert_eq!(serde_json::to_value(&store).unwrap(), before);
    assert!(import_moba(&vec![b'x'; 8 * 1024 * 1024 + 1]).is_err());
}
#[test]
fn duplicate_policies_preserve_or_regenerate_stable_ids() {
    let bytes = include_bytes!("fixtures/imports/moba-26.3-derived.ini");
    let mut store = Store::default();
    apply(
        &mut store,
        import_moba(bytes).unwrap(),
        DuplicatePolicy::Skip,
    )
    .unwrap();
    let original = store.sessions[0].id.clone();
    apply(
        &mut store,
        import_moba(bytes).unwrap(),
        DuplicatePolicy::Skip,
    )
    .unwrap();
    assert_eq!(store.sessions.len(), 1);
    apply(
        &mut store,
        import_moba(bytes).unwrap(),
        DuplicatePolicy::Replace,
    )
    .unwrap();
    assert_eq!(store.sessions[0].id, original);
    apply(
        &mut store,
        import_moba(bytes).unwrap(),
        DuplicatePolicy::KeepBoth,
    )
    .unwrap();
    assert_eq!(store.sessions.len(), 2);
    assert_ne!(store.sessions[1].id, original);
}
#[test]
fn encoding_variants_and_legacy_ids_round_trip_without_aliasing() {
    let utf8 = "[Bookmarks]\nSubRep=Team\nCafé=#109#0%example.test%22%alice\n";
    let (legacy, _, errors) = encoding_rs::WINDOWS_1252.encode(utf8);
    assert!(!errors);
    assert_eq!(
        import_moba(&legacy).unwrap().candidates[0].session.name,
        "Café"
    );
    let reg = include_str!("fixtures/imports/putty-0.85-derived.reg");
    let mut utf16 = vec![0xff, 0xfe];
    utf16.extend(reg.encode_utf16().flat_map(u16::to_le_bytes));
    assert_eq!(
        import_putty(&utf16).unwrap().candidates[0].session.host,
        "example.test"
    );
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("old.json");
    std::fs::write(&path, r#"{"sessions":[{"name":"A"},{"name":"A"}]}"#).unwrap();
    let mut store = Store::load(&path).unwrap();
    assert_ne!(store.sessions[0].id, store.sessions[1].id);
    let id = store.sessions[0].id.clone();
    store.sessions[0].name = "Renamed".into();
    store.save(&path).unwrap();
    assert_eq!(Store::load(&path).unwrap().sessions[0].id, id);
}
#[test]
fn malformed_registry_unicode_and_late_invalid_candidate_are_safe() {
    let prefix = "HKEY_CURRENT_USER\\Software\\SimonTatham\\PuTTY\\Sessions\\";
    let path = format!(
        "{}{}",
        "x".repeat((prefix.len() + 1) % 2),
        "é".repeat(prefix.len())
    );
    let bytes = format!("Windows Registry Editor Version 5.00\n[{path}]\n").into_bytes();
    assert!(
        std::panic::catch_unwind(|| import_putty(&bytes)).is_ok(),
        "unknown Unicode key must not panic"
    );
    let mut store = Store::default();
    let before = serde_json::to_value(&store).unwrap();
    let mut report = import_moba(include_bytes!("fixtures/imports/moba-26.3-derived.ini")).unwrap();
    let mut bad = report.candidates[0].clone();
    bad.session.port = 0;
    report.candidates.push(bad);
    assert!(apply(&mut store, report, DuplicatePolicy::KeepBoth).is_err());
    assert_eq!(serde_json::to_value(&store).unwrap(), before);
}
#[test]
fn documented_csv_preserves_quotes_and_reports_ignored_commands() {
    let report=import_moba(b"Session type,Hostname,Port number,Session name,Folder name,Key file,Command\r\nSSH,example.test,2222,\"App, server\",Team/Linux,\"/keys/with spaces\",not imported\r\n").unwrap();
    assert_eq!(report.candidates[0].session.name, "App, server");
    assert_eq!(report.candidates[0].session.key, "/keys/with spaces");
    assert!(report.candidates[0].session.command.is_empty());
    assert!(report.unsupported.iter().any(|f| f.field == "command"));
}
#[test]
fn declared_utf8_is_not_silently_reinterpreted_as_legacy_text() {
    let mut bytes = vec![0xef, 0xbb, 0xbf];
    bytes.extend_from_slice(b"[Bookmarks]\nCaf\xe9=#109#0%example.test%22%alice\n");
    assert!(
        import_moba(&bytes).is_err(),
        "invalid declared UTF-8 was silently converted"
    );
}
