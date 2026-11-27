use std::{
    ffi::OsStr,
    io,
    os::windows::{ffi::OsStrExt, io::AsRawHandle},
};
use windows_sys::{
    Wdk::Storage::FileSystem::{
        FILE_RENAME_INFORMATION, FileRenameInformation, NtSetInformationFile,
    },
    Win32::{Foundation::RtlNtStatusToDosError, System::IO::IO_STATUS_BLOCK},
};

pub(crate) fn remove_opened(file: &impl AsRawHandle) -> io::Result<()> {
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_DISPOSITION_FLAG_DELETE, FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
        FILE_DISPOSITION_INFO_EX, FileDispositionInfoEx, SetFileInformationByHandle,
    };
    let information = FILE_DISPOSITION_INFO_EX {
        Flags: FILE_DISPOSITION_FLAG_DELETE | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
    };
    // The kernel applies deletion to this handle and refuses nonempty folders.
    let ok = unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileDispositionInfoEx,
            (&information as *const FILE_DISPOSITION_INFO_EX).cast(),
            size_of::<FILE_DISPOSITION_INFO_EX>() as u32,
        )
    };
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Rename a synchronous handle opened with DELETE access into a held directory.
pub(crate) fn rename(
    file: &impl AsRawHandle,
    parent: &impl AsRawHandle,
    name: &OsStr,
    replace: bool,
) -> io::Result<()> {
    let name: Vec<_> = name.encode_wide().collect();
    if name.is_empty()
        || name.len() > 255
        || name
            .iter()
            .any(|unit| [0, b'/' as u16, b'\\' as u16, b':' as u16].contains(unit))
        || name
            .last()
            .is_some_and(|unit| [b'.' as u16, b' ' as u16].contains(unit))
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }

    let length = size_of::<FILE_RENAME_INFORMATION>() + name.len() * size_of::<u16>();
    let mut buffer = vec![0usize; length.div_ceil(size_of::<usize>())];
    let mut result = IO_STATUS_BLOCK::default();
    // Both handles are live. The aligned allocation includes the variable
    // UTF-16 tail. The native information class honors the directory handle.
    let status = unsafe {
        let information = buffer.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
        (*information).Anonymous.ReplaceIfExists = replace;
        (*information).RootDirectory = parent.as_raw_handle();
        (*information).FileNameLength = (name.len() * size_of::<u16>()) as u32;
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            std::ptr::addr_of_mut!((*information).FileName).cast(),
            name.len(),
        );
        NtSetInformationFile(
            file.as_raw_handle(),
            &mut result,
            information.cast(),
            length as u32,
            FileRenameInformation,
        )
    };
    if status >= 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(
            unsafe { RtlNtStatusToDosError(status) } as i32,
        ))
    }
}
