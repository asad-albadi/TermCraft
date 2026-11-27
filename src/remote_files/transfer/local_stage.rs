//! Private staging with directory identity retained through atomic publication.
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::sync::Arc;
#[derive(Clone)]
pub(crate) struct LocalStage {
    #[cfg(unix)]
    parent: Arc<std::fs::File>,
    #[cfg(unix)]
    file: Arc<std::fs::File>,
    #[cfg(unix)]
    source: Arc<std::fs::File>,
    #[cfg(unix)]
    container_name: Option<std::ffi::OsString>,
    #[cfg(unix)]
    archive: Option<Arc<std::fs::File>>,
    #[cfg(unix)]
    merge_target: Option<Arc<std::fs::File>>,
    #[cfg(windows)]
    windows: windows::WindowsStage,
    path: PathBuf,
    #[cfg(unix)]
    parent_path: PathBuf,
    #[cfg(unix)]
    name: std::ffi::OsString,
    #[cfg(unix)]
    target: std::ffi::OsString,
}
pub(crate) enum CommitError {
    Failed(String),
    Indeterminate(String),
}
impl LocalStage {
    #[cfg(unix)]
    pub async fn create(target: PathBuf) -> Result<(Self, tokio::fs::File), String> {
        tokio::task::spawn_blocking(move || {
            use std::os::unix::fs::OpenOptionsExt;
            let target_name = target
                .file_name()
                .ok_or("Choose a local filename")?
                .to_owned();
            let parent_path = std::fs::canonicalize(
                target
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or_else(|| Path::new(".")),
            )
            .map_err(|e| e.to_string())?;
            let parent = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(&parent_path)
                .map_err(|e| e.to_string())?;
            let mut random = [0; 16];
            getrandom::fill(&mut random).map_err(|e| e.to_string())?;
            let name = std::ffi::OsString::from(format!(
                ".termcraft-{}.part",
                random
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            ));
            let file = std::fs::File::from(
                rustix::fs::openat(
                    &parent,
                    &name,
                    rustix::fs::OFlags::RDWR
                        | rustix::fs::OFlags::CREATE
                        | rustix::fs::OFlags::EXCL
                        | rustix::fs::OFlags::NOFOLLOW
                        | rustix::fs::OFlags::CLOEXEC,
                    rustix::fs::Mode::from_raw_mode(0o600),
                )
                .map_err(|e| e.to_string())?,
            );
            let writer = file.try_clone().map_err(|e| e.to_string())?;
            Ok((
                Self {
                    source: Arc::new(parent.try_clone().map_err(|e| e.to_string())?),
                    container_name: None,
                    archive: None,
                    merge_target: None,
                    parent: Arc::new(parent),
                    file: Arc::new(file),
                    path: parent_path.join(&name),
                    parent_path,
                    name,
                    target: target_name,
                },
                tokio::fs::File::from_std(writer),
            ))
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(unix)]
    pub async fn create_tree(
        target: PathBuf,
    ) -> Result<(Self, std::fs::File, tokio::fs::File), String> {
        tokio::task::spawn_blocking(move || {
            use rustix::fs::{Mode, OFlags, mkdirat, openat};
            use std::os::unix::fs::OpenOptionsExt;
            let name = target
                .file_name()
                .ok_or("Choose a local folder")?
                .to_owned();
            let parent_path = std::fs::canonicalize(
                target
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or_else(|| Path::new(".")),
            )
            .map_err(|e| e.to_string())?;
            let parent = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(&parent_path)
                .map_err(|e| e.to_string())?;
            let mut random = [0; 16];
            getrandom::fill(&mut random).map_err(|e| e.to_string())?;
            let container_name = std::ffi::OsString::from(format!(
                ".termcraft-folder-{}",
                random
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            ));
            let result = (|| -> Result<_, std::io::Error> {
                mkdirat(&parent, &container_name, Mode::from_raw_mode(0o700))?;
                let container = std::fs::File::from(openat(
                    &parent,
                    &container_name,
                    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                    Mode::empty(),
                )?);
                mkdirat(&container, "tree", Mode::from_raw_mode(0o700))?;
                let tree = std::fs::File::from(openat(
                    &container,
                    "tree",
                    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                    Mode::empty(),
                )?);
                let archive = std::fs::File::from(openat(
                    &container,
                    "archive",
                    OFlags::RDWR
                        | OFlags::CREATE
                        | OFlags::EXCL
                        | OFlags::NOFOLLOW
                        | OFlags::CLOEXEC,
                    Mode::from_raw_mode(0o600),
                )?);
                Ok((container, tree, archive))
            })()
            .map_err(|e| e.to_string())?;
            let (container, tree, archive) = result;
            let stage = Self {
                parent: Arc::new(parent),
                source: Arc::new(container),
                file: Arc::new(tree.try_clone().map_err(|e| e.to_string())?),
                path: parent_path.join(&container_name).join("tree"),
                parent_path,
                name: "tree".into(),
                target: name,
                container_name: Some(container_name),
                archive: Some(Arc::new(archive.try_clone().map_err(|e| e.to_string())?)),
                merge_target: None,
            };
            Ok((stage, tree, tokio::fs::File::from_std(archive)))
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(unix)]
    pub async fn clean_tree_container(&self) -> Result<(), String> {
        let stage = self.clone();
        tokio::task::spawn_blocking(move || {
            use std::os::unix::fs::MetadataExt;
            let Some(name) = stage.container_name else {
                return Ok(());
            };
            let named =
                rustix::fs::statat(&*stage.parent, &name, rustix::fs::AtFlags::SYMLINK_NOFOLLOW)
                    .map_err(|e| e.to_string())?;
            let held = stage.source.metadata().map_err(|e| e.to_string())?;
            if named.st_dev as u64 != held.dev() || named.st_ino as u64 != held.ino() {
                return Err("Owned folder container was replaced; cleanup stopped".into());
            }
            let archive = stage
                .archive
                .as_ref()
                .ok_or("Missing owned archive handle")?
                .metadata()
                .map_err(|e| e.to_string())?;
            let named = rustix::fs::statat(
                &*stage.source,
                "archive",
                rustix::fs::AtFlags::SYMLINK_NOFOLLOW,
            )
            .map_err(|e| e.to_string())?;
            if archive.dev() != named.st_dev as u64 || archive.ino() != named.st_ino as u64 {
                return Err("Owned archive was replaced; cleanup stopped".into());
            }
            rustix::fs::unlinkat(&*stage.source, "archive", rustix::fs::AtFlags::empty())
                .map_err(|e| e.to_string())?;
            rustix::fs::unlinkat(&*stage.parent, &name, rustix::fs::AtFlags::REMOVEDIR)
                .map_err(|e| e.to_string())?;
            Ok(())
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(windows)]
    pub async fn create_tree(
        target: PathBuf,
    ) -> Result<(Self, std::fs::File, tokio::fs::File), String> {
        let (backend, path, root, archive) = tokio::task::spawn_blocking(move || {
            windows::WindowsStage::create_tree(target).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())??;
        Ok((
            Self {
                windows: backend,
                path,
            },
            root,
            tokio::fs::File::from_std(archive),
        ))
    }
    #[cfg(windows)]
    pub async fn pin_merge_target(path: PathBuf) -> Result<std::fs::File, String> {
        tokio::task::spawn_blocking(move || windows::WindowsStage::pin_merge_target(&path))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())
    }
    #[cfg(windows)]
    pub async fn attach_merge_target(
        &mut self,
        selected: std::fs::File,
    ) -> Result<std::fs::File, String> {
        let mut backend = self.windows.clone();
        let (backend, file) = tokio::task::spawn_blocking(move || {
            let file = backend
                .attach_merge_target(selected)
                .map_err(|e| e.to_string())?;
            Ok::<_, String>((backend, file))
        })
        .await
        .map_err(|e| e.to_string())??;
        self.windows = backend;
        Ok(file)
    }
    #[cfg(windows)]
    pub async fn clean_tree_container(&self) -> Result<(), String> {
        let backend = self.windows.clone();
        tokio::task::spawn_blocking(move || {
            backend.clean_tree_container().map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(windows)]
    pub async fn clean_merged_tree(&self) -> Result<(), String> {
        let backend = self.windows.clone();
        tokio::task::spawn_blocking(move || backend.clean_merged_tree().map_err(|e| e.to_string()))
            .await
            .map_err(|e| e.to_string())?
    }
    #[cfg(windows)]
    pub async fn validate_merge_root(&self) -> Result<(), String> {
        let backend = self.windows.clone();
        tokio::task::spawn_blocking(move || {
            backend.validate_merge_root().map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(windows)]
    pub async fn create(target: PathBuf) -> Result<(Self, tokio::fs::File), String> {
        let (backend, path, writer) = tokio::task::spawn_blocking(move || {
            windows::WindowsStage::create(target).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())??;
        Ok((
            Self {
                windows: backend,
                path,
            },
            tokio::fs::File::from_std(writer),
        ))
    }
    #[cfg(not(any(unix, windows)))]
    pub async fn create(_: PathBuf) -> Result<(Self, tokio::fs::File), String> {
        Err("Anchored SCP download publication is not yet available on this local platform; use SFTP".into())
    }
    #[cfg(unix)]
    pub async fn attach_merge_target(
        &mut self,
        expected: std::fs::Metadata,
    ) -> Result<std::fs::File, String> {
        let stage = self.clone();
        let file = tokio::task::spawn_blocking(move || {
            use std::os::unix::fs::MetadataExt;
            let file = std::fs::File::from(
                rustix::fs::openat(
                    &*stage.parent,
                    &stage.target,
                    rustix::fs::OFlags::RDONLY
                        | rustix::fs::OFlags::DIRECTORY
                        | rustix::fs::OFlags::NOFOLLOW
                        | rustix::fs::OFlags::CLOEXEC,
                    rustix::fs::Mode::empty(),
                )
                .map_err(|e| e.to_string())?,
            );
            let actual = file.metadata().map_err(|e| e.to_string())?;
            if actual.dev() != expected.dev() || actual.ino() != expected.ino() {
                return Err("Selected folder merge root changed; refresh and retry".into());
            }
            Ok::<_, String>(file)
        })
        .await
        .map_err(|e| e.to_string())??;
        self.merge_target = Some(Arc::new(file.try_clone().map_err(|e| e.to_string())?));
        Ok(file)
    }
    #[cfg(unix)]
    pub async fn validate_merge_root(&self) -> Result<(), String> {
        let stage = self.clone();
        tokio::task::spawn_blocking(move || {
            use std::os::unix::fs::MetadataExt;
            let same = |file: &std::fs::File, value: rustix::fs::Stat| -> Result<(), String> {
                let held = file.metadata().map_err(|e| e.to_string())?;
                if held.dev() != value.st_dev as u64 || held.ino() != value.st_ino as u64 { Err("Selected merge directory or staging identity changed; remaining files retained".into()) } else { Ok(()) }
            };
            let named = std::fs::symlink_metadata(&stage.parent_path).map_err(|e| e.to_string())?;
            let parent = stage.parent.metadata().map_err(|e| e.to_string())?;
            if !named.is_dir() || named.dev() != parent.dev() || named.ino() != parent.ino() { return Err("Selected merge parent moved; remaining files retained".into()); }
            let name = stage.container_name.as_ref().ok_or("Missing private folder container")?;
            same(&stage.source, rustix::fs::statat(&*stage.parent, name, rustix::fs::AtFlags::SYMLINK_NOFOLLOW).map_err(|e| e.to_string())?)?;
            same(&stage.file, rustix::fs::statat(&*stage.source, &stage.name, rustix::fs::AtFlags::SYMLINK_NOFOLLOW).map_err(|e| e.to_string())?)?;
            same(stage.merge_target.as_ref().ok_or("Missing merge root handle")?, rustix::fs::statat(&*stage.parent, &stage.target, rustix::fs::AtFlags::SYMLINK_NOFOLLOW).map_err(|e| e.to_string())?)
        }).await.map_err(|e| e.to_string())?
    }
    #[cfg(not(any(unix, windows)))]
    pub async fn validate_merge_root(&self) -> Result<(), String> {
        Err("Anchored SCP folder merges are unavailable on this local platform".into())
    }
    #[cfg(unix)]
    pub async fn clean_merged_tree(&self) -> Result<(), String> {
        self.validate_merge_root().await?;
        let stage = self.clone();
        tokio::task::spawn_blocking(move || {
            rustix::fs::unlinkat(&*stage.source, &stage.name, rustix::fs::AtFlags::REMOVEDIR)
                .map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())??;
        self.clean_tree_container().await
    }
    pub fn recovery_path(&self) -> Result<PathBuf, String> {
        #[cfg(windows)]
        {
            return self.windows.recovery_path().map_err(|e| e.to_string());
        }
        #[cfg(target_os = "linux")]
        {
            use std::os::{fd::AsRawFd, unix::fs::MetadataExt};
            let file = if self.container_name.is_some() {
                &self.source
            } else {
                &self.file
            };
            if file.metadata().map_err(|e| e.to_string())?.nlink() == 0 {
                return Err("The staging file was unlinked externally; its former path is not a recoverable file".into());
            }
            return std::fs::read_link(format!("/proc/self/fd/{}", file.as_raw_fd()))
                .map_err(|e| e.to_string());
        }
        #[cfg(target_os = "macos")]
        {
            use std::os::{fd::AsRawFd, unix::ffi::OsStringExt};
            let file = if self.container_name.is_some() {
                &self.source
            } else {
                &self.file
            };
            let mut bytes = vec![0u8; libc::PATH_MAX as usize];
            let result =
                unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETPATH, bytes.as_mut_ptr()) };
            if result < 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
            bytes.truncate(bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len()));
            return Ok(PathBuf::from(std::ffi::OsString::from_vec(bytes)));
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
        Ok(self.path.clone())
    }
    pub fn initial_path(&self) -> &Path {
        &self.path
    }
    #[cfg(unix)]
    pub async fn commit(&self, overwrite: bool) -> Result<bool, CommitError> {
        let stage = self.clone();
        tokio::task::spawn_blocking(move || {
            use std::os::unix::fs::MetadataExt;
            let fail = |e: std::io::Error| CommitError::Failed(e.to_string());
            let parent = stage.parent.metadata().map_err(fail)?;
            let named = std::fs::symlink_metadata(&stage.parent_path).map_err(fail)?;
            if !named.is_dir() || parent.dev() != named.dev() || parent.ino() != named.ino() {
                return Err(CommitError::Failed(
                    "Selected download directory moved or changed; original staging retained"
                        .into(),
                ));
            }
            if let Some(name) = &stage.container_name {
                let named =
                    rustix::fs::statat(&*stage.parent, name, rustix::fs::AtFlags::SYMLINK_NOFOLLOW)
                        .map_err(|e| CommitError::Failed(e.to_string()))?;
                let held = stage.source.metadata().map_err(fail)?;
                if held.dev() != named.st_dev as u64 || held.ino() != named.st_ino as u64 {
                    return Err(CommitError::Failed(
                        "Owned folder container changed; publication stopped".into(),
                    ));
                }
            }
            let file = stage.file.metadata().map_err(fail)?;
            let named = rustix::fs::statat(
                &*stage.source,
                &stage.name,
                rustix::fs::AtFlags::SYMLINK_NOFOLLOW,
            )
            .map_err(|e| CommitError::Failed(e.to_string()))?;
            if file.dev() != named.st_dev as u64 || file.ino() != named.st_ino as u64 {
                return Err(CommitError::Failed(
                    "Download staging name was replaced; no unrelated file was published".into(),
                ));
            }
            let result = if overwrite {
                rustix::fs::renameat(&*stage.source, &stage.name, &*stage.parent, &stage.target)
            } else {
                #[cfg(any(target_os = "linux", target_os = "macos"))]
                {
                    rustix::fs::renameat_with(
                        &*stage.source,
                        &stage.name,
                        &*stage.parent,
                        &stage.target,
                        rustix::fs::RenameFlags::NOREPLACE,
                    )
                }
                #[cfg(not(any(target_os = "linux", target_os = "macos")))]
                {
                    return Err(CommitError::Failed(
                        "Atomic exclusive publication is unavailable on this platform".into(),
                    ));
                }
            };
            match result {
                Ok(()) => Ok(true),
                Err(e) if e == rustix::io::Errno::EXIST => Ok(false),
                Err(e) => Err(CommitError::Failed(e.to_string())),
            }
        })
        .await
        .map_err(|e| {
            CommitError::Indeterminate(format!(
                "Local publication worker ended without an authoritative result: {e}"
            ))
        })?
    }
    #[cfg(windows)]
    pub async fn commit(&self, overwrite: bool) -> Result<bool, CommitError> {
        let stage = self.windows.clone();
        tokio::task::spawn_blocking(move || {
            stage
                .commit(overwrite)
                .map_err(|e| CommitError::Failed(e.to_string()))
        })
        .await
        .map_err(|e| {
            CommitError::Indeterminate(format!(
                "Local publication worker ended without an authoritative result: {e}"
            ))
        })?
    }
    #[cfg(not(any(unix, windows)))]
    pub async fn commit(&self, _: bool) -> Result<bool, CommitError> {
        Err(CommitError::Failed(
            "Anchored publication is unavailable on this platform".into(),
        ))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[tokio::test]
    async fn replaced_folder_containers_and_archives_are_never_published_or_cleaned() {
        let root = std::env::temp_dir().join(format!(
            "termcraft-folder-ownership-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let (stage, _tree, archive) = LocalStage::create_tree(root.join("destination"))
            .await
            .unwrap();
        drop(archive);
        let container = stage.initial_path().parent().unwrap();
        std::fs::rename(container.join("archive"), container.join("old-archive")).unwrap();
        std::fs::write(container.join("archive"), b"unrelated").unwrap();
        assert!(stage.clean_tree_container().await.is_err());
        assert_eq!(
            std::fs::read(container.join("archive")).unwrap(),
            b"unrelated"
        );
        let moved = root.join("moved-container");
        std::fs::rename(container, &moved).unwrap();
        std::fs::create_dir(container).unwrap();
        std::fs::create_dir(container.join("tree")).unwrap();
        std::fs::write(container.join("tree/unrelated"), b"keep").unwrap();
        assert!(matches!(
            stage.commit(false).await,
            Err(CommitError::Failed(_))
        ));
        assert!(!root.join("destination").exists());
        assert_eq!(
            std::fs::read(container.join("tree/unrelated")).unwrap(),
            b"keep"
        );
        drop(stage);
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(windows)]
#[path = "local_stage/windows.rs"]
mod windows;
