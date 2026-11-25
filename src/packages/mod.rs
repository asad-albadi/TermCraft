//! Private, versioned packages with transactional activation and shared launch leases.
mod index;
pub(crate) mod install;
pub mod legacy;

pub use index::{PackageIndex, PackageRelease};

use cap_fs_ext::DirExt;
use cap_std::{ambient_authority, fs::Dir};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
};

const METADATA_LIMIT: u64 = 1024 * 1024;
const RETIRED_LIMIT: usize = 128;

pub type Result<T> = std::result::Result<T, PackageError>;

#[derive(Debug)]
pub enum PackageError {
    Cancelled,
    Busy,
    Invalid(String),
    Io(io::Error),
}

impl std::fmt::Display for PackageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => f.write_str("Package operation cancelled"),
            Self::Busy => {
                f.write_str("Another package operation is committing changes. Try again.")
            }
            Self::Invalid(message) => f.write_str(message),
            Self::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for PackageError {}

impl From<io::Error> for PackageError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

pub(crate) fn cancelled(flag: &AtomicBool) -> Result<()> {
    if flag.load(Ordering::Acquire) {
        Err(PackageError::Cancelled)
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Progress {
    Verifying,
    Extracting,
    Activating,
    Removing,
}

#[derive(Debug)]
pub enum Outcome {
    Activated { version: String },
    Removed { version: String, deferred: bool },
    Collected { deferred: bool },
}

pub struct PackageJob {
    cancel: Arc<AtomicBool>,
    progress: Receiver<Progress>,
    result: Receiver<Result<Outcome>>,
}

impl PackageJob {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }

    pub fn progress(&self) -> Option<Progress> {
        self.progress.try_iter().last()
    }

    pub fn poll(&self) -> Option<Result<Outcome>> {
        match self.result.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(worker_ended())),
        }
    }

    pub fn wait(self) -> Result<Outcome> {
        self.result.recv().unwrap_or_else(|_| Err(worker_ended()))
    }
}

