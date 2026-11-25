//! Own an isolated helper process group until it has been killed and reaped.
use std::{
    io,
    os::unix::process::CommandExt,
    process::{Child, Command, Stdio},
};
pub(super) struct Process {
    child: Child,
    stopped: bool,
}
impl Process {
    pub(super) fn spawn() -> io::Result<(Self, std::process::ChildStdout, std::process::ChildStdin)>
    {
        let mut command = Command::new(std::env::current_exe()?);
        command.arg("--pcsc-worker");
        Self::launch(command)
    }
    #[cfg(test)]
    pub(super) fn fixture(
        command: Command,
    ) -> io::Result<(Self, std::process::ChildStdout, std::process::ChildStdin)> {
        Self::launch(command)
    }
    fn launch(
        mut command: Command,
    ) -> io::Result<(Self, std::process::ChildStdout, std::process::ChildStdin)> {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0);
        super::super::loader_env::sanitize(&mut command);
        let mut child = command.spawn()?;
        let input = child.stdout.take().unwrap();
        let output = child.stdin.take().unwrap();
        Ok((
            Self {
                child,
                stopped: false,
            },
            input,
            output,
        ))
    }
    #[cfg(any(test, feature = "pcsc-test-macos", feature = "pcsc-test-windows"))]
    pub(super) fn pid(&self) -> u32 {
        self.child.id()
    }
    pub(super) fn stop(&mut self) {
        if self.stopped {
            return;
        }
        // Do not reap before signalling: the owned child's unreaped PID anchors this
        // isolated process group, preventing PID reuse from targeting another job.
        unsafe {
            libc::kill(-(self.child.id() as libc::pid_t), libc::SIGKILL);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.stopped = true;
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        self.stop();
    }
}
