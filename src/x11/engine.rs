#[cfg(windows)]
pub(super) use super::native_windows_process::OwnedProcess;
#[cfg(not(windows))]
use std::io;
#[cfg(not(windows))]
use std::process::Child;

#[cfg(not(windows))]
pub(super) struct OwnedProcess {
    child: Option<Child>,
}

#[cfg(not(windows))]
impl OwnedProcess {
    #[cfg(any(
        all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"),
        target_os = "macos"
    ))]
    pub(super) fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }

    pub(super) fn id(&self) -> Option<u32> {
        self.child.as_ref().map(Child::id)
    }

    pub(super) fn is_running(&mut self) -> io::Result<bool> {
        #[cfg(target_os = "macos")]
        if let Some(child) = &mut self.child {
            // Observe exit without reaping: retain the PID/group identity until
            // every owned helper has received termination, then reap the leader.
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            if unsafe {
                libc::waitid(
                    libc::P_PID,
                    child.id(),
                    &mut info,
                    libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                )
            } < 0
            {
                return Err(io::Error::last_os_error());
            }
            if unsafe { info.si_pid() } == 0 {
                return Ok(true);
            }
            unsafe {
                libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
            }
            child.wait()?;
            self.child = None;
            return Ok(false);
        }
        #[cfg(not(target_os = "macos"))]
        if let Some(child) = &mut self.child {
            if child.try_wait()?.is_none() {
                return Ok(true);
            }
            self.child = None;
        }
        Ok(false)
    }

    pub(super) fn stop(&mut self) -> io::Result<()> {
        if !self.is_running()? {
            return Ok(());
        }
        let child = self.child.as_mut().expect("running child is owned");
        #[cfg(target_os = "linux")]
        {
            use std::time::{Duration, Instant};

            // The Child has not been reaped, so its PID cannot belong to another process.
            unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGTERM) };
            let deadline = Instant::now() + Duration::from_millis(500);
            while Instant::now() < deadline {
                if child.try_wait()?.is_some() {
                    self.child = None;
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        #[cfg(target_os = "macos")]
        unsafe {
            libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
        }
        #[cfg(not(target_os = "macos"))]
        child.kill()?;
        child.wait()?;
        self.child = None;
        Ok(())
    }
}

#[cfg(not(windows))]
impl Drop for OwnedProcess {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
