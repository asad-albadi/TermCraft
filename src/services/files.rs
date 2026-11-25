use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt};
use cap_std::{
    ambient_authority,
    fs::{Dir, File, OpenOptions},
};
use std::{
    ffi::OsStr,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

const STAGING: &str = ".harbor-service-stage-";
pub(super) const UPLOAD_LIMIT: u64 = 64 * 1024 * 1024;

pub(super) struct Share {
    root: Dir,
    writable: bool,
    mutations: Mutex<()>,
}

impl Share {
    pub(super) fn open(path: &Path, writable: bool) -> io::Result<Self> {
        Ok(Self {
            root: Dir::open_ambient_dir(path, ambient_authority())?,
            writable,
            mutations: Mutex::new(()),
        })
    }

    pub(super) fn directory(&self, path: &Path) -> io::Result<Dir> {
        let mut dir = self.root.try_clone()?;
        for name in path {
            dir = dir.open_dir_nofollow(name)?;
        }
        Ok(dir)
    }

    fn parent<'a>(&self, path: &'a Path) -> io::Result<(Dir, &'a OsStr)> {
        Ok((
            self.directory(path.parent().ok_or(io::ErrorKind::InvalidInput)?)?,
            path.file_name().ok_or(io::ErrorKind::InvalidInput)?,
        ))
    }

    pub(super) fn read(&self, path: &Path) -> io::Result<File> {
        let (dir, name) = self.parent(path)?;
        if !dir.symlink_metadata(name)?.is_file() {
            return Err(io::ErrorKind::NotFound.into());
        }
        let mut options = OpenOptions::new();
        options.read(true).follow(FollowSymlinks::No);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK);
        }
        let file = dir.open_with(name, &options)?;
        if !file.metadata()?.is_file() {
            return Err(io::ErrorKind::NotFound.into());
        }
        Ok(file)
    }

    pub(super) fn stage(&self, path: &Path) -> io::Result<Staged> {
        self.check_write()?;
        let (dir, destination) = self.parent(path)?;
        require_absent(&dir, destination)?;
        let mut random = [0u8; 16];
        getrandom::fill(&mut random)
            .map_err(|_| io::Error::other("Could not assign a staging filename"))?;
        let suffix = u128::from_ne_bytes(random);
        let temporary = format!("{STAGING}{suffix:032x}");
        let mut options = OpenOptions::new();
        options
            .write(true)
            .create_new(true)
            .follow(FollowSymlinks::No);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = dir.open_with(&temporary, &options)?;
        Ok(Staged {
            dir,
            file: Some(file),
            temporary,
            destination: destination.to_owned(),
        })
    }

    pub(super) fn check_write(&self) -> io::Result<()> {
        if self.writable {
            Ok(())
        } else {
            Err(io::ErrorKind::PermissionDenied.into())
        }
    }

    pub(super) fn create_directory(&self, path: &Path) -> io::Result<()> {
        self.check_write()?;
        let _guard = self.mutations.lock().unwrap();
        let (dir, name) = self.parent(path)?;
        dir.create_dir(name)
    }

    pub(super) fn kind(&self, path: &Path) -> io::Result<cap_std::fs::FileType> {
        let (dir, name) = self.parent(path)?;
        let kind = dir.symlink_metadata(name)?.file_type();
        if kind.is_file() || kind.is_dir() {
            Ok(kind)
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    }

    pub(super) fn remove(&self, path: &Path, directory: bool) -> io::Result<()> {
        self.check_write()?;
        let _guard = self.mutations.lock().unwrap();
        let (dir, name) = self.parent(path)?;
        let kind = dir.symlink_metadata(name)?.file_type();
        match directory {
            true if kind.is_dir() => dir.remove_dir(name),
            false if kind.is_file() => dir.remove_file(name),
            _ => Err(io::ErrorKind::NotFound.into()),
        }
    }

    pub(super) fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.check_write()?;
        let _guard = self.mutations.lock().unwrap();
        let (source_dir, source) = self.parent(from)?;
        let (destination_dir, destination) = self.parent(to)?;
        require_absent(&destination_dir, destination)?;
        self.kind(from)?;
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            // Both operations take exactly one validated child name of an opened directory.
            rustix::fs::renameat_with(
                &source_dir,
                source,
                &destination_dir,
                destination,
                rustix::fs::RenameFlags::NOREPLACE,
            )?;
            Ok(())
        }
        #[cfg(windows)]
        {
            rename_windows(&source_dir, source, &destination_dir, destination)
        }
    }
}

