#![cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]

#[path = "support/xserver.rs"]
mod support;

use harbor_workspace::x11::{XServerManager, XServerOptions};
use std::os::unix::{fs::PermissionsExt, net::UnixStream};

const ARTIFACT: &str = "x11/linux-x86_64/Xwayland";

fn start_error(root: &std::path::Path, options: XServerOptions) -> String {
    let result = XServerManager::for_installation(root.to_path_buf()).start(options);
    let Err(error) = result else {
        panic!("unexpectedly started an X server");
    };
    error
}

#[test]
fn missing_packaged_engine_is_reported_without_system_fallback() {
    let root = tempfile::tempdir().unwrap();
    let error = start_error(root.path(), XServerOptions::default());
    assert!(error.contains("Missing packaged X server"), "{error}");
}

#[test]
fn damaged_or_non_regular_engine_is_rejected_before_start() {
    let root = tempfile::tempdir().unwrap();
    let artifact = root.path().join(ARTIFACT);
    std::fs::create_dir_all(&artifact).unwrap();
    let error = start_error(root.path(), XServerOptions::default());
    assert!(error.contains("regular executable"), "{error}");

    std::fs::remove_dir(&artifact).unwrap();
    std::fs::write(&artifact, "not the pinned Xwayland engine").unwrap();
    std::fs::set_permissions(&artifact, std::fs::Permissions::from_mode(0o700)).unwrap();
    let error = start_error(root.path(), XServerOptions::default());
    assert!(error.contains("checksum mismatch"), "{error}");
}

#[test]
fn unsupported_artifact_and_monitor_are_reported_before_start() {
    let root = tempfile::tempdir().unwrap();
    let error = start_error(
        root.path(),
        XServerOptions {
            artifact_id: "/usr/bin/Xwayland".into(),
            ..XServerOptions::default()
        },
    );
    assert!(
        error.contains("Unsupported packaged X server artifact"),
        "{error}"
    );
    let error = start_error(
        root.path(),
        XServerOptions {
            monitor: Some("monitor-1".into()),
            ..XServerOptions::default()
        },
    );
    assert!(
        error.contains("monitor selection is unavailable"),
        "{error}"
    );
}

#[test]
#[ignore = "requires the staged package and a working Wayland desktop"]
fn packaged_xserver_starts_without_external_install() {
    let display_before = std::env::var_os("DISPLAY");
    let authority_before = std::env::var_os("XAUTHORITY");
    let lease = support::start();
    let pid = lease.process_id().unwrap();
    let executable = format!("/proc/{pid}/exe");
    let executed = std::fs::read_link(&executable).unwrap();
    let package = std::env::var_os("TERMCRAFT_TEST_XSERVER_PACKAGE").unwrap();
    let engine = std::path::PathBuf::from(package)
        .join(ARTIFACT)
        .canonicalize()
        .unwrap();
    assert_eq!(
        executed,
        std::path::Path::new("/memfd:termcraft-xwayland (deleted)")
    );
    assert_eq!(
        std::fs::read(executable).unwrap(),
        std::fs::read(&engine).unwrap()
    );
    use std::os::unix::ffi::OsStrExt;
    let command = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap();
    assert_eq!(
        command.split(|byte| *byte == 0).next().unwrap(),
        engine.as_os_str().as_bytes()
    );
    let client = support::client(&lease, true).unwrap();
    let packaged_x_client_window_visible = support::window_is_visible_on_display(&client);
    assert!(packaged_x_client_window_visible);
    assert_eq!(std::env::var_os("DISPLAY"), display_before);
    assert_eq!(std::env::var_os("XAUTHORITY"), authority_before);
    assert!(!lease.display().is_empty());
}

#[test]
#[ignore = "requires the staged package, a working Wayland desktop, and a pre-existing display :0"]
fn display_collision_does_not_kill_existing_server() {
    use std::os::unix::fs::MetadataExt;
    let manager = support::packaged_manager();
    let original = support::socket(":0");
    let metadata = original
        .metadata()
        .expect("display :0 must already exist for this test");
    let original_socket = (metadata.dev(), metadata.ino());
    let original_client = UnixStream::connect(&original).unwrap();

    let lease = manager.start(XServerOptions::default()).unwrap();
    assert_ne!(lease.display(), ":0");
    lease.stop_owned().unwrap();

    let metadata = original.metadata().unwrap();
    let fresh = UnixStream::connect(&original).unwrap();
    let same_process = peer_pid(&original_client) == peer_pid(&fresh);
    let preexisting_display_still_running =
        original_socket == (metadata.dev(), metadata.ino()) && same_process;
    assert!(preexisting_display_still_running);
    let pid: u32 = std::env::var("TERMCRAFT_TEST_XSERVER_FIXTURE_PID")
        .unwrap()
        .parse()
        .unwrap();
    assert!(std::path::Path::new(&format!("/proc/{pid}/exe")).exists());
}

