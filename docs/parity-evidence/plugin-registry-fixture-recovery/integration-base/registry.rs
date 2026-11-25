use super::{PluginManifest, Result, ReviewedPlugin, host, install::LICENSE, invalid};
use crate::packages::{
    PackageManager, cancelled, collect_file,
    install::{MAX_OWNED_FILE, copy_and_hash},
    lock_error, open_file, open_regular, read_json,
};
use cap_fs_ext::DirExt;
use cap_std::fs::Dir;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

const MAX_ACTIVE: usize = 64;
const MAX_RETIRED: usize = 128;

#[derive(Clone, Debug)]
pub struct PluginRegistry {
    store: PackageManager,
}

#[derive(Clone, Debug)]
pub struct InstalledPlugin {
    pub manifest: PluginManifest,
    pub release_digest: String,
    pub enabled: bool,
}

#[derive(Debug)]
pub struct Removal {
    pub deferred: bool,
    pub preserved_at: Option<PathBuf>,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Registry {
    active: BTreeMap<String, Record>,
    retired: Vec<Record>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Record {
    manifest: PluginManifest,
    release_digest: String,
    directory: String,
    grant: Option<String>,
    license_hash: String,
}

struct HeldLock(File);

impl Drop for HeldLock {
    fn drop(&mut self) {
        // Explicit unlock also releases the lock if an unrelated Unix fork
        // has briefly inherited this open file description before exec.
        let _ = self.0.unlock();
    }
}

impl Record {
    fn enabled(&self) -> bool {
        self.grant.as_ref() == Some(&self.release_digest)
    }

    fn installed(&self) -> InstalledPlugin {
        InstalledPlugin {
            manifest: self.manifest.clone(),
            release_digest: self.release_digest.clone(),
            enabled: self.enabled(),
        }
    }

    fn open(&self, versions: &Dir) -> io::Result<Dir> {
        let name = &self.directory;
        if !name.starts_with("plugin-")
            || name.len() != 13
            || !name[7..].bytes().all(|byte| byte.is_ascii_alphanumeric())
        {
            return Err(io::Error::other("Invalid installed plugin directory"));
        }
        versions.open_dir_nofollow(name)
    }
}

impl PluginRegistry {
    pub fn new(root: PathBuf) -> Self {
        Self {
            store: PackageManager::new(root),
        }
    }

    pub fn list(&self) -> Result<Vec<InstalledPlugin>> {
        Ok(self
            .read()?
            .active
            .values()
            .map(Record::installed)
            .collect())
    }

    pub fn inspect(&self, manifest: &Path, cancel: &AtomicBool) -> Result<ReviewedPlugin> {
        let reviewed = ReviewedPlugin::read(manifest, cancel)?;
        let stage = tempfile::tempdir()?;
        reviewed.stage(stage.path(), cancel)?;
        Ok(reviewed)
    }

    pub fn install(&self, review: &ReviewedPlugin, cancel: &AtomicBool) -> Result<InstalledPlugin> {
        cancelled(cancel)?;
        let _mutating = self.lock("mutation.lock")?;
        cancelled(cancel)?;
        let _ = self.collect_cancellable(cancel);
        let manifest = review.manifest();
        let versions = self.store.versions_path()?;
        let stage = tempfile::Builder::new()
            .prefix("plugin-")
            .tempdir_in(versions)?;
        let files = review.stage(stage.path(), cancel)?;
        File::create_new(stage.path().join("lease.lock"))?.sync_all()?;

        let commit = self.lock("registry.lock")?;
        let mut registry = self.read()?;
        if let Some(previous) = registry.active.get(&manifest.id) {
            if manifest.release_version() <= previous.manifest.release_version() {
                return Err(invalid("Plugin update must have a newer version"));
            }
        } else if registry.active.len() >= MAX_ACTIVE {
            return Err(invalid("The installed plugin limit has been reached"));
        }
        if registry.retired.len() >= MAX_RETIRED {
            return Err(invalid(
                "Too many retired plugin versions. Wait for active plugin processes to stop.",
            ));
        }
        let record = Record {
            manifest: manifest.clone(),
            release_digest: review.release_digest().into(),
            directory: stage
                .path()
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| invalid("Invalid plugin staging directory"))?
                .into(),
            grant: None,
            license_hash: files
                .get(LICENSE)
                .ok_or_else(|| invalid("Missing plugin license"))?
                .clone(),
        };
        let installed = record.installed();
        if let Some(previous) = registry.active.insert(manifest.id.clone(), record) {
            registry.retired.push(previous);
        }
        cancelled(cancel)?;
        self.write(&registry)?;
        let _ = stage.keep();
        drop(commit);
        let _ = self.collect_cancellable(cancel);
        Ok(installed)
    }

    pub fn enable(&self, id: &str, approved_release: &str) -> Result<()> {
        let _commit = self.lock("registry.lock")?;
        let mut registry = self.read()?;
        let record = registry
            .active
            .get_mut(id)
            .ok_or_else(|| invalid("Plugin is not installed"))?;
        if record.release_digest != approved_release {
            return Err(invalid(
                "Review and grant access to the currently installed plugin version",
            ));
        }
        record.grant = Some(approved_release.into());
        self.write(&registry)
    }

    pub fn disable(&self, id: &str) -> Result<()> {
        let _commit = self.lock("registry.lock")?;
        let mut registry = self.read()?;
        let record = registry
            .active
            .get_mut(id)
            .ok_or_else(|| invalid("Plugin is not installed"))?;
        record.grant = None;
        self.write(&registry)
    }

    pub fn remove(&self, id: &str, cancel: &AtomicBool) -> Result<Removal> {
        cancelled(cancel)?;
        let _mutating = self.lock("mutation.lock")?;
        cancelled(cancel)?;
        let _ = self.collect_cancellable(cancel);
        let commit = self.lock("registry.lock")?;
        let mut registry = self.read()?;
        if registry.retired.len() >= MAX_RETIRED {
            return Err(invalid(
                "Too many retired plugin versions. Wait for active plugin processes to stop.",
            ));
        }
        let record = registry
            .active
            .remove(id)
            .ok_or_else(|| invalid("Plugin is not installed"))?;
        cancelled(cancel)?;
        registry.retired.push(record.clone());
        self.write(&registry)?;
        drop(commit);
        // Activation and removal have already committed. Cleanup can be retried.
        Ok(self.collect_cancellable(cancel).map_or(
            Removal {
                deferred: true,
                preserved_at: None,
            },
            |retired| Removal {
                deferred: !retired.contains_key(&record.directory),
                preserved_at: retired
                    .get(&record.directory)
                    .copied()
                    .unwrap_or(false)
                    .then(|| self.store.root().join("versions").join(&record.directory)),
            },
        ))
    }

    pub fn run(&self, id: &str, entrypoint: &str, cancel: &AtomicBool) -> Result<()> {
        self.run_inner(id, None, entrypoint, cancel)
    }

    pub fn run_release(
        &self,
        id: &str,
        release_digest: &str,
        entrypoint: &str,
        cancel: &AtomicBool,
    ) -> Result<()> {
        self.run_inner(id, Some(release_digest), entrypoint, cancel)
    }

    fn run_inner(
        &self,
        id: &str,
        expected_release: Option<&str>,
        entrypoint: &str,
        cancel: &AtomicBool,
    ) -> Result<()> {
        self.run_inner_with(id, expected_release, entrypoint, cancel, |_| {})
    }

    fn run_inner_with(
        &self,
        id: &str,
        expected_release: Option<&str>,
        entrypoint: &str,
        cancel: &AtomicBool,
        after_verification: impl FnOnce(&Path),
    ) -> Result<()> {
        cancelled(cancel)?;
        let _ = self.collect_cancellable(cancel);
        cancelled(cancel)?;
        let commit = self.lock("registry.lock")?;
        let registry = self.read()?;
        let record = registry
            .active
            .get(id)
            .ok_or_else(|| invalid("Plugin is not installed"))?;
        record.manifest.validate()?;
        if expected_release.is_some_and(|digest| digest != record.release_digest) {
            return Err(invalid(
                "The selected plugin release has changed. Refresh the plugin list.",
            ));
        }
        if !record.enabled() {
            return Err(invalid(
                "Plugin is disabled. Native execution access must be granted before running it.",
            ));
        }
        if !record
            .manifest
            .entrypoints
            .iter()
            .any(|name| name == entrypoint)
        {
            return Err(invalid("Plugin entrypoint is not declared"));
        }
        let dir = record.open(&self.store.versions_dir()?)?;
        let lease = open_file(&dir, "lease.lock", true)?;
        // Exclusivity also bounds concurrent invocations of this release.
        lease.try_lock().map_err(lock_error)?;
        let lease = HeldLock(lease);
        let mut executable =
            open_executable(&dir.open_dir_nofollow("bin")?, &record.manifest.executable)?;
        #[cfg(target_os = "linux")]
        let mut frozen = host::verification_target()?;
        #[cfg(target_os = "linux")]
        let mut destination = &mut frozen;
        #[cfg(not(target_os = "linux"))]
        let mut destination = io::sink();
        let (digest, bytes) = copy_and_hash(
            &mut (&mut executable).take(MAX_OWNED_FILE + 1),
            &mut destination,
            cancel,
        )?;
        if bytes > MAX_OWNED_FILE || digest != record.manifest.binary_sha256 {
            return Err(invalid("The installed plugin executable was modified"));
        }
        #[cfg(target_os = "linux")]
        {
            host::finish_verification(&frozen)?;
            executable = frozen;
        }
        cancelled(cancel)?;
        let directory = self.store.root().join("versions").join(&record.directory);
        after_verification(&directory);
        let invocation = host::Invocation::start(
            &directory,
            &record.manifest.executable,
            &executable,
            entrypoint,
        )?;
        // A disable or removal can commit now, but retirement waits for the lease.
        drop(executable);
        drop(commit);
        let result = invocation.finish(cancel);
        drop(lease);
        let _ = self.collect_cancellable(cancel);
        result
    }

    fn lock(&self, name: &str) -> Result<HeldLock> {
        self.store.lock_named(name).map(HeldLock)
    }

    fn read(&self) -> Result<Registry> {
        let root = match self.store.open_root() {
            Ok(root) => root,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Registry::default()),
            Err(error) => return Err(error.into()),
        };
        let registry: Registry = match open_regular(&root, "registry.json") {
            Ok(file) => read_json(file)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Registry::default()),
            Err(error) => return Err(error.into()),
        };
        if registry.active.len() > MAX_ACTIVE
            || registry.retired.len() > MAX_RETIRED
            || registry
                .active
                .iter()
                .any(|(id, record)| id != &record.manifest.id)
        {
            return Err(invalid("Invalid plugin registry"));
        }
        for record in registry.active.values().chain(&registry.retired) {
            if record.manifest.digest()? != record.release_digest {
                return Err(invalid("Plugin registry release metadata was modified"));
            }
        }
        Ok(registry)
    }

    fn write(&self, registry: &Registry) -> Result<()> {
        let mut next = tempfile::NamedTempFile::new_in(self.store.root())?;
        serde_json::to_writer(&mut next, registry).map_err(io::Error::other)?;
        next.as_file().sync_all()?;
        next.persist(self.store.root().join("registry.json"))
            .map_err(|error| error.error)?;
        Ok(())
    }

    fn collect_cancellable(&self, cancel: &AtomicBool) -> Result<BTreeMap<String, bool>> {
        self.collect_with(cancel, |_, _| {})
    }

    fn collect_with(
        &self,
        cancel: &AtomicBool,
        before_move: impl Fn(&Path, &str),
    ) -> Result<BTreeMap<String, bool>> {
        cancelled(cancel)?;
        let _cleanup = self.lock("cleanup.lock")?;
        let retired = {
            let _commit = self.lock("registry.lock")?;
            self.read()?.retired
        };
        let versions = self.store.versions_dir()?;
        let mut completed = BTreeMap::new();
        // The registry bounds the complete scan to 128 records. Busy leases
        // do not consume a prefix budget and starve later closed releases.
        for record in &retired {
            cancelled(cancel)?;
            match cleanup(&versions, record, cancel, &before_move) {
                Ok(Some(preserved)) => {
                    completed.insert(record.directory.clone(), preserved);
                }
                Err(crate::packages::PackageError::Cancelled) => {
                    return Err(crate::packages::PackageError::Cancelled);
                }
                _ => {}
            }
        }
        if !completed.is_empty() {
            let _commit = self.lock("registry.lock")?;
            let mut registry = self.read()?;
            registry
                .retired
                .retain(|record| !completed.contains_key(&record.directory));
            self.write(&registry)?;
            for name in completed.keys() {
                if let Ok(dir) = versions.open_dir_nofollow(name) {
                    let _ = dir.remove_file("lease.lock");
                    drop(dir);
                    let _ = versions.remove_dir(name);
                }
            }
        }
        Ok(completed)
    }
}

