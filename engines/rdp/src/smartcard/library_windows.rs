use std::ffi::{CStr, c_void};
use windows_sys::Win32::{
    Foundation::{FreeLibrary, HMODULE},
    System::LibraryLoader::{GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW},
};
pub(super) struct Library(HMODULE);
impl Library {
    pub(super) fn open() -> Result<Self, ()> {
        let mut name = vec![0u16; 32768];
        let length = unsafe {
            windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW(
                name.as_mut_ptr(),
                name.len() as u32,
            )
        } as usize;
        if length == 0 || length >= name.len() {
            return Err(());
        }
        name.truncate(length);
        name.extend("\\WinSCard.dll".encode_utf16());
        name.push(0);
        // SAFETY: Search exclusively the protected Windows system directory.
        let raw = unsafe {
            LoadLibraryExW(
                name.as_ptr(),
                std::ptr::null_mut(),
                LOAD_LIBRARY_SEARCH_SYSTEM32,
            )
        };
        if raw.is_null() {
            Err(())
        } else {
            Ok(Self(raw))
        }
    }
    pub(super) fn symbol(&self, name: &[u8]) -> Result<*mut c_void, ()> {
        let name = CStr::from_bytes_with_nul(name).map_err(|_| ())?;
        // SAFETY: Owned live library and fixed ASCII symbol.
        unsafe { GetProcAddress(self.0, name.as_ptr().cast()) }
            .map(|v| v as *mut c_void)
            .ok_or(())
    }
}
impl Drop for Library {
    fn drop(&mut self) {
        unsafe {
            FreeLibrary(self.0);
        }
    }
}
