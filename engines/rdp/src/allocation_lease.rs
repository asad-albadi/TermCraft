//! Short-lived Linux write lease for trimming unused allocation on an opened file.
//! Unlike flock, leases exclude independent local opens. Acquisition failure is
//! an instruction to keep the allocation, never to truncate without exclusion.
use std::os::fd::{AsRawFd, RawFd};

// Linux fcntl UAPI commands absent from libc's GNU bindings.
const F_SETSIG: libc::c_int = 10;
const F_GETSIG: libc::c_int = 11;
const F_SETOWN_EX: libc::c_int = 15;
const F_GETOWN_EX: libc::c_int = 16;
#[repr(C)]
#[derive(Clone, Copy)]
struct Owner {
    kind: libc::c_int,
    pid: libc::pid_t,
}

pub(super) struct Lease<'a> {
    file: &'a cap_std::fs::File,
    owner: Owner,
    signal: libc::c_int,
    acquired: bool,
}
impl<'a> Lease<'a> {
    pub(super) fn acquire(file: &'a cap_std::fs::File) -> Option<Self> {
        let fd = file.as_raw_fd();
        // SIGURG has a nonfatal default disposition. Do not alter a global
        // handler or interfere with an application that installed its own.
        let mut action = unsafe { std::mem::zeroed::<libc::sigaction>() };
        if unsafe { libc::sigaction(libc::SIGURG, std::ptr::null(), &mut action) } != 0
            || ![libc::SIG_DFL, libc::SIG_IGN].contains(&action.sa_sigaction)
            || unsafe { libc::fcntl(fd, libc::F_GETLEASE) } != libc::F_UNLCK
        {
            return None;
        }
        let mut owner = Owner { kind: 0, pid: 0 };
        if unsafe { libc::fcntl(fd, F_GETOWN_EX, &mut owner) } != 0 {
            return None;
        }
        let signal = unsafe { libc::fcntl(fd, F_GETSIG) };
        if signal < 0 {
            return None;
        }
        let mut lease = Self {
            file,
            owner,
            signal,
            acquired: false,
        };
        let thread = Owner {
            kind: 0,
            pid: unsafe { libc::syscall(libc::SYS_gettid) as libc::pid_t },
        };
        if unsafe { libc::fcntl(fd, F_SETSIG, libc::SIGURG) } != 0
            || unsafe { libc::fcntl(fd, F_SETOWN_EX, &thread) } != 0
            || unsafe { libc::fcntl(fd, libc::F_SETLEASE, libc::F_WRLCK) } != 0
        {
            return None;
        }
        lease.acquired = true;
        Some(lease)
    }

    pub(super) fn release_tail(&self, top: u64) -> std::io::Result<()> {
        let end = self.file.metadata()?.len();
        if top > end && self.exclusive() {
            // ext4 clamps hole punching to EOF. A same-length truncate frees
            // its KEEP_SIZE tail. Keep this section short: Linux can forcibly
            // break a lease after its timeout if this thread is stalled.
            self.file.set_len(end)?;
        }
        Ok(())
    }

    pub(super) fn exclusive(&self) -> bool {
        // F_GETLEASE reports the requested target once a break is pending.
        unsafe { libc::fcntl(self.fd(), libc::F_GETLEASE) == libc::F_WRLCK }
    }
    fn fd(&self) -> RawFd {
        self.file.as_raw_fd()
    }
}
impl Drop for Lease<'_> {
    fn drop(&mut self) {
        unsafe {
            if self.acquired {
                libc::fcntl(self.fd(), libc::F_SETLEASE, libc::F_UNLCK);
            }
            libc::fcntl(self.fd(), F_SETOWN_EX, &self.owner);
            libc::fcntl(self.fd(), F_SETSIG, self.signal);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::OpenOptionsExt;

    #[test]
    fn duplicate_descriptors_allow_the_lease_and_a_new_open_requests_a_nonfatal_break() {
        if crate::devices_tests::isolated_allocation_test(
            "devices::allocation_lease::tests::duplicate_descriptors_allow_the_lease_and_a_new_open_requests_a_nonfatal_break",
        ) {
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("owned");
        let file = cap_std::fs::File::from_std(std::fs::File::create(&path).unwrap());
        use std::os::unix::fs::MetadataExt;
        rustix::fs::fallocate(&file, rustix::fs::FallocateFlags::KEEP_SIZE, 0, 1048576).unwrap();
        let duplicate = file.try_clone().unwrap();
        let lease = Lease::acquire(&file).expect("owned regular file must support a write lease");
        assert!(lease.exclusive());
        let error = std::fs::OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&path)
            .unwrap_err();
        assert_eq!(error.raw_os_error(), Some(libc::EAGAIN));
        assert!(!lease.exclusive(), "a requested break must prevent cleanup");
        lease.release_tail(1048576).unwrap();
        assert!(
            std::fs::metadata(&path).unwrap().blocks() * 512 >= 1048576,
            "a pending lease break must retain the reserved tail"
        );
        drop(lease);
        assert_eq!(
            unsafe { libc::fcntl(duplicate.as_raw_fd(), libc::F_GETLEASE) },
            libc::F_UNLCK
        );
        assert_eq!(unsafe { libc::fcntl(duplicate.as_raw_fd(), F_GETSIG) }, 0);
        let mut owner = Owner { kind: -1, pid: -1 };
        assert_eq!(
            unsafe { libc::fcntl(duplicate.as_raw_fd(), F_GETOWN_EX, &mut owner) },
            0
        );
        assert_eq!(owner.pid, 0);
        // Releasing the lease admits a separate process without a timeout.
        assert!(
            std::process::Command::new("/bin/sh")
                .args(["-c", "printf child >> \"$1\"", "rdp-fixture"])
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"child");
    }

    #[test]
    fn unsupported_file_kind_keeps_its_contents_and_notification_settings() {
        let root = tempfile::tempdir().unwrap();
        let file = cap_std::fs::File::from_std(std::fs::File::open(root.path()).unwrap());
        assert!(Lease::acquire(&file).is_none());
        assert_eq!(unsafe { libc::fcntl(file.as_raw_fd(), F_GETSIG) }, 0);
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }
}
