fn main() {
    #[cfg(any(feature = "pcsc-test-macos", feature = "pcsc-test-windows"))]
    if std::env::args().nth(1).as_deref() == Some("--fixture-parent") {
        d02_smartcard_native_component::fixture_parent();
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    if std::env::args().nth(1).as_deref() == Some("--fixture-watchdog-audit") {
        audit_watchdog();
        return;
    }

    if std::env::var_os("D02_NATIVE_STALL").is_some() {
        std::thread::sleep(std::time::Duration::from_secs(60));
        return;
    }
    #[cfg(any(feature = "pcsc-test-macos", feature = "pcsc-test-windows"))]
    if d02_smartcard_native_component::fixture_entry().is_err() {
        std::process::exit(1);
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn audit_watchdog() {
    // Subreaper is confined to this disposable audit process. Every orphan is
    // adopted and reaped here; the test does not leave container-init zombies.
    #[cfg(target_os = "linux")]
    assert_eq!(
        unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) },
        0
    );
    let marker = std::env::temp_dir().join(format!("d02-watchdog-audit-{}", std::process::id()));
    let _ = std::fs::remove_file(&marker);
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--fixture-parent")
        .env("D02_NATIVE_ESTABLISH_STALL", &marker)
        .output()
        .unwrap();
    assert!(output.status.success());
    let pid: libc::pid_t = std::str::from_utf8(&output.stdout)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    #[cfg(target_os = "macos")]
    loop {
        // launchd adopts and reaps the orphan. This test process cannot waitpid
        // it, and must not send a signal after the parent's ownership ends.
        if unsafe { libc::kill(pid, 0) } == -1 {
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(libc::ESRCH)
            );
            std::fs::remove_file(&marker).unwrap();
            break;
        }
        assert!(std::time::Instant::now() < deadline, "Owned worker survived parent death");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    #[cfg(target_os = "linux")]
    loop {
        let mut status = 0;
        let result = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
        if result == pid {
            assert!(libc::WIFSIGNALED(status));
            assert_eq!(libc::WTERMSIG(status), libc::SIGKILL);
            std::fs::remove_file(&marker).unwrap();
            break;
        }
        assert_eq!(result, 0);
        if std::time::Instant::now() >= deadline {
            unsafe {
                libc::kill(pid, libc::SIGKILL);
                libc::waitpid(pid, std::ptr::null_mut(), 0);
            }
            panic!("Owned worker survived parent death");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
