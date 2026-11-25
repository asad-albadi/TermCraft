use harbor_workspace::cli::{Cli, CliAction};
#[test]
fn desktop_launch_preserves_arguments_and_workspace() {
    let input = "termcraft://session/stable%20id%3B%24%28x%29";
    let invocation = Cli::parse_invocation(
        ["--data-dir", "/tmp/work space", "--open-url", input].map(str::to_owned),
    )
    .unwrap();
    assert_eq!(
        invocation.data_dir.unwrap(),
        std::path::PathBuf::from("/tmp/work space")
    );
    assert_eq!(
        invocation.action,
        CliAction::OpenSession {
            id: "stable id;$(x)".into(),
            cwd: None,
            exit_when_done: false
        }
    );
    assert_eq!(
        Cli::parse(
            ["--open-url", "termcraft://cwd?path=%2Ftmp%2Fa%20%24%28x%29"].map(str::to_owned)
        )
        .unwrap(),
        CliAction::OpenFolder {
            path: "/tmp/a $(x)".into(),
            exit_when_done: false
        }
    );
}
#[test]
fn received_launch_cannot_run_commands_or_override_workspace() {
    for url in [
        "termcraft://command/echo",
        "termcraft://user:pass@session/id",
        "termcraft://session/id?command=echo",
        "termcraft://session/id#secret",
        "termcraft://cwd?path=relative",
        "termcraft://cwd?path=%2Ftmp&path=%2Felse",
        "termcraft://session/%FF",
    ] {
        assert!(
            Cli::parse(["--open-url", url].map(str::to_owned)).is_err(),
            "{url}"
        );
    }
    assert!(
        Cli::parse(
            ["--open-url", "termcraft://session/id", "--command", "echo"].map(str::to_owned)
        )
        .is_err()
    );
}
#[test]
fn link_file_is_bounded_strict_and_only_selects_safe_actions() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("bookmark.termcraft-link");
    std::fs::write(&file, r#"{"version":1,"session":"bookmark"}"#).unwrap();
    assert!(
        matches!(Cli::parse(vec!["--open-file".into(),file.to_str().unwrap().into()]).unwrap(),CliAction::OpenSession{id,..} if id=="bookmark")
    );
    for text in [
        r#"{"version":1,"session":"id","command":"evil"}"#,
        r#"{"version":2,"session":"id"}"#,
        r#"{"version":1,"cwd":"relative"}"#,
        r#"{"version":1,"session":"id","cwd":"/tmp"}"#,
    ] {
        std::fs::write(&file, text).unwrap();
        assert!(Cli::parse(vec!["--open-file".into(), file.to_str().unwrap().into()]).is_err());
    }
    std::fs::write(&file, vec![b'x'; 16385]).unwrap();
    assert!(Cli::parse(vec!["--open-file".into(), file.to_str().unwrap().into()]).is_err());
}
#[cfg(unix)]
#[test]
fn received_link_fifo_is_rejected_without_blocking() {
    use std::{ffi::CString, sync::mpsc, time::Duration};
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("pipe.termcraft-link");
    let name = CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = Cli::parse(vec!["--open-file".into(), path.to_str().unwrap().into()]);
        let _ = tx.send(result);
    });
    assert!(
        rx.recv_timeout(Duration::from_millis(500))
            .expect("a nonregular bookmark must not block startup")
            .is_err()
    );
}
