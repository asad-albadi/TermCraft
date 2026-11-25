use super::{Preferences, Process, Session, connection};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const MAX_PENDING: usize = 4;
static PENDING: AtomicUsize = AtomicUsize::new(0);

type Started = Result<Process, String>;

#[derive(Default)]
struct Shared {
    cancelled: AtomicBool,
    result: Mutex<Option<Started>>,
}

pub(super) struct Pending {
    shared: Arc<Shared>,
    worker: JoinHandle<()>,
}

impl Pending {
    pub(super) fn start(
        session: Session,
        preferences: Preferences,
        cols: u16,
        rows: u16,
    ) -> Result<Self, String> {
        Self::start_with(move |cancelled| {
            let launch = connection::launch(&session, &preferences)?;
            if cancelled.load(Ordering::Acquire) {
                return Err("Mosh startup cancelled".into());
            }
            #[cfg(unix)]
            return Process::spawn_cancellable(
                &launch.program,
                &launch.args,
                launch.directory.as_deref(),
                cols,
                rows,
                cancelled,
            );
            #[cfg(windows)]
            Process::spawn(
                &launch.program,
                &launch.args,
                launch.directory.as_deref(),
                cols,
                rows,
            )
        })
    }

    pub(super) fn start_with(
        run: impl FnOnce(&AtomicBool) -> Started + Send + 'static,
    ) -> Result<Self, String> {
        let slot = Slot::reserve()?;
        let shared = Arc::new(Shared::default());
        let background = shared.clone();
        let worker = thread::Builder::new()
            .name("termcraft-mosh-start".into())
            .spawn(move || {
                let _slot = slot;
                if background.cancelled.load(Ordering::Acquire) {
                    return;
                }
                let result = run(&background.cancelled);
                *background.result.lock().unwrap_or_else(|p| p.into_inner()) = Some(result);
                loop {
                    let mut result = background.result.lock().unwrap_or_else(|p| p.into_inner());
                    if background.cancelled.load(Ordering::Acquire) {
                        // Drop an undelivered process here so the UI never waits for teardown.
                        let undelivered = result.take();
                        drop(result);
                        drop(undelivered);
                        return;
                    }
                    if result.is_none() {
                        return;
                    }
                    drop(result);
                    thread::park_timeout(Duration::from_millis(50));
                }
            })
            .map_err(|error| format!("Could not start the Mosh worker: {error}"))?;
        Ok(Self { shared, worker })
    }

    pub(super) fn poll(&self) -> Option<Started> {
        let result = self.shared.result.try_lock().ok()?.take();
        if result.is_some() {
            self.worker.thread().unpark();
            return result;
        }
        self.worker
            .is_finished()
            .then(|| Err("The Mosh startup worker stopped unexpectedly".into()))
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        self.shared.cancelled.store(true, Ordering::Release);
        self.worker.thread().unpark();
    }
}

struct Slot;

