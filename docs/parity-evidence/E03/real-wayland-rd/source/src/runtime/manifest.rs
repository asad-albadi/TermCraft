use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt, OpenOptionsMaybeDirExt};
use cap_std::{
    ambient_authority,
    fs::{Dir, OpenOptions},
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Component, Path, PathBuf},
};

const MAX_MANIFEST: u64 = 64 * 1024 * 1024;
const MAX_BINARY: u64 = 128 * 1024 * 1024;
const RDP_ENTRY: &str = if cfg!(windows) {
    "termcraft-rdp-engine.exe"
} else {
    "termcraft-rdp-engine"
};

#[derive(Debug, Deserialize)]
pub struct RuntimeManifest {
    pub schema_version: u32,
    pub artifacts: Vec<RuntimeArtifact>,
}

impl RuntimeManifest {
    pub fn pinned() -> Self {
        let manifest: Self =
            serde_json::from_str(include_str!("../../packaging/runtime-manifest.json"))
                .expect("runtime manifest is part of the build inputs");
        assert_eq!(manifest.schema_version, 1);
        manifest
    }
}

#[derive(Debug, Deserialize)]
pub struct RuntimeArtifact {
    pub tool: String,
    pub platform: String,
    pub architecture: String,
    pub path: PathBuf,
    pub version: String,
    pub sha256: String,
    pub archive_url: String,
    pub archive_sha256: String,
    pub source_url: String,
    pub source_sha256: String,
    pub license: String,
    /// Audited interpreter for an installed script, if the artifact is a script.
    pub interpreter: Option<String>,
}

impl RuntimeArtifact {
    pub(super) fn verify(&self, root: &Path) -> Result<VerifiedArtifact, String> {
        target(&self.platform, &self.architecture)?;
        verify(
            root,
            &self.path,
            &self.version,
            &self.sha256,
            self.interpreter.as_deref(),
        )
    }
}

#[derive(Deserialize)]
pub(super) struct PackageManifest {
    schema_version: u32,
    platform: String,
    architecture: String,
    rdp_helper_version: String,
    #[serde(default)]
    wayland_helper_version: Option<String>,
    files: BTreeMap<PathBuf, String>,
}

impl PackageManifest {
    pub(super) fn read(root: &Path) -> Result<Self, String> {
        let bytes = read(
            root,
            Path::new("PACKAGE-MANIFEST.json"),
            MAX_MANIFEST,
            false,
        )?;
        let manifest: Self = serde_json::from_slice(&bytes)
            .map_err(|error| format!("Invalid package runtime manifest: {error}"))?;
        if manifest.schema_version != 1 {
            return Err("Unsupported package runtime manifest schema".into());
        }
        target(&manifest.platform, &manifest.architecture)?;
        Ok(manifest)
    }

