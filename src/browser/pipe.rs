use std::fs::Permissions;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::Value;

const MAX_MESSAGE: usize = 12 * 1024 * 1024;

pub(super) struct Pipe {
    process: Child,
    writer: UnixStream,
    reader: UnixStream,
    pending: Vec<u8>,
    _profile: tempfile::TempDir,
    _runtime: Option<tempfile::TempDir>,
}

impl Pipe {
    #[cfg(test)]
    pub(super) fn start(proxy: &str) -> Result<Self, String> {
        Self::start_with_runtime(
            proxy,
            super::Runtime::SystemChromium,
            &AtomicBool::new(false),
        )
    }
    pub(super) fn start_with_runtime(
        proxy: &str,
        runtime: super::Runtime,
        cancelled: &AtomicBool,
    ) -> Result<Self, String> {
        let profile = tempfile::Builder::new()
            .prefix("termcraft-browser-")
            .permissions(Permissions::from_mode(0o700))
            .tempdir()
            .map_err(|_| "Could not create a private browser profile.")?;
        let (writer, browser_reader) = UnixStream::pair().map_err(transport_error)?;
        let (browser_writer, reader) = UnixStream::pair().map_err(transport_error)?;
        let browser_reader = duplicate(&browser_reader).map_err(transport_error)?;
        let browser_writer = duplicate(&browser_writer).map_err(transport_error)?;
        writer
            .set_write_timeout(Some(Duration::from_millis(100)))
            .map_err(transport_error)?;
        reader
            .set_read_timeout(Some(Duration::from_millis(30)))
            .map_err(transport_error)?;
        let catalog = crate::runtime::RuntimeCatalog::for_current_executable()?;
        let (chromium, runtime_snapshot) = match runtime {
            super::Runtime::SystemChromium => (
                catalog
                    .resolve(crate::runtime::ToolId::SystemChromium)?
                    .executable()?,
                None,
            ),
            super::Runtime::BundledChromium => {
                let (path, snapshot) = catalog.bundled_browser_snapshot(cancelled)?;
                (path, Some(snapshot))
            }
        };
        if cancelled.load(Ordering::Acquire) {
            return Err("Browser startup cancelled".into());
        }
        let mut command = Command::new(&chromium);
        command
            .env_remove("LD_LIBRARY_PATH")
            .env_remove("LD_PRELOAD")
            .env_remove("LD_AUDIT");
        #[cfg(target_os = "linux")]
        if runtime == super::Runtime::BundledChromium {
            command.env(
                "LD_LIBRARY_PATH",
                chromium
                    .parent()
                    .ok_or("Missing browser directory")?
                    .join("libraries"),
            );
        }
        command
            .args([
                "--headless",
                "--force-device-scale-factor=1",
                "--remote-debugging-pipe",
                "--no-first-run",
                "--no-default-browser-check",
                "--disable-background-networking",
                "--disable-component-update",
                "--disable-sync",
                "--disable-default-apps",
                "--disable-extensions",
                "--password-store=basic",
                "--disable-features=MediaRouter,OptimizationHints",
                "--use-mock-keychain",
                "--disable-breakpad",
                "--disable-crash-reporter",
            ])
            .arg(format!("--user-data-dir={}", profile.path().display()))
            .env("XDG_CONFIG_HOME", profile.path())
            .env("XDG_CACHE_HOME", profile.path())
            .env("XDG_DATA_HOME", profile.path())
            .env("XDG_RUNTIME_DIR", profile.path())
            .env("PATH", profile.path())
            .env_remove("BROWSER")
            .env("HOME", profile.path())
            .env_remove("DBUS_SESSION_BUS_ADDRESS")
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("CHROME_CONFIG_HOME")
            .env_remove("CHROME_USER_DATA_DIR")
            .env_remove("CHROME_LOG_FILE")
            .env_remove("CHROME_DEVEL_SANDBOX")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0);
        if proxy.is_empty() {
            command.arg("--no-proxy-server");
        } else {
            command
                .arg(format!("--proxy-server={proxy}"))
                .arg("--proxy-bypass-list=<-loopback>");
        }
        command.arg("about:blank");
        // High-numbered copies avoid clobbering the source if either original endpoint is fd 3 or 4.
        unsafe {
            command.pre_exec(move || {
                if libc::dup2(browser_reader.as_raw_fd(), 3) == -1
                    || libc::dup2(browser_writer.as_raw_fd(), 4) == -1
                {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let process = command.spawn().map_err(|_| {
            "Could not launch the selected Chromium runtime for Embedded Browser.".to_owned()
        })?;
        Ok(Self {
            process,
            writer,
            reader,
            pending: Vec::new(),
            _profile: profile,
            _runtime: runtime_snapshot,
        })
    }

    pub(super) fn send(&mut self, message: &impl serde::Serialize) -> Result<(), String> {
        let mut bytes = zeroize::Zeroizing::new(
            serde_json::to_vec(message).map_err(|_| "Invalid browser command.")?,
        );
        bytes.push(0);
        self.writer.write_all(&bytes).map_err(transport_error)
    }

    pub(super) fn receive(
        &mut self,
        until: Instant,
        cancel: &AtomicBool,
    ) -> Result<Option<Value>, String> {
        let mut checked = 0;
        loop {
            if cancel.load(Ordering::Acquire) {
                return Err("Browser closed.".into());
            }
            if let Some(i) = self.pending[checked..].iter().position(|byte| *byte == 0) {
                let end = checked + i;
                let message = serde_json::from_slice(&self.pending[..end])
                    .map_err(|_| "Invalid browser engine response.")?;
                self.pending.drain(..=end);
                return Ok(Some(message));
            }
            if Instant::now() >= until {
                return Ok(None);
            }
            checked = self.pending.len();
            let mut bytes = [0u8; 32 * 1024];
            match self.reader.read(&mut bytes) {
                Ok(0) => return Err("The Chromium process exited. Check the installed browser and sandbox configuration.".into()),
                Ok(count) if checked + count <= MAX_MESSAGE => self.pending.extend_from_slice(&bytes[..count]),
                Ok(_) => return Err("Browser engine response exceeded the size limit.".into()),
                Err(error) if matches!(error.kind(), io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => {}
                Err(error) => return Err(transport_error(error)),
            }
        }
    }
}

impl Drop for Pipe {
    fn drop(&mut self) {
        // A new group was created at spawn. Its leader remains unreaped until after this signal.
        unsafe { libc::kill(-(self.process.id() as libc::pid_t), libc::SIGKILL) };
        let _ = self.process.wait();
    }
}

fn duplicate(stream: &UnixStream) -> io::Result<OwnedFd> {
    let fd = unsafe { libc::fcntl(stream.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 10) };
    if fd == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

fn transport_error(_: io::Error) -> String {
    "Private Chromium connection failed.".into()
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn browser_startup_cancel_reaps_only_its_process_and_removes_private_profile() {
        let installed = std::path::Path::new("/usr/lib/chromium/chromium").is_file();
        assert!(installed || std::env::var_os("TERMCRAFT_REQUIRE_CHROMIUM").is_none());
        if !installed {
            return;
        }
        let other_profile = tempfile::tempdir().unwrap();
        let sentinel = other_profile.path().join("profile-sentinel");
        std::fs::write(&sentinel, b"unrelated profile").unwrap();
        let other_browser = Pipe::start("").unwrap();
        let browser = Pipe::start("").unwrap();
        let pid = browser.process.id();
        let profile = browser._profile.path().to_owned();
        assert_eq!(
            profile.metadata().unwrap().permissions().mode() & 0o777,
            0o700
        );
        drop(browser);
        assert!(!profile.exists());
        assert!(!std::path::Path::new("/proc").join(pid.to_string()).exists());
        assert!(other_browser._profile.path().exists());
        assert!(
            std::path::Path::new("/proc")
                .join(other_browser.process.id().to_string())
                .exists()
        );
        assert_eq!(std::fs::read(sentinel).unwrap(), b"unrelated profile");
    }
}

#[cfg(all(test, target_os = "linux"))]
mod bundled_qualification {
    use super::*;
    use serde_json::json;

    #[test]
    #[ignore = "requires relocated pinned browser payload beside this test executable"]
    fn bundled_renderer_keeps_namespace_seccomp_sandbox_and_reaps_private_profile() {
        let mut browser = Pipe::start_with_runtime(
            "",
            crate::browser::Runtime::BundledChromium,
            &AtomicBool::new(false),
        )
        .unwrap();
        let profile = browser._profile.path().to_owned();
        let runtime = browser._runtime.as_ref().unwrap().path().to_owned();
        let parent = browser.process.id();
        assert_eq!(
            profile.metadata().unwrap().permissions().mode() & 0o777,
            0o700
        );
        let arguments = std::fs::read(format!("/proc/{parent}/cmdline")).unwrap();
        for forbidden in [
            b"--no-sandbox".as_slice(),
            b"--ignore-certificate-errors".as_slice(),
            b"--remote-debugging-port".as_slice(),
        ] {
            assert!(
                !arguments
                    .windows(forbidden.len())
                    .any(|window| window == forbidden)
            );
        }
        let cancel = AtomicBool::new(false);
        browser
            .send(&json!({"id":1,"method":"Target.createTarget","params":{"url":"about:blank"}}))
            .unwrap();
        let until = Instant::now() + Duration::from_secs(8);
        loop {
            let reply = browser
                .receive(until, &cancel)
                .unwrap()
                .expect("target reply");
            if reply["id"] == 1 {
                assert!(reply.get("error").is_none());
                break;
            }
        }
        browser
            .send(&json!({"id":2,"method":"SystemInfo.getProcessInfo"}))
            .unwrap();
        let reply = loop {
            let reply = browser
                .receive(until, &cancel)
                .unwrap()
                .expect("process reply");
            if reply["id"] == 2 {
                break reply;
            }
        };
        let processes = reply["result"]["processInfo"]
            .as_array()
            .expect("browser process inventory");
        let renderer = processes
            .iter()
            .find(|process| process["type"] == "renderer")
            .expect("sandboxed renderer")["id"]
            .as_i64()
            .unwrap();
        let status = std::fs::read_to_string(format!("/proc/{renderer}/status")).unwrap();
        assert!(status.lines().any(|line| line == "Seccomp:\t2"), "{status}");
        assert!(
            status.lines().any(|line| line == "NoNewPrivs:\t1"),
            "{status}"
        );
        assert_ne!(
            std::fs::read_to_string(format!("/proc/{renderer}/uid_map")).unwrap(),
            std::fs::read_to_string("/proc/self/uid_map").unwrap()
        );
        drop(browser);
        assert!(!profile.exists());
        assert!(!std::path::Path::new(&format!("/proc/{parent}")).exists());
        assert!(!runtime.exists());
    }
}
