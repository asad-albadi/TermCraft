pub mod capture;
mod diff;
mod image;
mod scan;

pub use capture::PacketCapture;
pub use diff::{Change, DiffResult, FileDiff, FolderDiff, FolderDiffResult};
pub use image::ImagePreview;
pub use scan::{PortScan, PortStatus, ScanHandle, ScanReport, ScanTarget};

use std::{fs, io::Read, path::Path};

fn read_regular(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let name = path.display();
    let metadata = fs::symlink_metadata(path).map_err(|error| format!("{name}: {error}"))?;
    if !metadata.is_file() || is_link(&metadata) {
        return Err(format!("{name}: expected a regular file"));
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options
        .open(path)
        .map_err(|error| format!("{name}: {error}"))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("{name}: {error}"))?;
    if !metadata.is_file() || is_link(&metadata) {
        return Err(format!("{name}: expected a regular file"));
    }
    read_bounded(file, metadata.len(), path, limit)
}

fn read_bounded(file: impl Read, length: u64, path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let name = path.display();
    if length > limit {
        return Err(format!(
            "{name}: file exceeds the comparison or preview byte limit"
        ));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("{name}: {error}"))?;
    if bytes.len() as u64 > limit {
        return Err(format!(
            "{name}: file exceeds the comparison or preview byte limit"
        ));
    }
    Ok(bytes)
}

fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // Junctions and other reparse points can redirect a directory walk.
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.is_symlink()
    }
}
