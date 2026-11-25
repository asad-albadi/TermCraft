use super::*;
use harbor_workspace::services::{
    ServiceId, ServiceState, ServiceStatus, ServiceSupervisor,
    cron::{self, CronJob, CronServiceConfig, CronTimezone},
};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
};
#[derive(Clone, Default)]
struct Snapshot {
    jobs: Vec<CronJob>,
    timezone: CronTimezone,
    status: Option<ServiceStatus>,
    error: Option<String>,
    revision: u64,
    saved_identity: Option<String>,
}
enum Action {
    Save(Vec<CronJob>, CronTimezone, Option<String>),
    Start,
    Stop,
    Reload,
}
struct Worker {
    send: mpsc::SyncSender<Action>,
    snapshot: Arc<Mutex<Snapshot>>,
}
impl Worker {
    fn new(path: PathBuf) -> Self {
        let (send, receive) = mpsc::sync_channel(4);
        let snapshot = Arc::new(Mutex::new(Snapshot::default()));
        let shared = snapshot.clone();
        std::thread::spawn(move || {
            let supervisor = ServiceSupervisor::default();
            let mut id: Option<ServiceId> = None;
            let mut state = Snapshot::default();
            let reload = |state: &mut Snapshot| {
                if path.exists() {
                    match cron::load_jobs(&path)
                        .and_then(|jobs| Ok((jobs, cron::load_timezone(&path)?)))
                    {
                        Ok((jobs, zone)) => {
                            state.jobs = jobs;
                            state.timezone = zone;
                            state.error = None;
                        }
                        Err(e) => state.error = Some(e),
                    }
                }
                state.saved_identity = None;
                state.revision += 1;
            };
            reload(&mut state);
            loop {
                match receive.recv_timeout(Duration::from_millis(100)) {
                    Ok(Action::Save(jobs, zone, identity)) => {
                        let busy = id.and_then(|i| supervisor.status(i)).is_some_and(|s| {
                            matches!(
                                s.state,
                                ServiceState::Starting
                                    | ServiceState::Running
                                    | ServiceState::Stopping
                            )
                        });
                        if busy {
                            state.error = Some("Stop the scheduler before editing jobs.".into());
                        } else {
                            match cron::save_jobs_with_timezone(&path, jobs, zone) {
                                Ok(()) => {
                                    reload(&mut state);
                                    state.saved_identity = identity;
                                }
                                Err(e) => state.error = Some(e),
                            }
                        }
                    }
                    Ok(Action::Reload) => reload(&mut state),
                    Ok(Action::Start) => {
                        let busy = id.and_then(|i| supervisor.status(i)).is_some_and(|s| {
                            matches!(
                                s.state,
                                ServiceState::Starting
                                    | ServiceState::Running
                                    | ServiceState::Stopping
                            )
                        });
                        if !busy {
                            match supervisor.begin_cron(CronServiceConfig::new(path.clone())) {
                                Ok(i) => {
                                    id = Some(i);
                                    state.error = None;
                                }
                                Err(e) => state.error = Some(e),
                            }
                        }
                    }
                    Ok(Action::Stop) => {
                        if let Some(i) = id {
                            let _ = supervisor.stop(i);
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
                state.status = id.and_then(|i| supervisor.status(i));
                *shared.lock().unwrap() = state.clone();
            }
            // Dropping the supervisor cancels and joins only its owned workers.
        });
        Self { send, snapshot }
    }
}
#[derive(Default)]
pub(super) struct ManagedCron {
    worker: Option<Worker>,
    revision: u64,
    jobs: Vec<CronJob>,
    timezone: CronTimezone,
    selected: Option<usize>,
    name: String,
    expression: String,
    program: String,
    arguments: String,
    directory: String,
    seconds: u32,
    enabled: bool,
    error: Option<String>,
}
impl ManagedCron {
    fn send(&mut self, action: Action) {
        if self.worker.as_ref().unwrap().send.try_send(action).is_err() {
            self.error = Some("Scheduler is busy; try again shortly.".into());
        }
    }
    fn reset_editor(&mut self) {
        self.selected = None;
        self.name.clear();
        self.expression = "* * * * *".into();
        self.program.clear();
        self.arguments.clear();
        self.directory.clear();
        self.seconds = 300;
        self.enabled = true;
    }
    fn select(&mut self, index: usize) {
        let j = &self.jobs[index];
        self.selected = Some(index);
        self.name = j.id.clone();
        self.expression = j.schedule.clone();
        self.program = j.program.to_string_lossy().into();
        self.arguments = j.arguments.join("\n");
        self.directory = j.directory.to_string_lossy().into();
        self.seconds = j.max_seconds;
        self.enabled = j.enabled;
    }
    pub(super) fn card(&mut self, ui: &mut Ui, data: &Path) {
        if self.worker.is_none() {
            self.worker = Some(Worker::new(data.join("services/cron-jobs.json")));
            self.expression = "* * * * *".into();
            self.seconds = 300;
            self.enabled = true;
        }
        let snapshot = self
            .worker
            .as_ref()
            .unwrap()
            .snapshot
            .try_lock()
            .ok()
            .map(|s| s.clone())
            .unwrap_or_default();
        if snapshot.revision > self.revision {
            let identity = snapshot.saved_identity.clone().or_else(|| {
                self.selected
                    .and_then(|i| self.jobs.get(i))
                    .map(|j| j.id.clone())
            });
            self.revision = snapshot.revision;
            self.jobs = snapshot.jobs.clone();
            self.timezone = snapshot.timezone;
            if let Some(index) =
                identity.and_then(|id| self.jobs.iter().position(|job| job.id == id))
            {
                self.select(index);
            } else {
                self.reset_editor();
            }
        }
        let busy = snapshot.status.as_ref().is_some_and(|s| {
            matches!(
                s.state,
                ServiceState::Starting | ServiceState::Running | ServiceState::Stopping
            )
        });
        ui.ctx().request_repaint_after(Duration::from_millis(250));
        surface(ui).show(ui, |ui| {
            ui.label(RichText::new("Cron scheduled commands").size(18.).strong());
            ui.label("Runs only while this scheduler and workspace are open. Jobs run with your account’s full permissions. Stop cancels active jobs; closing the app stops scheduling.");
            if let Some(status) = &snapshot.status {
                ui.label(format!("Scheduler: {:?}", status.state));
                if let Some(error) = &status.error {
                    ui.colored_label(Color32::from_rgb(210, 65, 65), error);
                }
            }
            ui.horizontal(|ui| {
                if busy {
                    if ui.button("Stop scheduler / cancel jobs").clicked() {
                        self.send(Action::Stop);
                    }
                } else {
                    if ui.button("Start scheduler").clicked() {
                        self.send(Action::Start);
                    }
                    if ui.button("Reload saved jobs").clicked() {
                        self.send(Action::Reload);
                    }
                }
            });
            ui.add_enabled_ui(!busy, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Timezone");
                    ui.selectable_value(&mut self.timezone, CronTimezone::Utc, "UTC");
                    ui.selectable_value(&mut self.timezone, CronTimezone::SystemLocal, "System local");
                });
                ui.small("Five fields: minute hour day month weekday. Supports *, lists, ranges, steps, JAN–DEC and SUN–SAT. Sunday = 0 or 7; day and weekday use OR when neither starts with *. No @aliases, seconds, ?, L, W, #, or inline environment settings.");
                ui.small("Missed minutes are skipped; restart checks the current minute once. A recorded attempt is never replayed after clock rollback. Local DST gaps are skipped; repeated local minutes can run twice at their distinct UTC instants. No overlap per job; at most 4 concurrent commands / 32 jobs.");
                ui.horizontal_wrapped(|ui| {
                    for i in 0..self.jobs.len() {
                        if ui.selectable_label(self.selected == Some(i), &self.jobs[i].id).clicked() {
                            self.select(i);
                        }
                    }
                    if ui.button("New job").clicked() {
                        self.reset_editor();
                    }
                });
                Grid::new("cron-job-form").num_columns(2).show(ui, |ui| {
                    ui.label("Job name");
                    ui.text_edit_singleline(&mut self.name);
                    ui.end_row();
                    ui.label("Schedule");
                    ui.text_edit_singleline(&mut self.expression);
                    ui.end_row();
                    ui.label("Executable (absolute path)");
                    ui.text_edit_singleline(&mut self.program);
                    ui.end_row();
                    ui.label("Arguments (one per line)");
                    ui.add(TextEdit::multiline(&mut self.arguments).desired_rows(3));
                    ui.end_row();
                    ui.label("Working folder (absolute path)");
                    ui.text_edit_singleline(&mut self.directory);
                    ui.end_row();
                    ui.label("Maximum runtime");
                    ui.add(DragValue::new(&mut self.seconds).range(1..=86400).suffix(" seconds"));
                    ui.end_row();
                    ui.label("Enabled");
                    ui.checkbox(&mut self.enabled, "Allow scheduled runs");
                    ui.end_row();
                });
                ui.small("Commands and arguments are saved locally. Keep passwords and tokens out of arguments; use your command’s secure credential store. Output is discarded. Scripts require an explicitly chosen interpreter and script argument.");
                ui.horizontal(|ui| {
                    if ui.button("Save job and timezone").clicked() {
                        let job = CronJob {
                            id: self.name.clone(), schedule: self.expression.clone(), enabled: self.enabled,
                            program: self.program.clone().into(),
                            arguments: if self.arguments.is_empty() { vec![] } else { self.arguments.lines().map(String::from).collect() },
                            directory: self.directory.clone().into(), max_seconds: self.seconds,
                        };
                        let identity=job.id.clone();
                        let mut jobs = self.jobs.clone();
                        if let Some(i) = self.selected.filter(|i| *i < jobs.len()) { jobs[i] = job; } else { jobs.push(job); }
                        self.send(Action::Save(jobs, self.timezone, Some(identity)));
                    }
                    if self.selected.is_some() && ui.button("Remove selected job").clicked() {
                        let mut jobs = self.jobs.clone();
                        jobs.remove(self.selected.unwrap());
                        self.send(Action::Save(jobs, self.timezone, None));
                    }
                });
            });
            if let Some(status) = snapshot.status.as_ref().and_then(|s| s.cron.as_ref()) {
                for job in &status.jobs {
                    let next = job.next_run.and_then(|n| chrono::DateTime::from_timestamp(n as i64, 0))
                        .map(|d| d.to_rfc3339()).unwrap_or_else(|| "none within 8 years / disabled".into());
                    let result = job.last_result.as_ref().map(|s| format!(" ({s})")).unwrap_or_default();
                    ui.label(format!("{} — {}{} — next: {}", job.id, job.last_event, result, next));
                }
            }
            for error in [&self.error, &snapshot.error].into_iter().flatten() {
                ui.colored_label(Color32::from_rgb(210, 65, 65), error);
            }
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn text_position(shape: &egui::Shape, text: &str) -> Option<Pos2> {
        match shape {
            egui::Shape::Text(t) if t.galley.text() == text => Some(t.pos + t.galley.size() * 0.5),
            egui::Shape::Vec(v) => v.iter().find_map(|s| text_position(s, text)),
            _ => None,
        }
    }
    fn frame(
        ctx: &Context,
        form: &mut ManagedCron,
        data: &Path,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200., 1600.))),
                events,
                ..Default::default()
            },
            |ui| {
                form.card(ui, data);
            },
        );
        output.textures_delta.clear();
        output
    }
    fn click(ctx: &Context, form: &mut ManagedCron, data: &Path, label: &str) {
        let out = frame(ctx, form, data, vec![]);
        let pos = out
            .shapes
            .iter()
            .find_map(|s| text_position(&s.shape, label))
            .expect(label);
        for pressed in [true, false] {
            frame(
                ctx,
                form,
                data,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
    }
    #[cfg(unix)]
    #[test]
    fn pointer_save_start_due_stop_and_disabled_persistence() {
        let data = tempfile::tempdir().unwrap();
        let ctx = Context::default();
        let mut form = ManagedCron::default();
        frame(&ctx, &mut form, data.path(), vec![]);
        let end = Instant::now() + Duration::from_secs(10);
        while form.revision == 0 {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(20));
            frame(&ctx, &mut form, data.path(), vec![]);
        }
        form.name = "pointer job".into();
        form.program = "/bin/sh".into();
        form.arguments = "-c\nprintf pointer > marker; sleep 30".into();
        form.directory = data.path().to_string_lossy().into();
        click(&ctx, &mut form, data.path(), "Save job and timezone");
        while form.jobs.is_empty() {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(20));
            frame(&ctx, &mut form, data.path(), vec![]);
        }
        // Consecutive edits must update the saved job without reselecting its name.
        for expression in ["*/2 * * * *", "* * * * *"] {
            let revision = form.revision;
            form.expression = expression.into();
            click(&ctx, &mut form, data.path(), "Save job and timezone");
            loop {
                std::thread::sleep(Duration::from_millis(20));
                frame(&ctx, &mut form, data.path(), vec![]);
                let snapshot = form
                    .worker
                    .as_ref()
                    .unwrap()
                    .snapshot
                    .lock()
                    .unwrap()
                    .clone();
                assert!(snapshot.error.is_none(), "{:?}", snapshot.error);
                if form.revision > revision {
                    break;
                }
                assert!(Instant::now() < end);
            }
            let jobs = cron::load_jobs(&data.path().join("services/cron-jobs.json")).unwrap();
            assert_eq!(jobs.len(), 1);
            assert_eq!(jobs[0].schedule, expression);
        }
        click(&ctx, &mut form, data.path(), "Start scheduler");
        while !data.path().join("marker").exists() {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(20));
            frame(&ctx, &mut form, data.path(), vec![]);
        }
        assert_eq!(
            std::fs::read(data.path().join("marker")).unwrap(),
            b"pointer"
        );
        loop {
            let output = frame(&ctx, &mut form, data.path(), vec![]);
            if output
                .shapes
                .iter()
                .any(|s| text_position(&s.shape, "Scheduler: Running").is_some())
            {
                break;
            }
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(20));
        }
        click(&ctx, &mut form, data.path(), "Stop scheduler / cancel jobs");
        loop {
            std::thread::sleep(Duration::from_millis(20));
            frame(&ctx, &mut form, data.path(), vec![]);
            if form
                .worker
                .as_ref()
                .unwrap()
                .snapshot
                .lock()
                .unwrap()
                .status
                .as_ref()
                .unwrap()
                .state
                == ServiceState::Stopped
            {
                break;
            }
            assert!(Instant::now() < end);
        }
        form.select(0);
        click(&ctx, &mut form, data.path(), "Allow scheduled runs");
        assert!(!form.enabled);
        click(&ctx, &mut form, data.path(), "Save job and timezone");
        while cron::load_jobs(&data.path().join("services/cron-jobs.json")).unwrap()[0].enabled {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            cron::load_jobs(&data.path().join("services/cron-jobs.json")).unwrap()[0].enabled
                == false
        );
        let mut restarted = ManagedCron::default();
        loop {
            frame(&ctx, &mut restarted, data.path(), vec![]);
            if restarted.revision > 0 {
                break;
            }
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(restarted.jobs.len(), 1);
        assert_eq!(restarted.jobs[0].schedule, "* * * * *");
        assert!(!restarted.jobs[0].enabled);
    }
}
