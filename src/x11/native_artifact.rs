//! Copies only compiled-in pinned bytes into a fresh private executable tree.
//! The Windows handles deny write/delete sharing until the owned server exits.
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path},
};

pub struct VerifiedSnapshot {
    _held: Vec<File>,
    #[cfg(windows)]
    _directory: File,
    directory: tempfile::TempDir,
}
impl VerifiedSnapshot {
    pub fn create(root: &Path, files: &[(&str, &str)]) -> Result<Self, String> {
        if files.is_empty() {
            return Err("Native engine pin has no files".into());
        }
        let mut builder = tempfile::Builder::new();
        builder.prefix("termcraft-x11-image-");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            builder.permissions(std::fs::Permissions::from_mode(0o700));
        }
        let directory = builder.tempdir().map_err(|e| e.to_string())?;
        #[cfg(windows)]
        let directory_handle = protect_directory(directory.path())?;
        let mut held = Vec::new();
        let mut total = 0usize;
        for &(relative, expected) in files {
            if relative.is_empty()
                || relative.contains(['\\', ':'])
                || relative
                    .split('/')
                    .any(|v| v.is_empty() || v == "." || v == "..")
                || !Path::new(relative)
                    .components()
                    .all(|v| matches!(v, Component::Normal(_)))
            {
                return Err("Invalid native engine inventory path".into());
            }
            let mut source = open_verified_input(&root.join(relative))?;
            let size = source.metadata().map_err(|e| e.to_string())?.len();
            if size > 128 * 1024 * 1024 {
                return Err("Native engine file exceeds limit".into());
            }
            total = total
                .checked_add(size as usize)
                .ok_or("Native engine tree exceeds limit")?;
            if total > 512 * 1024 * 1024 {
                return Err("Native engine tree exceeds limit".into());
            }
            let mut bytes = Vec::new();
            (&mut source)
                .take(size + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            let checksum: String = Sha256::digest(&bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            if checksum != expected {
                return Err(format!("Native engine checksum mismatch: {relative}"));
            }
            let destination = directory.path().join(relative);
            std::fs::create_dir_all(destination.parent().ok_or("Invalid native destination")?)
                .map_err(|e| e.to_string())?;
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o500);
            }
            let mut out = options.open(&destination).map_err(|e| e.to_string())?;
            out.write_all(&bytes).map_err(|e| e.to_string())?;
            out.sync_all().map_err(|e| e.to_string())?;
            drop(out);
            // Retain handles to the snapshot, never the original package path.
            held.push(open_verified_input(&destination)?);
        }
        Ok(Self {
            _held: held,
            #[cfg(windows)]
            _directory: directory_handle,
            directory,
        })
    }
    pub fn root(&self) -> &Path {
        self.directory.path()
    }
}
fn open_verified_input(path: &Path) -> Result<File, String> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::*;
        options
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options
        .open(path)
        .map_err(|e| format!("Cannot open native engine input: {e}"))?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("Native engine input must be a regular file".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
        {
            return Err("Native engine input cannot be a reparse point".into());
        }
    }
    Ok(file)
}
#[cfg(windows)]
pub fn protect_directory(path: &Path) -> Result<File, String> {
    use std::os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    };
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::{Authorization::*, *},
        Storage::FileSystem::*,
    };
    let handle = OpenOptions::new()
        .read(true)
        .access_mode(READ_CONTROL | WRITE_DAC | WRITE_OWNER | FILE_LIST_DIRECTORY)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|e| e.to_string())?;
    let metadata = handle.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err("Private native X workspace must be an ordinary directory".into());
    }
    let sddl = crate::platform::private_file::user_owned_descriptor("OICI", "FA")
        .map_err(|e| e.to_string())?;
    unsafe {
        let mut descriptor = std::ptr::null_mut();
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &mut descriptor,
            std::ptr::null_mut(),
        ) == 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let (mut present, mut defaulted, mut dacl) = (0, 0, std::ptr::null_mut());
        let parsed = GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted);
        let mut owner = std::ptr::null_mut();
        let parsed_owner = GetSecurityDescriptorOwner(descriptor, &mut owner, &mut defaulted);
        let result = if parsed != 0 && present != 0 && parsed_owner != 0 && !owner.is_null() {
            SetSecurityInfo(
                handle.as_raw_handle(),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION
                    | DACL_SECURITY_INFORMATION
                    | PROTECTED_DACL_SECURITY_INFORMATION,
                owner,
                std::ptr::null_mut(),
                dacl,
                std::ptr::null_mut(),
            )
        } else {
            1
        };
        LocalFree(descriptor);
        if result != 0 {
            return Err(format!(
                "Cannot protect private native X directory: {result}"
            ));
        }
    }
    Ok(handle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    #[test]
    fn snapshot_retains_verified_bytes_when_package_is_modified() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("engine"), b"abc").unwrap();
        let snapshot = VerifiedSnapshot::create(
            root.path(),
            &[(
                "engine",
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            )],
        )
        .unwrap();
        std::fs::write(root.path().join("engine"), b"changed").unwrap();
        let mut bytes = Vec::new();
        std::fs::File::open(snapshot.root().join("engine"))
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes, b"abc");
    }
    #[test]
    fn mismatch_or_traversal_does_not_produce_an_executable_snapshot() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("engine"), b"wrong").unwrap();
        assert!(
            VerifiedSnapshot::create(
                root.path(),
                &[(
                    "engine",
                    "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
                )]
            )
            .is_err()
        );
        for path in [
            "../outside",
            "/absolute",
            "C:\\engine",
            "dir/../engine",
            "dir\\engine",
        ] {
            assert!(
                VerifiedSnapshot::create(root.path(), &[(path, "")]).is_err(),
                "{path}"
            );
        }
    }
    #[cfg(unix)]
    #[test]
    fn symlinked_source_is_never_copied() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("actual"), b"abc").unwrap();
        std::os::unix::fs::symlink("actual", root.path().join("engine")).unwrap();
        assert!(
            VerifiedSnapshot::create(
                root.path(),
                &[(
                    "engine",
                    "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
                )]
            )
            .is_err()
        );
    }
}
