//! Platform atomic rename without replacing an existing destination. Runs on a worker thread.
use std::{io, path::Path};
pub(super) fn rename_no_replace(source: &Path, target: &Path) -> io::Result<()> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        use std::os::unix::ffi::OsStrExt;
        let source = std::ffi::CString::new(source.as_os_str().as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Path contains NUL"))?;
        let target = std::ffi::CString::new(target.as_os_str().as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Path contains NUL"))?;
        #[cfg(target_os = "linux")]
        let result = unsafe {
            libc::renameat2(
                libc::AT_FDCWD,
                source.as_ptr(),
                libc::AT_FDCWD,
                target.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        #[cfg(target_os = "macos")]
        let result =
            unsafe { libc::renamex_np(source.as_ptr(), target.as_ptr(), libc::RENAME_EXCL) };
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let mut source = source.as_os_str().encode_wide().collect::<Vec<_>>();
        let mut target = target.as_os_str().encode_wide().collect::<Vec<_>>();
        if source.contains(&0) || target.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Path contains NUL",
            ));
        }
        source.push(0);
        target.push(0);
        if unsafe {
            windows_sys::Win32::Storage::FileSystem::MoveFileExW(
                source.as_ptr(),
                target.as_ptr(),
                0,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = (source, target);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Atomic no-replace rename is unavailable on this platform",
        ))
    }
}
pub(super) fn close_file(file: std::fs::File) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::fd::IntoRawFd;
        let fd = file.into_raw_fd();
        if unsafe { libc::close(fd) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::IntoRawHandle;
        let handle = file.into_raw_handle();
        if unsafe { windows_sys::Win32::Foundation::CloseHandle(handle) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    #[cfg(not(any(unix, windows)))]
    {
        drop(file);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Checked file close is unavailable on this platform",
        ))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atomic_commit_preserves_both_files_on_conflict() {
        let root = std::env::temp_dir().join(format!(
            "termcraft-commit-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let source = root.join("stage");
        let target = root.join("target");
        std::fs::write(&source, b"new").unwrap();
        std::fs::write(&target, b"keep").unwrap();
        assert!(rename_no_replace(&source, &target).is_err());
        assert_eq!(std::fs::read(&source).unwrap(), b"new");
        assert_eq!(std::fs::read(&target).unwrap(), b"keep");
        std::fs::remove_file(&target).unwrap();
        rename_no_replace(&source, &target).unwrap();
        assert!(!source.exists());
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        std::fs::remove_dir_all(root).unwrap();
    }
}
