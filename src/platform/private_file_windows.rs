//! Validate file ownership before installing an owner-only protected DACL.
use std::{
    fs::File,
    io,
    os::windows::io::{AsRawHandle, FromRawHandle},
    path::Path,
    ptr::null_mut,
};
use widestring::{U16CStr, U16CString};
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, *},
    Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, CREATE_NEW, CreateFileW, DELETE, FILE_ATTRIBUTE_NORMAL,
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, GetFileInformationByHandle, READ_CONTROL, WRITE_DAC,
    },
    System::Threading::*,
};

fn current_user() -> io::Result<Vec<usize>> {
    let mut token = null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut size = 0;
    unsafe { GetTokenInformation(token, TokenUser, null_mut(), 0, &mut size) };
    let mut user = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
    let result =
        unsafe { GetTokenInformation(token, TokenUser, user.as_mut_ptr().cast(), size, &mut size) };
    let error = (result == 0).then(io::Error::last_os_error);
    unsafe { CloseHandle(token) };
    match error {
        Some(error) => Err(error),
        None => Ok(user),
    }
}

pub(crate) fn user_owned_descriptor(inheritance: &str, access: &str) -> io::Result<U16CString> {
    let user = current_user()?;
    let mut sid_string = null_mut();
    // TokenUser points into the aligned buffer returned by GetTokenInformation.
    let sid = unsafe { (*(user.as_ptr().cast::<TOKEN_USER>())).User.Sid };
    if unsafe { ConvertSidToStringSidW(sid, &mut sid_string) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let owner = unsafe { U16CStr::from_ptr_str(sid_string) }.to_string_lossy();
    unsafe { LocalFree(sid_string.cast()) };
    U16CString::from_str(format!("O:{owner}D:P(A;{inheritance};{access};;;OW)"))
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

pub(crate) fn create_new(path: &Path) -> io::Result<File> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Missing private file name"))?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    // The canonical parent supplies a verbatim path for long Windows paths.
    let path = U16CString::from_os_str(std::fs::canonicalize(parent)?.join(name).as_os_str())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let sddl = user_owned_descriptor("", "FA")?;
    let mut descriptor = null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor,
        bInheritHandle: 0,
    };
    // Elevated tokens can default to the Administrators group. Assign the
    // actual account owner and owner-only ACL atomically on a new file.
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            GENERIC_READ | GENERIC_WRITE | READ_CONTROL | WRITE_DAC | DELETE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            &attributes,
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
            null_mut(),
        )
    };
    let error = (handle == INVALID_HANDLE_VALUE).then(io::Error::last_os_error);
    unsafe { LocalFree(descriptor) };
    match error {
        Some(error) => Err(error),
        None => {
            let file = unsafe { File::from_raw_handle(handle) };
            if let Err(mut error) = secure_file(&file) {
                if let Err(cleanup) = crate::platform::windows_rename::remove_opened(&file) {
                    error.push_str(&format!(
                        " The new private file could not be removed: {cleanup}"
                    ));
                }
                return Err(io::Error::new(io::ErrorKind::PermissionDenied, error));
            }
            Ok(file)
        }
    }
}

pub(super) fn secure_file(file: &File) -> Result<(), String> {
    unsafe {
        let metadata = file.metadata().map_err(|_| "Cannot inspect private file")?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err("Private file must be a regular file.".into());
        }
        let mut info: BY_HANDLE_FILE_INFORMATION = std::mem::zeroed();
        if GetFileInformationByHandle(file.as_raw_handle(), &mut info) == 0
            || info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        {
            return Err("Private file must be a regular, non-redirected file.".into());
        }
        if info.nNumberOfLinks != 1 {
            return Err("Private file must not have hard links.".into());
        }
        let user = current_user().map_err(|_| "Cannot inspect current user.")?;
        let mut owner = std::ptr::null_mut();
        let mut descriptor = std::ptr::null_mut();
        if GetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut descriptor,
        ) != 0
        {
            return Err("Cannot inspect private file ownership.".into());
        }
        let expected = (*(user.as_ptr().cast::<TOKEN_USER>())).User.Sid;
        let owned = EqualSid(owner, expected) != 0;
        LocalFree(descriptor);
        if !owned {
            return Err("Private file belongs to another user.".into());
        }
        let sddl = widestring::U16CString::from_str("D:P(A;;FA;;;OW)")
            .map_err(|_| "Invalid security descriptor")?;
        let mut protected = std::ptr::null_mut();
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &mut protected,
            std::ptr::null_mut(),
        ) == 0
        {
            return Err("Cannot protect private file.".into());
        }
        let (mut present, mut defaulted, mut dacl) = (0, 0, std::ptr::null_mut());
        let parsed = GetSecurityDescriptorDacl(protected, &mut present, &mut dacl, &mut defaulted);
        let result = if parsed != 0 && present != 0 {
            SetSecurityInfo(
                file.as_raw_handle(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                dacl,
                std::ptr::null_mut(),
            )
        } else {
            1
        };
        LocalFree(protected);
        if result != 0 {
            return Err("Cannot apply owner-only private file permissions.".into());
        }
        Ok(())
    }
}
