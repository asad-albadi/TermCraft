use super::*;
use harbor_workspace::{
    environment::UnixEnvironment,
    packages::{Outcome, PackageIndex, PackageJob, PackageManager, PackageRelease, Progress},
};

#[path = "legacy_packages.rs"]
mod legacy;

pub(super) const PORTABLE_SESSION: &str = "Portable Unix";

#[derive(Default)]
pub(super) struct State {
    legacy: legacy::State,
    version: Option<String>,
    status: String,
    job: Option<PackageJob>,
    progress: Option<Progress>,
    archive: Option<(PackageRelease, PathBuf, bool)>,
    installed: Option<Result<Vec<harbor_workspace::packages::InstalledPackage>, String>>,
    #[cfg(test)]
    delayed_open: Option<Box<dyn FnOnce(&AtomicBool) -> Started + Send>>,
}

impl Workspace {
    pub(super) fn portable_packages(&self) -> PackageManager {
        PackageManager::new(self.store_path.with_file_name("portable-unix"))
    }

    pub(super) fn portable_environment(&self) -> UnixEnvironment {
        UnixEnvironment::new(self.portable_packages())
    }

    pub(super) fn portable_packages_section(&mut self, ui: &mut Ui) {
        let manager = self.portable_packages();
        self.packages
            .legacy
            .render(ui, self.store_path.with_file_name("legacy-packages"));
        if self.packages.render(ui, &manager) {
            self.open_portable_terminal();
        }
    }

    fn open_portable_terminal(&mut self) {
        self.start(Session {
            protocol: PORTABLE_SESSION.into(),
            name: PORTABLE_SESSION.into(),
            ..Session::default()
        });
    }

    pub(super) fn start_portable_worker(
        &mut self,
        cols: u16,
        rows: u16,
    ) -> Result<PendingLaunch, String> {
        #[cfg(test)]
        if let Some(delayed) = self.packages.delayed_open.take() {
            return PendingLaunch::start_with(delayed);
        }
        PendingLaunch::start(self.portable_environment(), cols, rows)
    }
}