    pub(super) fn rdp(&self, bin: &Path) -> Result<VerifiedArtifact, String> {
        let path = Path::new(RDP_ENTRY);
        let manifest_path = if cfg!(target_os = "macos") && bin.ends_with("Contents/MacOS") {
            Path::new("Contents/MacOS").join(path)
        } else {
            path.into()
        };
        let digest = self
            .files
            .get(&manifest_path)
            .ok_or("Package manifest has no RDP helper checksum")?;
        verify(bin, path, &self.rdp_helper_version, digest, None)
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn wayland_command(
    root: &Path,
) -> Result<(std::process::Command, std::fs::File), String> {
    let manifest = PackageManifest::read(root).map_err(|_| {
        "Wayland sharing requires the optional packaged PipeWire helper and package manifest."
            .to_string()
    })?;
    let version = manifest
        .wayland_helper_version
        .as_deref()
        .ok_or("This package does not include the optional Wayland PipeWire helper.")?;
    let path = Path::new("termcraft-wayland-engine");
    let expected = manifest
        .files
        .get(path)
        .ok_or("Package manifest has no Wayland helper checksum.")?;
    let verified = verify(root, path, version, expected, None)?;
    verified.sealed_command()
}

pub(crate) struct VerifiedArtifact {
    pub(crate) path: PathBuf,
    pub(crate) version: String,
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub(super) bytes: Vec<u8>,
    #[cfg(windows)]
    _locked_file: std::fs::File,
}

impl VerifiedArtifact {
    #[cfg(target_os = "linux")]
    pub(crate) fn sealed_command(&self) -> Result<(std::process::Command, std::fs::File), String> {
        use std::io::{self, Seek, Write};
        use std::os::{
            fd::{AsRawFd, FromRawFd},
            unix::{fs::PermissionsExt, process::CommandExt},
        };

        let failure = |error| format!("Could not seal packaged RDP helper: {error}");
        let base = libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING;
        let mut fd =
            unsafe { libc::memfd_create(c"termcraft-rdp-engine".as_ptr(), base | libc::MFD_EXEC) };
        if fd < 0 && io::Error::last_os_error().raw_os_error() == Some(libc::EINVAL) {
            fd = unsafe { libc::memfd_create(c"termcraft-rdp-engine".as_ptr(), base) };
        }
        if fd < 0 {
            return Err(failure(io::Error::last_os_error()));
        }
        // The private descriptor holds the verified image through exec. Its seals also
        // prevent later changes via descriptors to the installed file or this snapshot.
        let mut image = unsafe { std::fs::File::from_raw_fd(fd) };
        image.write_all(&self.bytes).map_err(failure)?;
        image.rewind().map_err(failure)?;
        image
            .set_permissions(std::fs::Permissions::from_mode(0o500))
            .map_err(failure)?;
        let seals =
            libc::F_SEAL_WRITE | libc::F_SEAL_GROW | libc::F_SEAL_SHRINK | libc::F_SEAL_SEAL;
        if unsafe { libc::fcntl(image.as_raw_fd(), libc::F_ADD_SEALS, seals) } < 0 {
            return Err(failure(io::Error::last_os_error()));
        }
        let mut command =
            std::process::Command::new(format!("/proc/self/fd/{}", image.as_raw_fd()));
        command.arg0(&self.path);
        Ok((command, image))
    }
}

#[cfg(any(target_os = "macos", all(target_os = "linux", test)))]
pub(crate) fn private_rdp_snapshot(
    bytes: &[u8],
) -> Result<(std::process::Command, tempfile::TempDir), String> {
    use std::{
        fs::{OpenOptions, Permissions},
        io::Write,
        os::unix::fs::{OpenOptionsExt, PermissionsExt},
    };
    let failure = |error| format!("Could not prepare verified RDP helper snapshot: {error}");
    let directory = tempfile::Builder::new()
        .prefix("termcraft-rdp-")
        .permissions(Permissions::from_mode(0o700))
        .tempdir()
        .map_err(failure)?;
    let path = directory.path().join(RDP_ENTRY);
    let mut image = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o500)
        .open(&path)
        .map_err(failure)?;
    image.write_all(bytes).map_err(failure)?;
    drop(image);
    Ok((std::process::Command::new(path), directory))
}

#[cfg(target_os = "macos")]
impl VerifiedArtifact {
    pub(crate) fn snapshot_command(
        &self,
    ) -> Result<(std::process::Command, tempfile::TempDir), String> {
        private_rdp_snapshot(&self.bytes)
    }
}

fn verify(
    root: &Path,
    relative: &Path,
    version: &str,
    expected: &str,
    interpreter: Option<&str>,
) -> Result<VerifiedArtifact, String> {
    let (bytes, _image) = read_verified_input(root, relative, MAX_BINARY, true)?;
    let digest: String = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if expected != digest {
        return Err(format!(
            "Packaged helper {} checksum mismatch",
            relative.display()
        ));
    }
    if !match interpreter {
        Some(path) => bytes.starts_with(format!("#!{path}\n").as_bytes()),
        None => native_header(&bytes),
    } {
        return Err(format!(
            "Packaged helper {} has an unsupported interpreter or architecture",
            relative.display()
        ));
    }
    Ok(VerifiedArtifact {
        path: root.join(relative),
        version: version.into(),
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        bytes,
        #[cfg(windows)]
        _locked_file: _image,
    })
}

fn target(platform: &str, arch: &str) -> Result<(), String> {
    if platform != std::env::consts::OS || arch != std::env::consts::ARCH {
        return Err(format!(
            "Unsupported packaged runtime platform or architecture: {platform}/{arch}"
        ));
    }
    Ok(())
}

fn read(root: &Path, path: &Path, limit: u64, executable: bool) -> Result<Vec<u8>, String> {
    read_verified_input(root, path, limit, executable).map(|(bytes, _)| bytes)
}

fn read_verified_input(
    root: &Path,
    path: &Path,
    limit: u64,
    executable: bool,
) -> Result<(Vec<u8>, std::fs::File), String> {
    let failure =
        |error: &dyn std::fmt::Display| format!("Packaged runtime {}: {error}", path.display());
    if !root.is_absolute() {
        return Err(failure(&"installation directory must be absolute"));
    }
    let directory = match root.parent().zip(root.file_name()) {
        Some((parent, name)) => Dir::open_ambient_dir(parent, ambient_authority())
            .and_then(|dir| dir.open_dir_nofollow(name)),
        None => Dir::open_ambient_dir(root, ambient_authority()),
    };
    let mut directory = directory.map_err(|error| failure(&error))?;
    let mut parts = path.components().peekable();
    let file = loop {
        let Some(Component::Normal(name)) = parts.next() else {
            return Err(failure(&"invalid relative path"));
        };
        if parts.peek().is_some() {
            directory = directory
                .open_dir_nofollow(name)
                .map_err(|error| failure(&error))?;
        } else {
            let mut options = OpenOptions::new();
            options
                .read(true)
                .maybe_dir(false)
                .follow(FollowSymlinks::No);
            #[cfg(unix)]
            {
                use cap_std::fs::OpenOptionsExt;
                options.custom_flags(libc::O_NONBLOCK);
            }
            #[cfg(windows)]
            {
                use cap_std::fs::OpenOptionsExt;
                use windows_sys::Win32::Storage::FileSystem::{
                    FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
                };
                if executable {
                    options.share_mode(FILE_SHARE_READ);
                    options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
                }
            }
            break directory
                .open_with(name, &options)
                .map_err(|error| failure(&error))?
                .into_std();
        }
    };
    let info = file.metadata().map_err(|error| failure(&error))?;
    if !info.is_file() || info.len() > limit {
        return Err(failure(&"expected a bounded regular file"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if executable && info.permissions().mode() & 0o111 == 0 {
            return Err(failure(&"helper does not have executable permissions"));
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        if info.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(failure(&"expected a non-reparse file"));
        }
    }
    let mut bytes = Vec::new();
    (&file)
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| failure(&error))?;
    if bytes.len() as u64 > limit {
        return Err(failure(&"file exceeds its maximum size"));
    }
    Ok((bytes, file))
}

pub(super) fn native_header(bytes: &[u8]) -> bool {
    let arch = std::env::consts::ARCH;
    if cfg!(target_os = "linux") {
        let expected = match arch {
            "x86_64" => 62,
            "aarch64" => 183,
            _ => return false,
        };
        bytes.starts_with(b"\x7fELF\x02\x01") && bytes.get(18..20) == Some(&[expected, 0])
    } else if cfg!(windows) {
        let expected = match arch {
            "x86_64" => 0x8664_u16,
            "aarch64" => 0xaa64,
            _ => return false,
        };
        let offset = bytes
            .get(60..64)
            .and_then(|v| v.try_into().ok())
            .map(u32::from_le_bytes);
        bytes.starts_with(b"MZ")
            && offset.is_some_and(|offset| {
                let offset = offset as usize;
                bytes.get(offset..).is_some_and(|header| {
                    header.starts_with(b"PE\0\0")
                        && header.get(4..6) == Some(&expected.to_le_bytes())
                })
            })
    } else if cfg!(target_os = "macos") {
        let expected = match arch {
            "x86_64" => 0x01000007_u32,
            "aarch64" => 0x0100000c,
            _ => return false,
        };
        bytes.starts_with(b"\xcf\xfa\xed\xfe") && bytes.get(4..8) == Some(&expected.to_le_bytes())
    } else {
        false
    }
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
#[test]
fn rdp_snapshot_executes_verified_bytes_after_source_replacement() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("engine");
    let trusted = b"#!/bin/sh\nprintf verified-snapshot";
    let checksum: String = Sha256::digest(trusted)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    std::fs::write(&source, trusted).unwrap();
    std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o500)).unwrap();
    let verified = verify(
        root.path(),
        Path::new("engine"),
        "test",
        &checksum,
        Some("/bin/sh"),
    )
    .unwrap();
    std::fs::rename(&source, root.path().join("original")).unwrap();
    std::fs::write(&source, b"#!/bin/sh\nprintf substituted").unwrap();
    std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o500)).unwrap();
    let reopened = std::process::Command::new(&source)
        .env_clear()
        .output()
        .unwrap();
    assert_eq!(reopened.stdout, b"substituted");
    let (mut command, snapshot) = private_rdp_snapshot(&verified.bytes).unwrap();
    let executable = PathBuf::from(command.get_program());
    assert_eq!(
        std::fs::metadata(&executable).unwrap().permissions().mode() & 0o777,
        0o500
    );
    assert_eq!(
        command.env_clear().output().unwrap().stdout,
        b"verified-snapshot"
    );
    let path = snapshot.path().to_path_buf();
    drop(snapshot);
    assert!(!path.exists());
}

