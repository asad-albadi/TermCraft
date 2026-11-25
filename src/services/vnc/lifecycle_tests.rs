//! Controlled in-flight Source failures; never a native display, input or clipboard.
use super::*;
use crate::services::{ServiceId, ServiceState, ServiceSupervisor};
use std::sync::atomic::AtomicBool;
pub(super) struct Controlled {
    entered: Arc<AtomicBool>,
    release: Arc<AtomicBool>,
    released: Arc<AtomicBool>,
    clipboard: bool,
    captures: usize,
}
impl Controlled {
    pub(super) fn new(
        entered: Arc<AtomicBool>,
        release: Arc<AtomicBool>,
        released: Arc<AtomicBool>,
        clipboard: bool,
    ) -> Self {
        Self {
            entered,
            release,
            released,
            clipboard,
            captures: 0,
        }
    }
    fn fail_after_release<T>(&self) -> io::Result<T> {
        self.entered.store(true, Ordering::Release);
        let deadline = Instant::now() + Duration::from_secs(3);
        while !self.release.load(Ordering::Acquire) {
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Controlled source test deadline",
                ));
            }
            thread::sleep(Duration::from_millis(1));
        }
        // Interrupted alone must not be treated as intentional service cancellation.
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Controlled source failure",
        ))
    }
}
impl Source for Controlled {
    fn enable_clipboard(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn clipboard(&mut self) -> io::Result<Option<Zeroizing<String>>> {
        if self.clipboard {
            self.fail_after_release()
        } else {
            Ok(None)
        }
    }
    fn capture(&mut self) -> io::Result<Frame> {
        self.captures += 1;
        if self.captures > 1 && !self.clipboard {
            return self.fail_after_release();
        }
        Ok(Frame {
            width: 1,
            height: 1,
            rgba: vec![0, 0, 0, 255],
        })
    }
    fn input(&mut self, event: Input) -> io::Result<()> {
        if matches!(event, Input::Release) {
            self.released.store(true, Ordering::Release);
        }
        Ok(())
    }
}
fn await_terminal(supervisor: &ServiceSupervisor, id: ServiceId) -> crate::services::ServiceStatus {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let status = supervisor.status(id).unwrap();
        if matches!(status.state, ServiceState::Stopped | ServiceState::Failed) {
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "service did not finish: {status:?}"
        );
        thread::sleep(Duration::from_millis(2));
    }
}
fn run(clipboard: bool, stop: bool) {
    let supervisor = ServiceSupervisor::default();
    let entered = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let released = Arc::new(AtomicBool::new(false));
    let mut config = VncServiceConfig::local(
        VncSource::LifecycleFixture {
            entered: entered.clone(),
            release: release.clone(),
            released: released.clone(),
            clipboard,
        },
        Secret::new("fixture"),
    );
    config.allow_unencrypted = true;
    config.allow_clipboard = clipboard;
    config.browser_bind = Some("127.0.0.1:0".parse().unwrap());
    let id = supervisor.start_vnc(config).unwrap();
    let active = supervisor.status(id).unwrap();
    assert_eq!(active.state, ServiceState::Running);
    let deadline = Instant::now() + Duration::from_secs(2);
    while !entered.load(Ordering::Acquire) {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    if stop {
        supervisor.stop(id).unwrap();
    }
    release.store(true, Ordering::Release);
    let status = await_terminal(&supervisor, id);
    if stop {
        assert_eq!(
            status.state,
            ServiceState::Stopped,
            "intentional Stop must win an in-flight Source error"
        );
        assert!(status.error.is_none());
        assert!(
            released.load(Ordering::Acquire),
            "normal Source release cleanup was skipped"
        );
    } else {
        assert_eq!(
            status.state,
            ServiceState::Failed,
            "uncancelled Interrupted must remain a real failure"
        );
        assert!(status.error.unwrap().contains(if clipboard {
            "clipboard stopped"
        } else {
            "capture stopped"
        }));
    }
    assert!(TcpStream::connect(active.address.unwrap()).is_err());
    assert!(TcpStream::connect(active.browser_address.unwrap()).is_err());
}
#[test]
fn intentional_stop_during_source_capture_or_clipboard_failure_stays_stopped() {
    for clipboard in [false, true] {
        run(clipboard, true);
    }
}
#[test]
fn uncancelled_interrupted_source_capture_or_clipboard_failure_stays_failed() {
    for clipboard in [false, true] {
        run(clipboard, false);
    }
}
