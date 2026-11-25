use super::{PackageError, PackageRelease, Progress, Result, cancelled, open_regular};
use cap_std::{ambient_authority, fs::Dir};
use flate2::bufread::GzDecoder;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{File, OpenOptions},
    io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::Path,
    sync::{atomic::AtomicBool, mpsc::SyncSender},
};

const MAX_ARCHIVE: u64 = 64 * 1024 * 1024;
const MAX_EXPANDED: u64 = 64 * 1024 * 1024;
const MAX_ENTRY: u64 = 8 * 1024 * 1024;
pub(super) const LICENSE: &str = "notices/LICENSE";

pub(crate) const MAX_OWNED_FILE: u64 = MAX_ENTRY;

pub(crate) type Inventory = BTreeMap<String, String>;

pub(super) fn stage(
    release: &PackageRelease,
    archive: &Path,
    target: &Path,
    cancellation: &AtomicBool,
    progress: &SyncSender<Progress>,
) -> Result<()> {
    let commands = release
        .commands
        .values()
        .map(|name| format!("bin/{name}"))
        .collect();
    let inventory = stage_archive(
        ArchiveSpec {
            sha256: &release.sha256,
            binary_sha256: &release.binary_sha256,
            platform: &release.platform,
            architecture: &release.architecture,
            commands,
        },
        archive,
        target,
        cancellation,
        Some(progress),
    )?;
    if inventory.get(LICENSE) != Some(&release.license_sha256) {
        return Err(PackageError::Invalid(
            "License digest does not match the pinned release".into(),
        ));
    }
    Ok(())
}

pub(crate) struct ArchiveSpec<'a> {
    pub sha256: &'a str,
    pub binary_sha256: &'a str,
    pub platform: &'a str,
    pub architecture: &'a str,
    pub commands: BTreeSet<String>,
}

pub(crate) fn stage_archive(
    spec: ArchiveSpec<'_>,
    archive: &Path,
    target: &Path,
    cancellation: &AtomicBool,
    progress: Option<&SyncSender<Progress>>,
) -> Result<Inventory> {
    let parent = archive
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let dir = Dir::open_ambient_dir(parent, ambient_authority())?;
    let name = archive
        .file_name()
        .ok_or_else(|| PackageError::Invalid("Invalid package archive filename".into()))?;
    let source = open_regular(&dir, name)?;
    let mut private_copy = tempfile::tempfile_in(target)?;
    let mut reader = source.take(MAX_ARCHIVE + 1);
    if let Some(progress) = progress {
        let _ = progress.try_send(Progress::Verifying);
    }
    let (actual, size) = copy_and_hash(&mut reader, &mut private_copy, cancellation)?;
    if size > MAX_ARCHIVE {
        return Err(PackageError::Invalid(
            "Package archive exceeds the compressed size limit".into(),
        ));
    }
    if actual != spec.sha256 {
        return Err(PackageError::Invalid(
            "Package archive digest does not match the selected release".into(),
        ));
    }
    private_copy.seek(SeekFrom::Start(0))?;
    let mut decoder = GzDecoder::new(BufReader::new(private_copy));
    let reader = (&mut decoder).take(MAX_EXPANDED + 1);
    let mut allowed = spec.commands.clone();
    allowed.insert(LICENSE.into());
    if let Some(progress) = progress {
        let _ = progress.try_send(Progress::Extracting);
    }
    let inventory = extract(reader, target, allowed, &spec, cancellation)?;
    if !decoder.into_inner().fill_buf()?.is_empty() {
        return Err(PackageError::Invalid(
            "Unexpected data after gzip archive".into(),
        ));
    }
    cancelled(cancellation)?;
    Ok(inventory)
}

fn extract(
    reader: impl Read,
    target: &Path,
    mut allowed: BTreeSet<String>,
    spec: &ArchiveSpec<'_>,
    cancellation: &AtomicBool,
) -> Result<Inventory> {
    let mut archive = tar::Archive::new(reader);
    let mut inventory = Inventory::new();
    let mut bytes = 0_u64;
    std::fs::create_dir(target.join("bin"))?;
    std::fs::create_dir(target.join("notices"))?;
    for entry in archive.entries()?.raw(true) {
        cancelled(cancellation)?;
        let mut entry = entry?;
        let raw_path = entry.path_bytes();
        let path = std::str::from_utf8(&raw_path)
            .map_err(|_| PackageError::Invalid("Invalid archive entry name".into()))?
            .to_owned();
        if !entry.header().entry_type().is_file() || !safe_path(&path) || !allowed.remove(&path) {
            return Err(PackageError::Invalid(format!(
                "Unexpected package entry {path:?}"
            )));
        }
        let size = entry.size();
        bytes = bytes.saturating_add(size);
        if size == 0 || size > MAX_ENTRY || bytes > MAX_EXPANDED {
            return Err(PackageError::Invalid(
                "Package entry exceeds its size limit".into(),
            ));
        }
        let file = target.join(&path);
        let mut output = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&file)?;
        let (digest, copied) = copy_and_hash(&mut entry, &mut output, cancellation)?;
        if copied != size {
            return Err(PackageError::Invalid("Truncated package entry".into()));
        }
        if spec.commands.contains(&path) {
            if digest != spec.binary_sha256
                || !native_binary(&file, spec.platform, spec.architecture)?
            {
                return Err(PackageError::Invalid(format!(
                    "Invalid executable platform or architecture for {path:?}"
                )));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                output.set_permissions(std::fs::Permissions::from_mode(0o755))?;
            }
        }
        output.sync_all()?;
        inventory.insert(path, digest);
    }
    if !allowed.is_empty() {
        return Err(PackageError::Invalid(
            "Package archive is missing required entries".into(),
        ));
    }
    // Validate the gzip trailer and refuse concatenated tar contents after the end marker.
    let mut reader = archive.into_inner();
    let mut padding = 0;
    let mut buffer = [0_u8; 4096];
    loop {
        cancelled(cancellation)?;
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        padding += count;
        if padding > 10_240 || buffer[..count].iter().any(|&byte| byte != 0) {
            return Err(PackageError::Invalid(
                "Unexpected content after package archive".into(),
            ));
        }
    }
    Ok(inventory)
}

