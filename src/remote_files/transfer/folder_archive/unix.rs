use super::{Budget, FolderModes, components, header, link};
use rustix::fs::{self, AtFlags, Mode, OFlags};
use std::{
    collections::HashSet,
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
    },
    path::PathBuf,
};
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn raw_mode(value: u32) -> Result<Mode, String> {
    fs::RawMode::try_from(value)
        .map(Mode::from_raw_mode)
        .map_err(|_| "Folder archive mode does not fit the platform raw mode type".into())
}
fn check(cancelled: &tokio::sync::watch::Receiver<bool>) -> Result<(), String> {
    if *cancelled.borrow() {
        Err("Folder preparation cancelled".into())
    } else {
        Ok(())
    }
}
fn open_dir(root: &File, parts: &[&[u8]]) -> Result<File, String> {
    let mut directory = root.try_clone().map_err(error)?;
    for part in parts {
        directory = File::from(
            fs::openat(
                &directory,
                std::ffi::OsStr::from_bytes(part),
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(error)?,
        );
    }
    Ok(directory)
}
fn walk(
    root: &File,
    prefix: &[u8],
    out: &mut File,
    budget: &mut Budget,
    cancel: &tokio::sync::watch::Receiver<bool>,
) -> Result<(), String> {
    let mut directory = fs::Dir::read_from(root).map_err(error)?;
    while let Some(entry) = directory.read() {
        check(cancel)?;
        let entry = entry.map_err(error)?;
        let name = entry.file_name().to_bytes();
        if name == b"." || name == b".." {
            continue;
        }
        let mut path = prefix.to_vec();
        if !path.is_empty() {
            path.push(b'/');
        }
        path.extend_from_slice(name);
        budget.add(&path)?;
        let name = std::ffi::OsStr::from_bytes(name);
        let before = fs::statat(root, name, AtFlags::SYMLINK_NOFOLLOW).map_err(error)?;
        if fs::FileType::from_raw_mode(before.st_mode) == fs::FileType::Symlink {
            let value = fs::readlinkat(root, name, Vec::new())
                .map_err(error)?
                .into_bytes();
            link(&value)?;
            let after = fs::statat(root, name, AtFlags::SYMLINK_NOFOLLOW).map_err(error)?;
            if (
                before.st_dev,
                before.st_ino,
                before.st_mtime,
                before.st_mtime_nsec,
            ) != (
                after.st_dev,
                after.st_ino,
                after.st_mtime,
                after.st_mtime_nsec,
            ) {
                return Err("Source link changed while preparing folder".into());
            }
            header(out, 3, 0o777, &path, value.len() as u64)?;
            out.write_all(&value).map_err(error)?;
            continue;
        }
        let mut file = File::from(
            fs::openat(
                root,
                name,
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(error)?,
        );
        let metadata = file.metadata().map_err(error)?;
        if metadata.dev() != before.st_dev as u64
            || metadata.ino() != before.st_ino as u64
            || metadata.mode() != before.st_mode as u32
        {
            return Err("Source entry changed while preparing folder".into());
        }
        if metadata.is_dir() {
            header(out, 1, metadata.mode(), &path, 0)?;
            walk(&file, &path, out, budget, cancel)?;
        } else if metadata.is_file() {
            header(out, 2, metadata.mode(), &path, metadata.len())?;
            let mut remaining = metadata.len();
            let mut buffer = [0u8; 32768];
            while remaining > 0 {
                check(cancel)?;
                let n = remaining.min(buffer.len() as u64) as usize;
                file.read_exact(&mut buffer[..n]).map_err(error)?;
                out.write_all(&buffer[..n]).map_err(error)?;
                remaining -= n as u64;
            }
            if file.read(&mut buffer[..1]).map_err(error)? != 0 {
                return Err("Folder source exceeds its advertised size".into());
            }
            let after = file.metadata().map_err(error)?;
            if (
                metadata.len(),
                metadata.mtime(),
                metadata.mtime_nsec(),
                metadata.mode(),
            ) != (after.len(), after.mtime(), after.mtime_nsec(), after.mode())
            {
                return Err("Folder source changed while reading".into());
            }
        } else {
            return Err("Folder contains an unsupported special entry".into());
        }
    }
    Ok(())
}
pub(crate) async fn build(
    path: PathBuf,
    cancel: tokio::sync::watch::Receiver<bool>,
) -> Result<tokio::fs::File, String> {
    tokio::task::spawn_blocking(move || {
        let root = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)
            .map_err(error)?;
        // The private spool is immediately unlinked: its open handle owns its lifetime.
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).map_err(error)?;
        let name = format!(
            ".termcraft-folder-spool-{}",
            random
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        let parent = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(std::env::temp_dir())
            .map_err(error)?;
        let mut out = File::from(
            fs::openat(
                &parent,
                name.as_str(),
                OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::from_raw_mode(0o600),
            )
            .map_err(error)?,
        );
        fs::unlinkat(&parent, name.as_str(), AtFlags::empty()).map_err(error)?;
        out.write_all(b"TCAR1\0").map_err(error)?;
        out.write_all(&(root.metadata().map_err(error)?.mode() & 0o777).to_be_bytes())
            .map_err(error)?;
        walk(&root, b"", &mut out, &mut Budget::default(), &cancel)?;
        out.write_all(&[0]).map_err(error)?;
        out.sync_all().map_err(error)?;
        out.seek(SeekFrom::Start(0)).map_err(error)?;
        Ok(tokio::fs::File::from_std(out))
    })
    .await
    .map_err(error)?
}
pub(crate) async fn extract(
    mut archive: File,
    root: File,
    cancel: tokio::sync::watch::Receiver<bool>,
) -> Result<FolderModes, String> {
    tokio::task::spawn_blocking(move || {
        archive.seek(SeekFrom::Start(0)).map_err(error)?;
        let mut magic = [0u8; 6];
        archive.read_exact(&mut magic).map_err(error)?;
        if &magic != b"TCAR1\0" {
            return Err("Unsupported folder payload".into());
        }
        let u32read = |file: &mut File| -> Result<u32, String> {
            let mut b = [0; 4];
            file.read_exact(&mut b).map_err(error)?;
            Ok(u32::from_be_bytes(b))
        };
        let root_mode = u32read(&mut archive)? & 0o777;
        let mut budget = Budget::default();
        let mut seen = HashSet::new();
        let mut modes = Vec::new();
        loop {
            check(&cancel)?;
            let mut kind = [0];
            archive.read_exact(&mut kind).map_err(error)?;
            if kind[0] == 0 {
                let mut b = [0];
                if archive.read(&mut b).map_err(error)? != 0 {
                    return Err("Trailing folder payload".into());
                }
                break;
            }
            let mode = u32read(&mut archive)? & 0o777;
            let length = u32read(&mut archive)? as usize;
            let mut size = [0; 8];
            archive.read_exact(&mut size).map_err(error)?;
            let size = u64::from_be_bytes(size);
            if length == 0 || length > 32768 {
                return Err("Folder path exceeds 32 KiB".into());
            }
            let mut path = vec![0; length];
            archive.read_exact(&mut path).map_err(error)?;
            budget.add(&path)?;
            if !seen.insert(path.clone()) {
                return Err("Duplicate folder entry".into());
            }
            let parts = components(&path)?;
            let parent = open_dir(&root, &parts[..parts.len() - 1])?;
            let name = std::ffi::OsStr::from_bytes(parts.last().unwrap());
            match kind[0] {
                1 if size == 0 => {
                    fs::mkdirat(&parent, name, Mode::from_raw_mode(0o700)).map_err(error)?;
                    modes.push((path, mode));
                }
                2 => {
                    let mut file = File::from(
                        fs::openat(
                            &parent,
                            name,
                            OFlags::WRONLY
                                | OFlags::CREATE
                                | OFlags::EXCL
                                | OFlags::NOFOLLOW
                                | OFlags::CLOEXEC,
                            Mode::from_raw_mode(0o600),
                        )
                        .map_err(error)?,
                    );
                    let mut remaining = size;
                    let mut buffer = [0u8; 32768];
                    while remaining > 0 {
                        check(&cancel)?;
                        let n = remaining.min(buffer.len() as u64) as usize;
                        archive.read_exact(&mut buffer[..n]).map_err(error)?;
                        file.write_all(&buffer[..n]).map_err(error)?;
                        remaining -= n as u64;
                    }
                    fs::fchmod(&file, raw_mode(mode)?).map_err(error)?;
                    file.sync_all().map_err(error)?;
                    super::super::operations::close_recovery_file(file).map_err(error)?;
                }
                3 if size <= 32768 => {
                    let mut value = vec![0; size as usize];
                    archive.read_exact(&mut value).map_err(error)?;
                    link(&value)?;
                    fs::symlinkat(std::ffi::OsStr::from_bytes(&value), &parent, name)
                        .map_err(error)?;
                }
                _ => return Err("Unsupported folder payload entry".into()),
            }
        }
        root.sync_all().map_err(error)?;
        Ok(FolderModes {
            root: root_mode,
            directories: modes.into_iter().collect(),
        })
    })
    .await
    .map_err(error)?
}

#[cfg(test)]
mod tests {
    use super::*;
    fn temporary(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "termcraft-folder-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&path).unwrap();
        path
    }
    fn payload(path: &[u8], kind: u8, data: &[u8]) -> Vec<u8> {
        let mut bytes = b"TCAR1\0".to_vec();
        bytes.extend(0o755u32.to_be_bytes());
        bytes.push(kind);
        bytes.extend(0o644u32.to_be_bytes());
        bytes.extend((path.len() as u32).to_be_bytes());
        bytes.extend((data.len() as u64).to_be_bytes());
        bytes.extend(path);
        bytes.extend(data);
        bytes.push(0);
        bytes
    }
    #[tokio::test]
    async fn hostile_folder_payloads_cannot_escape_or_follow_directory_links() {
        let temporary = temporary("hostile");
        std::fs::create_dir(temporary.join("tree")).unwrap();
        std::fs::create_dir(temporary.join("outside")).unwrap();
        std::fs::write(temporary.join("outside/sentinel"), b"keep").unwrap();
        std::os::unix::fs::symlink("../outside", temporary.join("tree/alias")).unwrap();
        for bytes in [
            payload(b"../outside/sentinel", 2, b"replace"),
            payload(b"/absolute", 2, b"replace"),
            payload(b"alias/sentinel", 2, b"replace"),
            payload(b"link", 3, b"../outside"),
            payload(&vec![b'a'; 32769], 2, b"large-path"),
        ] {
            std::fs::write(temporary.join("archive"), bytes).unwrap();
            let (_, cancelled) = tokio::sync::watch::channel(false);
            assert!(
                extract(
                    File::open(temporary.join("archive")).unwrap(),
                    File::open(temporary.join("tree")).unwrap(),
                    cancelled
                )
                .await
                .is_err()
            );
            assert_eq!(
                std::fs::read(temporary.join("outside/sentinel")).unwrap(),
                b"keep"
            );
        }
        std::fs::remove_dir_all(temporary).unwrap();
    }
    #[tokio::test]
    async fn cancelled_folder_preparation_stops_before_copying() {
        let temporary = temporary("cancel");
        std::fs::write(temporary.join("file"), b"source").unwrap();
        let (sender, cancelled) = tokio::sync::watch::channel(true);
        let result = build(temporary.clone(), cancelled).await;
        assert!(result.is_err());
        assert_eq!(std::fs::read(temporary.join("file")).unwrap(), b"source");
        drop(sender);
        std::fs::remove_dir_all(temporary).unwrap();
    }
}

pub(crate) async fn finalize(root: File, modes: FolderModes) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let mut directories = modes.directories.into_iter().collect::<Vec<_>>();
        directories.sort_by_key(|(path, _)| std::cmp::Reverse(path.split(|b| *b == b'/').count()));
        for (path, mode) in directories {
            let parts = components(&path)?;
            let directory = open_dir(&root, &parts)?;
            fs::fchmod(&directory, raw_mode(mode)?).map_err(error)?;
        }
        fs::fchmod(&root, raw_mode(modes.root)?).map_err(error)?;
        Ok(())
    })
    .await
    .map_err(error)?
}
