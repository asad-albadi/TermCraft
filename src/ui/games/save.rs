//! Explicit, bounded game files. No chooser, codec or filesystem work runs on the GUI.
use cap_fs_ext::{DirExt, FollowSymlinks, MetadataExt, OpenOptionsFollowExt};
use cap_std::fs::{Dir, OpenOptions};
use eframe::egui;
use harbor_workspace::games::{GameSession, persistence::MAX_BYTES};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

#[derive(Default)]
struct Publication {
    // Cancellation and starting publication compete for the same one-shot claim.
    // Once the worker wins it can enter rename/link without holding a UI lock.
    claimed: AtomicBool,
    #[cfg(test)]
    before_commit: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
}

struct Job {
    events: mpsc::Receiver<Event>,
    publication: Arc<Publication>,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.publication.claimed.store(true, Ordering::SeqCst);
    }
}

#[derive(Default)]
pub(super) struct Files {
    pub generation: u64,
    pub revision: u64,
    pub clean: u64,
    pub pending: Option<Action>,
    job: Option<Job>,
    #[cfg(test)]
    before_commit: Option<Arc<dyn Fn() + Send + Sync>>,
    pub prompt: Option<Prompt>,
    #[cfg(test)]
    pub chooser: Option<std::sync::Arc<dyn Fn(bool) -> Option<PathBuf> + Send + Sync>>,
}
pub(super) struct Prompt {
    pub path: PathBuf,
    pub answer: mpsc::SyncSender<bool>,
}
enum Event {
    Question(u64, Prompt),
    Finished(Completion),
}
pub(super) enum Action {
    Start(harbor_workspace::games::GameId, u64),
    New,
    Reset,
    Close,
    Exit,
    Load,
    Adopt(GameSession),
}
pub(super) struct Completion {
    generation: u64,
    revision: u64,
    result: Result<Done, String>,
}
enum Done {
    Saved,
    Loaded(GameSession),
    Canceled,
}
impl Files {
    pub fn dirty(&self) -> bool {
        self.revision != self.clean
    }
    pub fn changed(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }
    pub fn replaced(&mut self) {
        self.invalidate();
        self.revision = 0;
        self.clean = 0;
        self.pending = None;
    }
    pub fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        if let Some(job) = &self.job {
            job.publication.claimed.store(true, Ordering::SeqCst);
        }
        self.prompt = None;
    }
    pub fn confirming(&self) -> bool {
        self.pending.is_some() || self.prompt.is_some()
    }
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }
    pub fn start(&mut self, session: Option<GameSession>, ctx: egui::Context) {
        if self.busy() {
            return;
        }
        let publication = Arc::new(Publication {
            #[cfg(test)]
            before_commit: self.before_commit.clone(),
            ..Publication::default()
        });
        let generation = self.generation;
        let revision = self.revision;
        let (tx, rx) = mpsc::sync_channel(1);
        self.job = Some(Job {
            events: rx,
            publication: publication.clone(),
        });
        // The snapshot is supplied only after an explicit Save click.
        #[cfg(test)]
        let chooser = self.chooser.clone();
        std::thread::spawn(move || {
            let result = if let Some(session) = session {
                (|| {
                    let pick = || {
                        rfd::FileDialog::new()
                            .set_file_name("game.termcraft-game")
                            .add_filter("TermCraft game", &["termcraft-game"])
                            .save_file()
                    };
                    #[cfg(test)]
                    let selected = chooser.as_ref().map_or_else(pick, |choose| choose(true));
                    #[cfg(not(test))]
                    let selected = pick();
                    let Some(path) = selected else {
                        return Ok(Done::Canceled);
                    };
                    if publication.claimed.load(Ordering::SeqCst) {
                        return Ok(Done::Canceled);
                    }
                    let bytes = session.save_bytes().map_err(str::to_owned)?;
                    let target = Target::new(&path)?;
                    // Keep the original handle and bounded content/identity snapshot
                    // alive across the visible consent dialog and publication.
                    let original = target.snapshot()?;
                    if let Some(original) = original.as_ref() {
                        let (answer, decision) = mpsc::sync_channel(1);
                        let question = Prompt { path, answer };
                        if tx.send(Event::Question(generation, question)).is_err() {
                            return Ok(Done::Canceled);
                        }
                        ctx.request_repaint();
                        if decision.recv() != Ok(true) || publication.claimed.load(Ordering::SeqCst)
                        {
                            return Ok(Done::Canceled);
                        }
                        target.validate(original)?;
                    }
                    target.write(&bytes, original.as_ref(), &publication)?;
                    Ok(Done::Saved)
                })()
            } else {
                (|| {
                    let pick = || {
                        rfd::FileDialog::new()
                            .add_filter("TermCraft game", &["termcraft-game"])
                            .pick_file()
                    };
                    #[cfg(test)]
                    let selected = chooser.as_ref().map_or_else(pick, |choose| choose(false));
                    #[cfg(not(test))]
                    let selected = pick();
                    let Some(path) = selected else {
                        return Ok(Done::Canceled);
                    };
                    if publication.claimed.load(Ordering::SeqCst) {
                        return Ok(Done::Canceled);
                    }
                    let bytes = read(&path)?;
                    GameSession::load_bytes(&bytes)
                        .map(Done::Loaded)
                        .map_err(str::to_owned)
                })()
            };
            let _ = tx.send(Event::Finished(Completion {
                generation,
                revision,
                result,
            }));
            ctx.request_repaint();
        });
    }
    pub fn poll(&mut self, message: &mut String) -> Option<GameSession> {
        let result = match self.job.as_ref()?.events.try_recv() {
            Ok(Event::Question(generation, prompt)) => {
                if generation == self.generation && self.pending.is_none() {
                    self.prompt = Some(prompt);
                }
                return None;
            }
            Ok(Event::Finished(result)) => result,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.job = None;
                *message = "Game file worker stopped. Your current game is unchanged.".into();
                return None;
            }
        };
        self.job = None;
        self.complete(result, message)
    }
    fn complete(&mut self, result: Completion, message: &mut String) -> Option<GameSession> {
        if result.generation != self.generation {
            return None;
        }
        match result.result {
            Ok(Done::Saved) => {
                if result.revision == self.revision {
                    self.clean = self.revision;
                    *message = "Game saved (TermCraft format).".into();
                } else {
                    *message =
                        "Earlier game state saved; your newer moves are still unsaved.".into();
                }
            }
            Ok(Done::Loaded(session)) => {
                if result.revision != self.revision && self.dirty() {
                    self.pending = Some(Action::Adopt(session));
                } else {
                    return Some(session);
                }
            }
            Ok(Done::Canceled) => {}
            Err(error) => *message = format!("Game file: {error}"),
        }
        None
    }
}
struct Target {
    dir: Dir,
    name: PathBuf,
}
struct Original {
    // An open handle prevents a removed file's identifier from being reused
    // while the overwrite question is visible.
    file: cap_std::fs::File,
    metadata: cap_std::fs::Metadata,
    contents: Vec<u8>,
}
const TARGET_CHANGED: &str =
    "The saved-game target changed; choose Save again to review the new file.";
