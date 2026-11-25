use harbor_workspace::{
    cli::{Cli, CliAction},
    ipc::{Binding, DispatchStatus, InstanceBroker},
};
use std::time::Duration;
#[test]
fn cli_missing_value_returns_error_not_panic() {
    for flag in [
        "--screenshot",
        "--data-dir",
        "--session",
        "--group",
        "--command",
        "--macro",
        "--cwd",
    ] {
        assert!(Cli::parse([flag.to_owned()]).is_err(), "{flag}");
    }
    assert!(Cli::parse(["--unknown".into()]).is_err());
    assert!(
        Cli::parse([
            "--group".into(),
            "Team".into(),
            "--session".into(),
            "id".into()
        ])
        .is_err()
    );
    let parsed = Cli::parse([
        "--session".into(),
        "saved-id".into(),
        "--cwd".into(),
        if cfg!(windows) {
            r"C:\path with spaces"
        } else {
            "/path with spaces"
        }
        .into(),
    ])
    .unwrap();
    assert!(matches!(parsed,CliAction::OpenSession{id,cwd:Some(_),..} if id=="saved-id"));
}
#[test]
fn same_workspace_dispatches_exactly_once() {
    let root = tempfile::tempdir().unwrap();
    let Binding::Primary(mut primary) = InstanceBroker::bind(root.path()).unwrap() else {
        panic!("owned fixture already active")
    };
    let Binding::Secondary(client) = InstanceBroker::bind(root.path()).unwrap() else {
        panic!("duplicate primary")
    };
    let action = CliAction::NewTab {
        cwd: None,
        exit_when_done: false,
    };
    let receipt = client
        .send_with_id("owned-request-id", action.clone())
        .unwrap();
    assert_eq!(receipt.status, DispatchStatus::Accepted);
    assert_eq!(
        client.send_with_id("owned-request-id", action).unwrap(),
        receipt
    );
    let request = primary.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(request.id, "owned-request-id");
    assert!(primary.recv_timeout(Duration::from_millis(100)).is_err());
    assert!(
        client
            .send_with_id("owned-request-id", CliAction::Help)
            .is_err()
    );
    assert!(
        client
            .send_with_id(
                "oversize",
                CliAction::RunCommand {
                    command: "X".repeat(70_000),
                    session: None,
                    cwd: None,
                    exit_when_done: false
                }
            )
            .is_err()
    );
}
#[test]
fn different_data_dirs_do_not_share_commands() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let Binding::Primary(mut pa) = InstanceBroker::bind(a.path()).unwrap() else {
        panic!()
    };
    let Binding::Primary(mut pb) = InstanceBroker::bind(b.path()).unwrap() else {
        panic!()
    };
    let Binding::Secondary(client) = InstanceBroker::bind(a.path()).unwrap() else {
        panic!()
    };
    client
        .send(CliAction::NewTab {
            cwd: None,
            exit_when_done: false,
        })
        .unwrap();
    assert!(pa.recv_timeout(Duration::from_secs(1)).is_ok());
    assert!(pb.recv_timeout(Duration::from_millis(100)).is_err());
}
#[test]
fn malformed_oversized_and_unauthenticated_ipc_are_rejected() {
    use interprocess::local_socket::{GenericNamespaced, Stream, prelude::*};
    use std::io::{Read, Write};
    let root = tempfile::tempdir().unwrap();
    let Binding::Primary(mut primary) = InstanceBroker::bind(root.path()).unwrap() else {
        panic!()
    };
    let record = if cfg!(windows) {
        "instance.json"
    } else {
        "instance.lock"
    };
    let metadata: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.path().join(record)).unwrap()).unwrap();
    for payload in [b"not-json".to_vec(),serde_json::to_vec(&serde_json::json!({"version":1,"workspace":metadata["workspace"],"token":"wrong","id":"bad-auth","action":{"NewTab":{"cwd":null,"exit_when_done":false}}})).unwrap(),vec![b'X';65537]]{
  let name=metadata["name"].as_str().unwrap().to_ns_name::<GenericNamespaced>().unwrap();let mut stream=Stream::connect(name).unwrap();
  #[cfg(unix)]
  stream.set_recv_timeout(Some(Duration::from_secs(2))).unwrap();
  stream.write_all(&(payload.len()as u32).to_be_bytes()).unwrap();if payload.len()<=65536{stream.write_all(&payload).unwrap();}
  let mut len=[0;4];stream.read_exact(&mut len).unwrap();let mut response=vec![0;u32::from_be_bytes(len)as usize];stream.read_exact(&mut response).unwrap();
  let response:serde_json::Value=serde_json::from_slice(&response).unwrap();assert!(response["receipt"].is_null());assert!(response["error"].is_string());
 }
    assert!(primary.recv_timeout(Duration::from_millis(100)).is_err());
}
#[cfg(unix)]
#[test]
fn instance_record_is_private_and_symlinks_are_rejected() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let root = tempfile::tempdir().unwrap();
    let Binding::Primary(primary) = InstanceBroker::bind(root.path()).unwrap() else {
        panic!()
    };
    let path = root.path().join("instance.lock");
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o077,
        0
    );
    drop(primary);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(InstanceBroker::bind(root.path()).is_err());
    std::fs::remove_file(&path).unwrap();
    let sentinel = root.path().join("untouched");
    std::fs::write(&sentinel, b"original").unwrap();
    symlink(&sentinel, &path).unwrap();
    assert!(InstanceBroker::bind(root.path()).is_err());
    assert_eq!(std::fs::read(sentinel).unwrap(), b"original");
}
#[cfg(target_os = "linux")]
#[test]
fn generated_shortcut_dispatches_through_the_real_binary_without_another_gui() {
    use harbor_workspace::platform::shortcuts::ShortcutSpec;
    use std::process::Command;
    let root = tempfile::tempdir().unwrap();
    let Binding::Primary(mut primary) = InstanceBroker::bind(root.path()).unwrap() else {
        panic!()
    };
    let cwd = root.path().join("cwd with \" quotes $HOME %f");
    std::fs::create_dir(&cwd).unwrap();
    let shortcut = ShortcutSpec::for_session("saved-stable-id".into())
        .unwrap()
        .with_program(std::path::PathBuf::from(env!(
            "CARGO_BIN_EXE_harbor-workspace"
        )))
        .unwrap()
        .with_workspace(root.path().to_owned())
        .unwrap()
        .with_cwd(cwd.clone())
        .unwrap();
    let path = root.path().join("session.desktop");
    shortcut.write_new(&path, "Owned session").unwrap();
    let launched = Command::new("gio")
        .args(["launch"])
        .arg(path)
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .unwrap();
    assert!(
        launched.status.success(),
        "{}",
        String::from_utf8_lossy(&launched.stderr)
    );
    let received = primary.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(
        received.action,
        CliAction::OpenSession {
            id: "saved-stable-id".into(),
            cwd: Some(cwd),
            exit_when_done: false
        }
    );
    assert!(primary.recv_timeout(Duration::from_millis(100)).is_err());
}

#[cfg(unix)]
#[test]
fn instance_record_cannot_overwrite_a_hard_linked_file() {
    let root = tempfile::tempdir().unwrap();
    let sentinel = root.path().join("keep");
    std::fs::write(&sentinel, b"KEEP_ORIGINAL").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&sentinel, std::fs::Permissions::from_mode(0o600)).unwrap();
    std::fs::hard_link(&sentinel, root.path().join("instance.lock")).unwrap();
    assert!(InstanceBroker::bind(root.path()).is_err());
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"KEEP_ORIGINAL");
}