fn cleanup(
    versions: &Dir,
    record: &Record,
    cancel: &AtomicBool,
    before_move: &impl Fn(&Path, &str),
) -> Result<Option<bool>> {
    record.manifest.validate()?;
    let dir = match record.open(versions) {
        Ok(dir) => dir,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Some(false)),
        Err(error) => return Err(error.into()),
    };
    let lease = open_file(&dir, "lease.lock", true)?;
    match lease.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => return Ok(None),
        Err(error) => return Err(lock_error(error)),
    }
    let _lease = HeldLock(lease);
    for (folder, name, pin) in [
        (
            "bin",
            record.manifest.executable.as_str(),
            &record.manifest.binary_sha256,
        ),
        ("notices", "LICENSE", &record.license_hash),
    ] {
        if let Ok(subdir) = dir.open_dir_nofollow(folder) {
            collect_file(
                &dir,
                &subdir,
                Path::new(&record.directory),
                name,
                pin,
                cancel,
                before_move,
            )?;
            drop(subdir);
            let _ = dir.remove_dir(folder);
        }
    }
    let preserved = dir
        .entries()?
        .any(|entry| entry.map_or(true, |entry| entry.file_name() != "lease.lock"));
    Ok(Some(preserved))
}

#[cfg(not(windows))]
fn open_executable(directory: &Dir, name: &str) -> io::Result<File> {
    open_regular(directory, name)
}

