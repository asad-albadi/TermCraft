use std::{
    fs::File,
    io::{self, Read},
    os::{fd::AsRawFd, unix::process::CommandExt},
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

pub(super) struct PluginChild {
    child: Child,
    stdout: Option<ChildStdout>,
    group: i32,
}

impl PluginChild {
    pub(super) fn spawn(
        directory: &Path,
        executable: &str,
        verified: &File,
    ) -> io::Result<(Self, ChildStdin)> {
        let path = directory.join("bin").join(executable);
        #[cfg(target_os = "linux")]
        let image = format!("/proc/self/fd/{}", verified.as_raw_fd());
        #[cfg(not(target_os = "linux"))]
        let image = {
            let _ = verified;
            &path
        };
        let parent = unsafe { libc::getpid() };
        let mut command = Command::new(image);
        command
            .arg0(&path)
            .current_dir(directory)
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0);
        // SAFETY: the closure uses only async-signal-safe system calls.
        unsafe {
            command.pre_exec(move || {
                #[cfg(target_os = "linux")]
                {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL, 0, 0, 0) != 0 {
                        return Err(io::Error::last_os_error());
                    }
                    if libc::getppid() != parent {
                        return Err(io::Error::from_raw_os_error(libc::ESRCH));
                    }
                }
                #[cfg(not(target_os = "linux"))]
                let _ = parent;
                let limits = libc::rlimit {
                    rlim_cur: 0,
                    rlim_max: 0,
                };
                if libc::setrlimit(libc::RLIMIT_CORE, &limits) != 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command.spawn()?;
        let mut process = Self {
            group: child.id() as i32,
            child,
            stdout: None,
        };
        let stdout = process
            .child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("Missing plugin stdout"))?;
        let fd = stdout.as_raw_fd();
        // SAFETY: fd is owned by a live ChildStdout.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags == -1 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1
        {
            return Err(io::Error::last_os_error());
        }
        process.stdout = Some(stdout);
        let stdin = process
            .child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("Missing plugin stdin"))?;
        Ok((process, stdin))
    }

    pub(super) fn exited(&mut self) -> io::Result<Option<bool>> {
        #[cfg(target_os = "linux")]
        {
            // Leave the child unreaped until its process group has been killed.
            // While the leader exists its PID cannot be reused by another group.
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            // SAFETY: info is a writable siginfo_t and child.id() belongs to this process.
            if unsafe {
                libc::waitid(
                    libc::P_PID,
                    self.child.id(),
                    &mut info,
                    libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                )
            } == -1
            {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    return Ok(None);
                }
                return Err(error);
            }
            Ok((unsafe { info.si_pid() } != 0)
                .then(|| info.si_code == libc::CLD_EXITED && unsafe { info.si_status() } == 0))
        }
        #[cfg(not(target_os = "linux"))]
        Ok(self.child.try_wait()?.map(|status| status.success()))
    }

    pub(super) fn read_available(&mut self, buffer: &mut [u8]) -> io::Result<Option<usize>> {
        let stdout = self
            .stdout
            .as_mut()
            .ok_or_else(|| io::Error::other("Missing plugin stdout"))?;
        match stdout.read(buffer) {
            Ok(count) => Ok(Some(count)),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }
}

impl Drop for PluginChild {
    fn drop(&mut self) {
        if self.group > 0 {
            // SAFETY: this group was created for the owned child at spawn.
            unsafe {
                libc::kill(-self.group, libc::SIGKILL);
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(all(test, target_os = "linux"))]
mod verified_image_regressions {
    use super::*;
    use std::{fs, io::Write, sync::atomic::AtomicBool};
    #[test]
    fn mutation_after_verification_executes_original_elf() {
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let executable = bin.join("fixture");
        for (name, value) in [("fixture", "true"), ("replacement", "false")] {
            let source = root.path().join(format!("{name}.c"));
            fs::write(&source, format!("#include <stdio.h>\nint main(void) {{ puts(\"{{\\\"protocol\\\":1,\\\"ok\\\":{value}}}\"); return 0; }}")).unwrap();
            assert!(
                Command::new("cc")
                    .arg("-o")
                    .arg(bin.join(name))
                    .arg(source)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let mut original = File::open(&executable).unwrap();
        let mut verified = verification_target().unwrap();
        let _ = crate::packages::install::copy_and_hash(
            &mut original,
            &mut verified,
            &AtomicBool::new(false),
        )
        .unwrap();
        finish_verification(&verified).unwrap();
        assert_eq!(
            (&verified)
                .write_all(b"mutation")
                .unwrap_err()
                .raw_os_error(),
            Some(libc::EPERM)
        );
        fs::write(&executable, fs::read(bin.join("replacement")).unwrap()).unwrap();
        let (mut child, mut stdin) = PluginChild::spawn(root.path(), "fixture", &verified).unwrap();
        stdin.write_all(b"{}\n").unwrap();
        drop(stdin);
        let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut output = Vec::new();
        loop {
            let mut buffer = [0; 256];
            if let Some(n) = child.read_available(&mut buffer).unwrap() {
                output.extend_from_slice(&buffer[..n]);
            }
            if child.exited().unwrap().is_some() {
                break;
            }
            assert!(std::time::Instant::now() < until);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let mut buffer = [0; 256];
        if let Some(n) = child.read_available(&mut buffer).unwrap() {
            output.extend_from_slice(&buffer[..n]);
        }
        assert!(String::from_utf8(output).unwrap().contains("\"ok\":true"));
    }
}

#[cfg(target_os = "linux")]
pub(in crate::plugins) fn verification_target() -> io::Result<File> {
    use std::os::fd::FromRawFd;
    // A sealable anonymous image has no mutable installed-file alias.
    let fd = unsafe {
        libc::memfd_create(
            c"TermCraft-plugin".as_ptr(),
            libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING,
        )
    };
    if fd == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}
#[cfg(target_os = "linux")]
pub(in crate::plugins) fn finish_verification(file: &File) -> io::Result<()> {
    let seals = libc::F_SEAL_WRITE | libc::F_SEAL_GROW | libc::F_SEAL_SHRINK | libc::F_SEAL_SEAL;
    if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_ADD_SEALS, seals) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
