use std::{
    fs::File,
    io::Read,
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
};
pub struct Process {
    pub rx: Receiver<Vec<u8>>,
    writer: File,
    input: super::InputQueue,
    #[cfg(target_os = "linux")]
    _private_mosh: Option<crate::runtime::MoshLaunch>,
    child: Child,
    stop: Arc<AtomicBool>,
    reader: Option<std::thread::JoinHandle<()>>,
    pub exited: bool,
    pub exit_message: String,
}
impl Process {
    #[cfg(unix)]
    pub fn spawn(
        program: &str,
        args: &[String],
        directory: Option<&Path>,
        cols: u16,
        rows: u16,
    ) -> Result<Self, String> {
        Self::spawn_with_environment(program, args, directory, cols, rows, &[])
    }

    pub fn spawn_with_environment(
        program: &str,
        args: &[String],
        directory: Option<&Path>,
        cols: u16,
        rows: u16,
        environment: &[(std::ffi::OsString, std::ffi::OsString)],
    ) -> Result<Self, String> {
        Self::spawn_inner(program, args, directory, cols, rows, environment, None)
    }

    pub fn spawn_with_environment_cancellable(
        program: &str,
        args: &[String],
        directory: Option<&Path>,
        cols: u16,
        rows: u16,
        environment: &[(std::ffi::OsString, std::ffi::OsString)],
        cancelled: &AtomicBool,
    ) -> Result<Self, String> {
        Self::spawn_inner(
            program,
            args,
            directory,
            cols,
            rows,
            environment,
            Some(cancelled),
        )
    }

    pub fn spawn_cancellable(
        program: &str,
        args: &[String],
        directory: Option<&Path>,
        cols: u16,
        rows: u16,
        cancelled: &AtomicBool,
    ) -> Result<Self, String> {
        Self::spawn_inner(program, args, directory, cols, rows, &[], Some(cancelled))
    }

