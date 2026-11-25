use harbor_workspace::runtime::{RuntimeCatalog, ToolId, ToolProvider};

#[test]
fn packaged_runtime_works_with_empty_path() {
    if std::env::var_os("TERMCRAFT_EMPTY_PATH_CHILD").is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "packaged_runtime_works_with_empty_path",
                "--nocapture",
            ])
            .env("PATH", "")
            .env("TERMCRAFT_EMPTY_PATH_CHILD", "1")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    let catalog = RuntimeCatalog::new(std::env::temp_dir());
    for tool in [
        ToolId::Ssh,
        ToolId::Sftp,
        ToolId::Scp,
        ToolId::Vnc,
        ToolId::Ftp,
        ToolId::Telnet,
        ToolId::Rlogin,
        ToolId::Rsh,
        ToolId::Serial,
    ] {
        assert_eq!(catalog.resolve(tool).unwrap(), ToolProvider::Embedded);
    }
    assert!(catalog.resolve(ToolId::OpenSsh).is_err());
}

#[test]
fn manifest_rejects_wrong_architecture_or_hash() {
    let root = tempfile::tempdir().unwrap();
    let catalog = RuntimeCatalog::new(root.path());
    for (architecture, checksum, expected) in [
        (
            "unsupported-test-architecture",
            "0".repeat(64),
            "architecture",
        ),
        (std::env::consts::ARCH, "0".repeat(64), "checksum"),
    ] {
        let package = serde_json::json!({
            "schema_version": 1,
            "platform": std::env::consts::OS,
            "architecture": architecture,
            "rdp_helper_version": "0.1.0",
            "files": { helper_name(): checksum },
        });
        std::fs::write(
            root.path().join("PACKAGE-MANIFEST.json"),
            package.to_string(),
        )
        .unwrap();
        let binary = root.path().join(helper_name());
        use std::io::Read;
        let mut bytes = Vec::new();
        let image = std::fs::File::open(std::env::current_exe().unwrap()).unwrap();
        image.take(4096).read_to_end(&mut bytes).unwrap();
        std::fs::write(&binary, bytes).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let result = catalog.resolve(ToolId::Rdp).unwrap_err();
        assert!(result.contains(expected), "{result}");
    }
}

#[cfg(target_os = "linux")]
#[test]
fn manifest_rejects_wrong_elf_and_symlinked_helper() {
    use sha2::{Digest, Sha256};
    use std::os::unix::fs::{PermissionsExt, symlink};

    let root = tempfile::tempdir().unwrap();
    let catalog = RuntimeCatalog::new(root.path());
    // Use the real test executable's native header with a different machine field.
    let mut header = [0u8; 64];
    let binary = std::env::current_exe().unwrap();
    std::io::Read::read_exact(&mut std::fs::File::open(binary).unwrap(), &mut header).unwrap();
    header[18] = if std::env::consts::ARCH == "x86_64" {
        183
    } else {
        62
    };
    let checksum: String = Sha256::digest(header)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let package = serde_json::json!({
        "schema_version": 1,
        "platform": "linux",
        "architecture": std::env::consts::ARCH,
        "rdp_helper_version": "0.1.0",
        "files": { helper_name(): checksum },
    });
    std::fs::write(
        root.path().join("PACKAGE-MANIFEST.json"),
        package.to_string(),
    )
    .unwrap();
    let executable = root.path().join(helper_name());
    std::fs::write(&executable, header).unwrap();
    let result = catalog.resolve(ToolId::Rdp).unwrap_err();
    assert!(result.contains("executable permissions"), "{result}");
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
    let result = catalog.resolve(ToolId::Rdp).unwrap_err();
    assert!(result.contains("architecture"), "{result}");
    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().join("not-in-package");
    std::fs::rename(&executable, &target).unwrap();
    symlink(&target, &executable).unwrap();
    let result = catalog.resolve(ToolId::Rdp).unwrap_err();
    assert!(
        !result.contains("unsupported interpreter or architecture"),
        "{result}"
    );
}

