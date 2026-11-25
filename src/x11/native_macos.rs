//! Direct private XQuartz engine; never uses open(1), launchctl, startx or a shell.
use super::{
    XDisplayMode, XServerLease, XServerOptions, native_artifact::VerifiedSnapshot, native_policy,
    native_transport,
};
use std::{
    os::{
        fd::AsRawFd,
        unix::{fs::PermissionsExt, net::UnixListener, process::CommandExt},
    },
    path::Path,
    process::{Command, Stdio},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
pub(super) fn bundled_runtime(root: &Path) -> Result<(std::path::PathBuf, String), String> {
    let pin = native_policy::macos_pin()?;
    let package = root.join("x11/macos-universal-private1");
    let _verified = VerifiedSnapshot::create(&package, pin.files)?;
    Ok((package.join(pin.executable), pin.version.into()))
}
pub(super) fn start(
    root: &Path,
    options: XServerOptions,
    flag: &AtomicBool,
) -> Result<XServerLease, String> {
    native_transport::cancelled(flag)?;
    if options.artifact_id != native_policy::MACOS_ARTIFACT {
        return Err("Unsupported packaged macOS X server artifact".into());
    }
    if options.monitor.is_some() || options.display_mode != XDisplayMode::Windowed {
        return Err("Private XQuartz monitor/fullscreen mode has not been qualified".into());
    }
    let pin = native_policy::macos_pin()?;
    launch(root, flag, pin)
}
fn launch(
    root: &Path,
    flag: &AtomicBool,
    pin: &native_policy::NativePin,
) -> Result<XServerLease, String> {
    native_transport::cancelled(flag)?;
    let image = VerifiedSnapshot::create(&root.join("x11/macos-universal-private1"), pin.files)?;
    let runtime = tempfile::Builder::new()
        .prefix("termcraft-x11-")
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir_in("/private/tmp")
        .map_err(|e| e.to_string())?;
    let socket = runtime.path().join("display:0");
    let listener = UnixListener::bind(&socket).map_err(|e| e.to_string())?;
    let fd = listener.as_raw_fd();
    let authority_path = runtime.path().join("Xauthority");
    let mut cookie = zeroize::Zeroizing::new([0; 16]);
    getrandom::fill(cookie.as_mut()).map_err(|e| e.to_string())?;
    native_transport::write_authority(&authority_path, 0, &cookie)?;
    let log = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(runtime.path().join("server.log"))
        .map_err(|e| e.to_string())?;
    let mut command = Command::new(image.root().join(pin.executable));
    command.process_group(0);
    command
        .args([
            ":0",
            "-nolisten",
            "tcp",
            "-nolisten",
            "local",
            "-noreset",
            "-iglx",
            "-auth",
        ])
        .arg(&authority_path)
        .arg("-fp")
        .arg(image.root().join("opt/X11/share/fonts/X11/misc"))
        .env_clear()
        .env("HOME", runtime.path())
        .env("TMPDIR", runtime.path())
        .env("PATH", image.root().join("opt/X11/bin"))
        .env("TERMCRAFT_X11_ROOT", image.root())
        .env("DISPLAY", &socket)
        .env("XAUTHORITY", &authority_path)
        .env("TERMCRAFT_OWNED_X11_FD", fd.to_string())
        .current_dir(runtime.path())
        .stdin(Stdio::null())
        .stdout(log.try_clone().map_err(|e| e.to_string())?)
        .stderr(log);
    // Preserve only macOS' text/locale hints. No dyld overrides, startup script,
    // user preference domain, X credentials or system DISPLAY are inherited.
    for key in ["LANG", "LC_ALL", "__CF_USER_TEXT_ENCODING"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    unsafe {
        command.pre_exec(move || {
            if libc::fcntl(fd, libc::F_SETFD, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    native_transport::cancelled(flag)?;
    let child = command
        .spawn()
        .map_err(|e| format!("Could not start private XQuartz image: {e}"))?;
    let mut process = super::engine::OwnedProcess::new(child);
    let deadline = Instant::now() + Duration::from_secs(12);
    let mut last_error = None;
    loop {
        native_transport::cancelled(flag)?;
        if !process.is_running().map_err(|e| e.to_string())? {
            return Err("Private XQuartz exited before authenticated readiness".into());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "Private XQuartz timed out opening its inherited display socket{}",
                last_error
                    .as_ref()
                    .map_or_else(String::new, |error| format!(": {error}"))
            ));
        }
        match native_transport::setup_unix(&socket, None, flag) {
            Ok(true) => return Err("Private XQuartz inherited listener bypassed Xauthority".into()),
            Ok(false) => {
                if native_transport::setup_unix(&socket, Some(&cookie), flag)? {
                    break;
                }
            }
            Err(error) => last_error = Some(error),
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    native_transport::cancelled(flag)?;
    drop(listener);
    Ok(XServerLease {
        alive: tokio::sync::watch::Sender::new(true),
        display: socket.to_string_lossy().into_owned(),
        authority_path,
        process,
        _native_image: image,
        _runtime: runtime,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    #[ignore = "requires an explicitly staged local XQuartz candidate and a macOS desktop"]
    fn local_candidate_uses_verified_snapshot_and_cleans_up() {
        let root = std::path::PathBuf::from(
            std::env::var_os("TERMCRAFT_TEST_XQUARTZ_ROOT")
                .expect("set the explicit fixture resource root"),
        );
        let candidate = root.join("x11/macos-universal-private1");
        let manifest_path = candidate.join("NATIVE-BUILD.json");
        let bytes = std::fs::read(manifest_path).unwrap();
        let expected = std::env::var("TERMCRAFT_TEST_XQUARTZ_MANIFEST_SHA256").unwrap();
        let digest: String = Sha256::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(digest, expected);
        let manifest: &'static serde_json::Value =
            Box::leak(Box::new(serde_json::from_slice(&bytes).unwrap()));
        let files = manifest["files"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(path, hash)| (path.as_str(), hash.as_str().unwrap()))
            .collect::<Vec<_>>();
        // Fixture-only pins have no path to the compiled runtime policy.
        let pin = native_policy::NativePin {
            files: Box::leak(files.into_boxed_slice()),
            executable: "XQuartz.app/Contents/MacOS/X11.bin",
            version: "local qualification fixture",
        };
        assert!(native_policy::macos_pin().is_err());
        let cancelled = AtomicBool::new(true);
        assert!(
            launch(&root, &cancelled, &pin)
                .err()
                .unwrap()
                .contains("cancel")
        );
        let lease = launch(&root, &AtomicBool::new(false), &pin).unwrap();
        let pid = lease.process.id().unwrap() as libc::pid_t;
        assert_eq!(unsafe { libc::getpgid(pid) }, pid);
        let display = std::path::PathBuf::from(&lease.display);
        let authority = lease.authority_path.clone();
        let snapshot = lease._native_image.root().to_path_buf();
        assert!(display.exists() && authority.exists() && snapshot.exists());
        drop(lease);
        assert!(!display.exists() && !authority.exists() && !snapshot.exists());
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        );
    }
}