pub(crate) fn safe_path(path: &str) -> bool {
    let Some((directory, name)) = path.split_once('/') else {
        return false;
    };
    if directory == "notices" {
        return path == LICENSE;
    }
    directory == "bin"
        && !name.is_empty()
        && name.len() <= 64
        && name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
        })
        && !name.starts_with('.')
        && !name.ends_with('.')
        && !matches!(
            name.split('.').next().unwrap_or_default(),
            "aux"
                | "con"
                | "nul"
                | "prn"
                | "com1"
                | "com2"
                | "com3"
                | "com4"
                | "com5"
                | "com6"
                | "com7"
                | "com8"
                | "com9"
                | "lpt1"
                | "lpt2"
                | "lpt3"
                | "lpt4"
                | "lpt5"
                | "lpt6"
                | "lpt7"
                | "lpt8"
                | "lpt9"
        )
}

fn native_binary(path: &Path, platform: &str, architecture: &str) -> io::Result<bool> {
    let mut file = File::open(path)?;
    let mut header = [0; 64];
    file.read_exact(&mut header)?;
    if architecture != "x86_64" {
        return Ok(false);
    }
    match platform {
        "linux" => Ok(header[..6] == *b"\x7fELF\x02\x01" && header[18..20] == [0x3e, 0]),
        "windows" if header[..2] == *b"MZ" => {
            let offset = u32::from_le_bytes(header[60..64].try_into().unwrap());
            if !(64..=4096).contains(&offset) {
                return Ok(false);
            }
            let mut pe = [0; 26];
            file.seek(SeekFrom::Start(offset.into()))?;
            file.read_exact(&mut pe)?;
            Ok(pe[..6] == *b"PE\0\0\x64\x86" && pe[24..26] == [0x0b, 0x02])
        }
        _ => Ok(false),
    }
}

pub(crate) fn copy_and_hash(
    source: &mut impl Read,
    output: &mut impl Write,
    cancellation: &AtomicBool,
) -> Result<(String, u64)> {
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut size = 0;
    loop {
        cancelled(cancellation)?;
        let count = source.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
        output.write_all(&buffer[..count])?;
        size += count as u64;
    }
    let hex = digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok((hex, size))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_paths_links_special_entries_and_oversized_records() {
        let release = PackageIndex::pinned().available().next();
        let Some(release) = release else { return };
        for (path, kind, size) in [
            (b"bin/../../outside".as_slice(), tar::EntryType::Regular, 1),
            (b"/tmp/outside", tar::EntryType::Regular, 1),
            (b"bin/tool", tar::EntryType::Symlink, 0),
            (b"bin/tool", tar::EntryType::Link, 0),
            (b"bin/tool", tar::EntryType::Fifo, 0),
            (b"bin/tool", tar::EntryType::XGlobalHeader, 1),
            (b"bin/tool", tar::EntryType::Regular, MAX_ENTRY + 1),
        ] {
            let mut header = tar::Header::new_ustar();
            header.as_mut_bytes()[..path.len()].copy_from_slice(path);
            header.set_entry_type(kind);
            header.set_size(size);
            header.set_cksum();
            let root = tempfile::tempdir().unwrap();
            let stage = tempfile::tempdir_in(root.path()).unwrap();
            let commands = BTreeSet::from(["bin/tool".into()]);
            let error = extract(
                header.as_bytes().as_slice(),
                stage.path(),
                commands.clone(),
                &ArchiveSpec {
                    sha256: &release.sha256,
                    binary_sha256: &release.binary_sha256,
                    platform: &release.platform,
                    architecture: &release.architecture,
                    commands,
                },
                &AtomicBool::new(false),
            )
            .unwrap_err();
            assert!(matches!(error, PackageError::Invalid(_)), "{error}");
            assert!(!root.path().join("outside").exists());
            assert!(!stage.path().join("bin/tool").exists());
        }
    }

    #[test]
    fn executable_header_must_match_architecture() {
        let Some(release) = PackageIndex::pinned()
            .available()
            .find(|item| item.platform == "linux")
        else {
            return;
        };
        let mut header = [0_u8; 64];
        header[..6].copy_from_slice(b"\x7fELF\x02\x01");
        header[18] = 0xb7; // AArch64.
        let mut input = tempfile::NamedTempFile::new().unwrap();
        input.write_all(&header).unwrap();
        assert!(!native_binary(input.path(), &release.platform, &release.architecture).unwrap());
    }

    use crate::packages::PackageIndex;
}
