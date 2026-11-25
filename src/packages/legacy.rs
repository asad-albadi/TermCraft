//! Inspected original ZIP containers, held separately from runnable packages.
//! Metadata below is generated locally, not an original vendor manifest.
use super::{PackageError, Result, cancelled, open_regular};
use cap_fs_ext::DirExt;
use cap_std::{ambient_authority, fs::Dir};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{self, BufRead, Cursor, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

const MAX_ARCHIVE: u64 = 128 * 1024 * 1024;
const MAX_FILE: usize = 16 * 1024 * 1024;
const MAX_TOTAL: usize = 128 * 1024 * 1024;
const MAX_ENTRIES: usize = 4096;
const MAX_DICTIONARY: u32 = 16 * 1024 * 1024;

fn invalid(message: impl Into<String>) -> PackageError {
    PackageError::Invalid(message.into())
}
fn zip_error(error: impl std::fmt::Display) -> PackageError {
    invalid(format!("Cannot read package archive: {error}"))
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LegacyFile {
    pub path: String,
    pub encoded_bytes: u64,
    pub decoded_bytes: u64,
    pub sha256: String,
    pub encoding: String,
    pub format: String,
    pub architecture: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LegacyMetadata {
    pub schema: String,
    pub provenance: String,
    pub source_name: String,
    pub source_sha256: String,
    pub container: String,
    pub execution_enabled: bool,
    pub runtime_status: String,
    pub files: Vec<LegacyFile>,
}

/// Holds the reviewed bytes privately; importing does not reopen a mutable source.
pub struct ReviewedArchive {
    metadata: LegacyMetadata,
    scratch: tempfile::TempDir,
}

impl ReviewedArchive {
    pub fn metadata(&self) -> &LegacyMetadata {
        &self.metadata
    }

    pub fn inspect(source: &Path, cancel: &AtomicBool) -> Result<Self> {
        cancelled(cancel)?;
        let parent = source
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let dir = Dir::open_ambient_dir(parent, ambient_authority())?;
        let source_name = source
            .file_name()
            .ok_or_else(|| invalid("Choose a package archive"))?;
        let mut source = open_regular(&dir, source_name)?;
        if source.metadata()?.len() > MAX_ARCHIVE {
            return Err(invalid("Package archive is too large"));
        }
        let scratch = tempfile::Builder::new()
            .prefix("termcraft-legacy-review-")
            .tempdir()?;
        let mut snapshot = tempfile::tempfile_in(scratch.path())?;
        let (source_sha256, copied) =
            copy_bounded(&mut source, &mut snapshot, MAX_ARCHIVE, cancel)?;
        if copied == 0 {
            return Err(invalid("Package archive is empty"));
        }
        preflight(&mut snapshot)?;
        snapshot.seek(SeekFrom::Start(0))?;
        let mut archive = zip::ZipArchive::new(CheckedArchive {
            file: snapshot,
            cancel,
        })
        .map_err(zip_error)?;
        archive.validate_header_consistency().map_err(zip_error)?;
        if archive.offset() != 0
            || archive.len() > MAX_ENTRIES
            || archive.has_overlapping_files().map_err(zip_error)?
        {
            return Err(invalid("Unsupported or overlapping package archive"));
        }
        let output = Dir::open_ambient_dir(scratch.path(), ambient_authority())?;
        output.create_dir("payload")?;
        let output = output.open_dir_nofollow("payload")?;
        let mut names = BTreeSet::new();
        let mut components = BTreeMap::new();
        let mut files_seen = BTreeSet::new();
        let mut total = 0usize;
        let mut encoded_total = 0u64;
        let mut files = Vec::new();
        for index in 0..archive.len() {
            cancelled(cancel)?;
            let mut entry = archive.by_index(index).map_err(zip_error)?;
            let name = std::str::from_utf8(entry.name_raw())
                .map_err(|_| invalid("Package filenames must be UTF-8"))?
                .to_owned();
            let is_dir = entry.is_dir();
            validate_path(&name, is_dir)?;
            let name = name.trim_end_matches('/').to_owned();
            if !names.insert(name.to_ascii_lowercase()) {
                return Err(invalid("Package contains duplicate filenames"));
            }
            let mut prefix = String::new();
            for component in name.split('/') {
                if !prefix.is_empty() {
                    prefix.push('/');
                }
                prefix.push_str(component);
                let folded = prefix.to_ascii_lowercase();
                if components
                    .insert(folded.clone(), prefix.clone())
                    .is_some_and(|previous| previous != prefix)
                    || (prefix != name && files_seen.contains(&folded))
                {
                    return Err(invalid("Package filenames conflict"));
                }
            }
            if !is_dir {
                let folded = name.to_ascii_lowercase();
                if components
                    .keys()
                    .any(|p| p.starts_with(&(folded.clone() + "/")))
                {
                    return Err(invalid("Package file conflicts with a directory"));
                }
                files_seen.insert(folded);
            }
            let kind = entry
                .unix_mode()
                .unwrap_or(if is_dir { 0o040755 } else { 0o100644 })
                & 0o170000;
            if entry.encrypted() || (kind != 0 && kind != if is_dir { 0o040000 } else { 0o100000 })
            {
                return Err(invalid(
                    "Package contains a link, special file or encrypted entry",
                ));
            }
            if is_dir {
                if entry.size() != 0 {
                    return Err(invalid("Package directory contains unexpected data"));
                }
                let mut check = [0u8; 1];
                if entry.read(&mut check).map_err(zip_error)? != 0 {
                    return Err(invalid("Invalid package directory"));
                }
                output.create_dir_all(&name)?;
                continue;
            }
            // ZIP sizes describe the ZIP output, which may itself be LZMA data.
            let encoded_bytes = entry.size();
            encoded_total = encoded_total.saturating_add(encoded_bytes);
            if encoded_bytes > MAX_FILE as u64 || encoded_total > MAX_ARCHIVE {
                return Err(invalid("Encoded package contents exceed the size limit"));
            }
            let mut encoded = Vec::new();
            copy_bounded(&mut entry, &mut encoded, MAX_FILE as u64, cancel)?; // reads through EOF to validate ZIP CRC
            if encoded.len() as u64 != encoded_bytes {
                return Err(invalid("Truncated package member"));
            }
            let (decoded, encoding) = decode_member(encoded, cancel)?;
            total = total
                .checked_add(decoded.len())
                .ok_or_else(|| invalid("Package is too large"))?;
            if total > MAX_TOTAL {
                return Err(invalid("Decoded package contents exceed the size limit"));
            }
            let (format, architecture) = identify(&decoded);
            if let Some(parent) = Path::new(&name)
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
            {
                output.create_dir_all(parent)?;
            }
            let mut options = cap_std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            let mut file = output.open_with(&name, &options)?;
            file.write_all(&decoded)?;
            file.sync_all()?;
            files.push(LegacyFile {
                path: name,
                encoded_bytes,
                decoded_bytes: decoded.len() as u64,
                sha256: digest(&decoded),
                encoding,
                format,
                architecture,
            });
        }
        if files.is_empty() {
            return Err(invalid("Package contains no files"));
        }
        cancelled(cancel)?;
        Ok(Self { scratch, metadata: LegacyMetadata {
            schema: "termcraft.local-legacy-import.v1".into(),
            provenance: "Local inspection of a user-selected archive; publisher and dependencies are not authenticated".into(),
            source_name: source_name.to_string_lossy().into_owned(), source_sha256,
            container: "ZIP".into(), execution_enabled: false,
            runtime_status: "Commands are disabled. A supported Windows/Cygwin runtime and verified dependencies are required; this import does not provide them.".into(),
            files,
        } })
    }

    /// Copy verified payload into an owned quarantine. No PATH or plugin registration.
    pub fn import(&self, root: &Path, cancel: &AtomicBool) -> Result<PathBuf> {
        cancelled(cancel)?;
        let parent = root
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let name = root
            .file_name()
            .ok_or_else(|| invalid("Invalid local package folder"))?;
        let parent = Dir::open_ambient_dir(parent, ambient_authority())?;
        #[cfg(unix)]
        let builder = {
            use cap_std::fs::DirBuilderExt;
            let mut builder = cap_std::fs::DirBuilder::new();
            builder.mode(0o700);
            builder
        };
        #[cfg(not(unix))]
        let builder = cap_std::fs::DirBuilder::new();
        match parent.create_dir_with(name, &builder) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
        let root_dir = parent.open_dir_nofollow(name)?;
        let final_name = format!("archive-{}", self.metadata.source_sha256);
        if root_dir.symlink_metadata(&final_name).is_ok() {
            return Err(invalid(
                "This archive is already imported, or its folder is occupied",
            ));
        }
        let mut nonce = [0u8; 16];
        getrandom::fill(&mut nonce).map_err(|e| io::Error::other(e.to_string()))?;
        let stage_name = format!(".import-{}", digest(&nonce));
        root_dir.create_dir(&stage_name)?;
        let result = (|| {
            let stage = root_dir.open_dir_nofollow(&stage_name)?;
            stage.create_dir("payload")?;
            let payload = stage.open_dir_nofollow("payload")?;
            let source = Dir::open_ambient_dir(self.scratch.path(), ambient_authority())?
                .open_dir_nofollow("payload")?;
            for file in &self.metadata.files {
                cancelled(cancel)?;
                if let Some(parent) = Path::new(&file.path)
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                {
                    payload.create_dir_all(parent)?;
                }
                let mut options = cap_std::fs::OpenOptions::new();
                options.write(true).create_new(true);
                let mut output = payload.open_with(&file.path, &options)?;
                let (sha, size) = copy_bounded(
                    &mut open_regular(&source, &file.path)?,
                    &mut output,
                    MAX_FILE as u64,
                    cancel,
                )?;
                if sha != file.sha256 || size != file.decoded_bytes {
                    return Err(invalid(
                        "Reviewed package files changed. Inspect the archive again.",
                    ));
                }
                output.sync_all()?;
            }
            let mut options = cap_std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            let mut metadata = stage.open_with("local-import.json", &options)?;
            serde_json::to_writer_pretty(&mut metadata, &self.metadata)
                .map_err(io::Error::other)?;
            metadata.sync_all()?;
            cancelled(cancel)?;
            // An exclusive root lock closes the concurrent-import existence/rename race.
            let mut options = cap_std::fs::OpenOptions::new();
            options.read(true).write(true).create(true);
            use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt};
            options.follow(FollowSymlinks::No);
            #[cfg(unix)]
            {
                use cap_std::fs::OpenOptionsExt;
                options.custom_flags(libc::O_NONBLOCK);
            }
            let lock = root_dir.open_with("import.lock", &options)?.into_std();
            if !lock.metadata()?.is_file() {
                return Err(invalid("Invalid local package lock"));
            }
            lock.try_lock().map_err(super::lock_error)?;
            if root_dir.symlink_metadata(&final_name).is_ok() {
                return Err(invalid(
                    "This archive is already imported, or its folder is occupied",
                ));
            }
            cancelled(cancel)?;
            root_dir.rename(&stage_name, &root_dir, &final_name)?;
            Ok(root.join(&final_name))
        })();
        if result.is_err() {
            let _ = root_dir.remove_dir_all(&stage_name);
        }
        result
    }
}

fn copy_bounded(
    reader: &mut impl Read,
    writer: &mut impl Write,
    limit: u64,
    cancel: &AtomicBool,
) -> Result<(String, u64)> {
    let mut hash = Sha256::new();
    let mut count = 0u64;
    let mut buffer = [0u8; 32 * 1024];
    loop {
        cancelled(cancel)?;
        let size = reader.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        count = count.saturating_add(size as u64);
        if count > limit {
            return Err(invalid("Package contents exceed the size limit"));
        }
        writer.write_all(&buffer[..size])?;
        hash.update(&buffer[..size]);
    }
    Ok((
        hash.finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        count,
    ))
}

fn preflight(file: &mut File) -> Result<()> {
    // These reference containers are ordinary single-disk ZIP, without ZIP64.
    // Bound the central directory before asking the ZIP parser to allocate it.
    let len = file.metadata()?.len();
    let tail_len = len.min(65557) as usize;
    file.seek(SeekFrom::End(-(tail_len as i64)))?;
    let mut tail = vec![0; tail_len];
    file.read_exact(&mut tail)?;
    let at = (0..tail.len().saturating_sub(21))
        .rev()
        .find(|&i| {
            tail[i..].starts_with(b"PK\x05\x06")
                && i + 22 + u16::from_le_bytes([tail[i + 20], tail[i + 21]]) as usize == tail.len()
        })
        .ok_or_else(|| invalid("Package is not a complete ordinary ZIP archive"))?;
    let u16at = |n| u16::from_le_bytes([tail[at + n], tail[at + n + 1]]);
    let u32at = |n| u32::from_le_bytes(tail[at + n..at + n + 4].try_into().unwrap());
    let entries = u16at(10) as usize;
    if u16at(4) != 0
        || u16at(6) != 0
        || u16at(8) as usize != entries
        || entries == 0
        || entries > MAX_ENTRIES
        || u32at(12) > 4 * 1024 * 1024
        || u32at(16) as u64 + u32at(12) as u64 != len - tail_len as u64 + at as u64
    {
        return Err(invalid("Unsupported or oversized ZIP directory"));
    }
    let directory_offset = u32at(16) as u64;
    let directory_end = directory_offset + u32at(12) as u64;
    file.seek(SeekFrom::Start(directory_offset))?;
    let mut names = BTreeSet::new();
    for _ in 0..entries {
        let mut header = [0u8; 46];
        file.read_exact(&mut header)?;
        if !header.starts_with(b"PK\x01\x02") {
            return Err(invalid("Invalid ZIP directory entry"));
        }
        let name_len = u16::from_le_bytes([header[28], header[29]]) as usize;
        let extra_len = u16::from_le_bytes([header[30], header[31]]) as usize;
        let comment_len = u16::from_le_bytes([header[32], header[33]]) as usize;
        if name_len == 0 || name_len > 513 {
            return Err(invalid("Invalid ZIP filename length"));
        }
        let mut name = vec![0; name_len];
        file.read_exact(&mut name)?;
        let name = std::str::from_utf8(&name).map_err(|_| invalid("Invalid ZIP filename"))?;
        validate_path(name, name.ends_with('/'))?;
        if !names.insert(name.trim_end_matches('/').to_ascii_lowercase()) {
            return Err(invalid("Package contains duplicate filenames"));
        }
        file.seek(SeekFrom::Current((extra_len + comment_len) as i64))?;
        if file.stream_position()? > directory_end {
            return Err(invalid("Invalid ZIP directory lengths"));
        }
    }
    if file.stream_position()? != directory_end {
        return Err(invalid("Unexpected ZIP directory contents"));
    }
    Ok(())
}

fn validate_path(name: &str, directory: bool) -> Result<()> {
    let path = if directory {
        name.strip_suffix('/').unwrap_or(name)
    } else {
        name
    };
    if path.is_empty()
        || path.len() > 512
        || !path.is_ascii()
        || path.split('/').any(|part| {
            let stem = part.split('.').next().unwrap_or("").to_ascii_lowercase();
            part.is_empty()
                || part == "."
                || part == ".."
                || part.ends_with(['.', ' '])
                || part.len() > 128
                || part
                    .bytes()
                    .any(|b| b < 32 || b == 127 || b"\\:*?\"<>|".contains(&b))
                || matches!(
                    stem.as_str(),
                    "con" | "prn" | "aux" | "nul" | "conin$" | "conout$"
                )
                || (stem.len() == 4
                    && (stem.starts_with("com") || stem.starts_with("lpt"))
                    && matches!(stem.as_bytes()[3], b'1'..=b'9'))
        })
    {
        return Err(invalid("Package contains an unsafe filename"));
    }
    Ok(())
}

fn decode_member(encoded: Vec<u8>, cancel: &AtomicBool) -> Result<(Vec<u8>, String)> {
    // LZMA-alone has no magic. Recognize its constrained dictionary or size
    // header; a recognized but malformed stream must never fall back to raw.
    if encoded.len() >= 5 && encoded.len() < 13 {
        let dict = u32::from_le_bytes(encoded[1..5].try_into().unwrap());
        if dict >= 4096
            && (dict.is_power_of_two() || (dict % 3 == 0 && (dict / 3).is_power_of_two()))
        {
            return Err(invalid("Truncated LZMA package member header"));
        }
    }
    let candidate = if encoded.len() >= 13 {
        let dict = u32::from_le_bytes(encoded[1..5].try_into().unwrap());
        let size = u64::from_le_bytes(encoded[5..13].try_into().unwrap());
        let canonical_dict = dict >= 4096
            && (dict.is_power_of_two() || (dict % 3 == 0 && (dict / 3).is_power_of_two()));
        canonical_dict || size <= MAX_FILE as u64 || size == u64::MAX
    } else {
        false
    };
    if !candidate {
        return Ok((encoded, "ZIP member".into()));
    }
    let props = encoded[0] as u32;
    let dict = u32::from_le_bytes(encoded[1..5].try_into().unwrap());
    let size = u64::from_le_bytes(encoded[5..13].try_into().unwrap());
    if props >= 225
        || props % 9 + (props / 9) % 5 > 8
        || dict > MAX_DICTIONARY
        || (size != u64::MAX && size > MAX_FILE as u64)
    {
        return Err(invalid(
            "LZMA package member exceeds supported decoding limits",
        ));
    }
    let mut input = CheckedInput {
        cursor: Cursor::new(encoded.as_slice()),
        cancel,
    };
    let mut output = LimitedOutput {
        bytes: Vec::new(),
        cancel,
    };
    let options = lzma_rs::decompress::Options {
        memlimit: Some(MAX_DICTIONARY as usize),
        ..Default::default()
    };
    lzma_rs::lzma_decompress_with_options(&mut input, &mut output, &options)
        .map_err(|e| invalid(format!("Cannot decode LZMA package member: {e}")))?;
    cancelled(cancel)?;
    if input.cursor.position() != encoded.len() as u64 {
        return Err(invalid("Unexpected data after LZMA package member"));
    }
    Ok((output.bytes, "ZIP member containing LZMA-alone".into()))
}

struct CheckedArchive<'a> {
    file: File,
    cancel: &'a AtomicBool,
}
impl Read for CheckedArchive<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        cancelled(self.cancel).map_err(io::Error::other)?;
        self.file.read(bytes)
    }
}
impl Seek for CheckedArchive<'_> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        cancelled(self.cancel).map_err(io::Error::other)?;
        self.file.seek(position)
    }
}