impl Drop for PackageJob {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn worker_ended() -> PackageError {
    PackageError::Invalid("Package operation ended unexpectedly".into())
}

#[derive(Clone, Debug)]
pub struct PackageManager {
    root: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InstalledPackage {
    pub id: String,
    pub version: String,
    directory: String,
}

#[derive(Default, Deserialize, Serialize)]
struct Registry {
    active: BTreeMap<String, InstalledPackage>,
    retired: Vec<InstalledPackage>,
}

#[derive(Clone, Debug)]
pub struct OwnedCommand {
    pub path: PathBuf,
    pub version: String,
    // Keep the shared lock for the entire process/tree lifetime.
    _lease: Arc<File>,
}

impl PackageManager {
    pub fn new(root: PathBuf) -> Self {
        let root = std::env::current_dir().map_or(root.clone(), |cwd| cwd.join(root));
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn list(&self) -> Result<Vec<InstalledPackage>> {
        Ok(self.read()?.active.into_values().collect())
    }

    pub fn install(&self, release: PackageRelease, archive: PathBuf) -> PackageJob {
        self.spawn(move |manager, cancel, progress| {
            manager.install_inner(release, &archive, cancel, progress, false)
        })
    }

    pub fn repair(&self, release: PackageRelease, archive: PathBuf) -> PackageJob {
        self.spawn(move |manager, cancel, progress| {
            manager.install_inner(release, &archive, cancel, progress, true)
        })
    }

    pub fn remove(&self, package: impl Into<String>) -> PackageJob {
        let package = package.into();
        self.spawn(move |manager, cancel, progress| {
            let _mutation = manager.lock_named("mutation.lock")?;
            let _ = progress.try_send(Progress::Removing);
            let lock = manager.lock()?;
            let mut registry = manager.read()?;
            let record = registry
                .active
                .remove(&package)
                .ok_or_else(|| PackageError::Invalid("Package is already absent".into()))?;
            cancelled(cancel)?;
            registry.retired.push(record.clone());
            manager.write(&registry)?;
            drop(lock);
            drop(_mutation);
            let deferred = manager.collect_cancellable(cancel).unwrap_or(true);
            Ok(Outcome::Removed {
                version: record.version,
                deferred,
            })
        })
    }

    pub fn cleanup(&self) -> PackageJob {
        self.spawn(|manager, cancel, progress| {
            cancelled(cancel)?;
            let _ = progress.try_send(Progress::Removing);
            manager
                .collect_cancellable(cancel)
                .map(|deferred| Outcome::Collected { deferred })
        })
    }

    fn spawn(
        &self,
        operation: impl FnOnce(Self, &AtomicBool, &SyncSender<Progress>) -> Result<Outcome>
        + Send
        + 'static,
    ) -> PackageJob {
        let (progress_tx, progress) = mpsc::sync_channel(4);
        let (result_tx, result) = mpsc::sync_channel(1);
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_flag = cancel.clone();
        let manager = self.clone();
        let _ = std::thread::Builder::new()
            .name("unix-package".into())
            .spawn(move || {
                let result = cancelled(&worker_flag)
                    .and_then(|()| operation(manager, &worker_flag, &progress_tx));
                let _ = result_tx.send(result);
            });
        PackageJob {
            cancel,
            progress,
            result,
        }
    }

    fn install_inner(
        &self,
        release: PackageRelease,
        archive: &Path,
        cancel: &AtomicBool,
        progress: &SyncSender<Progress>,
        repair: bool,
    ) -> Result<Outcome> {
        if !PackageIndex::pinned().authenticate(&release) {
            return Err(PackageError::Invalid(
                "This package or platform is absent from the pinned index".into(),
            ));
        }
        let _mutation = self.lock_named("mutation.lock")?;
        let versions = self.versions_path()?;
        let stage = tempfile::Builder::new()
            .prefix("pkg-")
            .tempdir_in(&versions)?;
        install::stage(&release, archive, stage.path(), cancel, progress)?;

        let _ = progress.try_send(Progress::Activating);
        let lock = self.lock()?;
        let mut registry = self.read()?;
        let active_version = registry
            .active
            .get(&release.id)
            .map(|record| &record.version);
        if !repair && active_version == Some(&release.version) {
            return Err(PackageError::Invalid(
                "This release is already installed. Choose Repair to reinstall it.".into(),
            ));
        }
        if repair && active_version != Some(&release.version) {
            return Err(PackageError::Invalid(
                "Repair requires the installed release".into(),
            ));
        }
        if registry.retired.len() >= RETIRED_LIMIT {
            return Err(PackageError::Invalid("Too many retained package releases. Close old terminals and clean up retired versions.".into()));
        }
        let record = InstalledPackage {
            id: release.id.clone(),
            version: release.version.clone(),
            directory: stage.path().file_name().unwrap().to_str().unwrap().into(),
        };
        let lease_name = lease_name(&record)?;
        if let Some(previous) = registry.active.insert(release.id, record) {
            registry.retired.push(previous);
        }
        cancelled(cancel)?;
        let mut options = cap_std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        self.open_root()?
            .open_with(&lease_name, &options)?
            .into_std()
            .sync_all()?;
        // The fully staged tree already has its final unique name. The only
        // activation point is replacement of the registry in the same directory.
        self.write(&registry)?;
        let _ = stage.keep();
        drop(lock);
        drop(_mutation);
        let _ = self.collect_cancellable(cancel);
        Ok(Outcome::Activated {
            version: release.version,
        })
    }

    pub fn command(&self, package: &str, name: &str) -> Result<OwnedCommand> {
        self.command_cancellable(package, name, &AtomicBool::new(false))
    }

    pub(crate) fn command_cancellable(
        &self,
        package: &str,
        name: &str,
        cancel: &AtomicBool,
    ) -> Result<OwnedCommand> {
        cancelled(cancel)?;
        let registry_lock = self.lock()?;
        let registry = self.read()?;
        let record = registry.active.get(package).ok_or_else(|| {
            PackageError::Invalid("Install the Portable Unix tools from Tools first".into())
        })?;
        let release = PackageIndex::pinned()
            .release(package, &record.version)
            .ok_or_else(|| {
                PackageError::Invalid(
                    "Installed package release is absent from this TermCraft index".into(),
                )
            })?;
        let file_name = release.commands.get(name).ok_or_else(|| {
            PackageError::Invalid(format!("{name} is absent from the installed package"))
        })?;
        let directory = open_version(&self.versions_dir()?, record)?;
        let lease = open_file(&self.open_root()?, &lease_name(record)?, true)?;
        lease.try_lock_shared().map_err(lock_error)?;
        drop(registry_lock);
        let bin = directory.open_dir_nofollow("bin")?;
        // Verify the entire command set before exposing this bin directory
        // to the child through PATH. User-edited files require repair.
        for name in release.commands.values() {
            let entry = open_regular(&bin, name)?;
            let digest = digest_entry_cancellable(entry, cancel)?;
            if digest != release.binary_sha256 {
                return Err(PackageError::Invalid(
                    "Installed tools were changed. Repair the package before opening a terminal."
                        .into(),
                ));
            }
        }
        Ok(OwnedCommand {
            path: self
                .root
                .join("versions")
                .join(&record.directory)
                .join("bin")
                .join(file_name),
            version: record.version.clone(),
            _lease: Arc::new(lease),
        })
    }

    /// Try to remove unchanged owned files from retired releases. A live
    /// process or an open Windows file is retried on the next cleanup.
    pub fn collect(&self) -> Result<bool> {
        self.collect_cancellable(&AtomicBool::new(false))
    }

    fn collect_cancellable(&self, cancel: &AtomicBool) -> Result<bool> {
        self.collect_with(cancel, |_, _| {})
    }

    fn collect_with(&self, cancel: &AtomicBool, before_move: impl Fn(&Path, &str)) -> Result<bool> {
        cancelled(cancel)?;
        // A cleanup can read and hash files without excluding registry readers
        // or new package installs. This lock only excludes other cleanups.
        let _cleanup = self.lock_named("cleanup.lock")?;
        let retired = {
            let _registry = self.lock()?;
            self.read()?.retired
        };
        if retired.is_empty() {
            return Ok(false);
        }
        let versions = self.versions_dir()?;
        let root = self.open_root()?;
        let mut completed = Vec::new();
        // Visit the whole bounded registry snapshot: a fixed prefix of live
        // leases must not starve later releases whose processes have exited.
        // Keep cancellation between records and inside file verification.
        for record in &retired {
            cancelled(cancel)?;
            match cleanup(&root, &versions, &self.root, record, cancel, &before_move) {
                Ok(true) => completed.push(record.directory.clone()),
                Err(PackageError::Cancelled) => return Err(PackageError::Cancelled),
                Ok(false) | Err(_) => {}
            }
        }
        let _registry = self.lock()?;
        let mut registry = self.read()?;
        if !completed.is_empty() {
            registry
                .retired
                .retain(|record| !completed.contains(&record.directory));
            self.write(&registry)?;
        }
        Ok(!registry.retired.is_empty())
    }

    pub(crate) fn open_root(&self) -> io::Result<Dir> {
        let parent = self
            .root
            .parent()
            .ok_or_else(|| io::Error::other("Missing package root parent"))?;
        let name = self
            .root
            .file_name()
            .ok_or_else(|| io::Error::other("Missing package root name"))?;
        Dir::open_ambient_dir(parent, ambient_authority())?.open_dir_nofollow(name)
    }

    fn read(&self) -> Result<Registry> {
        let root = match self.open_root() {
            Ok(root) => root,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Registry::default()),
            Err(error) => return Err(error.into()),
        };
        match open_regular(&root, "registry.json") {
            Ok(file) => read_json(file),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Registry::default()),
            Err(error) => Err(error.into()),
        }
    }

