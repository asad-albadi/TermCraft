use std::process::Command;

#[test]
fn packaged_entry_reports_compiled_version_without_workspace_or_display() {
    let scratch = tempfile::tempdir().unwrap();
    let invoke = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_harbor-workspace"))
            .args(args)
            .current_dir(scratch.path())
            .env("HOME", scratch.path())
            .env("XDG_CONFIG_HOME", scratch.path())
            .env("LOCALAPPDATA", scratch.path())
            .env("APPDATA", scratch.path())
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .output()
            .unwrap()
    };
    let output = invoke(&["--version"]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim_end(),
        concat!("TermCraft ", env!("CARGO_PKG_VERSION"))
    );
    for args in [
        vec!["--version", "--help"],
        vec!["--data-dir", ".", "--version"],
        vec!["--version", "--version"],
    ] {
        assert_eq!(invoke(&args).status.code(), Some(2), "{args:?}");
    }
    assert_eq!(scratch.path().read_dir().unwrap().count(), 0);
}
