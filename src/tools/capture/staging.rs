use std::{
    ffi::OsString,
    fs::{self, File},
    io,
    path::{Path, PathBuf},
};

fn target(path: &Path) -> io::Result<(PathBuf, OsString)> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::other("Choose a capture filename"))?;
    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()?;
    Ok((directory, name.into()))
}

fn random_name() -> io::Result<OsString> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|err| io::Error::other(err.to_string()))?;
    let suffix: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    Ok(format!(".harbor-capture-{suffix}").into())
}

const STAGED_FILE: &str = "capture.pcap";

#[cfg(unix)]
mod platform {
    use super::*;
    use rustix::fs::{AtFlags, Mode, OFlags, Stat, mkdirat, openat, statat, unlinkat};
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

    pub(super) fn same(file: &File, named: Stat) -> io::Result<bool> {
        let held = file.metadata()?;
        Ok(held.dev() == named.st_dev as u64 && held.ino() == named.st_ino as u64)
    }

    pub(crate) struct Stage {
        parent: File,
        parent_path: PathBuf,
        container: File,
        container_name: OsString,
        target: OsString,
        file: File,
        keep: bool,
    }

    impl Stage {
        pub(crate) fn create(path: &Path) -> io::Result<Self> {
            let (parent_path, target) = target(path)?;
            let parent = fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(&parent_path)?;
            match statat(&parent, &target, AtFlags::SYMLINK_NOFOLLOW) {
                Ok(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        "Capture output already exists",
                    ));
                }
                Err(rustix::io::Errno::NOENT) => {}
                Err(err) => return Err(err.into()),
            }