    fn write(&self, registry: &Registry) -> Result<()> {
        let mut next = tempfile::NamedTempFile::new_in(&self.root)?;
        serde_json::to_writer(&mut next, registry).map_err(io::Error::other)?;
        next.as_file().sync_all()?;
        next.persist(self.root.join("registry.json"))
            .map_err(|error| error.error)?;
        Ok(())
    }

    fn lock(&self) -> Result<File> {
        self.lock_named("registry.lock")
    }

    pub(crate) fn lock_named(&self, name: &str) -> Result<File> {
        let root = self.create_root()?;
        let mut options = cap_std::fs::OpenOptions::new();
        options.read(true).write(true).create_new(true);
        let lock = match root.open_with(name, &options) {
            Ok(file) => file.into_std(),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                open_file(&root, name, true)?
            }
            Err(error) => return Err(error.into()),
        };
        lock.try_lock().map_err(lock_error)?;
        Ok(lock)
    }

    fn create_root(&self) -> Result<Dir> {
        let parent = self
            .root
            .parent()
            .ok_or_else(|| io::Error::other("Missing package root parent"))?;
        fs::create_dir_all(parent)?;
        #[cfg(unix)]
        let builder = {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = fs::DirBuilder::new();
            builder.mode(0o700);
            builder
        };
        #[cfg(not(unix))]
        let builder = fs::DirBuilder::new();
        match builder.create(&self.root) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
        Ok(self.open_root()?)
    }

