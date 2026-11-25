use harbor_workspace::{
    model::{Session, Store},
    platform::shortcuts::ShortcutSpec,
    sessions::groups::SessionGroup,
};
#[test]
fn group_launch_reports_per_session_outcomes() {
    let mut store = Store::default();
    for (name, host, folder) in [
        ("first", "a.test", "Team"),
        ("broken", "-bad", "Team"),
        ("second", "b.test", "Team/Sub"),
        ("other", "else.test", "Teamwork"),
    ] {
        store.sessions.push(Session {
            name: name.into(),
            host: host.into(),
            folder: folder.into(),
            ..Default::default()
        });
    }
    let resolved = SessionGroup::resolve("Team", &store);
    assert_eq!(resolved.len(), 3);
    assert_eq!(resolved[0].id, store.sessions[0].id);
    let report = SessionGroup::launch("Team", &store, |s| {
        harbor_workspace::model::validate_host(&s.host)
    });
    assert_eq!(report.accepted.len(), 2);
    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.failed[0].id, store.sessions[1].id);
    assert_eq!(store.sessions.len(), 4);
}
#[cfg(target_os = "linux")]
#[test]
fn shortcut_preserves_space_and_quote_arguments() {
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        process::Command,
        time::{Duration, Instant},
    };
    let root = tempfile::tempdir().unwrap();
    let executable = root.path().join("owned recorder");
    let capture = root.path().join("captured.json");
    fs::write(
        &executable,
        format!(
            "#!/usr/bin/python3\nimport sys,json\nopen({:?},'w').write(json.dumps(sys.argv[1:]))\n",
            capture.to_str().unwrap()
        ),
    )
    .unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    let cwd = root.path().join("space \" quote $HOME `id` %f ; literal");
    fs::create_dir(&cwd).unwrap();
    let spec = ShortcutSpec::for_session("stable-saved-id".into())
        .unwrap()
        .with_program(executable.clone())
        .unwrap()
        .with_workspace(root.path().join("workspace with spaces"))
        .unwrap()
        .with_cwd(cwd.clone())
        .unwrap();
    assert_eq!(
        spec.arguments()
            .iter()
            .filter(|s| s.as_str() == cwd.to_str().unwrap())
            .count(),
        1
    );
    let path = root.path().join("launch.desktop");
    spec.write_new(&path, "Office \" session").unwrap();
    assert!(
        Command::new("desktop-file-validate")
            .arg(&path)
            .output()
            .unwrap()
            .status
            .success()
    );
    let launched = Command::new("gio")
        .arg("launch")
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        launched.status.success(),
        "{}",
        String::from_utf8_lossy(&launched.stderr)
    );
    let end = Instant::now() + Duration::from_secs(3);
    while !capture.is_file() && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(5));
    }
    let args: Vec<String> = serde_json::from_slice(&fs::read(capture).unwrap()).unwrap();
    assert_eq!(args, spec.arguments());
    assert!(spec.write_new(&path, "Overwrite").is_err());
    assert!(!ShortcutSpec::for_session("bad\nID".into()).is_ok());
}
