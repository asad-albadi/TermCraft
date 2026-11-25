//! Mutations use the already opened source and a pinned destination directory.
//! No operation reconstructs an ambient source or destination pathname.
use cap_std::fs::{Dir, File};
use std::{
    io,
    os::windows::{ffi::OsStrExt, io::AsRawHandle},
    path::Path,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ALLOCATION_INFO, FILE_DISPOSITION_INFO, FILE_ID_INFO, FILE_RENAME_INFO,
    FILE_STANDARD_INFO, FileAllocationInfo, FileDispositionInfo, FileIdInfo, FileRenameInfo,
    FileStandardInfo, GetFileInformationByHandleEx, SetFileInformationByHandle,
};

pub(super) fn allocate(file: &File, size: u64) -> io::Result<()> {
    let allocation = FILE_ALLOCATION_INFO {
        AllocationSize: i64::try_from(size)
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?,
    };
    let ok = unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileAllocationInfo,
            (&allocation as *const FILE_ALLOCATION_INFO).cast(),
            size_of::<FILE_ALLOCATION_INFO>() as u32,
        )
    };
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub(super) fn disposition(file: &File, deleting: bool) -> io::Result<()> {
    let disposition = FILE_DISPOSITION_INFO {
        DeleteFile: deleting,
    };
    let ok = unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileDispositionInfo,
            (&disposition as *const FILE_DISPOSITION_INFO).cast(),
            size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    };
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub(super) fn rename(file: &File, parent: &Dir, name: &Path, replace: bool) -> io::Result<()> {
    // A single validated component makes resolution relative to parent atomic;
    // an intervening symlink or local parent rename cannot escape this handle.
    let name: Vec<u16> = name.as_os_str().encode_wide().collect();
    if name.is_empty()
        || name.len() > 255
        || name
            .last()
            .is_some_and(|unit| [b' ' as u16, b'.' as u16].contains(unit))
        || name
            .iter()
            .any(|unit| [0, b'/' as u16, b'\\' as u16, b':' as u16].contains(unit))
        || name == [b'.' as u16]
        || name == [b'.' as u16, b'.' as u16]
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let bytes = std::mem::offset_of!(FILE_RENAME_INFO, FileName) + (name.len() + 1) * 2;
    // Vec<usize> supplies native alignment for the variable length Windows struct.
    let mut storage = vec![0usize; bytes.div_ceil(size_of::<usize>())];
    let info = storage.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    unsafe {
        (*info).Anonymous.ReplaceIfExists = replace;
        (*info).RootDirectory = parent.as_raw_handle();
        (*info).FileNameLength = (name.len() * 2) as u32;
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            std::ptr::addr_of_mut!((*info).FileName).cast(),
            name.len(),
        );
    }
    let ok = unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileRenameInfo,
            info.cast(),
            bytes as u32,
        )
    };
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub(super) fn allocation_size(file: &File) -> io::Result<u64> {
    let mut standard = FILE_STANDARD_INFO::default();
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileStandardInfo,
            (&mut standard as *mut FILE_STANDARD_INFO).cast(),
            size_of::<FILE_STANDARD_INFO>() as u32,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    u64::try_from(standard.AllocationSize).map_err(|_| io::ErrorKind::InvalidData.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cap_std::fs::{OpenOptions, OpenOptionsExt};
    fn opened(root: &Dir) -> File {
        let mut options = OpenOptions::new();
        options
            .read(true)
            .write(true)
            .create_new(true)
            .access_mode(0xc0010000)
            .share_mode(7);
        root.open_with("selected", &options).unwrap()
    }
    #[test]
    fn native_allocation_rename_and_delete_keep_opened_source_identity() {
        use std::io::Write;
        let temp = tempfile::tempdir().unwrap();
        let root = Dir::open_ambient_dir(temp.path(), cap_std::ambient_authority()).unwrap();
        let mut source = opened(&root);
        source.write_all(b"original").unwrap();
        allocate(&source, 65536).unwrap();
        assert_eq!(source.metadata().unwrap().len(), 8);
        assert!(allocation_size(&source).unwrap() >= 65536);
        std::fs::rename(
            temp.path().join("selected"),
            temp.path().join("moved-locally"),
        )
        .unwrap();
        std::fs::write(temp.path().join("selected"), b"replacement").unwrap();
        rename(&source, &root, Path::new("remote-name"), false).unwrap();
        assert_eq!(
            std::fs::read(temp.path().join("remote-name")).unwrap(),
            b"original"
        );
        assert_eq!(
            std::fs::read(temp.path().join("selected")).unwrap(),
            b"replacement"
        );
        disposition(&source, true).unwrap();
        drop(source);
        assert!(!temp.path().join("remote-name").exists());
        assert_eq!(
            std::fs::read(temp.path().join("selected")).unwrap(),
            b"replacement"
        );
    }
}

pub(super) fn identity(handle: &impl AsRawHandle) -> io::Result<(u64, u128)> {
    let mut identity = FILE_ID_INFO::default();
    let ok = unsafe {
        GetFileInformationByHandleEx(
            handle.as_raw_handle(),
            FileIdInfo,
            (&mut identity as *mut FILE_ID_INFO).cast(),
            size_of::<FILE_ID_INFO>() as u32,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((
        identity.VolumeSerialNumber,
        u128::from_le_bytes(identity.FileId.Identifier),
    ))
}
