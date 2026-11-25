//! Handle-scoped filesystem backend for Windows; portable tests also exercise it on Unix.
use super::{SourceEntry, err};
use cap_fs_ext::{DirExt, FollowSymlinks, MetadataExt, OpenOptionsFollowExt};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
#[derive(Clone)]
pub(super) struct CapDir {
    directory: Arc<cap_std::fs::Dir>,
}
pub(super) enum CapNode {
    Directory(CapDir),
    File { file: tokio::fs::File, mode: u32 },
    Symlink(Vec<u8>),
}
fn mode(metadata: &cap_std::fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        use cap_std::fs::MetadataExt;
        metadata.mode()
    }
    #[cfg(windows)]
    {
        if metadata.is_symlink() {
            0o120000
        } else if metadata.is_dir() {
            0o040755
        } else if metadata.is_file() {
            0o100000
                | if metadata.permissions().readonly() {
                    0o444
                } else {
                    0o644
                }
        } else {
            0
        }
    }
}
fn matches(metadata: &cap_std::fs::Metadata, entry: &SourceEntry) -> bool {
    metadata.dev() == entry.dev && metadata.ino() == entry.ino && mode(metadata) == entry.mode
}
fn target_bytes(path: &Path) -> Result<Vec<u8>, String> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Ok(path.as_os_str().as_bytes().to_vec())
    }
    #[cfg(windows)]
    {
        if path.components().any(|c| {
            matches!(
                c,
                std::path::Component::Prefix(_) | std::path::Component::RootDir
            )
        }) {
            return Err("Absolute Windows symlink targets are not transferred".into());
        }
        Ok(path
            .to_str()
            .ok_or("Symlink target cannot be represented by SFTP")?
            .replace('\\', "/")
            .into_bytes())
    }
}
impl CapDir {
    #[cfg(windows)]
    pub(super) fn from_file(file: std::fs::File) -> Self {
        Self {
            directory: Arc::new(cap_std::fs::Dir::from_std_file(file)),
        }
    }
    pub async fn open(path: PathBuf) -> Result<Self, String> {
        tokio::task::spawn_blocking(move || {
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            let name = path.file_name().ok_or("Choose a folder with a name")?;
            let ambient = cap_std::fs::Dir::open_ambient_dir(parent, cap_std::ambient_authority())
                .map_err(err)?;
            let directory = ambient.open_dir_nofollow(name).map_err(err)?;
            Ok(Self {
                directory: Arc::new(directory),
            })
        })
        .await
        .map_err(err)?
    }
    pub async fn entries(&self) -> Result<Vec<SourceEntry>, String> {
        let root = self.directory.clone();
        tokio::task::spawn_blocking(move || {
            let mut entries = Vec::new();
            for entry in root.entries().map_err(err)? {
                let entry = entry.map_err(err)?;
                if entries.len() >= 10_000 {
                    return Err("Source directory exceeds 10,000 entries".into());
                }
                let name = entry.file_name();
                let metadata = root.symlink_metadata(&name).map_err(err)?;
                entries.push(SourceEntry {
                    name,
                    dev: metadata.dev(),
                    ino: metadata.ino(),
                    mode: mode(&metadata),
                });
            }
            Ok(entries)
        })
        .await
        .map_err(err)?
    }
    pub async fn open_child(&self, entry: SourceEntry) -> Result<CapNode, String> {
        let root = self.directory.clone();
        tokio::task::spawn_blocking(move || match entry.mode & 0o170000 {
            0o040000 => {
                let directory = root.open_dir_nofollow(&entry.name).map_err(err)?;
                if !matches(&directory.dir_metadata().map_err(err)?, &entry) {
                    return Err("Source directory changed while transferring".into());
                }
                Ok(CapNode::Directory(Self {
                    directory: Arc::new(directory),
                }))
            }
            0o100000 => {
                let mut options = cap_std::fs::OpenOptions::new();
                options.read(true).follow(FollowSymlinks::No);
                let file = root.open_with(&entry.name, &options).map_err(err)?;
                let metadata = file.metadata().map_err(err)?;
                if !metadata.is_file() || !matches(&metadata, &entry) {
                    return Err("Source file changed while transferring".into());
                }
                Ok(CapNode::File {
                    file: tokio::fs::File::from_std(file.into_std()),
                    mode: entry.mode & 0o777,
                })
            }
            0o120000 => {
                let target = root.read_link_contents(&entry.name).map_err(err)?;
                if !matches(&root.symlink_metadata(&entry.name).map_err(err)?, &entry) {
                    return Err("Source symlink changed while transferring".into());
                }
                Ok(CapNode::Symlink(target_bytes(&target)?))
            }
            _ => Err("Source entry is not a regular file, directory or symlink".into()),
        })
        .await
        .map_err(err)?
    }
    pub async fn mode(&self) -> Result<u32, String> {
        let root = self.directory.clone();
        tokio::task::spawn_blocking(move || Ok(mode(&root.dir_metadata().map_err(err)?) & 0o777))
            .await
            .map_err(err)?
    }
    pub async fn directory_child(&self, name: std::ffi::OsString) -> Result<(Self, bool), String> {
        let root = self.directory.clone();
        tokio::task::spawn_blocking(move || {
            let created = match root.create_dir(&name) {
                Ok(()) => true,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => false,
                Err(e) => return Err(err(e)),
            };
            let directory = root.open_dir_nofollow(&name).map_err(err)?;
            Ok((
                Self {
                    directory: Arc::new(directory),
                },
                created,
            ))
        })
        .await
        .map_err(err)?
    }
    pub async fn file_mode(&self, name: std::ffi::OsString) -> Result<Option<u32>, String> {
        let root = self.directory.clone();
        tokio::task::spawn_blocking(move || match root.symlink_metadata(&name) {
            Ok(metadata) => Ok(metadata.is_file().then_some(mode(&metadata) & 0o777)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(err(e)),
        })
        .await
        .map_err(err)?
    }
    pub async fn rename_from(&self, source: &Self, name: std::ffi::OsString) -> Result<(), String> {
        let source = source.directory.clone();
        let target = self.directory.clone();
        tokio::task::spawn_blocking(move || source.rename(&name, &target, &name).map_err(err))
            .await
            .map_err(err)?
    }
    pub async fn remove_directory(&self, name: std::ffi::OsString) -> Result<(), String> {
        let root = self.directory.clone();
        tokio::task::spawn_blocking(move || root.remove_dir(name).map_err(err))
            .await
            .map_err(err)?
    }
    pub async fn set_mode(&self, mode: u32) -> Result<(), String> {
        #[cfg(unix)]
        {
            let root = self.directory.clone();
            tokio::task::spawn_blocking(move || {
                use std::os::unix::fs::PermissionsExt;
                root.set_permissions(
                    ".",
                    cap_std::fs::Permissions::from_std(std::fs::Permissions::from_mode(
                        mode & 0o777,
                    )),
                )
                .map_err(err)
            })
            .await
            .map_err(err)?
        }
        // POSIX mode bits do not map to Windows ACLs; inherited ACLs remain in force.
        #[cfg(windows)]
        {
            let _ = mode;
            Ok(())
        }
    }
    pub async fn validate_link(&self, target: Vec<u8>) -> Result<(), String> {
        let root = self.directory.clone();
        tokio::task::spawn_blocking(move || {
            let target = std::str::from_utf8(&target)
                .map_err(|_| "Link target cannot be represented locally")?;
            let parts = target
                .split('/')
                .filter(|p| !p.is_empty() && *p != ".")
                .collect::<Vec<_>>();
            let mut directory = root;
            for (i, name) in parts.iter().enumerate() {
                let metadata = match directory.symlink_metadata(name) {
                    Ok(metadata) => metadata,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                    Err(e) => return Err(err(e)),
                };
                if metadata.is_symlink() {
                    return Err("Cannot merge a link through another destination symlink".into());
                }
                if i + 1 < parts.len() {
                    directory = Arc::new(directory.open_dir_nofollow(name).map_err(err)?);
                }
            }
            Ok(())
        })
        .await
        .map_err(err)?
    }
}
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[tokio::test]
    async fn capability_backend_rejects_swapped_sources_and_destination_links() {
        let root = std::env::temp_dir().join(format!(
            "termcraft-cap-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("selected/sub")).unwrap();
        std::fs::create_dir(root.join("other")).unwrap();
        std::fs::write(root.join("selected/file"), b"selected").unwrap();
        std::fs::write(root.join("other/sentinel"), b"unselected").unwrap();
        let source = CapDir::open(root.join("selected")).await.unwrap();
        let entries = source.entries().await.unwrap();
        match source
            .open_child(entries.iter().find(|e| e.name == "file").unwrap().clone())
            .await
            .unwrap()
        {
            CapNode::File { mut file, mode } => {
                use tokio::io::AsyncReadExt;
                let mut bytes = Vec::new();
                file.read_to_end(&mut bytes).await.unwrap();
                assert_eq!(bytes, b"selected");
                assert_eq!(mode & 0o600, 0o600);
            }
            _ => panic!("Expected a regular source file"),
        }

        std::fs::rename(root.join("selected/sub"), root.join("old-sub")).unwrap();
        std::os::unix::fs::symlink("../other", root.join("selected/sub")).unwrap();
        std::fs::remove_file(root.join("selected/file")).unwrap();
        std::os::unix::fs::symlink("../other/sentinel", root.join("selected/file")).unwrap();
        for name in ["sub", "file"] {
            assert!(
                source
                    .open_child(entries.iter().find(|e| e.name == name).unwrap().clone())
                    .await
                    .is_err()
            );
        }
        assert!(source.directory_child("sub".into()).await.is_err());
        assert_eq!(
            std::fs::read(root.join("other/sentinel")).unwrap(),
            b"unselected"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn capability_backend_moves_files_between_anchored_directories() {
        let root = std::env::temp_dir().join(format!(
            "termcraft-cap-move-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("source")).unwrap();
        std::fs::create_dir(root.join("target")).unwrap();
        std::fs::write(root.join("source/file"), b"new").unwrap();
        std::fs::write(root.join("target/file"), b"old").unwrap();
        let source = CapDir::open(root.join("source")).await.unwrap();
        let target = CapDir::open(root.join("target")).await.unwrap();
        target.rename_from(&source, "file".into()).await.unwrap();
        assert_eq!(std::fs::read(root.join("target/file")).unwrap(), b"new");
        assert!(!root.join("source/file").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
