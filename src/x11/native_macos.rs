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
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

/// Read-only inspection of the authenticated display. The owning probe parent
/// bounds the subprocess lifetime.
pub(super) fn probe_capabilities() -> Result<super::XCapabilities, String> {
    use std::io::Read;
    use std::os::unix::net::UnixStream;
    use x11rb::{
        connection::Connection,
        protocol::{glx, randr, xproto::ConnectionExt},
        rust_connection::{DefaultStream, RustConnection},
    };

    let display = std::env::var_os("DISPLAY").ok_or("Missing private XQuartz display")?;
    let path = Path::new(&display);
    if !path.is_absolute() {
        return Err("Expected a private XQuartz socket path".into());
    }
    let authority = std::env::var_os("XAUTHORITY").ok_or("Missing private XQuartz authority")?;
    let mut bytes = zeroize::Zeroizing::new(Vec::new());
    std::fs::File::open(authority)
        .and_then(|file| file.take(1025).read_to_end(&mut bytes))
        .map_err(|error| error.to_string())?;
    if bytes.len() > 1024 {
        return Err("Private XQuartz authority is oversized".into());
    }
    let cookie = crate::ssh::x11::authority_cookie(&bytes, 0, b"", true)?;
    let socket = UnixStream::connect(path).map_err(|error| error.to_string())?;
    let (stream, _) = DefaultStream::from_unix_stream(socket).map_err(|error| error.to_string())?;
    let connection = RustConnection::connect_to_stream_with_auth_info(
        stream,
        0,
        b"MIT-MAGIC-COOKIE-1".to_vec(),
        cookie.to_vec(),
    )
    .map_err(|error| error.to_string())?;
    let screen = &connection.setup().roots[0];
    let mut report = super::XCapabilities {
        dimensions: [screen.width_in_pixels.into(), screen.height_in_pixels.into()],
        errors: vec!["OpenGL pixel readback and RandR mode switching are unavailable in the private XQuartz probe.".into()],
        ..Default::default()
    };
    if let Ok(request) = glx::query_version(&connection, 1, 4) {
        match request.reply() {
            Ok(version) => {
                report.glx_version = Some(format!(
                    "{}.{}",
                    version.major_version, version.minor_version
                ));
            }
            Err(error) => report
                .errors
                .push(format!("GLX version query failed: {error}")),
        }
    }
    if let Ok(request) = randr::query_version(&connection, 1, 5) {
        match request.reply() {
            Ok(version) => {
                report.randr_version = Some(format!(
                    "{}.{}",
                    version.major_version, version.minor_version
                ));
            }
            Err(error) => report
                .errors
                .push(format!("RandR version query failed: {error}")),
        }
    }
    let composite = connection
        .query_extension(b"Composite")
        .map_err(|error| error.to_string())?
        .reply()
        .map_err(|error| error.to_string())?;
    if composite.present {
        report
            .errors
            .push("Composite is advertised by this display and remains unqualified.".into());
    } else {
        report
            .errors
            .push("Composite is disabled in private XQuartz.".into());
    }
    Ok(report)
}

pub(super) fn bundled_runtime(root: &Path) -> Result<(std::path::PathBuf, String), String> {
    let pin = native_policy::macos_pin()?;
    let package = root.join(native_policy::MACOS_RUNTIME);
    let _verified = VerifiedSnapshot::create(&package, pin.files)?;
    Ok((package.join(pin.executable), pin.version.into()))
}
pub(super) fn start(
    root: &Path,
    options: XServerOptions,
    flag: &AtomicBool,
) -> Result<XServerLease, String> {
    native_transport::cancelled(flag)?;
    if options.artifact_id != native_policy::MACOS_ARTIFACT
        && options.artifact_id != "xquartz-2.8.6-termcraft-private1-macos-universal"
    {
        return Err("Unsupported packaged macOS X server artifact".into());
    }
    if options.monitor.is_some() || options.display_mode != XDisplayMode::Windowed {
        return Err("Private XQuartz monitor/fullscreen mode has not been qualified".into());
    }
    launch_verified(snapshot(root, flag)?, flag)
}

/// One checked image can back successive processes of the same owned session.
/// No process is launched against the mutable package tree.
pub(super) fn snapshot(root: &Path, flag: &AtomicBool) -> Result<Arc<VerifiedSnapshot>, String> {
    native_transport::cancelled(flag)?;
    let pin = native_policy::macos_pin()?;
    VerifiedSnapshot::create_cancellable(&root.join(native_policy::MACOS_RUNTIME), pin.files, flag)
        .map(Arc::new)
}

pub(super) fn launch_verified(
    image: Arc<VerifiedSnapshot>,
    flag: &AtomicBool,
) -> Result<XServerLease, String> {
    native_transport::cancelled(flag)?;
    let pin = native_policy::macos_pin()?;
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

    #[test]
    #[ignore = "requires an explicitly staged local XQuartz candidate and a macOS desktop"]
    fn local_candidate_uses_verified_snapshot_and_cleans_up() {
        let root = std::path::PathBuf::from(
            std::env::var_os("TERMCRAFT_TEST_XQUARTZ_ROOT")
                .expect("set the explicit fixture resource root"),
        );
        let options = XServerOptions::default();
        let cancelled = AtomicBool::new(true);
        assert!(
            start(&root, options.clone(), &cancelled)
                .err()
                .unwrap()
                .contains("cancel")
        );
        let lease = start(&root, options, &AtomicBool::new(false)).unwrap();
        let executable = root.with_file_name("MacOS").join("harbor-workspace");
        let report = super::super::XCapabilities::probe_with_executable(
            &lease.client_display(),
            &executable,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(report.dimensions.iter().all(|&value| value > 0));
        assert!(report.composite_version.is_none());
        assert!(!report.rendering_verified());
        assert!(report.randr_change.is_none());
        eprintln!("Private XQuartz capability report: {report:?}");
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
