//! Native suspended creation: no engine instruction runs outside its owned job.
use std::{
    ffi::{OsStr, OsString},
    io,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::Path,
    ptr::null,
};
use windows_sys::Win32::{
    Foundation::*,
    System::{JobObjects::*, Threading::*},
};
pub(super) struct OwnedProcess {
    process: OwnedHandle,
    job: OwnedHandle,
    pid: u32,
    running: bool,
}
fn wide(value: &OsStr) -> io::Result<Vec<u16>> {
    let units: Vec<_> = value.encode_wide().collect();
    if units.contains(&0) {
        return Err(io::Error::other("NUL in native process argument"));
    }
    Ok(units.into_iter().chain([0]).collect())
}
fn quote(value: &OsStr) -> io::Result<Vec<u16>> {
    let units = wide(value)?;
    let mut out = vec![b'"' as u16];
    let mut slashes = 0;
    for &unit in &units[..units.len() - 1] {
        if unit == b'\\' as u16 {
            slashes += 1;
            continue;
        }
        out.extend(std::iter::repeat_n(
            b'\\' as u16,
            if unit == b'"' as u16 {
                slashes * 2 + 1
            } else {
                slashes
            },
        ));
        out.push(unit);
        slashes = 0;
    }
    out.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
    out.push(b'"' as u16);
    Ok(out)
}
impl OwnedProcess {
    pub(super) fn spawn_suspended(
        exe: &Path,
        arguments: &[OsString],
        directory: &Path,
        environment: &[(OsString, OsString)],
    ) -> io::Result<(Self, OwnedHandle)> {
        let executable = wide(exe.as_os_str())?;
        let cwd = wide(directory.as_os_str())?;
        let mut command = quote(exe.as_os_str())?;
        for arg in arguments {
            command.push(b' ' as u16);
            command.extend(quote(arg)?);
        }
        command.push(0);
        // Use a minimal environment supplied by the provider, never inherited X
        // credentials, DLL override directories, or application startup settings.
        let mut values = std::collections::BTreeMap::new();
        for (name, value) in environment {
            if name.is_empty() || name.to_string_lossy().contains('=') {
                return Err(io::Error::other("Invalid native environment name"));
            }
            values.insert(name.to_string_lossy().to_uppercase(), (name, value));
        }
        let mut block = Vec::new();
        for (name, value) in values.values() {
            let key = wide(name)?;
            let value = wide(value)?;
            block.extend_from_slice(&key[..key.len() - 1]);
            block.push(b'=' as u16);
            block.extend(value);
        }
        block.push(0);
        if block.len() == 1 {
            block.push(0);
        }
        unsafe {
            let raw_job = CreateJobObjectW(null(), null());
            if raw_job.is_null() {
                return Err(io::Error::last_os_error());
            }
            let job = OwnedHandle::from_raw_handle(raw_job);
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                raw_job,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of_val(&limits) as u32,
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            let info = STARTUPINFOW {
                cb: size_of::<STARTUPINFOW>() as u32,
                ..Default::default()
            };
            let mut child = PROCESS_INFORMATION::default();
            if CreateProcessW(
                executable.as_ptr(),
                command.as_mut_ptr(),
                null(),
                null(),
                0,
                CREATE_SUSPENDED | CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT,
                block.as_ptr().cast(),
                cwd.as_ptr(),
                &info,
                &mut child,
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            let process = OwnedHandle::from_raw_handle(child.hProcess);
            let thread = OwnedHandle::from_raw_handle(child.hThread);
            // Failure here still leaves the primary thread suspended. Terminate
            // using the process handle, then wait, before returning ownership.
            if AssignProcessToJobObject(raw_job, process.as_raw_handle()) == 0 {
                let error = io::Error::last_os_error();
                TerminateProcess(process.as_raw_handle(), 1);
                WaitForSingleObject(process.as_raw_handle(), 2000);
                return Err(error);
            }
            Ok((
                Self {
                    process,
                    job,
                    pid: child.dwProcessId,
                    running: true,
                },
                thread,
            ))
        }
    }
    pub(super) fn resume(&self, thread: &OwnedHandle) -> io::Result<()> {
        if unsafe { ResumeThread(thread.as_raw_handle()) } == u32::MAX {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    pub(super) fn id(&self) -> Option<u32> {
        self.running.then_some(self.pid)
    }
    pub(super) fn is_running(&mut self) -> io::Result<bool> {
        match unsafe { WaitForSingleObject(self.process.as_raw_handle(), 0) } {
            WAIT_TIMEOUT => Ok(true),
            WAIT_OBJECT_0 => {
                // The owned job includes xkbcomp/render helpers after the primary exits.
                if unsafe { TerminateJobObject(self.job.as_raw_handle(), 1) } == 0 {
                    return Err(io::Error::last_os_error());
                }
                self.running = false;
                Ok(false)
            }
            _ => Err(io::Error::last_os_error()),
        }
    }
    pub(super) fn stop(&mut self) -> io::Result<()> {
        unsafe {
            // Terminate the owned job even after its primary process exited:
            // an engine helper must not survive a cancelled/closed generation.
            if TerminateJobObject(self.job.as_raw_handle(), 1) == 0 {
                return Err(io::Error::last_os_error());
            }
            if WaitForSingleObject(self.process.as_raw_handle(), 2000) != WAIT_OBJECT_0 {
                return Err(io::Error::other("Native X job did not stop"));
            }
        }
        self.running = false;
        Ok(())
    }
}
impl Drop for OwnedProcess {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