#[cfg(windows)]
fn open_executable(directory: &Dir, name: &str) -> io::Result<File> {
    use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt};
    use cap_std::fs::{OpenOptions, OpenOptionsExt};
    use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

    // Keep write and delete access denied until CreateProcess has loaded this image.
    let mut options = OpenOptions::new();
    options
        .read(true)
        .follow(FollowSymlinks::No)
        .share_mode(FILE_SHARE_READ);
    let file = directory.open_with(name, &options)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("Expected an executable file"));
    }
    Ok(file.into_std())
}

#[cfg(all(test, target_os = "linux"))]
mod lifecycle_regressions {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::{
        fs,
        sync::{Arc, atomic::Ordering, mpsc},
        time::Duration,
    };
    fn hash(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
    fn fixture(root: &Path) -> (PluginRegistry, Record, PathBuf) {
        let manager = PluginRegistry::new(root.join("plugins"));
        let versions = manager.store.versions_path().unwrap();
        let path = versions.join("plugin-ABC123");
        fs::create_dir(&path).unwrap();
        fs::create_dir(path.join("bin")).unwrap();
        fs::create_dir(path.join("notices")).unwrap();
        fs::write(path.join("bin/fixture"), b"owned executable").unwrap();
        fs::write(path.join("notices/LICENSE"), b"owned notice").unwrap();
        File::create_new(path.join("lease.lock")).unwrap();
        let manifest: PluginManifest = serde_json::from_value(serde_json::json!({
            "schema":1,"id":"fixture","version":"1.0.0","host_protocol":1,
            "platform":"linux","architecture":"x86_64","archive":"fixture.tar.gz",
            "archive_sha256": hash(b"archive"), "binary_sha256":hash(b"owned executable"),
            "executable":"fixture","entrypoints":["check"],"permissions":["native-execution"]
        }))
        .unwrap();
        let record = Record {
            release_digest: manifest.digest().unwrap(),
            manifest,
            directory: "plugin-ABC123".into(),
            grant: None,
            license_hash: hash(b"owned notice"),
        };
        let mut registry = Registry::default();
        registry.active.insert("fixture".into(), record.clone());
        registry.retired.push(record.clone());
        manager.write(&registry).unwrap();
        (manager, record, path)
    }
    #[test]
    fn registry_executes_granted_elf_when_installed_inode_changes_after_verification() {
        let root = tempfile::tempdir().unwrap();
        let (manager, mut record, path) = fixture(root.path());
        for (name, value) in [("fixture", "true"), ("replacement", "false")] {
            let source = root.path().join(format!("{name}.c"));
            fs::write(&source, format!("#include <stdio.h>\nint main(void) {{ puts(\"{{\\\"protocol\\\":1,\\\"ok\\\":{value}}}\"); return 0; }}")).unwrap();
            assert!(
                std::process::Command::new("cc")
                    .arg("-o")
                    .arg(path.join("bin").join(name))
                    .arg(source)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let executable = path.join("bin/fixture");
        record.manifest.binary_sha256 = hash(&fs::read(&executable).unwrap());
        record.release_digest = record.manifest.digest().unwrap();
        record.grant = Some(record.release_digest.clone());
        let mut registry = Registry::default();
        registry.active.insert("fixture".into(), record);
        manager.write(&registry).unwrap();
        let result =
            manager.run_inner_with("fixture", None, "check", &AtomicBool::new(false), |_| {
                fs::write(&executable, fs::read(path.join("bin/replacement")).unwrap()).unwrap();
            });
        assert!(
            result.is_ok(),
            "executed bytes outside the release grant: {result:?}"
        );
    }

    #[test]
    fn retirement_preserves_replacement_after_verification() {
        let root = tempfile::tempdir().unwrap();
        let (manager, _, path) = fixture(root.path());
        let replacement = root.path().join("editor");
        fs::write(&replacement, b"user replacement").unwrap();
        manager
            .collect_with(&AtomicBool::new(false), |_, name| {
                if name == "fixture" {
                    fs::rename(&replacement, path.join("bin/fixture")).unwrap();
                }
            })
            .unwrap();
        assert_eq!(
            fs::read(path.join("bin/fixture")).unwrap(),
            b"user replacement"
        );
    }
    #[test]
    fn paused_retirement_allows_registry_changes_and_cancels() {
        let root = tempfile::tempdir().unwrap();
        let (manager, record, path) = fixture(root.path());
        let (entered, waiting) = mpsc::sync_channel(1);
        let (resume, gated) = mpsc::sync_channel(1);
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_manager = manager.clone();
        let worker_cancel = cancel.clone();
        let worker = std::thread::spawn(move || {
            worker_manager.collect_with(&worker_cancel, |_, name| {
                if name == "fixture" {
                    entered.send(()).unwrap();
                    gated.recv_timeout(Duration::from_secs(5)).unwrap();
                }
            })
        });
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
        let enabled = manager.enable("fixture", &record.release_digest);
        cancel.store(true, Ordering::Release);
        resume.send(()).unwrap();
        let outcome = worker.join().unwrap();
        assert!(
            enabled.is_ok(),
            "registry blocked by retirement: {enabled:?}"
        );
        assert!(matches!(
            outcome,
            Err(crate::packages::PackageError::Cancelled)
        ));
        assert_eq!(
            fs::read(path.join("bin/fixture")).unwrap(),
            b"owned executable"
        );
    }
}