    pub(crate) fn versions_dir(&self) -> Result<Dir> {
        let root = self.open_root()?;
        Ok(root.open_dir_nofollow("versions")?)
    }

    pub(crate) fn versions_path(&self) -> Result<PathBuf> {
        let root = self.create_root()?;
        match root.create_dir("versions") {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
        let _versions = root.open_dir_nofollow("versions")?;
        Ok(self.root.join("versions"))
    }
}

pub(crate) fn open_regular(dir: &Dir, name: impl AsRef<Path>) -> io::Result<File> {
    open_file(dir, name, false)
}

pub(crate) fn open_file(dir: &Dir, name: impl AsRef<Path>, write: bool) -> io::Result<File> {
    use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt};
    let mut options = cap_std::fs::OpenOptions::new();
    options.read(true).write(write).follow(FollowSymlinks::No);
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = dir.open_with(name, &options)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("Expected a regular package file"));
    }
    Ok(file.into_std())
}

pub(crate) fn lock_error(error: std::fs::TryLockError) -> PackageError {
    match error {
        std::fs::TryLockError::WouldBlock => PackageError::Busy,
        std::fs::TryLockError::Error(error) => error.into(),
    }
}

pub(crate) fn read_json<T: serde::de::DeserializeOwned>(file: File) -> Result<T> {
    if file.metadata()?.len() > METADATA_LIMIT {
        return Err(PackageError::Invalid(
            "Package metadata exceeds the size limit".into(),
        ));
    }
    serde_json::from_reader(file.take(METADATA_LIMIT + 1))
        .map_err(|error| io::Error::other(error).into())
}

fn lease_name(record: &InstalledPackage) -> io::Result<String> {
    let name = &record.directory;
    if !name.starts_with("pkg-")
        || name.len() != 10
        || !name[4..].bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        return Err(io::Error::other("Invalid installed package directory"));
    }
    Ok(format!("{name}.lock"))
}

fn open_version(versions: &Dir, record: &InstalledPackage) -> io::Result<Dir> {
    lease_name(record)?;
    versions.open_dir_nofollow(&record.directory)
}

fn digest_entry_cancellable(file: File, cancel: &AtomicBool) -> Result<String> {
    let (digest, size) = install::copy_and_hash(
        &mut file.take(install::MAX_OWNED_FILE + 1),
        &mut io::sink(),
        cancel,
    )?;
    if size > install::MAX_OWNED_FILE {
        return Err(PackageError::Invalid(
            "Installed file exceeds its size limit".into(),
        ));
    }
    Ok(digest)
}

fn cleanup(
    root: &Dir,
    versions: &Dir,
    root_path: &Path,
    record: &InstalledPackage,
    cancel: &AtomicBool,
    before_move: &impl Fn(&Path, &str),
) -> Result<bool> {
    let directory = match open_version(versions, record) {
        Ok(directory) => directory,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(error.into()),
    };
    let lease = open_file(root, &lease_name(record)?, true)?;
    if let Err(error) = lease.try_lock() {
        return match error {
            std::fs::TryLockError::WouldBlock => Ok(false),
            std::fs::TryLockError::Error(error) => Err(error.into()),
        };
    }
    let Some(release) = PackageIndex::pinned().release(&record.id, &record.version) else {
        // An older TermCraft may not have this release's ownership metadata.
        return Ok(false);
    };
    let version_path = root_path.join("versions").join(&record.directory);
    for (folder, names, digest) in [
        (
            "bin",
            release
                .commands
                .values()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            &release.binary_sha256,
        ),
        (
            "notices",
            vec![install::LICENSE.split_once('/').unwrap().1],
            &release.license_sha256,
        ),
    ] {
        cancelled(cancel)?;
        let Ok(parent) = directory.open_dir_nofollow(folder) else {
            continue;
        };
        for name in names {
            collect_file(
                &directory,
                &parent,
                &version_path,
                name,
                digest,
                cancel,
                before_move,
            )?;
        }
        drop(parent);
        let _ = directory.remove_dir(folder);
    }
    drop(directory);
    let _ = versions.remove_dir(&record.directory);
    Ok(true)
}

