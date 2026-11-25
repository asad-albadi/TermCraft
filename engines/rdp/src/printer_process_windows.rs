//! Kernel-owned process lifetime; helper death on parent exit, no global spooler control.
use std::{
    mem::{size_of, zeroed},
    ptr::null,
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::JobObjects::*,
};
pub(super) struct Ownership(HANDLE);
impl Ownership {
    pub(super) fn assign(child: &tokio::process::Child) -> std::io::Result<Self> {
        unsafe {
            let job = CreateJobObjectW(null(), null());
            if job.is_null() {
                return Err(std::io::Error::last_os_error());
            }
            let owner = Self(job);
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ) == 0
            {
                return Err(std::io::Error::last_os_error());
            }
            let process = child
                .raw_handle()
                .ok_or_else(|| std::io::Error::other("Printer process exited"))?;
            if AssignProcessToJobObject(job, process as HANDLE) == 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(owner)
        }
    }
}
// HANDLE belongs solely to the actor runtime; no native spooler handle is shared.
impl Drop for Ownership {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}
