//! Darwin preallocation keeps logical EOF unchanged and uses the opened file.
use cap_std::fs::{File, MetadataExt};
use std::{io, os::fd::AsRawFd};
pub(super) fn allocate(file: &File, size: u64) -> io::Result<()> {
    let metadata = file.metadata()?;
    if size < metadata.len() {
        return file.set_len(size);
    }
    let allocated = metadata.blocks().saturating_mul(512);
    if size <= allocated {
        return Ok(());
    }
    // F_PEOFPOSMODE adds space beyond physical EOF; request only the deficit.
    // F_ALLOCATEALL refuses partial allocation, and deliberately omitting
    // F_ALLOCATEPERSIST lets Darwin release unused preallocation on close.
    let mut store = libc::fstore_t {
        fst_flags: libc::F_ALLOCATEALL,
        fst_posmode: libc::F_PEOFPOSMODE,
        fst_offset: 0,
        fst_length: i64::try_from(size - allocated)
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?,
        fst_bytesalloc: 0,
    };
    if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_PREALLOCATE, &mut store) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_preallocation_keeps_eof_and_retires_on_close() {
        let temp = tempfile::tempdir().unwrap();
        let root =
            cap_std::fs::Dir::open_ambient_dir(temp.path(), cap_std::ambient_authority()).unwrap();
        std::fs::write(temp.path().join("selected"), b"original").unwrap();
        let file = root
            .open_with(
                "selected",
                cap_std::fs::OpenOptions::new().read(true).write(true),
            )
            .unwrap();
        allocate(&file, 65536).unwrap();
        assert_eq!(file.metadata().unwrap().len(), 8);
        let first = file.metadata().unwrap().blocks();
        allocate(&file, 65536).unwrap();
        assert!(file.metadata().unwrap().blocks() >= first);
        drop(file);
        assert_eq!(
            std::fs::read(temp.path().join("selected")).unwrap(),
            b"original"
        );
    }
}
