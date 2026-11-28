use super::*;
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::ui::terminal_review_tests::owned_workspace;
    #[test]
    fn mixed_group_launch_opens_real_valid_peers_and_reports_bad_program() {
        let (ctx, root, mut w) = owned_workspace();
        let directory = root.path().join("cwd with spaces");
        fs::create_dir(&directory).unwrap();
        for (name, command, folder) in [
            ("first", "/bin/sh", "Team"),
            ("bad", "/definitely/missing/termcraft-program", "Team"),
            ("second", "/bin/sh", "Team/Sub"),
            ("outside", "/bin/sh", "Teamwork"),
        ] {
            w.store.sessions.push(Session {
                name: name.into(),
                protocol: "Local".into(),
                command: command.into(),
                directory: directory.to_str().unwrap().into(),
                folder: folder.into(),
                ..Default::default()
            });
        }
        w.launch_saved_group("Team");
        let report = w.session_groups.report.as_ref().unwrap();
        assert_eq!(report.accepted.len(), 2);
        assert_eq!(report.failed.len(), 1);
        let mut count = 0;
        for tab in &mut w.tabs {
            if tab.session.folder == "Team" || tab.session.folder == "Team/Sub" {
                assert!(tab.process.is_some());
                tab.send_generated_command(b"printf 'GROUP_READY\\n'; pwd\n");
                count += 1;
            }
        }
        assert_eq!(count, 2);
        let end = Instant::now() + Duration::from_secs(3);
        loop {
            for t in &mut w.tabs {
                t.poll();
            }
            if w.tabs
                .iter()
                .filter(|t| t.screen.plain().contains(directory.to_str().unwrap()))
                .count()
                == 2
            {
                break;
            }
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(5));
        }
        let _ = ctx;
    }
}
use harbor_workspace::{
    platform::shortcuts::{ShortcutSpec, ShortcutTarget},
    sessions::groups::{GroupFailure, GroupReport, SessionGroup},
};
#[derive(Default)]
pub(super) struct State {
    pub report: Option<GroupReport>,
    pub(super) folder: String,
    shortcut: Option<Receiver<Result<Option<PathBuf>, String>>>,
}
impl Workspace {
    pub(super) fn launch_saved_session(&mut self, s: Session) -> Result<u64, String> {
        if self.import_applying() {
            return Err("Session import is committing; try again when it finishes.".into());
        }
        if self.tabs.len() >= 64 {
            return Err(
                "64 terminal tabs are already open; close a tab before launching more.".into(),
            );
        }
        self.start(s);
        let index = self.active;
        let id = self.tabs[index].id;
        if !self.tabs[index].error.is_empty() {
            let error = self.tabs[index].error.clone();
            self.force_close_tab(index);
            Err(error)
        } else {
            Ok(id)
        }
    }
    pub(super) fn launch_saved_group(&mut self, folder: &str) {
        let sessions = SessionGroup::resolve(folder, &self.store);
        let mut report = GroupReport::default();
        for session in sessions {
            let id = session.id.clone();
            let name = session.title();
            match self.launch_saved_session(session) {
                Ok(_) => report.accepted.push(id),
                Err(error) => report.failed.push(GroupFailure { id, name, error }),
            }
        }
        if report.accepted.is_empty() && report.failed.is_empty() {
            self.message("No saved sessions found in that folder.");
            return;
        }
        let summary = format!(
            "{} launches submitted; {} failed. Authentication and remote connection results appear in each tab.",
            report.accepted.len(),
            report.failed.len()
        );
        if report.failed.is_empty() {
            self.message(summary);
        } else {
            self.error_in(errors::Domain::Session, summary);
        }
        self.session_groups.folder = folder.into();
        self.session_groups.report = Some(report);
    }
    pub(super) fn create_launch_shortcut(
        &mut self,
        target: ShortcutTarget,
        name: String,
        ctx: &Context,
    ) {
        if self.session_groups.shortcut.is_some() {
            self.message("A shortcut dialog is already open.");
            return;
        }
        let spec = match target {
            ShortcutTarget::Session(id) => ShortcutSpec::for_session(id),
            ShortcutTarget::Group(folder) => ShortcutSpec::for_group(folder),
        }
        .and_then(|s| s.with_workspace(self.store_path.parent().unwrap().to_owned()));
        let spec = match spec {
            Ok(s) => s,
            Err(e) => {
                self.error_in(errors::Domain::Shortcut, e);
                return;
            }
        };
        let (tx, rx) = mpsc::sync_channel(1);
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new()
            .name("session-shortcut".into())
            .spawn(move || {
                let safe_name = name
                    .chars()
                    .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '_' | '-'))
                    .take(80)
                    .collect::<String>();
                let filename = format!(
                    "{}.{}",
                    if safe_name.is_empty() {
                        "TermCraft"
                    } else {
                        &safe_name
                    },
                    ShortcutSpec::extension()
                );
                let result = match rfd::FileDialog::new()
                    .set_title("Save TermCraft launch shortcut")
                    .set_file_name(&filename)
                    .add_filter("Launch shortcut", &[ShortcutSpec::extension()])
                    .save_file()
                {
                    Some(path) => spec.write_new(&path, &name).map(|_| Some(path)),
                    None => Ok(None),
                };
                let _ = tx.send(result);
                ctx.request_repaint();
            });
        match spawned {
            Ok(_) => self.session_groups.shortcut = Some(rx),
            Err(e) => self.error_in(
                errors::Domain::Shortcut,
                format!("Could not start shortcut dialog: {e}"),
            ),
        }
    }
    pub(super) fn session_group_windows(&mut self, ctx: &Context) {
        if let Some(result) =
            self.session_groups
                .shortcut
                .as_ref()
                .and_then(|rx| match rx.try_recv() {
                    Ok(r) => Some(r),
                    Err(mpsc::TryRecvError::Disconnected) => {
                        Some(Err("Shortcut worker stopped unexpectedly.".into()))
                    }
                    Err(_) => None,
                })
        {
            self.session_groups.shortcut = None;
            match result {
                Ok(Some(path)) => {
                    self.message(format!("Launch shortcut saved: {}", path.display()))
                }
                Ok(None) => self.message("Shortcut creation canceled."),
                Err(e) => self.error_in(
                    errors::Domain::Shortcut,
                    format!("Shortcut was not saved: {e}"),
                ),
            }
        }
        if let Some(report) = &self.session_groups.report {
            let mut open = true;
            let size = fluent::dialog_size(ctx, 480., 120.);
            let mut close = false;
            fluent::modeless(ctx, Id::new("group-launch-report"), "Folder launch report", size).open(&mut open).show(ctx, |ui| {
                fluent::dialog_body(ui, size, |ui| {
                    ui.strong(&self.session_groups.folder);
                    ui.label(format!("{} submitted · {} failed", report.accepted.len(), report.failed.len()));
                    ui.label("Submitted sessions connect independently. Check each tab for authentication or later connection failures.");
                    for failure in &report.failed { crate::ui::errors::show(ui,crate::ui::errors::Domain::Session,format!("{}: {}", failure.name, failure.error)); }
                });
                close = ui.button("Close").clicked();
            });
            if close || !open {
                self.session_groups.report = None;
            }
        }
    }
}