            let container_name = random_name()?;
            mkdirat(&parent, &container_name, Mode::from_raw_mode(0o700))?;
            let container = File::from(openat(
                &parent,
                &container_name,
                OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )?);
            let info = container.metadata()?;
            if info.mode() & 0o077 != 0 || info.uid() != unsafe { libc::geteuid() } {
                return Err(io::Error::other("Capture staging directory is not private"));
            }
            let file = File::from(openat(
                &container,
                STAGED_FILE,
                OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::from_raw_mode(0o600),
            )?);
            Ok(Self {
                parent,
                parent_path,
                container,
                container_name,
                target,
                file,
                keep: false,
            })
        }

        pub(crate) fn file(&mut self) -> &mut File {
            &mut self.file
        }

        pub(crate) fn publish(&mut self) -> Result<PathBuf, String> {
            match self.publish_anchored() {
                Ok(output) => Ok(output),
                Err(err) => {
                    self.keep = true;
                    let path = recovery_path(&self.file).ok();
                    Err(match path {
                        Some(path) => format!(
                            "Could not save capture output: {err}. Capture retained at {}",
                            path.display()
                        ),
                        None => format!(
                            "Could not save capture output: {err}. Capture staging could not be located"
                        ),
                    })
                }
            }
        }

        fn publish_anchored(&self) -> io::Result<PathBuf> {
            let actual_parent = fs::symlink_metadata(&self.parent_path)?;
            let parent = self.parent.metadata()?;
            if !actual_parent.is_dir()
                || actual_parent.dev() != parent.dev()
                || actual_parent.ino() != parent.ino()
                || !same(
                    &self.container,
                    statat(
                        &self.parent,
                        &self.container_name,
                        AtFlags::SYMLINK_NOFOLLOW,
                    )?,
                )?
                || !same(
                    &self.file,
                    statat(&self.container, STAGED_FILE, AtFlags::SYMLINK_NOFOLLOW)?,
                )?
            {
                return Err(io::Error::other(
                    "Selected output directory or its staging entry changed",
                ));
            }
            #[cfg(target_os = "linux")]
            {
                use std::os::fd::AsRawFd;
                // Publish the inode held by the file descriptor even if the
                // staging name changes after the identity checks above.
                rustix::fs::linkat(
                    rustix::fs::CWD,
                    format!("/proc/self/fd/{}", self.file.as_raw_fd()),
                    &self.parent,
                    &self.target,
                    AtFlags::SYMLINK_FOLLOW,
                )?;
            }
            #[cfg(target_os = "macos")]
            rustix::fs::renameat_with(
                &self.container,
                STAGED_FILE,
                &self.parent,
                &self.target,
                rustix::fs::RenameFlags::NOREPLACE,
            )?;
            let parent = recovery_path(&self.parent).unwrap_or_else(|_| self.parent_path.clone());
            Ok(parent.join(&self.target))
        }
    }

    impl Drop for Stage {
        fn drop(&mut self) {
            if self.keep {
                return;
            }
            if statat(&self.container, STAGED_FILE, AtFlags::SYMLINK_NOFOLLOW)
                .ok()
                .is_some_and(|named| same(&self.file, named).unwrap_or(false))
            {
                let _ = unlinkat(&self.container, STAGED_FILE, AtFlags::empty());
            }
            if statat(
                &self.parent,
                &self.container_name,
                AtFlags::SYMLINK_NOFOLLOW,
            )
            .ok()
            .is_some_and(|named| same(&self.container, named).unwrap_or(false))
            {
                let _ = unlinkat(&self.parent, &self.container_name, AtFlags::REMOVEDIR);
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn recovery_path(file: &File) -> io::Result<PathBuf> {
        use std::os::fd::AsRawFd;
        if file.metadata()?.nlink() == 0 {
            return Err(io::Error::other("Capture staging was unlinked"));
        }
        fs::read_link(format!("/proc/self/fd/{}", file.as_raw_fd()))
    }

    #[cfg(target_os = "macos")]
    fn recovery_path(file: &File) -> io::Result<PathBuf> {
        use std::os::{fd::AsRawFd, unix::ffi::OsStringExt};
        let mut bytes = vec![0u8; libc::PATH_MAX as usize];
        if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETPATH, bytes.as_mut_ptr()) } < 0 {
            return Err(io::Error::last_os_error());
        }
        bytes.truncate(
            bytes
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(bytes.len()),
        );
        Ok(OsString::from_vec(bytes).into())
    }

    #[cfg(all(test, target_os = "linux"))]
    mod tests {
        use super::*;
        use std::io::Write;

        #[test]
        fn publishes_owned_capture_and_removes_private_staging() {
            let root = tempfile::tempdir().unwrap();
            let output = root.path().join("saved.pcap");
            let mut stage = Stage::create(&output).unwrap();
            let container = root.path().join(&stage.container_name);
            stage.file().write_all(b"capture").unwrap();
            assert_eq!(stage.publish().unwrap(), output);
            drop(stage);
            assert_eq!(fs::read(output).unwrap(), b"capture");
            assert!(!container.exists());
        }

        #[test]
        fn output_directory_replacement_keeps_capture_in_original_directory() {
            let root = tempfile::tempdir().unwrap();
            let chosen = root.path().join("chosen");
            fs::create_dir(&chosen).unwrap();
            let mut stage = Stage::create(&chosen.join("saved.pcap")).unwrap();
            let moved = root.path().join("original");
            fs::rename(&chosen, &moved).unwrap();
            fs::create_dir(&chosen).unwrap();
            let unrelated = chosen.join(&stage.container_name);
            fs::create_dir(&unrelated).unwrap();
            fs::write(unrelated.join(STAGED_FILE), b"unrelated").unwrap();

            stage.file().write_all(b"capture").unwrap();
            let error = stage.publish().unwrap_err();
            let retained = moved.join(&stage.container_name).join(STAGED_FILE);
            assert!(error.contains(retained.to_str().unwrap()), "{error}");
            drop(stage);
            assert_eq!(fs::read(retained).unwrap(), b"capture");
            assert_eq!(fs::read(unrelated.join(STAGED_FILE)).unwrap(), b"unrelated");
            assert!(!chosen.join("saved.pcap").exists());
        }

        #[test]
        fn cleanup_does_not_remove_a_replaced_staging_directory() {
            let root = tempfile::tempdir().unwrap();
            let mut stage = Stage::create(&root.path().join("saved.pcap")).unwrap();
            let original = root.path().join(&stage.container_name);
            let moved = root.path().join("original");
            fs::rename(&original, &moved).unwrap();
            fs::create_dir(&original).unwrap();
            fs::write(original.join(STAGED_FILE), b"unrelated").unwrap();
            stage.file().write_all(b"capture").unwrap();
            drop(stage);
            assert_eq!(fs::read(original.join(STAGED_FILE)).unwrap(), b"unrelated");
            assert!(!moved.join(STAGED_FILE).exists());
        }
    }
}

pub(super) use platform::Stage;

#[cfg(windows)]
#[path = "staging_windows.rs"]
mod platform;