impl Slot {
    fn reserve() -> Result<Self, String> {
        if PENDING.fetch_add(1, Ordering::AcqRel) < MAX_PENDING {
            Ok(Self)
        } else {
            PENDING.fetch_sub(1, Ordering::Release);
            Err("Four Mosh sessions are already starting. Try again in a moment.".into())
        }
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        PENDING.fetch_sub(1, Ordering::Release);
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::super::{Context, RawInput, Workspace};
    use super::*;
    use std::sync::mpsc;
    use std::time::Instant;

    #[test]
    fn mosh_startup_keeps_render_and_input_live_and_cancels_owned_process() {
        let ctx = Context::default();
        let work = tempfile::tempdir().unwrap();
        let mut workspace = Workspace::new_context(&ctx, None, work.path().join("workspace.json"));
        workspace.start_with_launch(
            Session {
                protocol: "Local".into(),
                ..Session::default()
            },
            Some(connection::Launch {
                program: "/bin/sh".into(),
                args: vec![
                    "-c".into(),
                    "read value; printf 'reply-%s\\n' \"$value\"".into(),
                ],
                directory: None,
                rdp_profile: None,
            }),
        );
        workspace.start(Session {
            protocol: "Mosh".into(),
            mosh_provider: crate::ui::MoshProvider::Bundled,
            host: "127.0.0.1".into(),
            ..Session::default()
        });
        let mosh = workspace.active;
        assert!(workspace.tabs[mosh].pending_mosh.is_some());
        assert!(!workspace.tabs[mosh].ready_for_input());
        let (entered, ready) = mpsc::sync_channel(1);
        let (resume, delayed) = mpsc::sync_channel(1);
        let (cancelled, stopped) = mpsc::sync_channel(1);
        let startup = Pending::start_with(move |flag| {
            let private = tempfile::tempdir().unwrap();
            let file = private.path().join("pid");
            let process = Process::spawn(
                "/bin/sh",
                &[
                    "-c".into(),
                    "printf '%s' \"$$\" >\"$1\"; exec /bin/sleep 30".into(),
                    "fixture".into(),
                    file.to_str().unwrap().into(),
                ],
                None,
                80,
                24,
            )?;
            let deadline = Instant::now() + Duration::from_secs(5);
            let pid = loop {
                if let Ok(value) = std::fs::read_to_string(&file)
                    && let Ok(pid) = value.parse::<u32>()
                {
                    break pid;
                }
                assert!(Instant::now() < deadline, "fixture process did not start");
                thread::sleep(Duration::from_millis(5));
            };
            entered.send((pid, private.path().to_path_buf())).unwrap();
            delayed.recv_timeout(Duration::from_secs(5)).unwrap();
            cancelled.send(flag.load(Ordering::Acquire)).unwrap();
            Ok(process)
        })
        .unwrap();
        let (pid, private_path) = ready.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(private_path.exists());
        assert!(startup.poll().is_none());
        workspace.tabs[mosh].pending_mosh = Some(startup);
        workspace.tabs[0].send(b"during-startup\n");
        let deadline = Instant::now() + Duration::from_secs(5);
        while !workspace.tabs[0]
            .screen
            .plain()
            .contains("reply-during-startup")
        {
            let mut output = ctx.run_ui(RawInput::default(), |ui| workspace.render(ui));
            output.textures_delta.clear();
            assert!(!output.shapes.is_empty());
            assert!(
                Instant::now() < deadline,
                "another terminal stopped responding during Mosh startup"
            );
            thread::sleep(Duration::from_millis(5));
        }
        assert!(workspace.tabs[mosh].pending_mosh.is_some());
        workspace.force_close_tab(mosh);
        resume.send(()).unwrap();
        assert!(stopped.recv_timeout(Duration::from_secs(5)).unwrap());
        let deadline = Instant::now() + Duration::from_secs(5);
        let process = std::path::PathBuf::from(format!("/proc/{pid}"));
        while private_path.exists() || process.exists() {
            assert!(
                Instant::now() < deadline,
                "cancelled startup retained private files or an owned child"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn cancelled_mosh_preparation_cannot_spawn_a_child() {
        let root = tempfile::tempdir().unwrap();
        let marker = root.path().join("spawned");
        let marker_path = marker.to_str().unwrap().to_owned();
        let (entered, ready) = mpsc::sync_channel(1);
        let (resume, delayed) = mpsc::sync_channel(1);
        let (completed, result) = mpsc::sync_channel(1);
        let startup = Pending::start_with(move |flag| {
            let private = tempfile::tempdir().unwrap();
            entered.send(private.path().to_path_buf()).unwrap();
            delayed.recv_timeout(Duration::from_secs(5)).unwrap();
            let process = Process::spawn_cancellable(
                "/bin/sh",
                &[
                    "-c".into(),
                    "printf spawned >\"$1\"".into(),
                    "fixture".into(),
                    marker_path,
                ],
                None,
                80,
                24,
                flag,
            );
            completed
                .send(matches!(&process, Err(error) if error.contains("cancelled")))
                .unwrap();
            process
        })
        .unwrap();
        let staged = ready.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(staged.is_dir());
        drop(startup);
        resume.send(()).unwrap();
        assert!(result.recv_timeout(Duration::from_secs(5)).unwrap());
        let deadline = Instant::now() + Duration::from_secs(5);
        while staged.exists() {
            assert!(
                Instant::now() < deadline,
                "cancelled preparation kept private files"
            );
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            !marker.exists(),
            "a cancelled preparation launched its child"
        );
    }
}