fn require_absent(dir: &Dir, name: &OsStr) -> io::Result<()> {
    match dir.symlink_metadata(name) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
        Ok(_) => Err(io::ErrorKind::AlreadyExists.into()),
    }
}

pub(super) struct Staged {
    dir: Dir,
    file: Option<File>,
    temporary: String,
    destination: std::ffi::OsString,
}

impl Staged {
    pub(super) fn append(&mut self, data: &[u8]) -> io::Result<()> {
        self.file
            .as_mut()
            .expect("staging file is open")
            .write_all(data)
    }

    pub(super) fn publish(mut self, confirm: impl FnOnce() -> io::Result<()>) -> io::Result<()> {
        let file = self.file.take().expect("staging file is open");
        file.sync_all()?;
        drop(file);
        confirm()?;
        self.dir
            .hard_link(&self.temporary, &self.dir, &self.destination)
    }
}

impl Drop for Staged {
    fn drop(&mut self) {
        drop(self.file.take());
        let _ = self.dir.remove_file(&self.temporary);
    }
}

pub(super) fn path(current: &Path, name: &str, allow_absolute: bool) -> io::Result<PathBuf> {
    if name
        .bytes()
        .any(|b| b.is_ascii_control() || b == b'\\' || b == b':')
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    if !allow_absolute
        && (name.starts_with('/')
            || name.split('/').any(|part| part.is_empty() || part == "..")
            || name.as_bytes().windows(3).any(|triplet| {
                triplet[0] == b'%' && triplet[1..].iter().all(u8::is_ascii_hexdigit)
            }))
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut result = if name.starts_with('/') {
        PathBuf::new()
    } else {
        current.to_owned()
    };
    for part in name.split('/') {
        match part {
            "" | "." => {}
            ".." if result.pop() => {}
            ".." => return Err(io::ErrorKind::PermissionDenied.into()),
            name if usable_name(name) => result.push(name),
            _ => return Err(io::ErrorKind::InvalidInput.into()),
        }
    }
    Ok(result)
}

pub(super) fn usable_name(name: &str) -> bool {
    if name.is_empty()
        || name
            .bytes()
            .any(|b| b.is_ascii_control() || b"/\\:*?\"<>|".contains(&b))
        || name.ends_with(['.', ' '])
        || name.to_ascii_lowercase().starts_with(STAGING)
    {
        return false;
    }
    #[cfg(windows)]
    {
        let stem = name
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || stem
                .strip_prefix("COM")
                .or_else(|| stem.strip_prefix("LPT"))
                .is_some_and(|digit| {
                    matches!(
                        digit,
                        "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                    )
                })
        {
            return false;
        }
    }
    true
}

#[cfg(windows)]
fn rename_windows(
    source_dir: &Dir,
    source: &OsStr,
    destination_dir: &Dir,
    destination: &OsStr,
) -> io::Result<()> {
    use cap_std::fs::OpenOptionsExt as _;
    use std::os::windows::{ffi::OsStrExt, io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::{
        DELETE, FILE_FLAG_BACKUP_SEMANTICS, FILE_RENAME_INFO, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, FileRenameInfo, SetFileInformationByHandle,
    };

    let mut options = OpenOptions::new();
    options.follow(FollowSymlinks::No);
    options
        .access_mode(DELETE)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS);
    let file = source_dir.open_with(source, &options)?;
    let metadata = file.metadata()?;
    if !(metadata.is_file() || metadata.is_dir()) {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let name: Vec<_> = destination.encode_wide().collect();
    let offset = std::mem::offset_of!(FILE_RENAME_INFO, FileName);
    let len = std::mem::size_of::<FILE_RENAME_INFO>().max(offset + name.len() * 2);
    let mut buffer = vec![0usize; len.div_ceil(std::mem::size_of::<usize>())];
    // SAFETY: both handles are live. The allocation holds the aligned structure and UTF-16 basename.
    let renamed = unsafe {
        let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
        (*info).Anonymous.ReplaceIfExists = false;
        (*info).RootDirectory = destination_dir.as_raw_handle();
        (*info).FileNameLength = (name.len() * 2) as u32;
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            buffer.as_mut_ptr().cast::<u8>().add(offset).cast::<u16>(),
            name.len(),
        );
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileRenameInfo,
            info.cast(),
            len as u32,
        )
    };
    if renamed != 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
