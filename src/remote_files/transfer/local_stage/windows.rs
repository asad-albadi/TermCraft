//! Rename the held staging file into the held destination directory, without reopening either path.
use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt};
use cap_std::fs::OpenOptionsExt as _;
use std::{
    fs::File,
    io,
    os::windows::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Path, PathBuf},
    sync::Arc,
};
use windows_sys::Win32::{
    Foundation::{ERROR_ALREADY_EXISTS, ERROR_FILE_EXISTS, HANDLE},
    Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_RENAME_INFO, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, FileRenameInfo, GetFileInformationByHandle, GetFinalPathNameByHandleW,
        SetFileInformationByHandle,
    },
};
#[derive(Clone)]
pub(super) struct WindowsStage {
    parent: Arc<File>,
    file: Arc<File>,
    parent_path: PathBuf,
    target: std::ffi::OsString,
    tree: Option<TreeHandles>,
}
#[derive(Clone)]
struct TreeHandles {
    container: Arc<File>,
    container_name: std::ffi::OsString,
    archive: Arc<File>,
    merge_target: Option<Arc<File>>,
}
fn info(file: &File) -> io::Result<BY_HANDLE_FILE_INFORMATION> {
    let mut information = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: the live File owns the handle and the output points to its correctly sized structure.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle() as HANDLE, &mut information) } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(information)
}
fn identity(file: &File) -> io::Result<(u32, u32, u32)> {
    let value = info(file)?;
    Ok((
        value.dwVolumeSerialNumber,
        value.nFileIndexHigh,
        value.nFileIndexLow,
    ))
}
fn directory(path: &Path) -> io::Result<File> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::other(
            "Selected download directory is a reparse point or non-directory",
        ));
    }
    Ok(file)
}
impl WindowsStage {
    pub(super) fn create(target: PathBuf) -> io::Result<(Self, PathBuf, File)> {
        let name = target
            .file_name()
            .ok_or_else(|| io::Error::other("Choose a local filename"))?
            .to_owned();
        // Names are a single Windows basename, never an alternate data stream or rooted NT path.
        let text = name
            .to_str()
            .ok_or_else(|| io::Error::other("Filename cannot be represented on Windows"))?;
        if text.contains([':', '/', '\\'])
            || text.ends_with(['.', ' '])
            || matches!(text, "." | "..")
        {
            return Err(io::Error::other("Unsafe Windows filename"));
        }
        let parent_path = std::fs::canonicalize(
            target
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new(".")),
        )?;
        let parent = directory(&parent_path)?;
        let capability = cap_std::fs::Dir::from_std_file(parent.try_clone()?);
        let mut random = [0; 16];
        getrandom::fill(&mut random).map_err(|e| io::Error::other(e.to_string()))?;
        let stage_name = format!(
            ".termcraft-{}.part",
            random
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        let mut options = cap_std::fs::OpenOptions::new();
        options
            .read(true)
            .write(true)
            .create_new(true)
            .follow(FollowSymlinks::No)
            .access_mode(0x80000000 | 0x40000000 | 0x00010000)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE);
        let file = capability.open_with(&stage_name, &options)?.into_std();
        let writer = file.try_clone()?;
        Ok((
            Self {
                parent: Arc::new(parent),
                file: Arc::new(file),
                parent_path: parent_path.clone(),
                target: name,
                tree: None,
            },
            parent_path.join(stage_name),
            writer,
        ))
    }
    pub(super) fn create_tree(target: PathBuf) -> io::Result<(Self, PathBuf, File, File)> {
        let name = target
            .file_name()
            .ok_or_else(|| io::Error::other("Choose a folder name"))?
            .to_owned();
        let text = name
            .to_str()
            .ok_or_else(|| io::Error::other("Filename cannot be represented on Windows"))?;
        if text.contains([':', '/', '\\'])
            || text.ends_with(['.', ' '])
            || matches!(text, "." | "..")
        {
            return Err(io::Error::other("Unsafe Windows folder name"));
        }
        let parent_path = std::fs::canonicalize(
            target
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new(".")),
        )?;
        let parent = directory(&parent_path)?;
        let capability = cap_std::fs::Dir::from_std_file(parent.try_clone()?);
        let mut random = [0; 16];
        getrandom::fill(&mut random).map_err(|e| io::Error::other(e.to_string()))?;
        let container_name = std::ffi::OsString::from(format!(
            ".termcraft-folder-{}",
            random
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        ));
        capability.create_dir(&container_name)?;
        let container = capability.open_dir_nofollow(&container_name)?;
        container.create_dir("tree")?;
        let root = container.open_dir_nofollow("tree")?.into_std_file();
        let path = parent_path.join(&container_name).join("tree");
        // Only reopen the newly created directory to obtain DELETE access. Its held identity must
        // match the capability-created root before it can receive bytes or authorize a rename.
        let rename_root = std::fs::OpenOptions::new()
            .read(true)
            .access_mode(0x80000000 | 0x00010000)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&path)?;
        if identity(&rename_root)? != identity(&root)? {
            return Err(io::Error::other(
                "Owned folder root changed while opening its rename handle",
            ));
        }
        let mut options = cap_std::fs::OpenOptions::new();
        options
            .read(true)
            .write(true)
            .create_new(true)
            .follow(FollowSymlinks::No)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE);
        let archive = container.open_with("archive", &options)?.into_std();
        let stage = Self {
            parent: Arc::new(parent),
            file: Arc::new(rename_root),
            parent_path,
            target: name,
            tree: Some(TreeHandles {
                container: Arc::new(container.into_std_file()),
                container_name,
                archive: Arc::new(archive.try_clone()?),
                merge_target: None,
            }),
        };
        stage.validate_container(true)?;
        Ok((stage, path, root, archive))
    }
    fn validate_parent(&self) -> io::Result<()> {
        if identity(&directory(&self.parent_path)?)? != identity(&self.parent)? {
            return Err(io::Error::other(
                "Selected download parent moved or changed; original staging retained",
            ));
        }
        Ok(())
    }
    fn validate_container(&self, tree_needed: bool) -> io::Result<()> {
        self.validate_parent()?;
        let tree = self
            .tree
            .as_ref()
            .ok_or_else(|| io::Error::other("Missing private folder container"))?;
        let parent = cap_std::fs::Dir::from_std_file(self.parent.try_clone()?);
        let named = parent
            .open_dir_nofollow(&tree.container_name)?
            .into_std_file();
        if identity(&named)? != identity(&tree.container)? {
            return Err(io::Error::other(
                "Owned folder container changed; no unrelated publication or cleanup attempted",
            ));
        }
        if tree_needed {
            let container = cap_std::fs::Dir::from_std_file(tree.container.try_clone()?);
            let named = container.open_dir_nofollow("tree")?.into_std_file();
            if identity(&named)? != identity(&self.file)? {
                return Err(io::Error::other(
                    "Owned staged tree changed; publication stopped",
                ));
            }
        }
        Ok(())
    }
    pub(super) fn attach_merge_target(&mut self) -> io::Result<File> {
        self.validate_container(true)?;
        let parent = cap_std::fs::Dir::from_std_file(self.parent.try_clone()?);
        let selected = parent.open_dir_nofollow(&self.target)?.into_std_file();
        self.tree
            .as_mut()
            .ok_or_else(|| io::Error::other("Missing folder staging"))?
            .merge_target = Some(Arc::new(selected.try_clone()?));
        Ok(selected)
    }
    pub(super) fn validate_merge_root(&self) -> io::Result<()> {
        self.validate_container(true)?;
        let tree = self.tree.as_ref().unwrap();
        let parent = cap_std::fs::Dir::from_std_file(self.parent.try_clone()?);
        let selected = parent.open_dir_nofollow(&self.target)?.into_std_file();
        if identity(&selected)?
            != identity(
                tree.merge_target
                    .as_ref()
                    .ok_or_else(|| io::Error::other("Missing merge root handle"))?,
            )?
        {
            return Err(io::Error::other(
                "Selected merge root changed; remaining files retained",
            ));
        }
        Ok(())
    }
    pub(super) fn clean_tree_container(&self) -> io::Result<()> {
        self.validate_container(false)?;
        let tree = self.tree.as_ref().unwrap();
        let container = cap_std::fs::Dir::from_std_file(tree.container.try_clone()?);
        let mut options = cap_std::fs::OpenOptions::new();
        options.read(true).follow(FollowSymlinks::No);
        let named = container.open_with("archive", &options)?.into_std();
        if identity(&named)? != identity(&tree.archive)? {
            return Err(io::Error::other(
                "Owned archive changed; no unrelated cleanup attempted",
            ));
        }
        drop(named);
        container.remove_file("archive")?;
        let parent = cap_std::fs::Dir::from_std_file(self.parent.try_clone()?);
        parent.remove_dir(&tree.container_name)
    }
    pub(super) fn clean_merged_tree(&self) -> io::Result<()> {
        self.validate_merge_root()?;
        let tree = self.tree.as_ref().unwrap();
        let container = cap_std::fs::Dir::from_std_file(tree.container.try_clone()?);
        container.remove_dir("tree")?;
        self.clean_tree_container()
    }
    pub(super) fn recovery_path(&self) -> io::Result<PathBuf> {
        let file = self
            .tree
            .as_ref()
            .map(|tree| &tree.container)
            .unwrap_or(&self.file);
        if info(file)?.nNumberOfLinks == 0 {
            return Err(io::Error::other(
                "The staging file has been unlinked; its former path is not recoverable",
            ));
        }
        let mut path = vec![0u16; 32768];
        // SAFETY: the output buffer has the stated UTF-16 capacity and the File keeps its handle live.
        let length = unsafe {
            GetFinalPathNameByHandleW(
                file.as_raw_handle() as HANDLE,
                path.as_mut_ptr(),
                path.len() as u32,
                0,
            )
        } as usize;
        if length == 0 {
            return Err(io::Error::last_os_error());
        }
        if length >= path.len() {
            return Err(io::Error::other("Staging recovery path exceeds its bound"));
        }
        use std::os::windows::ffi::OsStringExt;
        Ok(PathBuf::from(std::ffi::OsString::from_wide(
            &path[..length],
        )))
    }
    pub(super) fn commit(&self, overwrite: bool) -> io::Result<bool> {
        if identity(&directory(&self.parent_path)?)? != identity(&self.parent)? {
            return Err(io::Error::other(
                "Selected download parent moved or changed; original staging retained",
            ));
        }
        if self.tree.is_some() {
            self.validate_container(true)?;
        }
        let name = self.target.encode_wide().collect::<Vec<_>>();
        let offset = std::mem::offset_of!(FILE_RENAME_INFO, FileName);
        let length = std::mem::size_of::<FILE_RENAME_INFO>().max(offset + name.len() * 2);
        let mut buffer = vec![0usize; length.div_ceil(std::mem::size_of::<usize>())];
        // SAFETY: usize allocation provides pointer alignment; allocation holds the entire variable-size
        // FILE_RENAME_INFO, including its UTF-16 tail. The source and parent handles stay owned throughout.
        let result = unsafe {
            let information = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
            (*information).Anonymous.ReplaceIfExists = overwrite;
            (*information).RootDirectory = self.parent.as_raw_handle() as HANDLE;
            (*information).FileNameLength = (name.len() * 2) as u32;
            std::ptr::copy_nonoverlapping(
                name.as_ptr(),
                buffer.as_mut_ptr().cast::<u8>().add(offset).cast::<u16>(),
                name.len(),
            );
            SetFileInformationByHandle(
                self.file.as_raw_handle() as HANDLE,
                FileRenameInfo,
                information.cast(),
                length as u32,
            )
        };
        if result != 0 {
            return Ok(true);
        }
        let error = io::Error::last_os_error();
        if !overwrite
            && matches!(error.raw_os_error(), Some(code) if code == ERROR_ALREADY_EXISTS as i32 || code == ERROR_FILE_EXISTS as i32)
        {
            Ok(false)
        } else {
            Err(error)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_handle_publication_preserves_conflicts_and_moved_parent_identity() {
        use std::io::Write;
        let root = std::env::temp_dir().join(format!(
            "termcraft-win-scp-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(root.join("chosen")).unwrap();
        let target = root.join("chosen/destination");
        let (stage, _, mut writer) = WindowsStage::create(target.clone()).unwrap();
        writer.write_all(b"staged").unwrap();
        writer.sync_all().unwrap();
        drop(writer);
        std::fs::write(&target, b"existing").unwrap();
        assert!(!stage.commit(false).unwrap());
        assert_eq!(std::fs::read(&target).unwrap(), b"existing");
        assert_eq!(
            std::fs::read(stage.recovery_path().unwrap()).unwrap(),
            b"staged"
        );
        assert!(stage.commit(true).unwrap());
        assert_eq!(std::fs::read(&target).unwrap(), b"staged");
        drop(stage);
        let (stage, _, writer) = WindowsStage::create(root.join("chosen/other")).unwrap();
        drop(writer);
        std::fs::rename(root.join("chosen"), root.join("moved")).unwrap();
        std::fs::create_dir(root.join("chosen")).unwrap();
        std::fs::write(root.join("chosen/other"), b"unrelated").unwrap();
        assert!(stage.commit(true).is_err());
        assert_eq!(
            std::fs::read(root.join("chosen/other")).unwrap(),
            b"unrelated"
        );
        drop(stage);
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod tree_tests {
    use super::*;
    #[test]
    fn native_tree_stage_keeps_root_and_archive_ownership_through_publication() {
        use std::io::Write;
        let temporary = tempfile::tempdir().unwrap();
        let destination = temporary.path().join("destination");
        let (stage, path, root, mut archive) =
            WindowsStage::create_tree(destination.clone()).unwrap();
        let dir = cap_std::fs::Dir::from_std_file(root);
        dir.write("file", b"content").unwrap();
        archive.write_all(b"retained archive").unwrap();
        archive.sync_all().unwrap();
        drop(archive);
        drop(dir);
        assert!(stage.commit(false).unwrap());
        assert_eq!(std::fs::read(destination.join("file")).unwrap(), b"content");
        stage.clean_tree_container().unwrap();
        assert!(!path.parent().unwrap().exists());
    }
    #[test]
    fn native_tree_stage_rejects_replaced_merge_roots_and_unrelated_archive_cleanup() {
        let temporary = tempfile::tempdir().unwrap();
        let destination = temporary.path().join("destination");
        std::fs::create_dir(&destination).unwrap();
        let (mut stage, path, _root, archive) =
            WindowsStage::create_tree(destination.clone()).unwrap();
        drop(archive);
        let target = stage.attach_merge_target().unwrap();
        drop(target);
        std::fs::rename(&destination, temporary.path().join("moved")).unwrap();
        std::fs::create_dir(&destination).unwrap();
        assert!(stage.validate_merge_root().is_err());
        let container = path.parent().unwrap();
        std::fs::rename(container.join("archive"), container.join("owned-archive")).unwrap();
        std::fs::write(container.join("archive"), b"unrelated").unwrap();
        assert!(stage.clean_tree_container().is_err());
        assert_eq!(
            std::fs::read(container.join("archive")).unwrap(),
            b"unrelated"
        );
    }
}