pub(crate) fn collect_file(
    version: &Dir,
    directory: &Dir,
    version_path: &Path,
    name: &str,
    pinned_digest: &str,
    cancel: &AtomicBool,
    before_move: &impl Fn(&Path, &str),
) -> Result<()> {
    cancelled(cancel)?;
    let Ok(original) = open_regular(directory, name) else {
        return Ok(());
    };
    if !matches_digest(original.try_clone()?, pinned_digest, cancel)? {
        return Ok(());
    }
    before_move(version_path, name);
    cancelled(cancel)?;

    // Move the current occupant, then verify the moved file. An editor can
    // replace an entry at its original pathname while this is in progress.
    // The unlink only ever targets the isolated, rechecked moved entry.
    let (private_name, quarantine) = open_quarantine(version)?;
    if let Err(error) = directory.rename(name, &quarantine, name) {
        let _ = version.remove_dir(&private_name);
        return Err(error.into());
    }
    let moved = open_regular(&quarantine, name).ok();
    let checked = match &moved {
        Some(file) if same_file(&original, file).unwrap_or(false) => {
            matches_digest(file.try_clone()?, pinned_digest, cancel)
        }
        _ => Ok(false),
    };
    // Some platforms won't delete an open file. Once isolated, names and
    // their contents are independent of the original directory entry.
    let can_restore = moved.is_some();
    drop(moved);
    drop(original);
    if matches!(checked, Ok(true)) && !cancel.load(Ordering::Acquire) {
        quarantine.remove_file(name)?;
    } else if can_restore {
        // Keep the moved occupant recoverable in the private directory even
        // if another editor fills or replaces its original name again.
        let _ = quarantine.hard_link(name, directory, name);
    }
    drop(quarantine);
    let _ = version.remove_dir(&private_name);
    checked.map(|_| ())
}

fn open_quarantine(version: &Dir) -> Result<(String, Dir)> {
    let mut nonce = [0_u8; 16];
    getrandom::fill(&mut nonce).map_err(|error| io::Error::other(error.to_string()))?;
    let hex: String = nonce.iter().map(|byte| format!("{byte:02x}")).collect();
    let name = format!(".retained-{hex}");
    #[cfg(unix)]
    let builder = {
        use cap_std::fs::DirBuilderExt;
        let mut builder = cap_std::fs::DirBuilder::new();
        builder.mode(0o700);
        builder
    };
    #[cfg(not(unix))]
    let builder = cap_std::fs::DirBuilder::new();
    version.create_dir_with(&name, &builder)?;
    let quarantine = version.open_dir_nofollow(&name)?;
    Ok((name, quarantine))
}

fn matches_digest(file: File, expected: &str, cancel: &AtomicBool) -> Result<bool> {
    match digest_entry_cancellable(file, cancel) {
        Ok(digest) => Ok(digest == expected),
        Err(PackageError::Cancelled) => Err(PackageError::Cancelled),
        Err(_) => Ok(false),
    }
}

#[cfg(unix)]
fn same_file(original: &File, moved: &File) -> io::Result<bool> {
    use std::os::unix::fs::MetadataExt;
    let original = original.metadata()?;
    let moved = moved.metadata()?;
    Ok(original.dev() == moved.dev() && original.ino() == moved.ino())
}

#[cfg(windows)]
fn same_file(original: &File, moved: &File) -> io::Result<bool> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };

    fn identity(file: &File) -> io::Result<(u32, u32, u32)> {
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok((
            info.dwVolumeSerialNumber,
            info.nFileIndexHigh,
            info.nFileIndexLow,
        ))
    }

    Ok(identity(original)? == identity(moved)?)
}

#[cfg(all(test, target_os = "linux", target_arch = "x86_64"))]
mod retirement_tests {
    use super::*;