impl State {
    fn render(&mut self, ui: &mut Ui, manager: &PackageManager) -> bool {
        let mut open_terminal = false;
        ui.collapsing("Portable Unix", |ui| {
            let index = PackageIndex::pinned();
            let releases = index.available().rev().collect::<Vec<_>>();
            let Some(recommended) = releases.first() else {
                ui.label("No portable Unix package is available for this platform.");
                return;
            };
            if ui.small_button("Refresh package status").clicked() {
                self.installed = None;
            }
            let current = match self
                .installed
                .get_or_insert_with(|| manager.list().map_err(|error| error.to_string()))
            {
                Ok(items) => items.iter().find(|item| item.id == recommended.id),
                Err(error) => {
                    ui.colored_label(ui.visuals().error_fg_color, error.to_string());
                    return;
                }
            };
            let installed_version = current.map(|item| item.version.clone());
            let installed = installed_version.as_deref();
            ui.label(installed.map_or_else(
                || "BusyBox tools are available to install".into(),
                |version| format!("BusyBox {version} installed"),
            ));
            if cfg!(windows) {
                ui.label("Use paths like C:/Users to access Windows drives.");
            }
            ui.label(format!(
                "Portable home · {}",
                manager.root().join("home").display()
            ));

            if let Some(job) = &self.job {
                self.progress = job.progress().or(self.progress);
                if let Some(result) = job.poll() {
                    self.job = None;
                    self.progress = None;
                    self.installed = None;
                    self.status = match result {
                        Ok(Outcome::Activated { version }) => {
                            format!("BusyBox {version} is ready.")
                        }
                        Ok(Outcome::Removed { deferred, .. }) if deferred => {
                            "Package removed. Close older terminals, then clean up retired files."
                                .into()
                        }
                        Ok(Outcome::Removed { .. }) => "Package removed.".into(),
                        Ok(Outcome::Collected { deferred: false }) => {
                            "Retired package files have been processed.".into()
                        }
                        Ok(Outcome::Collected { deferred: true }) => {
                            "Some retired files remain. Close older terminals and run cleanup again.".into()
                        }
                        Err(error) => error.to_string(),
                    };
                    ui.ctx().request_repaint();
                } else {
                    ui.horizontal_wrapped(|ui| {
                        ui.spinner();
                        ui.label(match self.progress {
                            Some(Progress::Verifying) => "Verifying package…",
                            Some(Progress::Extracting) => "Extracting package…",
                            Some(Progress::Activating) => "Activating release…",
                            Some(Progress::Removing) => "Removing package…",
                            None => "Preparing package operation…",
                        });
                        if ui.button("Cancel").clicked() {
                            job.cancel();
                        }
                    });
                    ui.ctx().request_repaint_after(Duration::from_millis(80));
                    return;
                }
            }

            let version = self
                .version
                .get_or_insert_with(|| recommended.version.clone());
            ComboBox::from_id_salt("portable-package-version")
                .selected_text(version.as_str())
                .show_ui(ui, |ui| {
                    for release in &releases {
                        ui.selectable_value(version, release.version.clone(), &release.version);
                    }
                });
            let Some(release) = index.release(&recommended.id, version) else {
                return;
            };
            ui.label(format!("License · {}", release.license));
            ui.hyperlink_to("Corresponding source", &release.source_url);
            ui.collapsing("Package details", |ui| {
                ui.label(&release.file_name);
                ui.monospace(&release.sha256);
            });
            ui.horizontal_wrapped(|ui| {
                let repair = installed == Some(version);
                let bundled = release.bundled_archive();
                let action = match (repair, installed.is_some(), bundled.is_some()) {
                    (true, _, true) => "Repair",
                    (false, true, true) => "Change version",
                    (false, false, true) => "Install",
                    (true, _, false) => "Choose repair archive…",
                    (false, true, false) => "Choose update archive…",
                    (false, false, false) => "Choose install archive…",
                };
                if ui.button(action).clicked()
                    && let Some(archive) = bundled.or_else(|| {
                        rfd::FileDialog::new()
                            .add_filter("TermCraft Unix package", &["gz"])
                            .pick_file()
                    })
                {
                    self.status.clear();
                    self.archive = Some((release.clone(), archive, repair));
                }
                if ui
                    .add_enabled(installed.is_some(), Button::new("Open Unix terminal"))
                    .clicked()
                {
                    open_terminal = true;
                }
                if ui
                    .add_enabled(installed.is_some(), Button::new("Remove"))
                    .clicked()
                {
                    self.status.clear();
                    self.archive = None;
                    self.job = Some(manager.remove(recommended.id.clone()));
                }
                if ui.button("Clean up retired files").clicked() {
                    self.status.clear();
                    self.archive = None;
                    self.job = Some(manager.cleanup());
                }
            });
            if let Some((release, archive, repair)) = self.archive.as_ref() {
                let dismissed = fluent::surface(ui).show(ui,|ui| {
                ui.strong(format!(
                    "BusyBox {} · {}",
                    release.version,
                    archive.display()
                ));
                ui.monospace(&release.sha256);
                ui
                    .horizontal_wrapped(|ui| {
                        if ui.button("Confirm package operation").clicked() {
                            self.job = Some(if *repair {
                                manager.repair(release.clone(), archive.clone())
                            } else {
                                manager.install(release.clone(), archive.clone())
                            });
                            true
                        } else {
                            ui.button("Cancel selection").clicked()
                        }
                    })
                    .inner
                }).inner;
                if dismissed {
                    self.archive = None;
                }
            }
            if !self.status.is_empty() {
                ui.label(&self.status);
            }
        });
        open_terminal
    }
}

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::thread::{self, JoinHandle};

