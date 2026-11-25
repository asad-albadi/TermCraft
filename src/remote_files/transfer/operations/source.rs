//! Upload sources are anchored to opened directories; child path swaps cannot redirect reads.
use std::path::PathBuf;
#[cfg(not(windows))]
use std::sync::Arc;
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
#[derive(Clone)]
pub(in crate::remote_files::transfer) struct SourceEntry {
    pub name: std::ffi::OsString,
    dev: u64,
    ino: u64,
    pub mode: u32,
}
#[derive(Clone)]
pub(in crate::remote_files::transfer) struct SourceDir {
    #[cfg(not(windows))]
    file: Arc<std::fs::File>,
    #[cfg(windows)]
    backend: cap_backend::CapDir,
}
pub(in crate::remote_files::transfer) enum SourceNode {
    Directory(SourceDir),
    File { file: tokio::fs::File, mode: u32 },
    Symlink(Vec<u8>),
}

fn rustix_mode(value: u32) -> Result<rustix::fs::Mode, String> {
    u16::try_from(value)
        .map(rustix::fs::Mode::from_raw_mode)
        .map_err(|_| "POSIX mode does not fit the platform raw mode type".into())
}

#[cfg(test)]
mod mode_tests {
    #[test]
    fn encoded_modes_are_checked_before_rustix_conversion() {
        assert_eq!(super::rustix_mode(0o755).unwrap().as_raw_mode(), 0o755);
        assert!(super::rustix_mode(u32::from(u16::MAX) + 1).is_err());
    }
}
#[cfg(unix)]
impl SourceDir {
    pub(in crate::remote_files::transfer) fn from_file(file: std::fs::File) -> Self {
        Self {
            file: Arc::new(file),
        }
    }
    pub async fn open(path: PathBuf) -> Result<Self, String> {
        tokio::task::spawn_blocking(move || {
            use std::os::unix::fs::OpenOptionsExt;
            let file = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY | libc::O_CLOEXEC)
                .open(path)
                .map_err(err)?;
            Ok(Self {
                file: Arc::new(file),
            })
        })
        .await
        .map_err(err)?
    }
    pub async fn entries(&self) -> Result<Vec<SourceEntry>, String> {
        let root = self.file.clone();
        tokio::task::spawn_blocking(move || {
            use std::os::unix::ffi::OsStringExt;
            let mut directory = rustix::fs::Dir::read_from(&*root).map_err(err)?;
            let mut entries = Vec::new();
            while let Some(entry) = directory.read() {
                let entry = entry.map_err(err)?;
                let name = entry.file_name().to_bytes();
                if name == b"." || name == b".." {
                    continue;
                }
                if entries.len() >= 10_000 {
                    return Err("Source directory exceeds 10,000 entries".into());
                }
                let name = std::ffi::OsString::from_vec(name.to_vec());
                let stat = rustix::fs::statat(&*root, &name, rustix::fs::AtFlags::SYMLINK_NOFOLLOW)
                    .map_err(err)?;
                entries.push(SourceEntry {
                    name,
                    dev: stat.st_dev as u64,
                    ino: stat.st_ino as u64,
                    mode: stat.st_mode as u32,
                });
            }
            Ok(entries)
        })
        .await
        .map_err(err)?
    }
    pub async fn open_child(&self, entry: SourceEntry) -> Result<SourceNode, String> {
        let root = self.file.clone();
        tokio::task::spawn_blocking(move || {
            use rustix::fs::{AtFlags, FileType, Mode, OFlags};
            use std::os::unix::fs::MetadataExt;
            if FileType::from_raw_mode(rustix_mode(entry.mode)?.as_raw_mode()) == FileType::Symlink
            {
                let target = rustix::fs::readlinkat(&*root, &entry.name, Vec::new())
                    .map_err(err)?
                    .into_bytes();
                let stat = rustix::fs::statat(&*root, &entry.name, AtFlags::SYMLINK_NOFOLLOW)
                    .map_err(err)?;
                if stat.st_dev as u64 != entry.dev
                    || stat.st_ino as u64 != entry.ino
                    || stat.st_mode as u32 != entry.mode
                {
                    return Err("Source link changed while transferring".into());
                }
                return Ok(SourceNode::Symlink(target));
            }
            let flags = OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK;
            let file = std::fs::File::from(
                rustix::fs::openat(&*root, &entry.name, flags, Mode::empty()).map_err(err)?,
            );
            let metadata = file.metadata().map_err(err)?;
            if metadata.dev() != entry.dev
                || metadata.ino() != entry.ino
                || metadata.mode() != entry.mode
            {
                return Err("Source entry changed while transferring".into());
            }
            if metadata.is_dir() {
                Ok(SourceNode::Directory(SourceDir {
                    file: Arc::new(file),
                }))
            } else if metadata.is_file() {
                Ok(SourceNode::File {
                    file: tokio::fs::File::from_std(file),
                    mode: metadata.mode() & 0o777,
                })
            } else {
                Err("Source entry is not a regular file or directory".into())
            }
        })
        .await
        .map_err(err)?
    }
    pub async fn directory_child(&self, name: std::ffi::OsString) -> Result<(Self, bool), String> {
        let root = self.file.clone();
        tokio::task::spawn_blocking(move || {
            use rustix::fs::{Mode, OFlags};
            let mut created = false;
            match rustix::fs::mkdirat(&*root, &name, Mode::from_raw_mode(0o700)) {
                Ok(()) => created = true,
                Err(e) if e == rustix::io::Errno::EXIST => {}
                Err(e) => return Err(err(e)),
            }
            let child = rustix::fs::openat(
                &*root,
                &name,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(err)?;
            Ok((
                Self {
                    file: Arc::new(std::fs::File::from(child)),
                },
                created,
            ))
        })
        .await
        .map_err(err)?
    }
    pub async fn file_mode(&self, name: std::ffi::OsString) -> Result<Option<u32>, String> {
        let root = self.file.clone();
        tokio::task::spawn_blocking(move || {
            use rustix::fs::{AtFlags, FileType};
            match rustix::fs::statat(&*root, &name, AtFlags::SYMLINK_NOFOLLOW) {
                Ok(stat) => Ok(
                    (FileType::from_raw_mode(stat.st_mode) == FileType::RegularFile)
                        .then_some(stat.st_mode as u32 & 0o777),
                ),
                Err(e) if e == rustix::io::Errno::NOENT => Ok(None),
                Err(e) => Err(err(e)),
            }
        })
        .await
        .map_err(err)?
    }
    pub async fn rename_from(&self, source: &Self, name: std::ffi::OsString) -> Result<(), String> {
        let from = source.file.clone();
        let to = self.file.clone();
        tokio::task::spawn_blocking(move || {
            rustix::fs::renameat(&*from, &name, &*to, &name).map_err(err)
        })
        .await
        .map_err(err)?
    }
    pub async fn remove_directory(&self, name: std::ffi::OsString) -> Result<(), String> {
        let root = self.file.clone();
        tokio::task::spawn_blocking(move || {
            rustix::fs::unlinkat(&*root, &name, rustix::fs::AtFlags::REMOVEDIR).map_err(err)
        })
        .await
        .map_err(err)?
    }
    pub async fn set_mode(&self, mode: u32) -> Result<(), String> {
        let root = self.file.clone();
        tokio::task::spawn_blocking(move || {
            use std::os::unix::fs::PermissionsExt;
            root.set_permissions(std::fs::Permissions::from_mode(mode & 0o777))
                .map_err(err)
        })
        .await
        .map_err(err)?
    }
    pub async fn validate_link(&self, target: Vec<u8>) -> Result<(), String> {
        let root = self.file.clone();
        tokio::task::spawn_blocking(move || {
            use rustix::fs::{AtFlags, FileType, Mode, OFlags};
            use std::os::unix::ffi::OsStrExt;
            let mut directory = root.try_clone().map_err(err)?;
            let parts = target
                .split(|b| *b == b'/')
                .filter(|p| !p.is_empty() && *p != b".")
                .collect::<Vec<_>>();
            for (i, part) in parts.iter().enumerate() {
                let name = std::ffi::OsStr::from_bytes(part);
                let stat = match rustix::fs::statat(&directory, name, AtFlags::SYMLINK_NOFOLLOW) {
                    Ok(stat) => stat,
                    Err(e) if e == rustix::io::Errno::NOENT => return Ok(()),
                    Err(e) => return Err(err(e)),
                };
                let kind = FileType::from_raw_mode(stat.st_mode);
                if kind == FileType::Symlink {
                    return Err("Cannot merge a link through another destination symlink".into());
                }
                if i + 1 < parts.len() {
                    if kind != FileType::Directory {
                        return Err("Link target has a nondirectory component".into());
                    }
                    directory = std::fs::File::from(
                        rustix::fs::openat(
                            &directory,
                            name,
                            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                            Mode::empty(),
                        )
                        .map_err(err)?,
                    );
                }
            }
            Ok(())
        })
        .await
        .map_err(err)?
    }
    pub async fn mode(&self) -> Result<u32, String> {
        use std::os::unix::fs::MetadataExt;
        let file = self.file.clone();
        tokio::task::spawn_blocking(move || file.metadata().map(|m| m.mode() & 0o777).map_err(err))
            .await
            .map_err(err)?
    }
}
#[cfg(not(any(unix, windows)))]
impl SourceDir {
    pub async fn open(_: PathBuf) -> Result<Self, String> {
        Err("Anchored folder uploads are not available on this platform yet".into())
    }
    pub async fn entries(&self) -> Result<Vec<SourceEntry>, String> {
        Err("Anchored folder uploads are not available on this platform yet".into())
    }
    pub async fn open_child(&self, _: SourceEntry) -> Result<SourceNode, String> {
        Err("Anchored folder uploads are not available on this platform yet".into())
    }
    pub async fn directory_child(&self, _: std::ffi::OsString) -> Result<(Self, bool), String> {
        Err("Anchored folder merge is unavailable on this platform".into())
    }
    pub async fn file_mode(&self, _: std::ffi::OsString) -> Result<Option<u32>, String> {
        Err("Anchored folder merge is unavailable on this platform".into())
    }
    pub async fn rename_from(&self, _: &Self, _: std::ffi::OsString) -> Result<(), String> {
        Err("Anchored folder merge is unavailable on this platform".into())
    }
    pub async fn remove_directory(&self, _: std::ffi::OsString) -> Result<(), String> {
        Err("Anchored folder merge is unavailable on this platform".into())
    }
    pub async fn set_mode(&self, _: u32) -> Result<(), String> {
        Err("Anchored folder merge is unavailable on this platform".into())
    }
    pub async fn validate_link(&self, _: Vec<u8>) -> Result<(), String> {
        Err("Anchored folder merge is unavailable on this platform".into())
    }
    pub async fn mode(&self) -> Result<u32, String> {
        Ok(0o755)
    }
}
pub(super) async fn open_file(path: PathBuf) -> Result<(tokio::fs::File, u32), String> {
    tokio::task::spawn_blocking(move || {
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.custom_flags(0x00200000);
        }
        let file = options.open(path).map_err(err)?;
        let metadata = file.metadata().map_err(err)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err("Upload source is not a regular file".into());
        }
        #[cfg(unix)]
        let mode = {
            use std::os::unix::fs::MetadataExt;
            metadata.mode() & 0o777
        };
        #[cfg(not(unix))]
        let mode = 0o644;
        Ok((tokio::fs::File::from_std(file), mode))
    })
    .await
    .map_err(err)?
}
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[tokio::test]
    async fn replaced_children_are_not_followed_after_enumeration() {
        let root = std::env::temp_dir().join(format!(
            "termcraft-source-race-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("selected/child")).unwrap();
        std::fs::create_dir(root.join("other")).unwrap();
        std::fs::write(root.join("other/sentinel"), b"not selected").unwrap();
        std::fs::write(root.join("selected/file"), b"selected").unwrap();
        let source = SourceDir::open(root.join("selected")).await.unwrap();
        let entries = source.entries().await.unwrap();
        let directory = entries.iter().find(|e| e.name == "child").unwrap();
        let file = entries.iter().find(|e| e.name == "file").unwrap();
        std::fs::rename(root.join("selected/child"), root.join("old-child")).unwrap();
        std::os::unix::fs::symlink("../other", root.join("selected/child")).unwrap();
        std::fs::rename(root.join("selected/file"), root.join("old-file")).unwrap();
        std::os::unix::fs::symlink("../other/sentinel", root.join("selected/file")).unwrap();
        let directory_rejected = source.open_child(directory.clone()).await.is_err();
        let file_rejected = source.open_child(file.clone()).await.is_err();
        std::fs::remove_dir_all(root).unwrap();
        assert!(directory_rejected);
        assert!(file_rejected);
    }
}

#[cfg(any(windows, test))]
#[cfg_attr(all(test, not(windows)), allow(dead_code))]
#[path = "source/cap_backend.rs"]
mod cap_backend;

#[cfg(windows)]
impl SourceDir {
    pub(in crate::remote_files::transfer) fn from_file(file: std::fs::File) -> Self {
        Self {
            backend: cap_backend::CapDir::from_file(file),
        }
    }
    pub async fn open(path: PathBuf) -> Result<Self, String> {
        Ok(Self {
            backend: cap_backend::CapDir::open(path).await?,
        })
    }
    pub async fn entries(&self) -> Result<Vec<SourceEntry>, String> {
        self.backend.entries().await
    }
    pub async fn open_child(&self, entry: SourceEntry) -> Result<SourceNode, String> {
        match self.backend.open_child(entry).await? {
            cap_backend::CapNode::Directory(backend) => Ok(SourceNode::Directory(Self { backend })),
            cap_backend::CapNode::File { file, mode } => Ok(SourceNode::File { file, mode }),
            cap_backend::CapNode::Symlink(target) => Ok(SourceNode::Symlink(target)),
        }
    }
    pub async fn mode(&self) -> Result<u32, String> {
        self.backend.mode().await
    }
    pub async fn directory_child(&self, name: std::ffi::OsString) -> Result<(Self, bool), String> {
        let (backend, created) = self.backend.directory_child(name).await?;
        Ok((Self { backend }, created))
    }
    pub async fn file_mode(&self, name: std::ffi::OsString) -> Result<Option<u32>, String> {
        self.backend.file_mode(name).await
    }
    pub async fn rename_from(&self, source: &Self, name: std::ffi::OsString) -> Result<(), String> {
        self.backend.rename_from(&source.backend, name).await
    }
    pub async fn remove_directory(&self, name: std::ffi::OsString) -> Result<(), String> {
        self.backend.remove_directory(name).await
    }
    pub async fn set_mode(&self, mode: u32) -> Result<(), String> {
        self.backend.set_mode(mode).await
    }
    pub async fn validate_link(&self, target: Vec<u8>) -> Result<(), String> {
        self.backend.validate_link(target).await
    }
}
