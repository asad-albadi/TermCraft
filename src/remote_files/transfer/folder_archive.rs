//! Bounded, byte-preserving folder payload. No archive paths are resolved through links.
use std::{fs::File, io::Write};
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
#[cfg(any(windows, test))]
fn check(cancelled: &tokio::sync::watch::Receiver<bool>) -> Result<(), String> {
    if *cancelled.borrow() {
        Err("Folder preparation cancelled".into())
    } else {
        Ok(())
    }
}
fn components(path: &[u8]) -> Result<Vec<&[u8]>, String> {
    let parts = path.split(|b| *b == b'/').collect::<Vec<_>>();
    if path.is_empty()
        || path.len() > 32768
        || path.contains(&0)
        || parts.len() > 64
        || parts
            .iter()
            .any(|p| p.is_empty() || *p == b"." || *p == b"..")
    {
        return Err("Unsafe or oversized folder entry path".into());
    }
    Ok(parts)
}
fn link(target: &[u8]) -> Result<(), String> {
    if target.is_empty()
        || target.len() > 32768
        || target[0] == b'/'
        || target.contains(&0)
        || target.contains(&b'\\')
        || target.contains(&b':')
        || target.split(|b| *b == b'/').any(|p| p == b"..")
    {
        return Err("Folder link must stay inside its selected tree".into());
    }
    Ok(())
}
#[derive(Default)]
struct Budget {
    count: usize,
    paths: usize,
}
impl Budget {
    fn add(&mut self, path: &[u8]) -> Result<(), String> {
        components(path)?;
        self.count += 1;
        self.paths = self.paths.saturating_add(path.len());
        if self.count > 10000 || self.paths > 4 * 1024 * 1024 {
            Err("Folder exceeds 10000 entries / 4 MiB path budget".into())
        } else {
            Ok(())
        }
    }
}
fn header(out: &mut File, kind: u8, mode: u32, path: &[u8], size: u64) -> Result<(), String> {
    out.write_all(&[kind]).map_err(error)?;
    out.write_all(&(mode & 0o777).to_be_bytes())
        .map_err(error)?;
    out.write_all(&(path.len() as u32).to_be_bytes())
        .map_err(error)?;
    out.write_all(&size.to_be_bytes()).map_err(error)?;
    out.write_all(path).map_err(error)
}

pub(crate) struct FolderModes {
    pub root: u32,
    pub directories: std::collections::HashMap<Vec<u8>, u32>,
}
#[cfg(unix)]
#[path = "folder_archive/unix.rs"]
mod unix;
#[cfg(unix)]
pub(crate) use unix::{build, extract, finalize};
#[cfg(any(windows, test))]
#[path = "folder_archive/portable.rs"]
mod portable;
#[cfg(windows)]
pub(crate) use portable::{build, extract, finalize};