fn unchanged(a: &cap_std::fs::Metadata, b: &cap_std::fs::Metadata) -> bool {
    // cap_fs_ext maps dev/ino to device/inode on Unix and volume serial/file
    // index on Windows; metadata must come from an opened file on Windows.
    a.is_file()
        && b.is_file()
        && a.dev() == b.dev()
        && a.ino() == b.ino()
        && a.len() == b.len()
        && a.modified().ok() == b.modified().ok()
}
fn snapshot_contents(file: &mut cap_std::fs::File) -> Result<Vec<u8>, String> {
    if file.metadata().map_err(|e| e.to_string())?.len() > MAX_BYTES as u64 {
        return Err("The existing target exceeds 256 KiB; choose a different save file.".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_BYTES {
        return Err("The existing target exceeds 256 KiB; choose a different save file.".into());
    }
    Ok(bytes)
}

impl Target {
    fn new(path: &Path) -> Result<Self, String> {
        let name = path
            .file_name()
            .ok_or("Choose a regular game file.")?
            .into();
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let absolute = std::path::absolute(parent).map_err(|e| e.to_string())?;
        let root = absolute.ancestors().last().ok_or("Invalid game folder.")?;
        let mut dir =
            Dir::open_ambient_dir(root, cap_std::ambient_authority()).map_err(|e| e.to_string())?;
        for component in absolute
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .components()
        {
            let std::path::Component::Normal(name) = component else {
                return Err("Choose a folder without parent traversal.".into());
            };
            dir = dir
                .open_dir_nofollow(name)
                .map_err(|e| format!("Game folders must be real directories: {e}"))?;
        }
        Ok(Self { dir, name })
    }
    fn exists(&self) -> Result<bool, String> {
        match self.dir.symlink_metadata(&self.name) {
            Ok(m) if m.is_file() && !m.file_type().is_symlink() => Ok(true),
            Ok(_) => Err("Choose a regular file; links and devices are not supported.".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.to_string()),
        }
    }
    fn open_regular(&self) -> Result<cap_std::fs::File, String> {
        if !self.exists()? {
            return Err(TARGET_CHANGED.into());
        }
        let mut options = OpenOptions::new();
        options.read(true).follow(FollowSymlinks::No);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK);
        }
        let file = self
            .dir
            .open_with(&self.name, &options)
            .map_err(|e| format!("{TARGET_CHANGED} {e}"))?;
        if !file.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err(TARGET_CHANGED.into());
        }
        Ok(file)
    }
    fn snapshot(&self) -> Result<Option<Original>, String> {
        if !self.exists()? {
            return Ok(None);
        }
        let mut file = self.open_regular()?;
        let metadata = file.metadata().map_err(|e| e.to_string())?;
        let contents = snapshot_contents(&mut file)?;
        if !unchanged(&metadata, &file.metadata().map_err(|e| e.to_string())?) {
            return Err(TARGET_CHANGED.into());
        }
        let original = Original {
            file,
            metadata,
            contents,
        };
        self.validate(&original)?;
        Ok(Some(original))
    }
    fn validate(&self, original: &Original) -> Result<(), String> {
        let mut current = self.open_regular()?;
        let before = current.metadata().map_err(|e| e.to_string())?;
        if !unchanged(&original.metadata, &before)
            || !unchanged(
                &original.metadata,
                &original.file.metadata().map_err(|e| e.to_string())?,
            )
        {
            return Err(TARGET_CHANGED.into());
        }
        let contents = snapshot_contents(&mut current)?;
        let after = current.metadata().map_err(|e| e.to_string())?;
        if contents != original.contents || !unchanged(&original.metadata, &after) {
            return Err(TARGET_CHANGED.into());
        }
        // Check the directory entry after reading; current may have been renamed
        // away while its contents were read. Opening without following links
        // preserves the regular-file policy for this final identity check.
        let entry = self.open_regular()?.metadata().map_err(|e| e.to_string())?;
        if !unchanged(&original.metadata, &entry) {
            return Err(TARGET_CHANGED.into());
        }
        Ok(())
    }
    fn write(
        &self,
        bytes: &[u8],
        original: Option<&Original>,
        publication: &Publication,
    ) -> Result<(), String> {
        if bytes.len() > MAX_BYTES {
            return Err("Saved game exceeds the size limit.".into());
        }
        // Anchor staging and publication to the same opened directory, even if
        // an ancestor is renamed while the file chooser/overwrite dialog is open.
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).map_err(|e| e.to_string())?;
        let temp = format!(
            ".termcraft-game-{}.tmp",
            random
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        let mut options = OpenOptions::new();
        options
            .write(true)
            .create_new(true)
            .follow(FollowSymlinks::No);
        let mut file = self
            .dir
            .open_with(&temp, &options)
            .map_err(|e| e.to_string())?;
        let result = (|| {
            file.write_all(bytes)?;
            file.sync_all()?;
            drop(file);
            if let Some(original) = original {
                self.validate(original).map_err(std::io::Error::other)?;
                // Portable rename has a residual interval after this final
                // identity/content check; this is not an atomic compare-and-swap.
                // Consent-window changes completed before the check are rejected.
            }
            #[cfg(test)]
            if let Some(hook) = &publication.before_commit {
                hook();
            }
            if publication.claimed.swap(true, Ordering::SeqCst) {
                return Err(std::io::Error::other("Game save canceled."));
            }
            if original.is_some() {
                self.dir.rename(&temp, &self.dir, &self.name)?;
            } else {
                // Atomic no-clobber publication; a racing creator is preserved.
                self.dir.hard_link(&temp, &self.dir, &self.name)?;
            }
            Ok(())
        })();
        let _ = self.dir.remove_file(&temp);
        result.map_err(|e: std::io::Error| e.to_string())
    }
}
fn read(path: &Path) -> Result<Vec<u8>, String> {
    let target = Target::new(path)?;
    if !target.exists()? {
        return Err("Game file was removed.".into());
    }
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = target
        .dir
        .open_with(&target.name, &options)
        .map_err(|e| e.to_string())?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_BYTES as u64 {
        return Err("Expected a regular game file within 256 KiB.".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_BYTES {
        return Err("Game file exceeds 256 KiB.".into());
    }
    Ok(bytes)
}
#[cfg(test)]
mod tests {
    use super::*;
    use harbor_workspace::games::{GameId, GameRegistry};
    // Hold the worker in the native chooser or directly before publication.
    // In both cases the UI must invalidate the job without waiting for it.
    fn invalidated_worker(existing: bool) {
        use std::sync::{Arc, Mutex};
        use std::time::{Duration, Instant};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("game");
        if existing {
            std::fs::write(&path, b"original").unwrap();
        }
        let (reached, at_barrier) = mpsc::sync_channel(1);
        let (release, resume) = mpsc::sync_channel(1);
        let resume = Mutex::new(resume);
        let pause = Arc::new(move || {
            reached.send(()).unwrap();
            resume
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(10))
                .unwrap();
        });
        let mut files = Files::default();
        let selected = path.clone();
        let chooser_pause = pause.clone();
        files.chooser = Some(Arc::new(move |_| {
            if !existing {
                chooser_pause();
            }
            Some(selected.clone())
        }));
        if existing {
            files.before_commit = Some(pause);
        }
        files.changed();
        files.start(
            Some(GameRegistry::start(GameId::Netwalk, 7)),
            egui::Context::default(),
        );
        let mut message = String::new();
        let deadline = Instant::now() + Duration::from_secs(10);
        if existing {
            while files.prompt.is_none() && Instant::now() < deadline {
                files.poll(&mut message);
                std::thread::yield_now();
            }
            files.prompt.take().unwrap().answer.try_send(true).unwrap();
        }
        at_barrier.recv_timeout(Duration::from_secs(10)).unwrap();
        files.invalidate();
        release.send(()).unwrap();
        while files.busy() && Instant::now() < deadline {
            files.poll(&mut message);
            std::thread::yield_now();
        }
        assert!(!files.busy() && files.prompt.is_none());
        assert!(files.dirty() && message.is_empty());
        if existing {
            assert_eq!(std::fs::read(&path).unwrap(), b"original");
        } else {
            assert!(!path.exists(), "invalidated chooser published a file");
        }
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            usize::from(existing)
        );
    }
    #[test]
    fn invalidated_worker_choosing_new_target_preserves_absence() {
        invalidated_worker(false);
    }
    #[test]
    fn invalidated_worker_after_yes_preserves_original() {
        invalidated_worker(true);
    }
    #[test]
    fn save_ui_files_bounded_atomic_no_clobber_and_links() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("game");
        let target = Target::new(&path).unwrap();
        target.write(b"old", None, &Publication::default()).unwrap();
        assert!(target.write(b"new", None, &Publication::default()).is_err());
        assert_eq!(read(&path).unwrap(), b"old");
        let original = target.snapshot().unwrap().unwrap();
        target
            .write(b"new", Some(&original), &Publication::default())
            .unwrap();
        assert_eq!(read(&path).unwrap(), b"new");
        assert!(
            target
                .write(&vec![0; MAX_BYTES + 1], None, &Publication::default())
                .is_err()
        );
        assert_eq!(read(&path).unwrap(), b"new");
        std::fs::write(&path, vec![0; MAX_BYTES + 1]).unwrap();
        assert!(read(&path).is_err());
        #[cfg(unix)]
        {
            let link = root.path().join("link");
            std::os::unix::fs::symlink(&path, &link).unwrap();
            assert!(read(&link).is_err());
            let folder_link = root.path().join("folder-link");
            std::os::unix::fs::symlink(root.path(), &folder_link).unwrap();
            assert!(read(&folder_link.join("game")).is_err());
            assert!(read(root.path()).is_err());
            assert!(
                Target::new(&link)
                    .unwrap()
                    .write(b"bad", None, &Publication::default())
                    .is_err()
            );
        }
    }
    #[test]
    fn save_ui_completions_preserve_newer_revisions_and_generations() {
        let mut files = Files::default();
        let mut message = String::new();
        files.changed();
        files.complete(
            Completion {
                generation: 0,
                revision: 0,
                result: Ok(Done::Saved),
            },
            &mut message,
        );
        assert!(files.dirty());
        assert!(message.contains("newer"));
        let game = GameRegistry::start(GameId::Netwalk, 1);
        assert!(
            files
                .complete(
                    Completion {
                        generation: 0,
                        revision: 0,
                        result: Ok(Done::Loaded(game.clone()))
                    },
                    &mut message
                )
                .is_none()
        );
        assert!(matches!(files.pending, Some(Action::Adopt(_))));
        files.replaced();
        let before = message.clone();
        assert!(
            files
                .complete(
                    Completion {
                        generation: 0,
                        revision: 0,
                        result: Ok(Done::Loaded(game))
                    },
                    &mut message
                )
                .is_none()
        );
        assert_eq!(message, before);
        files.changed();
        files.complete(
            Completion {
                generation: 1,
                revision: 1,
                result: Ok(Done::Saved),
            },
            &mut message,
        );
        assert!(!files.dirty());
    }
    #[test]
    fn fix1_save_ui_overwrite_snapshot_rechecks_before_publication_and_is_bounded() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("game");
        let target = Target::new(&path).unwrap();
        std::fs::write(&path, b"old").unwrap();
        let original = target.snapshot().unwrap().unwrap();
        target.validate(&original).unwrap();
        std::fs::write(&path, b"changed after initial validation").unwrap();
        assert!(
            target
                .write(b"new game", Some(&original), &Publication::default())
                .unwrap_err()
                .contains("target changed")
        );
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"changed after initial validation"
        );
        std::fs::write(&path, vec![0; MAX_BYTES + 1]).unwrap();
        assert!(target.snapshot().err().unwrap().contains("256 KiB"));
    }
}