#[test]
fn existing_mosh_profiles_keep_the_installed_provider() {
    use harbor_workspace::model::{MoshProvider, Session};
    let mut legacy: Session = serde_json::from_str(r#"{"protocol":"Mosh"}"#).unwrap();
    assert_eq!(legacy.mosh_provider, MoshProvider::System);
    legacy.mosh_provider = MoshProvider::Bundled;
    let restored: Session = serde_json::from_str(&serde_json::to_string(&legacy).unwrap()).unwrap();
    assert_eq!(restored.mosh_provider, MoshProvider::Bundled);
}

fn helper_name() -> &'static str {
    if cfg!(windows) {
        "termcraft-rdp-engine.exe"
    } else {
        "termcraft-rdp-engine"
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[path = "support/ssh_server.rs"]
#[allow(dead_code)]
mod ssh_server;

#[test]
#[ignore = "requires staged Linux x86_64 archive and genuine Mosh server fixture"]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn staged_runtime_runs_with_empty_path() {
    use harbor_workspace::{connection, process::Process};
    use std::{
        path::PathBuf,
        time::{Duration, Instant},
    };

    let root =
        PathBuf::from(std::env::var_os("TERMCRAFT_STAGED_RUNTIME").expect("staged runtime root"));
    if std::env::var_os("TERMCRAFT_EMPTY_PATH_CHILD").is_none() {
        use std::os::unix::fs::PermissionsExt;
        let server = std::env::var("TERMCRAFT_MOSH_SERVER").expect("pinned test mosh-server");
        let fixture = tempfile::tempdir().unwrap();
        let server_entry = fixture.path().join("mosh-server");
        let script = format!(
            "#!/bin/sh\nexec '{}' \"$@\" -- /bin/sh -c 'printf \"runtime-%s\\n\" mosh-udp; read result'\n",
            server.replace('\'', "'\\''"),
        );
        std::fs::write(&server_entry, script).unwrap();
        std::fs::set_permissions(&server_entry, std::fs::Permissions::from_mode(0o700)).unwrap();
        let ssh = ssh_server::SshServer::start_with_settings(
            false,
            "",
            &format!("SetEnv PATH={}:/usr/bin", fixture.path().display()),
        );
        ssh.config(1, true);
        let config = ssh.root.join("ssh_config");
        std::fs::write(
            &config,
            format!(
                "Host *\n    BatchMode yes\n    StrictHostKeyChecking yes\n    GlobalKnownHostsFile /dev/null\n    UserKnownHostsFile {}\n    IdentitiesOnly yes\n",
                ssh.root.join("known_hosts").display(),
            ),
        ).unwrap();

        let test_package = tempfile::tempdir_in(root.parent().unwrap()).unwrap();
        let test_root = test_package.path();
        std::fs::create_dir_all(test_root.join("runtime/linux-x86_64")).unwrap();
        for relative in [
            "PACKAGE-MANIFEST.json",
            helper_name(),
            "runtime/linux-x86_64/mosh",
            "runtime/linux-x86_64/mosh-client",
        ] {
            std::fs::copy(root.join(relative), test_root.join(relative)).unwrap();
        }
        let probe = test_root.join("runtime-probe");
        let source = std::env::current_exe().unwrap();
        std::fs::hard_link(&source, &probe)
            .expect("staging root and Cargo output must be on the same filesystem");
        let status = std::process::Command::new(&probe)
            .args([
                "--exact",
                "staged_runtime_runs_with_empty_path",
                "--ignored",
                "--nocapture",
            ])
            .env("PATH", "")
            .env("TERMCRAFT_EMPTY_PATH_CHILD", "1")
            .env("TERMCRAFT_STAGED_RUNTIME", test_root)
            .env("TERMCRAFT_TEST_SSH_CONFIG", config)
            .env("TERMCRAFT_TEST_SSH_KEY", ssh.root.join("client"))
            .env("TERMCRAFT_TEST_SSH_PORT", ssh.port.to_string())
            .status();
        assert!(status.unwrap().success());
        return;
    }

    let catalog = RuntimeCatalog::for_current_executable().unwrap();
    let rdp = catalog.resolve(ToolId::Rdp).unwrap().executable().unwrap();
    let process = std::process::Command::new(rdp)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    // The real engine emits the private protocol handshake before reading its input.
    assert!(
        process.stdout.windows(5).any(|bytes| bytes == b"Ready"),
        "{:?}",
        String::from_utf8_lossy(&process.stdout)
    );

    let mut session = harbor_workspace::model::Session {
        protocol: "Mosh".into(),
        mosh_provider: harbor_workspace::model::MoshProvider::Bundled,
        host: "127.0.0.1".into(),
        user: std::env::var("USER").unwrap(),
        key: std::env::var("TERMCRAFT_TEST_SSH_KEY").unwrap(),
        port: 0,
        mosh_ssh_port: std::env::var("TERMCRAFT_TEST_SSH_PORT")
            .unwrap()
            .parse()
            .unwrap(),
        ..Default::default()
    };
    session.ssh_options.config_file = Some(
        std::env::var_os("TERMCRAFT_TEST_SSH_CONFIG")
            .unwrap()
            .into(),
    );
    let launch = connection::launch(&session, &Default::default()).unwrap();
    assert_eq!(
        launch.program,
        root.join("runtime/linux-x86_64/mosh").to_str().unwrap()
    );
    let missing_client = root.join("runtime/linux-x86_64/mosh-client");
    let saved_client = missing_client.with_extension("saved");
    std::fs::rename(&missing_client, &saved_client).unwrap();
    let result = catalog.resolve(ToolId::Mosh);
    let launch_result = connection::launch(&session, &Default::default());
    std::fs::rename(&saved_client, &missing_client).unwrap();
    assert!(
        launch_result.is_err(),
        "Connect accepted an incomplete Mosh payload"
    );
    assert!(
        result.is_err(),
        "incomplete Mosh payload was reported available"
    );

    // Package bytes replaced after launch resolution must be blocked by the
    // process entry point, before either of the new executables could run.
    for name in ["mosh", "mosh-client"] {
        let executable = root.join("runtime/linux-x86_64").join(name);
        let saved = executable.with_extension("verified");
        std::fs::rename(&executable, &saved).unwrap();
        std::fs::copy(&saved, &executable).unwrap();
        let mut changed = std::fs::OpenOptions::new()
            .append(true)
            .open(&executable)
            .unwrap();
        std::io::Write::write_all(&mut changed, b"changed").unwrap();
        drop(changed);
        let result = Process::spawn(&launch.program, &launch.args, None, 80, 24);
        let unavailable = catalog.resolve(ToolId::Mosh).is_err();
        let launch_unavailable = connection::launch(&session, &Default::default()).is_err();
        std::fs::rename(&saved, &executable).unwrap();
        assert!(unavailable, "corrupt {name} was reported available");
        assert!(launch_unavailable, "Connect accepted corrupt {name}");
        let Err(error) = result else {
            panic!("modified {name} was accepted for launch");
        };
        assert!(error.contains("checksum mismatch"), "{error}");
    }

    let mut terminal = Process::spawn(
        &launch.program,
        &launch.args,
        launch.directory.as_deref(),
        80,
        24,
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut output = Vec::new();
    while Instant::now() < deadline {
        if let Ok(data) = terminal.rx.recv_timeout(Duration::from_millis(200)) {
            output.extend(data);
            if output.windows(16).any(|bytes| bytes == b"runtime-mosh-udp") {
                terminal.write(b"done\n").unwrap();
                return;
            }
        }
        terminal.poll();
        if terminal.exited {
            break;
        }
    }
    panic!(
        "Genuine Mosh client did not receive its UDP terminal frame: {}",
        String::from_utf8_lossy(&output)
    );
}