#[test]
#[ignore = "requires the staged package and a working Wayland desktop"]
fn stop_only_terminates_owned_engine() {
    let first = support::start();
    let mut second = support::start();
    assert_ne!(first.display(), second.display());
    let first_socket = support::socket(first.display());
    let first_auth = first.authority_path().to_owned();
    let pid = first.process_id().unwrap();
    let mut parent = first_auth.parent().unwrap().to_path_buf();
    first.stop_owned().unwrap();
    assert!(!parent.exists());
    let owned_servers_after_stop =
        usize::from(std::path::Path::new(&format!("/proc/{pid}")).exists());
    assert_eq!(owned_servers_after_stop, 0);
    assert!(UnixStream::connect(first_socket).is_err());
    assert!(second.is_running().unwrap());
    assert!(support::window_is_visible_on_display(
        &support::client(&second, true).unwrap()
    ));
    parent = second.authority_path().parent().unwrap().to_path_buf();
    let second_socket = support::socket(second.display());
    drop(second);
    assert!(!parent.exists());
    assert!(UnixStream::connect(second_socket).is_err());
}

#[test]
#[ignore = "requires the staged package and a working Wayland desktop"]
fn wrong_cookie_is_rejected() {
    use x11rb::errors::ConnectError;
    let lease = support::start();
    assert!(matches!(
        support::client(&lease, false),
        Err(ConnectError::SetupFailed(_))
    ));
    assert!(support::client(&lease, true).is_ok());
    let display: u16 = lease.display().trim_start_matches(':').parse().unwrap();
    assert!(std::net::TcpStream::connect(("127.0.0.1", 6000 + display)).is_err());
}

fn peer_pid(socket: &UnixStream) -> libc::pid_t {
    use std::os::fd::AsRawFd;

    let mut cred = std::mem::MaybeUninit::<libc::ucred>::uninit();
    let mut length = size_of::<libc::ucred>() as libc::socklen_t;
    let result = unsafe {
        libc::getsockopt(
            socket.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            cred.as_mut_ptr().cast(),
            &mut length,
        )
    };
    assert_eq!(result, 0);
    assert_eq!(length as usize, size_of::<libc::ucred>());
    unsafe { cred.assume_init() }.pid
}

#[test]
#[ignore = "requires the isolated Wayland fixture and a qualified Xwayland bundle"]
fn startup_with_an_unavailable_wayland_socket_releases_the_display() {
    let manager = support::packaged_manager();
    const CHILD_CASE: &str = "TERMCRAFT_XSERVER_FAILED_START_CHILD";
    if std::env::var_os(CHILD_CASE).is_some() {
        let result = manager.start(XServerOptions::default());
        let Err(error) = result else {
            panic!("unexpectedly started without a Wayland compositor");
        };
        assert!(error.contains("exited before opening a display"), "{error}");
        assert!(error.contains("Wayland session"), "{error}");
        return;
    }

    let list_sockets = || {
        std::fs::read_dir("/tmp/.X11-unix")
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<std::collections::BTreeSet<_>>()
    };
    let before = list_sockets();
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .env(CHILD_CASE, "1")
        .env("WAYLAND_DISPLAY", "/tmp/termcraft-missing-wayland-socket")
        .args([
            "--exact",
            "startup_with_an_unavailable_wayland_socket_releases_the_display",
            "--ignored",
            "--nocapture",
        ])
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(list_sockets(), before);
    let leaked = std::fs::read_dir("/tmp").unwrap().any(|entry| {
        let name = entry.unwrap().file_name();
        let name = name.to_string_lossy();
        (name.starts_with(".X") && name.ends_with("-lock") && name != ".X0-lock")
            || name.starts_with("termcraft-x11-")
    });
    assert!(
        !leaked,
        "failed startup left an X display lock or runtime directory"
    );
}