type Started = Result<(Process, harbor_workspace::environment::Launch), String>;
const MAX_STARTING: usize = 4;
static STARTING: AtomicUsize = AtomicUsize::new(0);

#[derive(Default)]
struct LaunchState {
    cancelled: AtomicBool,
    result: Mutex<Option<Started>>,
}

pub(super) struct PendingLaunch {
    shared: Arc<LaunchState>,
    worker: JoinHandle<()>,
}

impl PendingLaunch {
    pub(super) fn start(
        environment: UnixEnvironment,
        cols: u16,
        rows: u16,
    ) -> Result<Self, String> {
        Self::start_with(move |cancel| {
            let launch = environment.launch_config_cancellable(cancel)?;
            let process = launch.spawn_terminal_cancellable(cols, rows, cancel)?;
            Ok((process, launch))
        })
    }

    fn start_with(
        run: impl FnOnce(&AtomicBool) -> Started + Send + 'static,
    ) -> Result<Self, String> {
        let slot = StartSlot::reserve()?;
        let shared = Arc::new(LaunchState::default());
        let state = shared.clone();
        let worker = thread::Builder::new()
            .name("termcraft-portable-start".into())
            .spawn(move || {
                let _slot = slot;
                if state.cancelled.load(Ordering::Acquire) {
                    return;
                }
                let result = run(&state.cancelled);
                *state
                    .result
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner()) = Some(result);
                loop {
                    let mut result = state
                        .result
                        .lock()
                        .unwrap_or_else(|poison| poison.into_inner());
                    if state.cancelled.load(Ordering::Acquire) {
                        let undelivered = result.take();
                        drop(result);
                        // Process teardown precedes dropping the installed-version lease.
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
            .map_err(|error| format!("Could not start Portable Unix worker: {error}"))?;
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
            .then(|| Err("Portable Unix startup stopped unexpectedly".into()))
    }
}

impl Drop for PendingLaunch {
    fn drop(&mut self) {
        self.shared.cancelled.store(true, Ordering::Release);
        self.worker.thread().unpark();
    }
}

struct StartSlot;

impl StartSlot {
    fn reserve() -> Result<Self, String> {
        if STARTING.fetch_add(1, Ordering::AcqRel) < MAX_STARTING {
            Ok(Self)
        } else {
            STARTING.fetch_sub(1, Ordering::Release);
            Err("Four Portable Unix terminals are already starting. Try again in a moment.".into())
        }
    }
}

impl Drop for StartSlot {
    fn drop(&mut self) {
        STARTING.fetch_sub(1, Ordering::Release);
    }
}

#[cfg(all(test, target_os = "linux", target_arch = "x86_64"))]
mod tests {
    use super::*;

    fn click_text(ctx: &Context, workspace: &mut Workspace, label: &str) {
        fn find(shape: &egui::epaint::Shape, label: &str) -> Option<Pos2> {
            match shape {
                egui::epaint::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.pos + text.galley.rect.center().to_vec2())
                }
                egui::epaint::Shape::Vec(shapes) => {
                    shapes.iter().find_map(|shape| find(shape, label))
                }
                _ => None,
            }
        }
        let size = Rect::from_min_size(Pos2::ZERO, vec2(1200., 800.));
        let position = (0..8)
            .find_map(|_| {
                let mut output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(size),
                        ..Default::default()
                    },
                    |ui| workspace.render(ui),
                );
                output.textures_delta.clear();
                output.shapes.iter().find_map(|entry| {
                    find(&entry.shape, label).filter(|position| entry.clip_rect.contains(*position))
                })
            })
            .unwrap_or_else(|| panic!("Could not find painted control {label:?}"));
        for pressed in [true, false] {
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(size),
                    events: vec![
                        Event::PointerMoved(position),
                        Event::PointerButton {
                            pos: position,
                            button: PointerButton::Primary,
                            pressed,
                            modifiers: Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |ui| workspace.render(ui),
            );
            output.textures_delta.clear();
        }
    }

