//! Private, regular, single-link files owned by the current user.
use std::{
    fs::{File, OpenOptions},
    io,
    path::Path,
};
fn options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::{
            Foundation::{GENERIC_READ, GENERIC_WRITE},
            Storage::FileSystem::{FILE_FLAG_OPEN_REPARSE_POINT, READ_CONTROL, WRITE_DAC},
        };
        options
            .access_mode(GENERIC_READ | GENERIC_WRITE | READ_CONTROL | WRITE_DAC)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    options
}
pub(crate) fn open(path: &Path, create: bool) -> Result<File, String> {
    #[cfg(windows)]
    if create {
        match create_new(path) {
            Ok(file) => return Ok(file),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    let mut options = options();
    #[cfg(unix)]
    options.create(create);
    let file = options.open(path).map_err(|e| e.to_string())?;
    verify(&file)?;
    Ok(file)
}
#[cfg(unix)]
pub(crate) fn create_new(path: &Path) -> io::Result<File> {
    let file = options().create_new(true).open(path)?;
    verify(&file).map_err(|e| io::Error::new(io::ErrorKind::PermissionDenied, e))?;
    Ok(file)
}
#[cfg(windows)]
pub(crate) use windows::{create_new, user_owned_descriptor};
#[cfg(unix)]
fn verify(file: &File) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    let m = file.metadata().map_err(|e| e.to_string())?;
    if m.nlink() != 1
        || !m.is_file()
        || m.uid() != unsafe { libc::geteuid() }
        || m.mode() & 0o077 != 0
    {
        return Err("Private file must be a single-link regular file owned by this user with no group/other permissions.".into());
    }
    Ok(())
}
#[cfg(windows)]
fn verify(file: &File) -> Result<(), String> {
    windows::secure_file(file)
}
#[cfg(windows)]
#[path = "private_file_windows.rs"]
mod windows;
