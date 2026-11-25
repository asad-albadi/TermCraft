use std::ffi::{CStr, c_void};
pub(super) struct Library(*mut c_void);
impl Library {
    pub(super) fn open() -> Result<Self, ()> {
        // SAFETY: Absolute protected system framework, no search or environment override.
        let raw = unsafe {
            libc::dlopen(
                c"/System/Library/Frameworks/PCSC.framework/PCSC".as_ptr(),
                libc::RTLD_NOW | libc::RTLD_LOCAL,
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
        // SAFETY: Owned live library and validated NUL terminated symbol.
        let raw = unsafe { libc::dlsym(self.0, name.as_ptr()) };
        if raw.is_null() { Err(()) } else { Ok(raw) }
    }
}
impl Drop for Library {
    fn drop(&mut self) {
        unsafe {
            libc::dlclose(self.0);
        }
    }
}
