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
        #[cfg(target_os = "macos")]
        if runtime == super::Runtime::BundledChromium {
            use std::os::unix::ffi::OsStrExt;
            // macOS initializes Crashpad even with the crash reporting flags
            // below. Keep its database inside the owned profile, not Cocoa HOME.
            command.env("BREAKPAD_DUMP_LOCATION", profile.path().join("Crashpad"));
            for (name, _) in std::env::vars_os() {
                let bytes = name.as_os_str().as_bytes();
                if bytes.starts_with(b"DYLD_") || bytes.starts_with(b"__XPC_DYLD_") {
                    command.env_remove(name);
                }
            }
        }
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

    #[cfg(test)]
    pub(super) fn test_connection() -> (Self, UnixStream, UnixStream) {
        let (writer, browser_reader) = UnixStream::pair().unwrap();
        let (browser_writer, reader) = UnixStream::pair().unwrap();
        reader
            .set_read_timeout(Some(Duration::from_millis(30)))
            .unwrap();
        let process = Command::new("/bin/sleep")
            .arg("60")
            .process_group(0)
            .spawn()
            .unwrap();
        (
            Self {
                process,
                writer,
                reader,
                pending: Vec::new(),
                _profile: tempfile::tempdir().unwrap(),
                _runtime: None,
            },
            browser_writer,
            browser_reader,
        )
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

#[cfg(all(
    test,
    target_os = "macos",
    target_arch = "aarch64",
    feature = "macos-chromium-qualification"
))]
mod macos_bundled_qualification {
    use super::*;
    use serde_json::json;

    unsafe extern "C" {
        fn sandbox_check(
            pid: libc::pid_t,
            operation: *const libc::c_char,
            filter: libc::c_int,
            ...
        ) -> libc::c_int;
    }

    fn request(pipe: &mut Pipe, id: u32, method: &str, params: Value) -> Value {
        pipe.send(&json!({"id": id, "method": method, "params": params}))
            .unwrap();
        let until = Instant::now() + Duration::from_secs(if id == 1 { 25 } else { 10 });
        let cancel = AtomicBool::new(false);
        loop {
            let reply = pipe.receive(until, &cancel).unwrap().expect("CDP reply");
            if reply["id"] == id {
                assert!(reply.get("error").is_none(), "{reply}");
                return reply["result"].clone();
            }
        }
    }

    fn process_inventory() -> String {
        let result = Command::new("/bin/ps")
            .args(["-axww", "-o", "pid=,pgid=,command="])
            .output()
            .unwrap();
        assert!(result.status.success());
        String::from_utf8_lossy(&result.stdout).into_owned()
    }

    #[test]
    #[ignore = "requires pinned native fixture in relocated TermCraft.app Resources"]
    fn bundled_renderer_is_seatbelt_sandboxed_and_owned_processes_are_reaped() {
        let start = || {
            Pipe::start_with_runtime(
                "",
                super::super::Runtime::BundledChromium,
                &AtomicBool::new(false),
            )
            .unwrap()
        };
        let mut peer = start();
        let mut browser = start();
        let profile = browser._profile.path().to_owned();
        let runtime = browser._runtime.as_ref().unwrap().path().to_owned();
        let leader = browser.process.id() as libc::pid_t;
        for dir in [&profile, &runtime] {
            assert_eq!(dir.metadata().unwrap().permissions().mode() & 0o777, 0o700);
        }
        let version = request(&mut browser, 1, "Browser.getVersion", json!({}));
        assert!(
            version["product"]
                .as_str()
                .unwrap()
                .ends_with("/160.0.8185.0")
        );
        request(
            &mut browser,
            2,
            "Target.createTarget",
            json!({"url":"about:blank"}),
        );
        let processes = request(&mut browser, 3, "SystemInfo.getProcessInfo", json!({}));
        let renderer = processes["processInfo"]
            .as_array()
            .unwrap()
            .iter()
            .find(|process| process["type"] == "renderer")
            .expect("renderer")["id"]
            .as_i64()
            .unwrap() as libc::pid_t;
        // Chromium itself uses the NULL-operation, no-filter Seatbelt status query.
        let parent_seatbelt = unsafe { sandbox_check(leader, std::ptr::null(), 0) };
        let renderer_seatbelt = unsafe { sandbox_check(renderer, std::ptr::null(), 0) };
        assert_eq!((parent_seatbelt, renderer_seatbelt), (0, 1));
        println!("Seatbelt query: browser={parent_seatbelt}, renderer={renderer_seatbelt}");
        let before = process_inventory();
        let private_path = runtime.to_str().unwrap();
        let owned = before
            .lines()
            .filter(|line| line.contains(private_path))
            .collect::<Vec<_>>();
        assert!(owned.len() >= 2);
        let crash_db = format!("--database={}", profile.join("Crashpad").display());
        for line in &owned {
            if line.contains("/Helpers/chrome_crashpad_handler") {
                assert!(
                    line.contains(&crash_db),
                    "Crashpad database is not private: {line}"
                );
            }
            for flag in [
                "--no-sandbox",
                "--ignore-certificate-errors",
                "--remote-debugging-port",
            ] {
                assert!(!line.contains(flag), "{line}");
            }
        }
        let args = owned
            .iter()
            .find(|line| line.split_whitespace().next() == Some(&leader.to_string()))
            .unwrap();
        assert!(args.contains("--remote-debugging-pipe"));
        println!("Owned Chromium processes before close: {}", owned.len());
        drop(browser);
        assert!(!profile.exists());
        assert!(!runtime.exists());
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let inventory = process_inventory();
            let live = inventory
                .lines()
                .filter(|line| line.contains(private_path))
                .collect::<Vec<_>>();
            if live.is_empty() && unsafe { libc::kill(-leader, 0) } == -1 {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Chromium processes survived close: {live:?}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!profile.exists(), "Crashpad recreated the closed profile");
        assert!(peer._profile.path().exists());
        request(&mut peer, 1, "Browser.getVersion", json!({}));
        println!("Owned process group, profile and snapshot removed; peer still responsive");
    }
}