    fn install_fixture(manager: &PackageManager, version: &str) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/p05-bundles");
        let release = PackageIndex::pinned()
            .release("busybox", version)
            .unwrap()
            .clone();
        let archive = root.join(&release.file_name);
        manager.install(release, archive).wait().unwrap();
    }

    #[test]
    #[ignore = "requires pinned BusyBox fixture archives"]
    fn closed_ninth_release_is_collected_behind_eight_live_leases() {
        let root = tempfile::tempdir().unwrap();
        let manager = PackageManager::new(root.path().join("portable"));
        let release = PackageIndex::pinned()
            .release("busybox", "1.35.0")
            .unwrap()
            .clone();
        let archive = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/p05-bundles")
            .join(&release.file_name);
        install_fixture(&manager, "1.35.0");
        let mut live = Vec::new();
        for _ in 0..8 {
            live.push(manager.command("busybox", "ash").unwrap());
            manager
                .repair(release.clone(), archive.clone())
                .wait()
                .unwrap();
        }
        let ninth = manager.command("busybox", "ash").unwrap();
        let ninth_path = ninth.path.clone();
        // Retire the ninth version while its lease still lives, then close it.
        manager.remove("busybox").wait().unwrap();
        drop(ninth);
        assert!(manager.collect().unwrap(), "eight versions remain in use");
        assert!(!ninth_path.exists(), "closed ninth release was starved");
        assert_eq!(manager.read().unwrap().retired.len(), 8);
        for command in &live {
            assert!(command.path.is_file(), "a live leased release was removed");
        }
        drop(live);
        assert!(!manager.collect().unwrap());
        assert!(manager.read().unwrap().retired.is_empty());
    }

    #[test]
    #[ignore = "requires pinned BusyBox fixture archives"]
    fn concurrent_editor_save_is_preserved_by_retirement_quarantine() {
        let root = tempfile::tempdir().unwrap();
        let manager = PackageManager::new(root.path().join("portable"));
        install_fixture(&manager, "1.35.0");
        let leased = manager.command("busybox", "cat").unwrap();
        let candidate = leased.path.clone();
        manager.remove("busybox").wait().unwrap();
        drop(leased);
        let replacement = root.path().join("editor.tmp");
        fs::write(&replacement, b"original user bytes").unwrap();
        let fifo = root.path().join("fifo.tmp");
        use std::os::unix::{ffi::OsStrExt, fs::FileTypeExt};
        let fifo_path = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo_path.as_ptr(), 0o600) }, 0);
        let grep = candidate.with_file_name("grep");
        let cancel = AtomicBool::new(false);
        assert!(
            !manager
                .collect_with(&cancel, |_, name| {
                    if name == "cat" {
                        fs::rename(&replacement, &candidate).unwrap();
                    } else if name == "grep" {
                        fs::rename(&fifo, &grep).unwrap();
                    }
                })
                .unwrap()
        );
        assert_eq!(fs::read(&candidate).unwrap(), b"original user bytes");
        let retained = candidate
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .read_dir()
            .unwrap()
            .map(|result| result.unwrap())
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".retained-")
            })
            .collect::<Vec<_>>();
        assert!(retained.iter().any(|entry| {
            fs::read(entry.path().join("cat")).is_ok_and(|bytes| bytes == b"original user bytes")
        }));
        assert!(retained.iter().any(|entry| {
            fs::symlink_metadata(entry.path().join("grep"))
                .is_ok_and(|meta| meta.file_type().is_fifo())
        }));
    }

    #[test]
    #[ignore = "requires pinned BusyBox fixture archives"]
    fn paused_collection_leaves_launch_and_install_available_and_can_cancel() {
        let root = tempfile::tempdir().unwrap();
        let manager = PackageManager::new(root.path().join("portable"));
        install_fixture(&manager, "1.31.0");
        let old = manager.command("busybox", "ash").unwrap();
        let path = old.path.clone();
        install_fixture(&manager, "1.35.0");
        drop(old);
        let (entered, waiting) = mpsc::sync_channel(1);
        let (resume, gated) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let background_cancel = cancelled.clone();
        let background_manager = manager.clone();
        let worker = std::thread::spawn(move || {
            background_manager.collect_with(&background_cancel, |_, name| {
                if name == "ash" {
                    entered.send(()).unwrap();
                    gated
                        .recv_timeout(std::time::Duration::from_secs(20))
                        .unwrap();
                }
            })
        });
        waiting
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        let active = manager.command("busybox", "ash").unwrap();
        assert_eq!(active.version, "1.35.0");
        install_fixture(&manager, "1.31.0");
        cancelled.store(true, Ordering::Release);
        resume.send(()).unwrap();
        assert!(matches!(
            worker.join().unwrap(),
            Err(PackageError::Cancelled)
        ));
        assert!(path.is_file());
        assert_eq!(manager.command("busybox", "ash").unwrap().version, "1.31.0");
        drop(active);
        assert!(!manager.collect().unwrap());
        assert!(!path.exists());
    }
}