    fn spawn_inner(
        program: &str,
        args: &[String],
        directory: Option<&Path>,
        cols: u16,
        rows: u16,
        environment: &[(std::ffi::OsString, std::ffi::OsString)],
        cancelled: Option<&AtomicBool>,
    ) -> Result<Self, String> {
        use std::os::{
            fd::{AsRawFd, FromRawFd},
            unix::process::CommandExt,
        };
        let mut master = -1;
        let mut slave = -1;
        let mut size = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        if unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::addr_of_mut!(size),
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let master = unsafe { File::from_raw_fd(master) };
        let slave = unsafe { File::from_raw_fd(slave) };
        // The PTY pair belongs to this session. Prevent descriptor inheritance at exec.
        for file in [&master, &slave] {
            rustix::io::fcntl_setfd(file, rustix::io::FdFlags::CLOEXEC)
                .map_err(|error| error.to_string())?;
        }
        let reader = master.try_clone().map_err(|e| e.to_string())?;
        let input = super::InputQueue::new(master.try_clone().map_err(|e| e.to_string())?)?;
        let fd = slave.as_raw_fd();
        #[cfg(target_os = "linux")]
        let private_mosh = crate::runtime::prepare_mosh_launch(program, args)?;
        #[cfg(target_os = "linux")]
        let (command_path, command_args) = private_mosh
            .as_ref()
            .map(|runtime| (runtime.program.as_path(), runtime.args.as_slice()))
            .unwrap_or((Path::new(program), args));
        #[cfg(not(target_os = "linux"))]
        let (command_path, command_args) = (Path::new(program), args);
        let mut command = Command::new(command_path);
        command
            .args(command_args)
            .envs(environment.iter().map(|(name, value)| (name, value)));
        command
            .env("TERM", "xterm-256color")
            .env("COLORTERM", "truecolor");
        if let Some(d) = directory {
            command.current_dir(d);
        }
        command
            .stdin(Stdio::from(slave.try_clone().map_err(|e| e.to_string())?))
            .stdout(Stdio::from(slave.try_clone().map_err(|e| e.to_string())?))
            .stderr(Stdio::from(slave));
        unsafe {
            command.pre_exec(move || {
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::ioctl(fd, libc::TIOCSCTTY as _, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        if cancelled.is_some_and(|flag| flag.load(Ordering::Acquire)) {
            return Err("Terminal startup cancelled".into());
        }
        let child = command
            .spawn()
            .map_err(|e| format!("Could not start {program}: {e}"))?;
        let stop = Arc::new(AtomicBool::new(false));
        let (rx, reader) = read_thread(reader, stop.clone());
        Ok(Self {
            rx,
            writer: master,
            input,
            #[cfg(target_os = "linux")]
            _private_mosh: private_mosh,
            child,
            stop,
            reader: Some(reader),
            exited: false,
            exit_message: String::new(),
        })
    }
    pub fn input_error(&self) -> Option<String> {
        self.input.error()
    }
    pub fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.input.submit(bytes)
    }
    pub fn poll(&mut self) {
        if !self.exited {
            unsafe {
                let mut info: libc::siginfo_t = std::mem::zeroed();
                if libc::waitid(
                    libc::P_PID,
                    self.child.id() as _,
                    &mut info,
                    libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                ) == 0
                    && info.si_pid() != 0
                {
                    self.exited = true;
                    self.exit_message = format!("Process ended with status {}", info.si_status());
                }
            }
        }
    }
    pub fn resize(&self, cols: u16, rows: u16) {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            let s = libc::winsize {
                ws_row: rows,
                ws_col: cols,
                ws_xpixel: 0,
                ws_ypixel: 0,
            };
            unsafe {
                libc::ioctl(self.writer.as_raw_fd(), libc::TIOCSWINSZ as _, &s);
            }
        }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        use std::os::fd::AsRawFd;
        self.stop.store(true, Ordering::Release);
        let leader = self.child.id() as i32;
        let foreground = unsafe { libc::tcgetpgrp(self.writer.as_raw_fd()) };
        unsafe {
            for group in [foreground, leader] {
                if group > 0 {
                    libc::kill(-group, libc::SIGHUP);
                    libc::kill(-group, libc::SIGTERM);
                }
            }
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        // Keep the unreaped session leader alive until the final group signals, so its PID cannot be reused.
        unsafe {
            for group in [foreground, leader] {
                if group > 0 {
                    libc::kill(-group, libc::SIGKILL);
                }
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn read_thread(
    mut file: File,
    stop: Arc<AtomicBool>,
) -> (Receiver<Vec<u8>>, std::thread::JoinHandle<()>) {
    use std::os::fd::AsRawFd;
    let (tx, rx) = mpsc::sync_channel(256);
    let handle = thread::spawn(move || {
        let mut buf = [0u8; 32768];
        while !stop.load(Ordering::Acquire) {
            let mut fd = libc::pollfd {
                fd: file.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            let ready = unsafe { libc::poll(&mut fd, 1, 100) };
            if ready < 0 {
                if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                break;
            }
            if ready == 0 {
                continue;
            }
            let n = match file.read(&mut buf) {
                Ok(0) => break,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) =>
                {
                    continue;
                }
                Err(_) => break,
                Ok(n) => n,
            };
            let mut bytes = buf[..n].to_vec();
            loop {
                match tx.try_send(bytes) {
                    Ok(()) => break,
                    Err(mpsc::TrySendError::Disconnected(_)) => return,
                    Err(mpsc::TrySendError::Full(v)) => {
                        bytes = v;
                        if stop.load(Ordering::Acquire) {
                            return;
                        }
                        thread::sleep(std::time::Duration::from_millis(1));
                    }
                }
            }
        }
    });
    (rx, handle)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_shell_uses_pty_and_returns_output() {
        #[cfg(unix)]
        {
            use std::time::{Duration, Instant};
            let mut p = Process::spawn(
                "/bin/sh",
                &["-c".into(), "test -t 0 && printf 'PTY_OK'".into()],
                None,
                80,
                24,
            )
            .unwrap();
            let mut output = Vec::new();
            let deadline = Instant::now() + Duration::from_secs(3);
            while Instant::now() < deadline {
                if let Ok(b) = p.rx.recv_timeout(Duration::from_millis(50)) {
                    output.extend(b);
                }
                p.poll();
                if p.exited {
                    while let Ok(b) = p.rx.try_recv() {
                        output.extend(b);
                    }
                    break;
                }
            }
            assert!(String::from_utf8_lossy(&output).contains("PTY_OK"));
        }
    }
}
#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    #[test]
    fn dropping_tab_ends_foreground_and_background_jobs() {
        use std::time::Duration;
        let marker = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data/child-pid");
        std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
        let script = format!(
            "trap '' HUP; sleep 120 & echo $! > '{}'; exit",
            marker.display()
        );
        let mut p = Process::spawn("/bin/sh", &["-c".into(), script], None, 80, 24).unwrap();
        for _ in 0..100 {
            if marker.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let pid: i32 = std::fs::read_to_string(&marker)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        std::thread::sleep(Duration::from_millis(30));
        p.poll();
        drop(p);
        std::thread::sleep(Duration::from_millis(150));
        let running = std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok()
            .is_some_and(|s| !s.split_whitespace().nth(2).is_some_and(|s| s == "Z"));
        if running {
            unsafe {
                libc::kill(pid, libc::SIGKILL);
            }
        }
        let _ = std::fs::remove_file(marker);
        assert!(!running, "background process outlived closed terminal");
    }
}
