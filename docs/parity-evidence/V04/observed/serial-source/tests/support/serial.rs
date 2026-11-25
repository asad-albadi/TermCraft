//! Test-owned pseudo-terminal. No physical device or global terminal settings.
#[cfg(unix)]
pub struct SerialPty {
    pub master: std::fs::File,
    pub path: String,
    _slave: std::fs::File,
}
#[cfg(unix)]
impl SerialPty {
    pub fn new() -> Self {
        use std::os::fd::FromRawFd;
        let (mut master, mut slave) = (-1, -1);
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut master,
                    &mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    std::ptr::null(),
                )
            },
            0,
            "{}",
            std::io::Error::last_os_error()
        );
        let mut name = [0i8; 1024];
        assert_eq!(
            unsafe { libc::ttyname_r(slave, name.as_mut_ptr(), name.len()) },
            0
        );
        let path = unsafe { std::ffi::CStr::from_ptr(name.as_ptr()) }
            .to_str()
            .unwrap()
            .to_string();
        Self {
            master: unsafe { std::fs::File::from_raw_fd(master) },
            _slave: unsafe { std::fs::File::from_raw_fd(slave) },
            path,
        }
    }
}
