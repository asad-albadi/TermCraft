//! Validate file ownership before installing an owner-only protected DACL.
use std::{fs::File, os::windows::io::AsRawHandle};
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, *},
    Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle},
    System::Threading::*,
};
pub(super) fn secure_file(file: &File) -> Result<(), String> {
    unsafe {
        let metadata = file.metadata().map_err(|_| "Cannot inspect private file")?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err("Private file must be a regular file.".into());
        }
        let mut info: BY_HANDLE_FILE_INFORMATION = std::mem::zeroed();
        if GetFileInformationByHandle(file.as_raw_handle(), &mut info) == 0
            || info.nNumberOfLinks != 1
        {
            return Err("Private file must not have hard links.".into());
        }
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err("Cannot inspect current user.".into());
        }
        let mut size = 0;
        GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut size);
        let mut user = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
        let result =
            GetTokenInformation(token, TokenUser, user.as_mut_ptr().cast(), size, &mut size);
        CloseHandle(token);
        if result == 0 {
            return Err("Cannot inspect current user.".into());
        }
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
