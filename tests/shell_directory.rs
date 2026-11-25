use harbor_workspace::{
    remote_files::{BrowserFollowMode, BrowserState, DirectorySnapshot, RemotePath},
    ssh::{
        ConnectionCommand,
        shell_integration::{ShellDirectoryTracker, ShellFlavor, integration_script},
    },
};
fn path(bytes: &[u8]) -> RemotePath {
    RemotePath::from_bytes(bytes.to_vec()).unwrap()
}
#[test]
fn split_osc7_updates_only_matching_connection() {
    let mut tracker = ShellDirectoryTracker::new("host");
    assert!(tracker.feed(b"normal\x1b").is_empty());
    assert!(tracker.feed(b"]7;file://ho").is_empty());
    let notifications = tracker.feed(b"st/home/a%20b\x1b\\");
    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].as_bytes(), b"/home/a b");
    let mut browser = BrowserState::new(42);
    browser.path = path(b"/kept");
    browser.loading = false;
    browser.set_follow_mode(BrowserFollowMode::FollowShell);
    assert!(
        browser
            .on_shell_directory(41, notifications[0].clone())
            .is_none()
    );
    let command = browser
        .on_shell_directory(42, notifications[0].clone())
        .unwrap();
    match command {
        ConnectionCommand::List { request, path } => {
            assert_eq!(path.as_bytes(), b"/home/a b");
            assert_eq!(request, browser.request);
        }
        _ => panic!("Directory metadata must only request a listing"),
    }
    assert_eq!(browser.path.as_bytes(), b"/kept");
}
#[test]
fn manual_mode_ignores_shell_directory() {
    let mut browser = BrowserState::new(42);
    browser.path = path(b"/manual");
    browser.loading = false;
    assert_eq!(browser.follow_mode(), BrowserFollowMode::Manual);
    assert!(browser.on_shell_directory(42, path(b"/shell")).is_none());
    assert_eq!(browser.path.as_bytes(), b"/manual");
    assert_eq!(browser.request, 0);
}
#[test]
fn unsupported_shell_keeps_manual_browser() {
    let mut tracker = ShellDirectoryTracker::new("host");
    let mut browser = BrowserState::new(42);
    browser.path = path(b"/kept");
    browser.loading = false;
    assert!(
        tracker
            .feed(b"ordinary shell output without integration\r\n")
            .is_empty()
    );
    assert!(tracker.manual_reason().is_some());
    assert!(integration_script(ShellFlavor::Unsupported, "host").is_err());
    assert_eq!(browser.path.as_bytes(), b"/kept");
    assert_eq!(browser.follow_mode(), BrowserFollowMode::Manual);
    assert_eq!(browser.request, 0);
}
#[test]
fn disabling_follow_invalidates_only_its_pending_listing() {
    let mut browser = BrowserState::new(42);
    assert!(browser.apply_snapshot(DirectorySnapshot {
        connection: 42,
        request: 0,
        path: path(b"/kept"),
        entries: vec![]
    }));
    browser.set_follow_mode(BrowserFollowMode::FollowShell);
    let command = browser.on_shell_directory(42, path(b"/shell")).unwrap();
    let ConnectionCommand::List { request, .. } = command else {
        panic!()
    };
    browser.set_follow_mode(BrowserFollowMode::Manual);
    assert!(!browser.loading);
    assert!(!browser.apply_snapshot(DirectorySnapshot {
        connection: 42,
        request,
        path: path(b"/shell"),
        entries: vec![]
    }));
    assert_eq!(browser.path.as_bytes(), b"/kept");
    browser.begin(9);
    browser.set_follow_mode(BrowserFollowMode::Manual);
    assert!(browser.apply_snapshot(DirectorySnapshot {
        connection: 42,
        request: 9,
        path: path(b"/chosen"),
        entries: vec![]
    }));
}
#[test]
fn disabling_follow_before_initial_listing_does_not_fabricate_an_empty_directory() {
    let mut browser = BrowserState::new(42);
    browser.set_follow_mode(BrowserFollowMode::FollowShell);
    browser.on_shell_directory(42, path(b"/shell")).unwrap();
    browser.set_follow_mode(BrowserFollowMode::Manual);
    assert!(
        browser.loading,
        "An uninitialized browser must wait for an actual manual listing"
    );
}
#[cfg(unix)]
#[test]
#[ignore = "requires local interactive Bash; run with --include-ignored"]
fn bash_prompt_pipeline_cleanup_and_export_regressions() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let setup = integration_script(ShellFlavor::Bash, "host").unwrap();
    let mut failures = Vec::new();
    for (name, prefix, suffix) in [
        (
            "pipeline",
            "PROMPT_COMMAND='printf \"PIPE:%s\\n\" \"${PIPESTATUS[*]}\"'\n",
            "false | true\nexit\n",
        ),
        (
            "cleanup",
            "PROMPT_COMMAND=true\n",
            "PROMPT_COMMAND=\"true; $PROMPT_COMMAND\"\n__termcraft_osc7_remove\nprintf CLEAN\\n\nexit\n",
        ),
        (
            "export",
            "export PROMPT_COMMAND='printf CHILD\\n'\n",
            "bash --noprofile --norc -i <<<'exit'\nexit\n",
        ),
    ] {
        let mut child = Command::new("bash")
            .args(["--noprofile", "--norc", "-i"])
            .env_remove("BASH_ENV")
            .env_remove("ENV")
            .env_remove("PROMPT_COMMAND")
            .env("PS1", "")
            .env("PS2", "")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(format!("{prefix}{setup}\n{suffix}").as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("command not found") {
            failures.push(format!("{name}: {stderr}"));
        }
        if name == "pipeline" && !stdout.contains("PIPE:1 0") {
            failures.push(format!("pipeline: {stdout}"));
        }
        if name == "export" && !stdout.contains("CHILD") {
            failures.push(format!("export: {stdout}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
#[test]
fn malformed_or_other_host_metadata_cannot_navigate() {
    for uri in [
        "file://other/home",
        "file://host/a%00b",
        "file://host/a%GG",
        "https://host/path",
        "file://user@host/path",
        "file://host/path?query",
        "file://host/path#fragment",
        "file://host",
    ] {
        let mut tracker = ShellDirectoryTracker::new("host");
        let message = format!("\x1b]7;{uri}\x07");
        assert!(tracker.feed(message.as_bytes()).is_empty(), "{uri}");
        assert!(tracker.last_directory().is_none());
    }
}
#[test]
fn notices_are_bounded_and_recover_after_a_terminated_oversized_message() {
    let mut tracker = ShellDirectoryTracker::new("host");
    let mut message = b"\x1b]7;file://host/".to_vec();
    message.extend(vec![b'a'; 140_000]);
    message.push(7);
    assert!(tracker.feed(&message).is_empty());
    assert!(tracker.manual_reason().is_some());
    let events = tracker.feed(b"\x1b]7;file://host/good%FF%20name\x07");
    assert_eq!(events[0].as_bytes(), b"/good\xff name");
    assert!(tracker.manual_reason().is_none());
    assert!(
        tracker
            .feed(b"\x1b]7;file://host/good%FF%20name\x07")
            .is_empty()
    );
}
#[cfg(unix)]
#[test]
#[ignore = "requires local Bash, Zsh and Fish; run with --include-ignored"]
fn opt_in_shell_scripts_report_literal_paths_and_restore_existing_hooks() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let root = std::env::temp_dir().join(format!("termcraft-osc7-{}", std::process::id()));
    let cwd = root.join("spaces ' é\n?hash#% $(touch pwn)");
    std::fs::create_dir_all(&cwd).unwrap();
    let cwd = std::fs::canonicalize(cwd).unwrap();
    for flavor in [ShellFlavor::Bash, ShellFlavor::Zsh, ShellFlavor::Fish] {
        let setup = integration_script(flavor, "host").unwrap();
        let (program, args, prefix, suffix) = match flavor {
            ShellFlavor::Bash => (
                "bash",
                vec!["--noprofile", "--norc"],
                "PROMPT_COMMAND='printf ORIGINAL'\n",
                "false\n__termcraft_osc7\ncode=$?\n[ \"$code\" = 1 ] || exit 61\n__termcraft_osc7_remove\n[ \"$PROMPT_COMMAND\" = 'printf ORIGINAL' ] || exit 62\nif declare -F __termcraft_osc7 >/dev/null; then exit 63; fi\n",
            ),
            ShellFlavor::Zsh => (
                "zsh",
                vec!["-f"],
                "precmd_functions=(original_hook)\n",
                "false\n__termcraft_osc7\ncode=$?\n[[ $code = 1 ]] || exit 61\n__termcraft_osc7_remove\n[[ ${#precmd_functions} = 1 && $precmd_functions[1] = original_hook ]] || exit 62\n(( ${+functions[__termcraft_osc7]} )) && exit 63\ntrue\n",
            ),
            ShellFlavor::Fish => (
                "fish",
                vec!["--no-config"],
                "function original_hook --on-event fish_prompt\nend\n",
                "false\n__termcraft_osc7\nset -l code $status\nif test $code -ne 1\nexit 61\nend\n__termcraft_osc7_remove\nif functions -q __termcraft_osc7\nexit 63\nend\nfunctions -q original_hook; or exit 62\n",
            ),
            _ => unreachable!(),
        };
        let mut child = Command::new(program)
            .args(args)
            .current_dir(&cwd)
            .env_remove("BASH_ENV")
            .env_remove("ENV")
            .env_remove("SHELLOPTS")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Shell-script fixture prerequisite is unavailable");
        let script = format!("{prefix}{setup}\n{setup}\n{suffix}");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(script.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{flavor:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut tracker = ShellDirectoryTracker::new("host");
        let events = tracker.feed(&output.stdout);
        assert_eq!(
            events.len(),
            1,
            "{flavor:?}: {:?}",
            String::from_utf8_lossy(&output.stdout)
        );
        use std::os::unix::ffi::OsStrExt;
        assert_eq!(
            events[0].as_bytes(),
            cwd.as_os_str().as_bytes(),
            "{flavor:?}"
        );
        assert!(!cwd.join("pwn").exists());
    }
    std::fs::remove_dir_all(root).unwrap();
}
