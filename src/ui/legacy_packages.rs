use eframe::egui::{self, Ui};
use harbor_workspace::packages::legacy::ReviewedArchive;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::JoinHandle,
};

#[derive(Default)]
pub(super) struct State {
    review: Option<ReviewedArchive>,
    job: Option<Job>,
    status: String,
}

struct Job {
    receiver: mpsc::Receiver<Result<Reply, String>>,
    cancel: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
enum Reply {
    Reviewed(ReviewedArchive),
    Imported(PathBuf),
}

impl State {
    fn apply(&mut self, reply: Result<Reply, String>) {
        match reply {
            Ok(Reply::Reviewed(review)) => {
                self.review = Some(review);
                self.status = "Archive inspected. Review its files before importing.".into();
            }
            Ok(Reply::Imported(path)) => {
                self.review = None;
                self.status = format!("Saved in {}. Commands remain disabled.", path.display());
            }
            Err(error) => {
                self.status = error;
            }
        }
    }

    fn start(
        &mut self,
        ui: &Ui,
        operation: impl FnOnce(&AtomicBool) -> Result<Reply, String> + Send + 'static,
    ) {
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let context = ui.ctx().clone();
        let (sender, receiver) = mpsc::sync_channel(1);
        match std::thread::Builder::new()
            .name("legacy-package-review".into())
            .spawn(move || {
                let result = operation(&flag);
                let _ = sender.send(result);
                context.request_repaint();
            }) {
            Ok(worker) => {
                self.job = Some(Job {
                    receiver,
                    cancel,
                    worker: Some(worker),
                });
                self.status.clear();
            }
            Err(error) => {
                self.status = format!("Could not start package inspection: {error}");
            }
        }
    }

    pub(super) fn render(&mut self, ui: &mut Ui, root: PathBuf) {
        if let Some(job) = &self.job {
            let reply = match job.receiver.try_recv() {
                Ok(reply) => Some(reply),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err("Package inspection ended unexpectedly.".into()))
                }
            };
            if let Some(reply) = reply {
                self.job = None;
                self.apply(reply);
            }
        }
        ui.collapsing("Original package archives", |ui| {
            ui.label("Inspect a local MobaXterm archive and save its files separately. Imported commands stay disabled.");
            ui.label("These files need a supported Windows/Cygwin runtime and their dependencies before they can run.");
            if let Some(job) = &self.job {
                ui.horizontal_wrapped(|ui| {
                    ui.spinner();
                    ui.label("Processing package archive…");
                    if ui.button("Cancel").clicked() { job.cancel.store(true, Ordering::Release); }
                });
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
            } else {
                if ui.button("Inspect package archive…").clicked() {
                    if let Some(path) = rfd::FileDialog::new().add_filter("Package archives", &["mxt3", "plugin", "plugin64", "zip"]).pick_file() {
                        self.review = None;
                        self.start(ui, move |cancel| ReviewedArchive::inspect(&path, cancel).map(Reply::Reviewed).map_err(|e| e.to_string()));
                    }
                }
                if let Some(review) = &self.review {
                    let mut import = false;
                    let mut cancel = false;
                    crate::ui::fluent::surface(ui).show(ui, |ui| {
                    let metadata = review.metadata();
                    ui.strong(&metadata.source_name);
                    ui.label(format!("{} files · {:.1} MB unpacked", metadata.files.len(), metadata.files.iter().map(|file| file.decoded_bytes).sum::<u64>() as f64 / (1024.0 * 1024.0)));
                    ui.collapsing("Archive fingerprint", |ui| { ui.monospace(&metadata.source_sha256); });
                    egui::ScrollArea::vertical().id_salt("legacy-package-file-preview").max_height(190.0).show(ui, |ui| {
                        for file in &metadata.files {
                            ui.label(format!("{} · {}{} · {} bytes", file.path, file.format, file.architecture.as_ref().map(|a| format!(" ({a})")).unwrap_or_default(), file.decoded_bytes));
                        }
                    });
                    ui.label("Importing saves a local file inventory and fingerprints. It does not verify the publisher or install missing dependencies.");
                    ui.horizontal_wrapped(|ui| {
                        import = ui.button("Import files with commands disabled").clicked();
                        cancel = ui.button("Discard preview").clicked();
                    });
                    });
                    if import {
                        let review = self.review.take().unwrap();
                        self.start(ui, move |cancel| review.import(&root, cancel).map(Reply::Imported).map_err(|e| e.to_string()));
                    } else if cancel { self.review = None; }
                }
            }
            if !self.status.is_empty() { ui.label(&self.status); }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_package_import_result_keeps_commands_disabled_in_user_copy() {
        let mut state = State::default();
        state.apply(Ok(Reply::Imported(PathBuf::from("private-package-folder"))));
        assert!(state.status.contains("Commands remain disabled"));
        assert!(state.status.contains("private-package-folder"));
        assert!(state.review.is_none());
    }
    #[test]
    fn legacy_package_worker_failure_is_visible() {
        let mut state = State::default();
        state.apply(Err("Package archive CRC check failed".into()));
        assert_eq!(state.status, "Package archive CRC check failed");
    }
    #[test]
    fn legacy_package_worker_reviews_and_imports_real_zip_off_the_caller_thread() {
        use std::io::Write;
        let source = tempfile::NamedTempFile::new().unwrap();
        let mut zip = zip::ZipWriter::new(source.reopen().unwrap());
        zip.start_file(
            "usr/share/doc/README",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(b"local package documentation").unwrap();
        zip.finish().unwrap();
        let context = egui::Context::default();
        let mut state = State::default();
        let caller = std::thread::current().id();
        let path = source.path().to_owned();
        let mut operation = Some(move |cancel: &AtomicBool| {
            assert_ne!(caller, std::thread::current().id());
            ReviewedArchive::inspect(&path, cancel)
                .map(Reply::Reviewed)
                .map_err(|e| e.to_string())
        });
        let mut frame = context.run_ui(egui::RawInput::default(), |ui| {
            if let Some(operation) = operation.take() {
                state.start(ui, operation);
            }
        });
        frame.textures_delta.clear();
        let reply = state
            .job
            .as_ref()
            .unwrap()
            .receiver
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        state.job = None;
        state.apply(reply);
        assert_eq!(state.review.as_ref().unwrap().metadata().files.len(), 1);
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("legacy");
        let review = state.review.take().unwrap();
        let mut operation = Some(move |cancel: &AtomicBool| {
            review
                .import(&root, cancel)
                .map(Reply::Imported)
                .map_err(|e| e.to_string())
        });
        let mut frame = context.run_ui(egui::RawInput::default(), |ui| {
            if let Some(operation) = operation.take() {
                state.start(ui, operation);
            }
        });
        frame.textures_delta.clear();
        let reply = state
            .job
            .as_ref()
            .unwrap()
            .receiver
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        state.job = None;
        let path = match &reply {
            Ok(Reply::Imported(path)) => path.clone(),
            _ => panic!("Expected imported files"),
        };
        state.apply(reply);
        assert_eq!(
            std::fs::read(path.join("payload/usr/share/doc/README")).unwrap(),
            b"local package documentation"
        );
        assert!(state.status.contains("Commands remain disabled"));
    }
}