#[cfg(all(test, windows))]
#[test]
fn verified_rdp_input_denies_write_and_replacement_until_released() {
    let root = tempfile::tempdir().unwrap();
    let engine = root.path().join("engine.exe");
    let mut system = PathBuf::from(std::env::var_os("SystemRoot").unwrap());
    system.push("System32/cmd.exe");
    std::fs::copy(system, &engine).unwrap();
    let bytes = std::fs::read(&engine).unwrap();
    let checksum: String = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let verified = verify(
        root.path(),
        Path::new("engine.exe"),
        "test",
        &checksum,
        None,
    )
    .unwrap();
    assert!(
        std::fs::OpenOptions::new()
            .write(true)
            .open(&engine)
            .is_err()
    );
    let alternate = root.path().join("alternate.exe");
    let system = PathBuf::from(std::env::var_os("SystemRoot").unwrap());
    std::fs::copy(system.join("System32/where.exe"), &alternate).unwrap();
    let moved = root.path().join("moved.exe");
    assert!(std::fs::rename(&engine, &moved).is_err());
    assert!(std::fs::rename(&alternate, &engine).is_err());
    let outcome = std::process::Command::new(&engine)
        .args(["/D", "/C", "echo verified-engine"])
        .output()
        .unwrap();
    assert!(outcome.status.success());
    assert_eq!(
        String::from_utf8_lossy(&outcome.stdout).trim(),
        "verified-engine"
    );
    drop(verified);
    std::fs::rename(&alternate, &engine).unwrap();
}
