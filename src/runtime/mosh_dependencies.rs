//! Verify the complete pinned Perl/OpenSSH/native dependency tree before Mosh launch.
use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt};
use cap_std::{
    ambient_authority,
    fs::{Dir, OpenOptions},
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Deserialize)]
struct Manifest {
    schema_version: u32,
    platform: String,
    architecture: String,
    root: PathBuf,
    executable: PathBuf,
    version: String,
    files: Vec<Entry>,
}
#[derive(Deserialize)]
struct Entry {
    path: PathBuf,
    size: u64,
    sha256: String,
    executable: bool,
}
#[cfg(test)]
pub(crate) static VERIFIED_CHUNKS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

const FAILURE: &str = "The selected bundled Mosh dependency runtime is unavailable or damaged";

fn relative(path: &Path) -> Result<(), String> {
    if path.components().next().is_none()
        || !path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err(FAILURE.into());
    }
    Ok(())
}

fn file(root: &Dir, path: &Path) -> Result<cap_std::fs::File, String> {
    relative(path)?;
    let mut dir = root.try_clone().map_err(|_| FAILURE)?;
    let mut parts = path.components().peekable();
    while let Some(Component::Normal(name)) = parts.next() {
        if parts.peek().is_some() {
            dir = dir.open_dir_nofollow(name).map_err(|_| FAILURE)?;
        } else {
            let mut options = OpenOptions::new();
            options.read(true).follow(FollowSymlinks::No);
            #[cfg(unix)]
            {
                use cap_std::fs::OpenOptionsExt;
                options.custom_flags(libc::O_NONBLOCK);
            }
            return dir.open_with(name, &options).map_err(|_| FAILURE.into());
        }
    }
    Err(FAILURE.into())
}

pub(super) fn resolve(resources: &Path) -> Result<super::ToolProvider, String> {
    resolve_cancellable(resources, &AtomicBool::new(false))
}

pub(super) fn resolve_cancellable(
    resources: &Path,
    cancelled: &AtomicBool,
) -> Result<super::ToolProvider, String> {
    verify(resources, cancelled, None)
}

#[cfg(unix)]
pub(super) fn prepare(
    resources: &Path,
    cancelled: &AtomicBool,
) -> Result<(super::ToolProvider, tempfile::TempDir), String> {
    use std::os::unix::fs::PermissionsExt;
    let snapshot = tempfile::Builder::new()
        .prefix("termcraft-mosh-")
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir_in("/tmp")
        .map_err(|_| FAILURE)?;
    let provider = verify(resources, cancelled, Some(snapshot.path()))?;
    Ok((provider, snapshot))
}

