use super::*;
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::ui::terminal_review_tests::owned_workspace;
    fn ready(ctx: &Context, w: &mut Workspace) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while w.session_import.as_ref().is_some_and(|s| s.job.is_some()) {
            w.poll_session_import(ctx);
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn background_import_previews_before_apply_and_persists_only_after_apply() {
        let (ctx, root, mut w) = owned_workspace();
        let path = root.path().join("source.ini");
        fs::write(
            &path,
            include_bytes!("../../tests/fixtures/imports/moba-26.3-derived.ini"),
        )
        .unwrap();
        w.begin_session_import(Format::Moba, path, &ctx);
        ready(&ctx, &mut w);
        assert!(w.store.sessions.is_empty());
        assert_eq!(
            w.session_import
                .as_ref()
                .unwrap()
                .preview
                .as_ref()
                .unwrap()
                .candidates
                .len(),
            1
        );
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200., 850.))),
                ..Default::default()
            },
            |ui| w.session_import_window(ui.ctx()),
        );
        output.textures_delta.clear();
        assert!(w.store.sessions.is_empty());
        w.apply_session_import(&ctx);
        assert!(w.import_applying());
        assert!(
            !w.cancel_session_import(),
            "an explicit commit cannot be cancelled after saving starts"
        );
        let mut raw = RawInput::default();
        raw.viewports
            .get_mut(&ViewportId::ROOT)
            .unwrap()
            .events
            .push(ViewportEvent::Close);
        let mut output = ctx.run_ui(raw, |ui| assert!(w.block_import_close(ui.ctx())));
        output.textures_delta.clear();
        ready(&ctx, &mut w);
        assert_eq!(w.store.sessions.len(), 1);
        let loaded = Store::load(&w.store_path).unwrap();
        assert_eq!(loaded.sessions[0].id, w.store.sessions[0].id);
        assert_eq!(loaded.sessions[0].key, "/keys/with spaces");
    }
    #[test]
    fn cancelled_or_failed_import_cannot_apply_a_late_result() {
        let (ctx, root, mut w) = owned_workspace();
        let source = root.path().join("source.ini");
        fs::write(
            &source,
            include_bytes!("../../tests/fixtures/imports/moba-26.3-derived.ini"),
        )
        .unwrap();
        w.begin_session_import(Format::Moba, source.clone(), &ctx);
        assert!(w.cancel_session_import());
        let deadline = Instant::now() + Duration::from_secs(3);
        while w
            .session_import_busy
            .load(std::sync::atomic::Ordering::Acquire)
        {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        w.poll_session_import(&ctx);
        assert!(w.store.sessions.is_empty());
        w.begin_session_import(Format::Moba, source, &ctx);
        ready(&ctx, &mut w);
        assert!(w.session_import.as_ref().unwrap().preview.is_some());
        w.store_path = root.path().join("directory");
        fs::create_dir(&w.store_path).unwrap();
        w.apply_session_import(&ctx);
        ready(&ctx, &mut w);
        assert!(w.store.sessions.is_empty());
        assert!(!w.session_import.as_ref().unwrap().error.is_empty());
        assert!(w.store_path.is_dir());
    }
}
use harbor_workspace::sessions::{
    self,
    report::{ApplySummary, DuplicatePolicy, ImportReport},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
#[derive(Clone, Copy)]
pub(super) enum Format {
    Moba,
    Putty,
}
impl Format {
    fn label(self) -> &'static str {
        match self {
            Self::Moba => "MobaXterm",
            Self::Putty => "PuTTY",
        }
    }
}
enum Outcome {
    Loaded(ImportReport),
    Applied(Store, ApplySummary),
}
struct Job {
    rx: Receiver<Result<Outcome, String>>,
    cancel: Arc<AtomicBool>,
    applying: bool,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}