struct CheckedInput<'a> {
    cursor: Cursor<&'a [u8]>,
    cancel: &'a AtomicBool,
}
impl Read for CheckedInput<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        cancelled(self.cancel).map_err(io::Error::other)?;
        self.cursor.read(bytes)
    }
}
impl BufRead for CheckedInput<'_> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        cancelled(self.cancel).map_err(io::Error::other)?;
        self.cursor.fill_buf()
    }
    fn consume(&mut self, amount: usize) {
        self.cursor.consume(amount);
    }
}

struct LimitedOutput<'a> {
    bytes: Vec<u8>,
    cancel: &'a AtomicBool,
}
impl Write for LimitedOutput<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        cancelled(self.cancel).map_err(io::Error::other)?;
        if bytes.len() > MAX_FILE.saturating_sub(self.bytes.len()) {
            return Err(io::Error::other("Decoded package member is too large"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn identify(bytes: &[u8]) -> (String, Option<String>) {
    if bytes.starts_with(b"MZ") && bytes.len() >= 64 {
        let offset = u32::from_le_bytes(bytes[60..64].try_into().unwrap()) as usize;
        if let Some(header) = bytes
            .get(offset..offset.saturating_add(6))
            .filter(|h| h.starts_with(b"PE\0\0"))
        {
            let architecture = match u16::from_le_bytes([header[4], header[5]]) {
                0x14c => Some("x86".into()),
                0x8664 => Some("x86_64".into()),
                0xaa64 => Some("aarch64".into()),
                _ => None,
            };
            return ("Windows PE".into(), architecture);
        }
    }
    if bytes.starts_with(b"\x1f\x8b") {
        ("gzip data".into(), None)
    } else if std::str::from_utf8(bytes).is_ok() && !bytes.contains(&0) {
        ("text".into(), None)
    } else {
        ("data".into(), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::{ZipWriter, write::SimpleFileOptions};

    fn archive(entries: &[(&str, Vec<u8>)]) -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut zip = ZipWriter::new(file.reopen().unwrap());
        for (name, bytes) in entries {
            zip.start_file(
                *name,
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
        file
    }

    #[test]
    fn legacy_plain_zip_without_custom_manifest_is_reviewable() {
        let file = archive(&[("usr/share/doc/README", b"vendor documentation".to_vec())]);
        let review = ReviewedArchive::inspect(file.path(), &AtomicBool::new(false)).unwrap();
        assert_eq!(
            std::fs::read(review.scratch.path().join("payload/usr/share/doc/README")).unwrap(),
            b"vendor documentation"
        );
    }

    #[test]
    fn legacy_nested_lzma_member_is_decoded() {
        let mut encoded = Vec::new();
        lzma_rs::lzma_compress(&mut Cursor::new(b"decoded companion member"), &mut encoded)
            .unwrap();
        let file = archive(&[("bin/example", encoded)]);
        ReviewedArchive::inspect(file.path(), &AtomicBool::new(false)).unwrap();
    }

    #[test]
    fn legacy_rejects_unsafe_and_colliding_paths() {
        for name in [
            "../escape",
            "/absolute",
            "bin/../escape",
            "bin\\escape",
            "C:/escape",
            "bin/nul.txt",
            "bin/COM9",
            "bin/file.",
            "bin/file ",
            "bin/a:b",
            "bin//file",
        ] {
            let source = archive(&[(name, b"data".to_vec())]);
            assert!(
                ReviewedArchive::inspect(source.path(), &AtomicBool::new(false)).is_err(),
                "accepted {name}"
            );
        }
        for entries in [
            vec![("bin/A", vec![1]), ("bin/a", vec![2])],
            vec![("Bin/a", vec![1]), ("bin/b", vec![2])],
            vec![("bin", vec![1]), ("bin/a", vec![2])],
            vec![("bin/a", vec![1]), ("bin", vec![2])],
        ] {
            let source = archive(&entries);
            assert!(ReviewedArchive::inspect(source.path(), &AtomicBool::new(false)).is_err());
        }
    }

    #[test]
    fn legacy_rejects_duplicate_central_names_before_zip_deduplication() {
        let source = archive(&[("bin/a", vec![1]), ("bin/b", vec![2])]);
        let mut bytes = std::fs::read(source.path()).unwrap();
        for at in 0..bytes.len().saturating_sub(5) {
            if &bytes[at..at + 5] == b"bin/b" {
                bytes[at + 4] = b'a';
            }
        }
        std::fs::write(source.path(), bytes).unwrap();
        assert!(ReviewedArchive::inspect(source.path(), &AtomicBool::new(false)).is_err());
    }

    #[test]
    fn legacy_crc_damage_and_trailing_zip_data_are_rejected() {
        let source = archive(&[("bin/a", b"unique-payload".to_vec())]);
        let original = std::fs::read(source.path()).unwrap();
        let mut damaged = original.clone();
        let at = damaged
            .windows(14)
            .position(|w| w == b"unique-payload")
            .unwrap();
        damaged[at] ^= 1;
        std::fs::write(source.path(), damaged).unwrap();
        assert!(ReviewedArchive::inspect(source.path(), &AtomicBool::new(false)).is_err());
        let mut trailing = original;
        trailing.extend_from_slice(b"hidden data");
        std::fs::write(source.path(), trailing).unwrap();
        assert!(ReviewedArchive::inspect(source.path(), &AtomicBool::new(false)).is_err());
    }

    #[test]
    fn legacy_rejects_lzma_limits_corruption_and_trailing_data_without_raw_fallback() {
        let mut good = Vec::new();
        lzma_rs::lzma_compress(&mut Cursor::new(b"small payload"), &mut good).unwrap();
        let mut cases = Vec::new();
        let mut bytes = good.clone();
        bytes[1..5].copy_from_slice(&(MAX_DICTIONARY + 1).to_le_bytes());
        cases.push(bytes);
        let mut bytes = good.clone();
        bytes[5..13].copy_from_slice(&((MAX_FILE as u64) + 1).to_le_bytes());
        cases.push(bytes);
        let mut bytes = good.clone();
        bytes[0] = 224;
        cases.push(bytes); // lc+lp = 12
        let mut bytes = good.clone();
        bytes[0] = 225;
        cases.push(bytes);
        let mut bytes = good.clone();
        bytes.extend_from_slice(b"trailing");
        cases.push(bytes);
        cases.push(good[..14].to_vec());
        cases.push(good[..12].to_vec());
        for bytes in cases {
            let source = archive(&[("bin/a", bytes)]);
            assert!(ReviewedArchive::inspect(source.path(), &AtomicBool::new(false)).is_err());
        }
    }

    #[test]
    fn legacy_unknown_length_lzma_cannot_expand_past_separate_decoded_quota() {
        let plain = vec![b'a'; MAX_FILE + 1];
        let mut encoded = Vec::new();
        lzma_rs::lzma_compress(&mut Cursor::new(&plain), &mut encoded).unwrap();
        assert!(encoded.len() < MAX_FILE);
        let source = archive(&[("bin/a", encoded)]);
        assert!(ReviewedArchive::inspect(source.path(), &AtomicBool::new(false)).is_err());
    }

    #[test]
    fn legacy_import_preserves_reviewed_bytes_and_does_not_replace_existing_install() {
        let cancel = AtomicBool::new(false);
        let source = archive(&[("bin/a", b"quarantined bytes".to_vec())]);
        let review = ReviewedArchive::inspect(source.path(), &cancel).unwrap();
        std::fs::write(source.path(), b"source replaced after review").unwrap();
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("legacy");
        let imported = review.import(&root, &cancel).unwrap();
        assert_eq!(
            std::fs::read(imported.join("payload/bin/a")).unwrap(),
            b"quarantined bytes"
        );
        let metadata: LegacyMetadata =
            serde_json::from_slice(&std::fs::read(imported.join("local-import.json")).unwrap())
                .unwrap();
        assert_eq!(metadata, *review.metadata());
        assert!(!metadata.execution_enabled);
        assert_eq!(metadata.schema, "termcraft.local-legacy-import.v1");
        assert!(review.import(&root, &cancel).is_err());
        assert_eq!(
            std::fs::read(imported.join("payload/bin/a")).unwrap(),
            b"quarantined bytes"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(imported.join("payload/bin/a"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o111,
                0
            );
            assert_eq!(
                std::fs::metadata(&root).unwrap().permissions().mode() & 0o077,
                0
            );
        }
    }

    #[test]
    fn legacy_changed_review_and_cancellation_leave_no_partial_install() {
        let source = archive(&[("bin/a", b"original".to_vec())]);
        let review = ReviewedArchive::inspect(source.path(), &AtomicBool::new(false)).unwrap();
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("legacy");
        std::fs::write(review.scratch.path().join("payload/bin/a"), b"tampered").unwrap();
        assert!(review.import(&root, &AtomicBool::new(false)).is_err());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
        assert!(matches!(
            review.import(&root, &AtomicBool::new(true)),
            Err(PackageError::Cancelled)
        ));
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn legacy_rejects_archive_symlink_fifo_and_quarantine_symlinks() {
        use std::os::unix::fs::symlink;
        let parent = tempfile::tempdir().unwrap();
        let source = archive(&[("bin/a", b"data".to_vec())]);
        let link = parent.path().join("archive-link");
        symlink(source.path(), &link).unwrap();
        assert!(ReviewedArchive::inspect(&link, &AtomicBool::new(false)).is_err());
        let fifo = parent.path().join("fifo");
        let name = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        assert!(ReviewedArchive::inspect(&fifo, &AtomicBool::new(false)).is_err());
        let review = ReviewedArchive::inspect(source.path(), &AtomicBool::new(false)).unwrap();
        let root = parent.path().join("root-link");
        let outside = tempfile::tempdir().unwrap();
        symlink(outside.path(), &root).unwrap();
        assert!(review.import(&root, &AtomicBool::new(false)).is_err());
        assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
    }

    #[test]
    fn legacy_rejects_zip_symlink_entries() {
        let source = tempfile::NamedTempFile::new().unwrap();
        let mut zip = ZipWriter::new(source.reopen().unwrap());
        zip.add_symlink("bin/link", "../../outside", SimpleFileOptions::default())
            .unwrap();
        zip.finish().unwrap();
        assert!(ReviewedArchive::inspect(source.path(), &AtomicBool::new(false)).is_err());
    }

    #[test]
    fn legacy_encoded_and_source_quotas_reject_before_expansion() {
        let source = archive(&[("bin/a", vec![1])]);
        let mut bytes = std::fs::read(source.path()).unwrap();
        let central = bytes.windows(4).position(|w| w == b"PK\x01\x02").unwrap();
        bytes[central + 24..central + 28].copy_from_slice(&((MAX_FILE + 1) as u32).to_le_bytes());
        std::fs::write(source.path(), bytes).unwrap();
        assert!(ReviewedArchive::inspect(source.path(), &AtomicBool::new(false)).is_err());
        source.as_file().set_len(MAX_ARCHIVE + 1).unwrap();
        assert!(ReviewedArchive::inspect(source.path(), &AtomicBool::new(false)).is_err());
    }

    #[test]
    fn legacy_special_file_attributes_are_rejected() {
        for mode in [0o010644u32, 0o020644, 0o060644, 0o140644] {
            let source = archive(&[("bin/a", vec![1])]);
            let mut bytes = std::fs::read(source.path()).unwrap();
            let central = bytes.windows(4).position(|w| w == b"PK\x01\x02").unwrap();
            bytes[central + 5] = 3; // UNIX creator, so upper external attributes carry file type
            bytes[central + 38..central + 42].copy_from_slice(&(mode << 16).to_le_bytes());
            std::fs::write(source.path(), bytes).unwrap();
            assert!(ReviewedArchive::inspect(source.path(), &AtomicBool::new(false)).is_err());
        }
    }

    #[test]
    fn legacy_copy_checks_cancellation_between_chunks() {
        struct CancelsAfterRead<'a> {
            flag: &'a AtomicBool,
        }
        impl Read for CancelsAfterRead<'_> {
            fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
                output[0] = 1;
                self.flag.store(true, std::sync::atomic::Ordering::Release);
                Ok(1)
            }
        }
        let cancel = AtomicBool::new(false);
        let mut output = Vec::new();
        let result = copy_bounded(
            &mut CancelsAfterRead { flag: &cancel },
            &mut output,
            MAX_ARCHIVE,
            &cancel,
        );
        assert!(matches!(result, Err(PackageError::Cancelled)));
        assert_eq!(output, [1]);
    }

    #[cfg(unix)]
    #[test]
    fn legacy_import_lock_fifo_is_rejected_without_blocking_or_leaving_stage() {
        let source = archive(&[("bin/a", b"data".to_vec())]);
        let review = ReviewedArchive::inspect(source.path(), &AtomicBool::new(false)).unwrap();
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("legacy");
        std::fs::create_dir(&root).unwrap();
        let fifo = root.join("import.lock");
        let name = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        assert!(review.import(&root, &AtomicBool::new(false)).is_err());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    }

    #[test]
    fn legacy_rejects_different_local_and_central_zip_filenames() {
        let source = archive(&[("bin/a", b"payload".to_vec())]);
        let mut bytes = std::fs::read(source.path()).unwrap();
        let at = bytes.windows(5).position(|w| w == b"bin/a").unwrap();
        bytes[at + 4] = b'b'; // Only the local header differs; central name stays bin/a.
        std::fs::write(source.path(), bytes).unwrap();
        assert!(ReviewedArchive::inspect(source.path(), &AtomicBool::new(false)).is_err());
    }

    #[test]
    #[ignore = "requires LEGACY_SQLITE_ARCHIVE, LEGACY_CYGUTILS_ARCHIVE and LEGACY_CYGUTILS64_ARCHIVE supplied paths"]
    fn legacy_actual_reference_archives_import_without_execution() {
        let cancel = AtomicBool::new(false);
        for (key, count) in [
            ("LEGACY_SQLITE_ARCHIVE", 7),
            ("LEGACY_CYGUTILS_ARCHIVE", 974),
            ("LEGACY_CYGUTILS64_ARCHIVE", 356),
        ] {
            let source =
                std::env::var_os(key).expect("Set the explicit original-archive prerequisite");
            let review = ReviewedArchive::inspect(Path::new(&source), &cancel).unwrap();
            assert_eq!(review.metadata.files.len(), count);
            eprintln!(
                "{key}: archive_sha256={} files={} decoded_bytes={}",
                review.metadata.source_sha256,
                review.metadata.files.len(),
                review
                    .metadata
                    .files
                    .iter()
                    .map(|f| f.decoded_bytes)
                    .sum::<u64>()
            );
            if let Some(report_dir) = std::env::var_os("LEGACY_IMPORT_REPORT_DIR") {
                std::fs::create_dir_all(&report_dir).unwrap();
                std::fs::write(
                    Path::new(&report_dir).join(format!("{key}.json")),
                    serde_json::to_vec_pretty(&review.metadata).unwrap(),
                )
                .unwrap();
            }
            let destination = tempfile::tempdir().unwrap();
            let imported = review
                .import(&destination.path().join("quarantine"), &cancel)
                .unwrap();
            for file in &review.metadata.files {
                assert_eq!(
                    digest(&std::fs::read(imported.join("payload").join(&file.path)).unwrap()),
                    file.sha256
                );
            }
            assert!(!review.metadata.execution_enabled);
            if key == "LEGACY_SQLITE_ARCHIVE" {
                assert_eq!(
                    review.metadata.source_sha256,
                    "3bd4979a8b7241b8a27b7020bb2eb08ab950ad15b2dde854c840da465f311c77"
                );
                let sqlite = review
                    .metadata
                    .files
                    .iter()
                    .find(|f| f.path == "bin/sqlite3.exe")
                    .unwrap();
                assert_eq!(sqlite.format, "Windows PE");
                assert_eq!(sqlite.encoding, "ZIP member");
                assert!(
                    imported
                        .join("payload/usr/share/doc/Cygwin/sqlite3.README")
                        .is_file()
                );
            } else if key == "LEGACY_CYGUTILS_ARCHIVE" {
                let greed = review
                    .metadata
                    .files
                    .iter()
                    .find(|f| f.path == "bin/greed.exe")
                    .unwrap();
                assert_eq!(greed.format, "Windows PE");
                assert_eq!(
                    review
                        .metadata
                        .files
                        .iter()
                        .find(|f| f.path == "bin/greed.exe")
                        .unwrap()
                        .sha256,
                    "6bd41af503c2acf433d5c878c3679a07f6376440f1d19e32c08162c8b6275d0d"
                );
                assert_eq!(
                    review
                        .metadata
                        .files
                        .iter()
                        .find(|f| f.path == "bin/sudoku.exe")
                        .unwrap()
                        .sha256,
                    "2bed2565a72468fc5c5b1b706913df4c46e4f2add886cf50f71303025cea909d"
                );
                assert_eq!(
                    review
                        .metadata
                        .files
                        .iter()
                        .find(|f| f.path == "bin/soliterm.exe")
                        .unwrap()
                        .sha256,
                    "61135275b72804d4eac811c7f71e53c9c65c3029acba7e2912a62f98fa1ab610"
                );
                assert_eq!(
                    review
                        .metadata
                        .files
                        .iter()
                        .find(|f| f.path == "bin/termwalk.exe")
                        .unwrap()
                        .sha256,
                    "209ab4832c9e8627b561655e665bee2620d80ae1d219ac3cc89897201c4377c4"
                );
                assert_eq!(
                    review
                        .metadata
                        .files
                        .iter()
                        .find(|f| f.path == "bin/ctris.exe")
                        .unwrap()
                        .sha256,
                    "ab550fbb73e851d8a423fa08b4e77df5313177b4ca3bed09f432007e9504d365"
                );
                assert_eq!(
                    review
                        .metadata
                        .files
                        .iter()
                        .find(|f| f.path == "bin/ninvaders.exe")
                        .unwrap()
                        .sha256,
                    "0afc333055ef30d40923705d264077a1359ea76480d531d6b385cc9fbb33a9d4"
                );
                assert_eq!(
                    review
                        .metadata
                        .files
                        .iter()
                        .find(|f| f.path == "bin/nimesweeper.exe")
                        .unwrap()
                        .sha256,
                    "7bf9da43fa963fd32ac6202f879dbf4f1b3229b8936563b72d0f43a825726b60"
                );
                assert_eq!(greed.decoded_bytes, 15872);
                assert_eq!(
                    greed.sha256,
                    "6bd41af503c2acf433d5c878c3679a07f6376440f1d19e32c08162c8b6275d0d"
                );
                assert_eq!(greed.encoding, "ZIP member containing LZMA-alone");
            }
        }
    }
}