    #[test]
    #[ignore = "requires pinned BusyBox fixture archives"]
    fn portable_open_button_keeps_render_and_other_input_live_and_cleans_cancelled_child() {
        let ctx = Context::default();
        let root = tempfile::tempdir().unwrap();
        let mut workspace = Workspace::new_context(&ctx, None, root.path().to_path_buf());
        workspace.start_with_launch(
            Session {
                protocol: "Local".into(),
                ..Session::default()
            },
            Some(connection::Launch {
                program: "/bin/sh".into(),
                args: vec![
                    "-c".into(),
                    "read value; printf 'other-pty-%s\\n' \"$value\"".into(),
                ],
                directory: None,
                rdp_profile: None,
            }),
        );

        let manager = workspace.portable_packages();
        let release = PackageIndex::pinned()
            .release("busybox", "1.35.0")
            .unwrap()
            .clone();
        let archive = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/p05-bundles")
            .join(&release.file_name);
        manager.install(release, archive).wait().unwrap();
        let environment = UnixEnvironment::new(manager.clone());
        let (entered, ready) = mpsc::sync_channel(1);
        let (resume, gated) = mpsc::sync_channel(1);
        let (cancel_seen, cancelled) = mpsc::sync_channel(1);
        workspace.packages.delayed_open = Some(Box::new(move |flag| {
            let launch = environment.launch_config_cancellable(flag)?;
            let mut process = launch.spawn_terminal_cancellable(80, 24, flag)?;
            process.write(b"printf '%s' \"$$\" > pid; exec sleep 30\n")?;
            let deadline = Instant::now() + Duration::from_secs(5);
            let marker = launch.directory.join("pid");
            let pid = loop {
                if let Ok(value) = fs::read_to_string(&marker)
                    && let Ok(pid) = value.parse::<u32>()
                {
                    break pid;
                }
                assert!(Instant::now() < deadline, "portable process did not start");
                thread::sleep(Duration::from_millis(5));
            };
            entered.send(pid).unwrap();
            gated.recv_timeout(Duration::from_secs(5)).unwrap();
            cancel_seen.send(flag.load(Ordering::Acquire)).unwrap();
            Ok((process, launch))
        }));
        workspace.page = Page::Tools;
        click_text(&ctx, &mut workspace, "Portable Unix");
        click_text(&ctx, &mut workspace, "Open Unix terminal");
        let portable = workspace.active;
        assert!(workspace.tabs[portable].terminal_starting());
        assert!(!workspace.tabs[portable].ready_for_input());
        let pid = ready.recv_timeout(Duration::from_secs(5)).unwrap();
        workspace.tabs[0].send(b"during-start\n");
        let deadline = Instant::now() + Duration::from_secs(5);
        while !workspace.tabs[0]
            .screen
            .plain()
            .contains("other-pty-during-start")
        {
            let mut output = ctx.run_ui(RawInput::default(), |ui| workspace.render(ui));
            output.textures_delta.clear();
            assert!(!output.shapes.is_empty());
            assert!(Instant::now() < deadline, "other PTY stopped responding");
            thread::sleep(Duration::from_millis(5));
        }
        assert!(workspace.tabs[portable].terminal_starting());
        workspace.force_close_tab(portable);
        manager.remove("busybox").wait().unwrap();
        assert!(
            manager.collect().unwrap(),
            "closing the pending tab retains the lease during teardown"
        );
        resume.send(()).unwrap();
        assert!(cancelled.recv_timeout(Duration::from_secs(5)).unwrap());
        let process = PathBuf::from(format!("/proc/{pid}"));
        let deadline = Instant::now() + Duration::from_secs(5);
        while process.exists() || manager.collect().unwrap() {
            assert!(
                Instant::now() < deadline,
                "cancelled startup kept a child or package lease"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }
}
