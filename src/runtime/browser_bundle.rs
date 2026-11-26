//! Verify the complete pinned Chromium resource tree before launch. Distribution
//! shared libraries and namespace sandbox support remain declared host requirements.
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
    #[cfg(windows)]
    #[serde(default)]
    approved: bool,
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

const FAILURE: &str = "The selected bundled Chromium runtime is unavailable or damaged";
#[cfg(windows)]
const PIN: &[u8] = include_bytes!("../../packaging/browser/runtime-manifest-windows.json");
#[cfg(not(windows))]
const PIN: &[u8] = include_bytes!("../../packaging/browser/runtime-manifest.json");

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

#[cfg(windows)]
pub(super) fn prepare_windows(
    resources: &Path,
    snapshot: &Path,
    cancelled: &AtomicBool,
) -> Result<super::ToolProvider, String> {
    verify(resources, cancelled, Some(snapshot))
}

#[cfg(unix)]
pub(super) fn prepare(
    resources: &Path,
    cancelled: &AtomicBool,
) -> Result<(super::ToolProvider, tempfile::TempDir), String> {
    use std::os::unix::fs::PermissionsExt;
    let snapshot = tempfile::Builder::new()
        .prefix("termcraft-browser-runtime-")
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
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
            Err("Browser startup cancelled".to_owned())
        } else {
            Ok(())
        }
    };
    check_cancel()?;
    let info: Manifest = serde_json::from_slice(PIN).map_err(|_| FAILURE)?;
    if info.schema_version != 1
        || info.platform != std::env::consts::OS
        || info.architecture != std::env::consts::ARCH
        || !resources.is_absolute()
    {
        return Err(format!("{FAILURE}: no qualified payload for this platform"));
    }
    #[cfg(windows)]
    if !info.approved {
        return Err(format!("{FAILURE}: Windows artifact approval required"));
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
    let mut installed = file(&directory, Path::new("BROWSER-MANIFEST.json"))?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut installed)
        .take(1024 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|_| FAILURE)?;
    if bytes != PIN {
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
mod tests {
    use super::*;
    #[cfg(all(windows, target_arch = "x86_64"))]
    #[test]
    fn local_manifest_cannot_approve_the_windows_browser() {
        let resources = tempfile::tempdir().unwrap();
        let installed = resources.path().join("browser/windows-x86_64");
        std::fs::create_dir_all(&installed).unwrap();
        std::fs::write(
            installed.join("BROWSER-MANIFEST.json"),
            b"{\"approved\":true}",
        )
        .unwrap();
        let error = resolve(resources.path()).unwrap_err();
        let pin: Manifest = serde_json::from_slice(PIN).unwrap();
        let expected = if pin.approved {
            "manifest mismatch"
        } else {
            "Windows artifact approval required"
        };
        assert!(error.contains(expected), "{error}");
    }

    #[test]
    fn rejects_escaping_or_empty_runtime_paths() {
        for path in [
            "",
            "/tmp/browser",
            "../browser",
            "browser/../engine",
            "./engine",
        ] {
            assert!(relative(Path::new(path)).is_err(), "{path}");
        }
    }
    #[test]
    #[ignore = "requires explicitly staged full Chromium payload"]
    fn complete_bundle_checks_resources_manifests_permissions_and_links() {
        let root = PathBuf::from(std::env::var_os("TERMCRAFT_TEST_BROWSER_ROOT").unwrap());
        resolve(&root).unwrap();
        let resource = root.join("browser/linux-x86_64/locales/en-US.pak");
        let original = std::fs::read(&resource).unwrap();
        std::fs::write(&resource, b"damaged locale").unwrap();
        assert!(
            resolve(&root)
                .unwrap_err()
                .contains("invalid locales/en-US.pak")
        );
        std::fs::write(&resource, &original).unwrap();
        let manifest = root.join("browser/linux-x86_64/BROWSER-MANIFEST.json");
        let pin = std::fs::read(&manifest).unwrap();
        std::fs::write(&manifest, b"{}").unwrap();
        assert!(resolve(&root).unwrap_err().contains("manifest mismatch"));
        std::fs::write(&manifest, pin).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let detached = root.join("detached-locale");
            std::fs::rename(&resource, &detached).unwrap();
            symlink(&detached, &resource).unwrap();
            assert!(resolve(&root).is_err());
            std::fs::remove_file(&resource).unwrap();
            std::fs::rename(detached, &resource).unwrap();
        }
        resolve(&root).unwrap();
    }
}

#[cfg(all(test, target_os = "linux"))]
mod snapshot_qualification {
    use super::*;
    #[test]
    #[ignore = "requires staged full pinned browser payload"]
    fn verified_snapshot_executes_after_source_image_is_replaced() {
        use std::io::{Seek, SeekFrom};
        let root = PathBuf::from(std::env::var_os("TERMCRAFT_TEST_BROWSER_ROOT").unwrap());
        let (provider, snapshot) = prepare(&root, &AtomicBool::new(false)).unwrap();
        let engine = provider.executable().unwrap();
        let source = root.join("browser/linux-x86_64/chromium");
        struct Restore(std::fs::File, u8);
        impl Drop for Restore {
            fn drop(&mut self) {
                self.0.seek(SeekFrom::Start(0)).unwrap();
                self.0.write_all(&[self.1]).unwrap();
            }
        }
        let mut source = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(source)
            .unwrap();
        let mut byte = [0];
        source.read_exact(&mut byte).unwrap();
        let mut restore = Restore(source, byte[0]);
        restore.0.seek(SeekFrom::Start(0)).unwrap();
        restore.0.write_all(b"X").unwrap();
        let output = std::process::Command::new(&engine)
            .arg("--version")
            .env_clear()
            .env("PATH", "")
            .env("HOME", snapshot.path())
            .env("LD_LIBRARY_PATH", snapshot.path().join("libraries"))
            .output()
            .unwrap();
        assert!(output.status.success(), "{:?}", output.status);
        assert!(
            String::from_utf8(output.stdout)
                .unwrap()
                .contains("158.0.8127.88")
        );
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            snapshot.path().metadata().unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            engine.metadata().unwrap().permissions().mode() & 0o777,
            0o500
        );
        drop(restore);
        drop(snapshot);
        assert!(!engine.exists());
    }
}
