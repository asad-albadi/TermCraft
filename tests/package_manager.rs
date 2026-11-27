#![cfg(all(any(target_os = "linux", windows), target_arch = "x86_64"))]
use harbor_workspace::{
    environment::UnixEnvironment,
    packages::{PackageIndex, PackageManager},
};
use std::fs;
use std::path::PathBuf;

fn archive(version: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/p05-bundles")
        .join(format!(
            "busybox-{version}-{}-x86_64.tar.gz",
            std::env::consts::OS
        ))
}

fn install(manager: &PackageManager, version: &str) {
    let release = PackageIndex::pinned()
        .release("busybox", version)
        .unwrap()
        .clone();
    assert!(
        archive(version).is_file(),
        "Prepare the real fixture with packaging/unix-runtime/stage.py"
    );
    manager.install(release, archive(version)).wait().unwrap();
}

fn wait_pty_text(process: &harbor_workspace::process::Process, needle: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut output = Vec::new();
    while let Some(timeout) = deadline.checked_duration_since(std::time::Instant::now()) {
        let Ok(bytes) = process.rx.recv_timeout(timeout) else {
            break;
        };
        output.extend(bytes);
        if String::from_utf8_lossy(&output).contains(needle) {
            return;
        }
    }
    panic!(
        "PTY output did not contain {needle:?}: {}",
        String::from_utf8_lossy(&output)
    );
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires the pinned upstream artifacts staged by packaging/unix-runtime/stage.py"]
fn real_portable_tools_survive_upgrade_and_home_survives_restart() {
    let root = tempfile::tempdir().unwrap();
    let manager = PackageManager::new(root.path().join("Portable root"));
    install(&manager, "1.31.0");
    let env = UnixEnvironment::new(manager.clone());
    let old = env.launch_config().unwrap();
    let mut process = old.spawn_terminal(80, 24).unwrap();
    process
        .write(b"printf 'pty-%s\\n' ready; read go; grep hello file.txt | sed s/hello/old/; exit\n")
        .unwrap();
    wait_pty_text(&process, "pty-ready");
    fs::write(old.directory.join("file.txt"), b"hello world\n").unwrap();
    install(&manager, "1.35.0");
    let new = UnixEnvironment::new(PackageManager::new(manager.root().into()))
        .launch_config()
        .unwrap();
    assert_ne!(old.program, new.program);
    assert_eq!(old.directory, new.directory);
    let output = new
        .command([
            "-c",
            "mkdir -p work && cp file.txt work/copy && grep hello work/copy | sed s/hello/new/",
        ])
        .env_clear()
        .envs(new.environment.iter().map(|(key, value)| (key, value)))
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"new world\n");
    process.write(b"go\n").unwrap();
    wait_pty_text(&process, "old world");
    drop(process);
    drop(old);
    let changed = new.program.parent().unwrap().join("cat");
    let owned = new.program.clone();
    fs::write(&changed, b"user edit").unwrap();
    manager.remove("busybox").wait().unwrap();
    assert!(
        new.program.is_file(),
        "a retained lease keeps the active files"
    );
    drop(new);
    manager.collect().unwrap();
    assert!(env.launch_config().is_err());
    assert!(!owned.exists());
    assert_eq!(fs::read(changed).unwrap(), b"user edit");
    assert_eq!(
        fs::read(manager.root().join("home/file.txt")).unwrap(),
        b"hello world\n"
    );
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires the pinned upstream artifacts staged by packaging/unix-runtime/stage.py"]
fn failed_package_install_rolls_back() {
    let root = tempfile::tempdir().unwrap();
    let manager = PackageManager::new(root.path().join("environment"));
    install(&manager, "1.31.0");
    let candidate = PackageIndex::pinned()
        .release("busybox", "1.35.0")
        .unwrap()
        .clone();
    let wrong_bytes = root.path().join("wrong.tar.gz");
    fs::write(&wrong_bytes, b"wrong digest").unwrap();
    assert!(
        manager
            .install(candidate.clone(), wrong_bytes)
            .wait()
            .unwrap_err()
            .to_string()
            .contains("digest")
    );
    let job = manager.install(candidate, archive("1.35.0"));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if job.progress() == Some(harbor_workspace::packages::Progress::Extracting) {
            job.cancel();
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "extraction never began"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(matches!(
        job.wait(),
        Err(harbor_workspace::packages::PackageError::Cancelled)
    ));
    assert_eq!(manager.list().unwrap()[0].version, "1.31.0");
    let shell = UnixEnvironment::new(manager).launch_config().unwrap();
    assert!(
        shell
            .command(["-c", "echo intact"])
            .output()
            .unwrap()
            .status
            .success()
    );
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires the pinned upstream artifacts staged by packaging/unix-runtime/stage.py"]
fn portable_shell_runs_in_real_pty_with_private_child_environment() {
    let root = tempfile::tempdir().unwrap();
    let manager = PackageManager::new(root.path().join("portable space"));
    install(&manager, "1.35.0");
    let home_before = std::env::var_os("HOME");
    let path_before = std::env::var_os("PATH");
    let launch = UnixEnvironment::new(manager).launch_config().unwrap();
    let mut shell = launch.spawn_terminal(80, 24).unwrap();
    shell
        .write(b"printf 'alphabeta\\n' > note; cat note | sed s/alpha/portable/; exit\n")
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut output = Vec::new();
    while let Some(timeout) = deadline.checked_duration_since(std::time::Instant::now()) {
        match shell.rx.recv_timeout(timeout) {
            Ok(bytes) => output.extend(bytes),
            Err(_) => break,
        }
    }
    assert!(String::from_utf8_lossy(&output).contains("portablebeta"));
    assert_eq!(
        fs::read(launch.directory.join("note")).unwrap(),
        b"alphabeta\n"
    );
    assert_eq!(std::env::var_os("HOME"), home_before);
    assert_eq!(std::env::var_os("PATH"), path_before);
}

#[cfg(windows)]
#[test]
#[ignore = "requires native Windows and the staged BusyBox-w32 upstream artifact"]
fn windows_drive_mapping_resolves_expected_file() {
    let root = tempfile::tempdir().unwrap();
    let manager = PackageManager::new(root.path().join("portable space"));
    install(&manager, "FRP-6354");
    let launch = UnixEnvironment::new(manager).launch_config().unwrap();
    let outside = root.path().join("drive fixture.txt");
    fs::write(&outside, b"drive data\n").unwrap();
    let windows_path = outside.to_str().unwrap().replace('\\', "/");
    let path = windows_path.strip_prefix("//?/").unwrap_or(&windows_path);
    assert!(path.as_bytes().get(1) == Some(&b':'));
    let output = launch
        .command(["-c", "cat \"$1\"", "ash", path])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"),
        "drive data\n"
    );
}

#[cfg(windows)]
#[test]
#[ignore = "requires native Windows and the staged BusyBox-w32 upstream artifact"]
fn windows_portable_shell_runs_in_conpty_and_keeps_its_private_home() {
    let root = tempfile::tempdir().unwrap();
    let manager = PackageManager::new(root.path().join("portable space"));
    install(&manager, "FRP-6354");
    let environment = UnixEnvironment::new(manager);
    let home_before = std::env::var_os("HOME");
    let path_before = std::env::var_os("PATH");
    let launch = environment.launch_config().unwrap();
    let mut shell = launch.spawn_terminal(80, 24).unwrap();
    shell
        .write(b"printf '%s%s\\n' portable -input > note; cat note; exit\r")
        .unwrap();
    wait_pty_text(&shell, "portable-input");
    assert_eq!(
        fs::read(launch.directory.join("note")).unwrap(),
        b"portable-input\n"
    );
    drop(shell);
    assert_eq!(
        environment.launch_config().unwrap().directory,
        launch.directory
    );
    assert_eq!(std::env::var_os("HOME"), home_before);
    assert_eq!(std::env::var_os("PATH"), path_before);
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires pinned upstream bundles"]
fn retired_cleanup_preserves_tampered_receipt_extras_and_special_files() {
    use std::os::unix::ffi::OsStrExt;
    let root = tempfile::tempdir().unwrap();
    let manager = PackageManager::new(root.path().join("portable"));
    install(&manager, "1.35.0");
    let launch = UnixEnvironment::new(manager.clone())
        .launch_config()
        .unwrap();
    let bin = launch.program.parent().unwrap().to_path_buf();
    let version = bin.parent().unwrap();
    let extra = bin.join("extra");
    fs::write(&extra, b"user-owned").unwrap();
    let inventory = version.join("inventory.json");
    fs::write(&inventory, br#"{"files":{"bin/extra":"efb914797a54ec6597ecf009ea241b9b7294e286e18c952816e862d8bed51bce","../outside":"invalid"}}"#).unwrap();
    let outside = version.parent().unwrap().join("outside");
    fs::write(&outside, b"outside").unwrap();
    let fifo = bin.join("cat");
    fs::remove_file(&fifo).unwrap();
    let path = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    let resolver = manager.clone();
    std::thread::spawn(move || {
        let _ = send.send(resolver.command("busybox", "ash"));
    });
    assert!(
        receive
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("resolution blocked on a named pipe")
            .is_err()
    );

    manager.remove("busybox").wait().unwrap();
    drop(launch);
    let cleanup = manager.cleanup();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if let Some(result) = cleanup.poll() {
            result.unwrap();
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "cleanup blocked on a named pipe"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(fs::read(&extra).unwrap(), b"user-owned");
    assert_eq!(fs::read(&outside).unwrap(), b"outside");
    assert!(fifo.exists());
    assert!(!bin.join("ash").exists());
}