fn verify(
    resources: &Path,
    cancelled: &AtomicBool,
    snapshot: Option<&Path>,
) -> Result<super::ToolProvider, String> {
    let check_cancel = || {
        if cancelled.load(Ordering::Acquire) {
            Err("Mosh startup cancelled".to_owned())
        } else {
            Ok(())
        }
    };
    check_cancel()?;
    let info: Manifest = serde_json::from_str(include_str!(
        "../../packaging/runtime/mosh-dependencies.json"
    ))
    .map_err(|_| FAILURE)?;
    if info.schema_version != 1
        || info.platform != std::env::consts::OS
        || info.architecture != std::env::consts::ARCH
        || !resources.is_absolute()
    {
        return Err(format!("{FAILURE}: no qualified payload for this platform"));
    }
    if info.files.len() > 4096
        || info.files.iter().map(|entry| entry.size).sum::<u64>() > 256 * 1024 * 1024
        || !info
            .files
            .iter()
            .any(|entry| entry.path == Path::new("bin/ssh") && entry.executable)
    {
        return Err(FAILURE.into());
    }
    relative(&info.root)?;
    relative(&info.executable)?;
    let mut directory = match resources.parent().zip(resources.file_name()) {
        Some((parent, name)) => Dir::open_ambient_dir(parent, ambient_authority())
            .and_then(|dir| dir.open_dir_nofollow(name)),
        None => Dir::open_ambient_dir(resources, ambient_authority()),
    }
    .map_err(|_| FAILURE)?;
    for part in info.root.components() {
        let Component::Normal(name) = part else {
            return Err(FAILURE.into());
        };
        directory = directory.open_dir_nofollow(name).map_err(|_| FAILURE)?;
    }
    // The installed manifest is data too; the compiled pin remains authoritative.
    let mut installed = file(&directory, Path::new("MOSH-DEPENDENCIES.json"))?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut installed)
        .take(1024 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|_| FAILURE)?;
    if bytes != include_bytes!("../../packaging/runtime/mosh-dependencies.json") {
        return Err(format!("{FAILURE}: manifest mismatch"));
    }
    let mut entry_found = false;
    for entry in &info.files {
        check_cancel()?;
        let input = file(&directory, &entry.path)?;
        let meta = input.metadata().map_err(|_| FAILURE)?;
        if !meta.is_file() || meta.len() != entry.size || entry.size > 1024 * 1024 * 1024 {
            return Err(format!("{FAILURE}: invalid {}", entry.path.display()));
        }
        #[cfg(unix)]
        {
            use cap_std::fs::MetadataExt;
            let mode = meta.mode();
            if entry.executable && mode & 0o111 == 0 || mode & 0o6000 != 0 {
                return Err(format!("{FAILURE}: unsafe executable permissions"));
            }
        }
        let mut output = if let Some(snapshot) = snapshot {
            let path = snapshot.join(&entry.path);
            std::fs::create_dir_all(path.parent().ok_or(FAILURE)?).map_err(|_| FAILURE)?;
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            Some(options.open(path).map_err(|_| FAILURE)?)
        } else {
            None
        };
        let mut input = input.take(entry.size + 1);
        let mut total = 0;
        let mut digest = Sha256::new();
        let mut header = Vec::new();
        let mut buffer = [0u8; 64 * 1024];
        loop {
            check_cancel()?;
            let count = input.read(&mut buffer).map_err(|_| FAILURE)?;
            check_cancel()?;
            if count == 0 {
                break;
            }
            total += count as u64;
            #[cfg(test)]
            VERIFIED_CHUNKS.fetch_add(1, std::sync::atomic::Ordering::Release);
            if header.is_empty() {
                header.extend_from_slice(&buffer[..count.min(4096)]);
            }
            digest.update(&buffer[..count]);
            if let Some(output) = &mut output {
                output.write_all(&buffer[..count]).map_err(|_| FAILURE)?;
            }
        }
        if total != entry.size
            || digest
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
                != entry.sha256
        {
            return Err(format!(
                "{FAILURE}: checksum mismatch for {}",
                entry.path.display()
            ));
        }
        #[cfg(unix)]
        if let Some(output) = &output {
            use std::os::unix::fs::PermissionsExt;
            output
                .set_permissions(std::fs::Permissions::from_mode(if entry.executable {
                    0o500
                } else {
                    0o400
                }))
                .map_err(|_| FAILURE)?;
        }
        if entry.path == info.executable {
            entry_found = entry.executable && super::manifest::native_header(&header);
        }
        // Resource-native helpers must have matching machine headers as well.
        if entry.executable && !super::manifest::native_header(&header) {
            return Err(format!("{FAILURE}: invalid native image"));
        }
    }
    if !entry_found {
        return Err(FAILURE.into());
    }
    Ok(super::ToolProvider::Bundled {
        path: snapshot
            .map_or_else(|| resources.join(&info.root), Path::to_path_buf)
            .join(info.executable),
        version: info.version,
    })
}

#[cfg(test)]
mod closure_cancellation_tests {
    use super::*;
    #[test]
    #[ignore = "requires the authentic staged closure"]
    fn actual_dependency_snapshot_checks_cancellation_between_chunks() {
        use std::sync::Arc;
        use std::time::{Duration, Instant};
        let root = PathBuf::from(std::env::var_os("TERMCRAFT_STAGED_RUNTIME").unwrap());
        let flag = Arc::new(AtomicBool::new(false));
        let worker_flag = flag.clone();
        let before = VERIFIED_CHUNKS.load(Ordering::Acquire);
        let worker = std::thread::spawn(move || prepare(&root, &worker_flag));
        let deadline = Instant::now() + Duration::from_secs(5);
        while VERIFIED_CHUNKS.load(Ordering::Acquire) <= before {
            assert!(Instant::now() < deadline, "No dependency bytes were read");
            std::thread::yield_now();
        }
        let start = Instant::now();
        flag.store(true, Ordering::Release);
        let result = worker.join().unwrap();
        assert!(matches!(result, Err(ref error) if error.contains("cancelled")));
        assert!(start.elapsed() < Duration::from_secs(2));
        assert!(VERIFIED_CHUNKS.load(Ordering::Acquire) - before < 100);
    }
}