struct Busy(Arc<AtomicBool>);
impl Drop for Busy {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
pub(super) struct State {
    job: Option<Job>,
    preview: Option<ImportReport>,
    policy: DuplicatePolicy,
    error: String,
    source: String,
    applied: Option<ApplySummary>,
}
impl State {
    pub(super) fn applying(&self) -> bool {
        self.job.as_ref().is_some_and(|j| j.applying)
    }
}
fn job(
    busy: Arc<AtomicBool>,
    ctx: Context,
    applying: bool,
    work: impl FnOnce() -> Result<Outcome, String> + Send + 'static,
) -> Result<Job, String> {
    if busy
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err(
            "An earlier import operation is still finishing. Retry after it finishes.".into(),
        );
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let cancelled = cancel.clone();
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let guard = Busy(busy);
        let result = if cancelled.load(Ordering::Acquire) {
            Err("Cancelled".into())
        } else {
            work()
        };
        drop(guard);
        if !cancelled.load(Ordering::Acquire) {
            let _ = tx.try_send(result);
            ctx.request_repaint();
        }
    });
    Ok(Job {
        rx,
        cancel,
        applying,
    })
}
impl Workspace {
    pub(super) fn import_applying(&self) -> bool {
        self.session_import.as_ref().is_some_and(State::applying)
    }
    pub(super) fn begin_session_import(&mut self, format: Format, path: PathBuf, ctx: &Context) {
        if self.import_applying() {
            return;
        }
        let source = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "selected file".into());
        let pending = job(
            self.session_import_busy.clone(),
            ctx.clone(),
            false,
            move || {
                use std::io::Read;
                let metadata = fs::metadata(&path)
                    .map_err(|e| format!("Could not inspect import file: {e}"))?;
                if !metadata.is_file() || metadata.len() > sessions::MAX_INPUT as u64 {
                    return Err("Choose a regular import file no larger than 8 MiB".into());
                }
                let file =
                    fs::File::open(path).map_err(|e| format!("Could not open import file: {e}"))?;
                let mut bytes = Vec::new();
                file.take(sessions::MAX_INPUT as u64 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| format!("Could not read import file: {e}"))?;
                let report = match format {
                    Format::Moba => sessions::import_moba(&bytes),
                    Format::Putty => sessions::import_putty(&bytes),
                }?;
                Ok(Outcome::Loaded(report))
            },
        );
        let (mut pending, error) = match pending {
            Ok(job) => (Some(job), String::new()),
            Err(error) => (None, error),
        };
        self.session_import = Some(State {
            job: pending.take(),
            preview: None,
            policy: DuplicatePolicy::Skip,
            error,
            source: format!("{} · {source}", format.label()),
            applied: None,
        });
    }
    pub(super) fn cancel_session_import(&mut self) -> bool {
        if self.import_applying() {
            return false;
        }
        self.session_import = None;
        true
    }
    pub(super) fn poll_session_import(&mut self, _ctx: &Context) {
        let Some(state) = &mut self.session_import else {
            return;
        };
        let Some(job) = &state.job else {
            return;
        };
        let result = match job.rx.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(_) => Err("The import worker ended without a result".into()),
        };
        state.job = None;
        match result {
            Ok(Outcome::Loaded(report)) => state.preview = Some(report),
            Ok(Outcome::Applied(store, summary)) => {
                self.store = store;
                self.shared_sources.dirty = true;
                state.preview = None;
                state.applied = Some(summary);
                state.error.clear();
            }
            Err(error) => state.error = error,
        }
    }
    pub(super) fn apply_session_import(&mut self, ctx: &Context) {
        if !self.store_ok {
            if let Some(state) = &mut self.session_import {
                state.error =
                    "Workspace settings are damaged; export/recover them before applying imports"
                        .into();
            }
            return;
        }
        let Some(state) = &mut self.session_import else {
            return;
        };
        if state.job.is_some() {
            return;
        }
        let Some(report) = state.preview.clone() else {
            return;
        };
        let policy = state.policy;
        let mut staged = self.store.clone();
        let path = self.store_path.clone();
        match job(
            self.session_import_busy.clone(),
            ctx.clone(),
            true,
            move || {
                let summary = sessions::report::apply(&mut staged, report, policy)?;
                staged.save(&path)?;
                Ok(Outcome::Applied(staged, summary))
            },
        ) {
            Ok(job) => {
                state.job = Some(job);
                state.error.clear();
            }
            Err(error) => state.error = error,
        }
    }
    pub(super) fn block_import_close(&mut self, ctx: &Context) -> bool {
        if self.defer_vault_close(ctx) {
            return true;
        }
        if self.import_applying() && ctx.input(|i| i.viewport().close_requested()) {
            self.allow_remote_exit = false;
            self.allow_session_exit = false;
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.message("The session import is being saved. Close the app after it finishes.");
            true
        } else {
            false
        }
    }
    pub(super) fn choose_session_import(&mut self, format: Format, ctx: &Context) {
        let dialog = rfd::FileDialog::new();
        let dialog = match format {
            Format::Moba => dialog.add_filter(
                "MobaXterm bookmarks or documented CSV",
                &["ini", "mxtsessions", "moba", "csv"],
            ),
            Format::Putty => dialog.add_filter("PuTTY Registry export", &["reg"]),
        };
        if let Some(path) = dialog.pick_file() {
            self.begin_session_import(format, path, ctx);
        }
    }
    pub(super) fn session_import_window(&mut self, ctx: &Context) {
        let Some(mut state) = self.session_import.take() else {
            return;
        };
        let mut apply = false;
        let mut close = false;
        let applying = state.applying();
        let size = fluent::dialog_size(ctx, 680., 120.);
        let response = fluent::modal(
            ctx,
            Id::new("session-import-preview"),
            "Import saved sessions",
            size,
            |ui| {
                fluent::dialog_body(ui, size, |ui| {
                    ui.label(&state.source);
                    if state.job.is_some() {
                        ui.spinner();
                        ui.label(if applying {
                            "Saving the reviewed sessions…"
                        } else {
                            "Reading and checking the source…"
                        });
                    }
                    if !state.error.is_empty() {
                        ui.colored_label(ui.visuals().error_fg_color, &state.error);
                    }
                    if let Some(summary) = &state.applied {
                        ui.label(format!(
                            "Imported {} · replaced {} · skipped {}",
                            summary.added, summary.replaced, summary.skipped
                        ));
                    }
                    if let Some(report) = &state.preview {
                        ui.label(format!(
                            "{} · {} · {} candidate sessions",
                            report.format,
                            report.encoding,
                            report.candidates.len()
                        ));
                        ui.small("Preview only until Apply. Credentials and unsupported options are omitted; no connection is started.");
                        ScrollArea::vertical()
                            .id_salt("import-candidates")
                            .max_height(260.)
                            .show_rows(ui, 54., report.candidates.len(), |ui, range| {
                                for i in range {
                                    let entry = &report.candidates[i];
                                    let s = &entry.session;
                                    ui.label(
                                        RichText::new(format!(
                                            "{} · {}:{} · {}",
                                            s.name, s.host, s.port, s.folder
                                        ))
                                        .strong(),
                                    )
                                    .fluent_hint(format!("Source line {}", entry.location.line));
                                    ui.add(
                                        Label::new(format!(
                                            "User: {} · key: {}",
                                            s.user,
                                            if s.key.is_empty() { "none" } else { &s.key }
                                        ))
                                        .truncate(),
                                    );
                                    ui.add_space(8.);
                                }
                            });
                        if !report.unsupported.is_empty() {
                            ui.collapsing(
                                format!(
                                    "{} unsupported/excluded fields (values omitted)",
                                    report.unsupported.len()
                                ),
                                |ui| {
                                    ScrollArea::vertical()
                                        .id_salt("import-warnings")
                                        .max_height(180.)
                                        .show_rows(
                                            ui,
                                            40.,
                                            report.unsupported.len(),
                                            |ui, range| {
                                                for i in range {
                                                    let f = &report.unsupported[i];
                                                    ui.label(format!(
                                                        "Line {} · {}",
                                                        f.location.line, f.field
                                                    ));
                                                    ui.add(Label::new(&f.message).truncate());
                                                }
                                            },
                                        );
                                },
                            );
                        }
                        if report.omitted_warnings > 0 {
                            ui.label(format!(
                                "{} more warning details omitted at the 1,000-detail display limit",
                                report.omitted_warnings
                            ));
                        }
                        ui.add_enabled_ui(!applying,|ui|{
                    ui.horizontal(|ui|{ui.label("Same name and folder:");ui.selectable_value(&mut state.policy,DuplicatePolicy::Skip,"Skip");ui.selectable_value(&mut state.policy,DuplicatePolicy::KeepBoth,"Keep both");ui.selectable_value(&mut state.policy,DuplicatePolicy::Replace,"Replace");});
                    ui.small("Replace keeps the existing saved-session ID. Keep both creates a new identity and name.");

                });
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    if let Some(report) = &state.preview {
                        apply = ui
                            .add_enabled(
                                !applying && self.store_ok && !report.candidates.is_empty(),
                                Button::new("Apply reviewed import"),
                            )
                            .clicked();
                    }
                    close = ui
                        .add_enabled(
                            !applying,
                            Button::new(if state.applied.is_some() {
                                "Close"
                            } else {
                                "Cancel import"
                            }),
                        )
                        .clicked();
                });
            },
        );
        if response.should_close() && !applying {
            close = true;
        }
        self.session_import = Some(state);
        if close {
            self.cancel_session_import();
        } else if apply {
            self.apply_session_import(ctx);
        }
    }
}
