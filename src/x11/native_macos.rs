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
    pin: &native_policy::NativePin<'_>,
) -> Result<XServerLease, String> {
    let image = VerifiedSnapshot::create(&root.join("x11/macos-universal-private1"), pin.files)?;
    let runtime = tempfile::Builder::new()
        .prefix("termcraft-x11-")
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
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
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        native_transport::cancelled(flag)?;
        if !process.is_running().map_err(|e| e.to_string())? {
            return Err("Private XQuartz exited before authenticated readiness".into());
        }
        if Instant::now() >= deadline {
            return Err("Private XQuartz timed out opening its inherited display socket".into());
        }
        match native_transport::setup_unix(&socket, None, flag) {
            Ok(true) => return Err("Private XQuartz inherited listener bypassed Xauthority".into()),
            Ok(false) => {
                if native_transport::setup_unix(&socket, Some(&cookie), flag)? {
                    break;
                }
            }
            Err(_) => {}
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
