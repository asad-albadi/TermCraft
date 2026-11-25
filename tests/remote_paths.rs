use harbor_workspace::remote_files::{
    BrowserState, DirectorySnapshot, EntryKind, RemoteEntry, RemotePath,
};

#[test]
fn remote_paths_preserve_literal_bytes_and_use_posix_separators() {
    let root = RemotePath::from_bytes(b"/home/alice".to_vec()).unwrap();
    let child = root.child(b"a\\b 'quote'\n$(touch nope)").unwrap();
    assert_eq!(child.as_bytes(), b"/home/alice/a\\b 'quote'\n$(touch nope)");
    assert!(root.child(b"../outside").is_err());
    assert!(root.child(b"..").is_err());
    assert!(root.child(b".").is_err());
    assert!(root.child(b"nul\0name").is_err());
    assert!(RemotePath::from_bytes(b"/nul\0path".to_vec()).is_err());
    assert_eq!(
        RemotePath::from_bytes(b"/".to_vec())
            .unwrap()
            .parent()
            .as_bytes(),
        b"/"
    );
    assert_eq!(child.parent(), root);
    let bytes = vec![b'/', 0xff, b'x'];
    assert_eq!(
        RemotePath::from_bytes(bytes.clone()).unwrap().as_bytes(),
        bytes
    );
}

#[test]
fn stale_connection_or_request_cannot_replace_directory() {
    let mut browser = BrowserState::new(42);
    browser.begin(3);
    let snapshot = |connection, request| DirectorySnapshot {
        connection,
        request,
        path: RemotePath::from_bytes(b"/home".to_vec()).unwrap(),
        entries: vec![RemoteEntry {
            path: RemotePath::from_bytes(b"/home/fixture.txt".to_vec()).unwrap(),
            display_name: "fixture.txt".into(),
            kind: EntryKind::File,
            size: Some(7),
        }],
    };
    assert!(!browser.apply_snapshot(snapshot(41, 3)));
    assert!(!browser.apply_snapshot(snapshot(42, 2)));
    assert!(browser.apply_snapshot(snapshot(42, 3)));
    assert_eq!(browser.entries[0].display_name, "fixture.txt");
    browser.begin(4);
    browser.fail(42, 4, "Permission denied".into());
    assert_eq!(browser.entries[0].display_name, "fixture.txt");
    assert_eq!(browser.error.as_deref(), Some("Permission denied"));
    browser.fail(41, 4, "stale".into());
    assert_eq!(browser.error.as_deref(), Some("Permission denied"));
}
#[test]
fn displayed_paths_distinguish_raw_bytes_and_literal_escape_sequences() {
    use harbor_workspace::remote_files::RemotePath;
    for (a, b) in [
        (b"/file\xff".as_slice(), b"/file\xfe".as_slice()),
        (b"/file\n".as_slice(), b"/file\\n".as_slice()),
    ] {
        assert_ne!(
            RemotePath::from_bytes(a.to_vec()).unwrap().display(),
            RemotePath::from_bytes(b.to_vec()).unwrap().display()
        );
    }
}
#[test]
fn transfer_refresh_cannot_supersede_pending_user_navigation() {
    use harbor_workspace::remote_files::{BrowserState, DirectorySnapshot, RemotePath};
    let mut browser = BrowserState::new(42);
    browser.loading = false;
    browser.path = RemotePath::from_bytes(b"/old".to_vec()).unwrap();
    browser.begin(8);
    assert!(browser.refresh_path().is_none());
    let new_path = RemotePath::from_bytes(b"/new".to_vec()).unwrap();
    assert!(browser.apply_snapshot(DirectorySnapshot {
        connection: 42,
        request: 8,
        path: new_path.clone(),
        entries: vec![]
    }));
    assert_eq!(browser.refresh_path(), Some(new_path));
}
