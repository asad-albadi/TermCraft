use std::{
    fs::File,
    io::{self, Read},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::Path,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::{ERROR_BROKEN_PIPE, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
    Security::SECURITY_ATTRIBUTES,
    System::{
        JobObjects::*,
        Pipes::{CreatePipe, PeekNamedPipe},
        Threading::*,
    },
};

pub(super) struct PluginChild {
    job: OwnedHandle,
    process: OwnedHandle,
    stdout: File,
    sources: Option<[OwnedHandle; 3]>,
    _verified: File,
}

struct Attributes(LPPROC_THREAD_ATTRIBUTE_LIST);

impl Drop for Attributes {
    fn drop(&mut self) {
        // SAFETY: successful initialization precedes construction.
        unsafe { DeleteProcThreadAttributeList(self.0) };
    }
}

impl PluginChild {
    pub(super) fn spawn(
        directory: &Path,
        executable: &str,
        verified: &File,
    ) -> io::Result<(Self, File)> {
        Self::spawn_with(directory, executable, verified, |_| {})
    }

    fn spawn_with(
        directory: &Path,
        executable: &str,
        verified: &File,
        prepared: impl FnOnce(&[HANDLE]),
    ) -> io::Result<(Self, File)> {
        let program = directory.join("bin").join(executable);
        let app = std::env::current_exe()?;
        let image = wide(&app);
        let mut command_line: Vec<u16> = format!("\"{}\" --exact {ENTRY}", app.display())
            .encode_utf16()
            .chain([0])
            .collect();
        let cwd = wide(directory);
        let (stdin_read, stdin_write) = pipe()?;
        let (stdout_read, stdout_write) = pipe()?;
        // Neither the log nor another session sees raw plugin stderr.
        let stderr = File::options().write(true).open(r"\\.\NUL")?;
        let inheritance = [
            stdin_read.as_raw_handle(),
            stdout_write.as_raw_handle(),
            stderr.as_raw_handle(),
        ];
        let job = create_job()?;
        // SAFETY: the STARTUPINFOEX attribute inputs and backing memory remain
        // live and correctly aligned for the entire CreateProcessW call.
        unsafe {
            // Parent endpoints remain non-inheritable even while unrelated
            // ordinary piped child processes are being created concurrently.
            prepared(&inheritance);
            let mut size = 0;
            InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut size);
            let mut storage = vec![0usize; size.div_ceil(size_of::<usize>())];
            let list = storage.as_mut_ptr().cast();
            if InitializeProcThreadAttributeList(list, 1, 0, &mut size) == 0 {
                return Err(io::Error::last_os_error());
            }
            let attributes = Attributes(list);
            let jobs = [job.as_raw_handle()];
            for (attribute, value) in [(PROC_THREAD_ATTRIBUTE_JOB_LIST, jobs.as_slice())] {
                if UpdateProcThreadAttribute(
                    attributes.0,
                    0,
                    attribute as usize,
                    value.as_ptr().cast(),
                    size_of_val(value),
                    null_mut(),
                    null(),
                ) == 0
                {
                    return Err(io::Error::last_os_error());
                }
            }
            let mut startup = STARTUPINFOEXW::default();
            startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
            startup.lpAttributeList = attributes.0;
            let mut info = PROCESS_INFORMATION::default();
            let mut environment = Vec::new();
            for (key, value) in [
                ("TERMCRAFT_PLUGIN_PID", std::process::id().to_string()),
                (
                    "TERMCRAFT_PLUGIN_STDIN",
                    (inheritance[0] as usize).to_string(),
                ),
                (
                    "TERMCRAFT_PLUGIN_STDOUT",
                    (inheritance[1] as usize).to_string(),
                ),
                (
                    "TERMCRAFT_PLUGIN_STDERR",
                    (inheritance[2] as usize).to_string(),
                ),
            ] {
                environment.extend(format!("{key}={value}").encode_utf16());
                environment.push(0);
            }
            environment.extend("TERMCRAFT_PLUGIN_IMAGE=".encode_utf16());
            environment.extend(program.as_os_str().encode_wide());
            environment.extend([0, 0]);
            if CreateProcessW(
                image.as_ptr(),
                command_line.as_mut_ptr(),
                null(),
                null(),
                0,
                EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT | CREATE_NO_WINDOW,
                environment.as_ptr().cast(),
                cwd.as_ptr(),
                &startup.StartupInfo,
                &mut info,
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            let process = OwnedHandle::from_raw_handle(info.hProcess);
            drop(OwnedHandle::from_raw_handle(info.hThread));
            Ok((
                Self {
                    job,
                    process,
                    stdout: File::from(stdout_read),
                    sources: Some([stdin_read, stdout_write, stderr.into()]),
                    _verified: verified.try_clone()?,
                },
                File::from(stdin_write),
            ))
        }
    }

    pub(super) fn exited(&mut self) -> io::Result<Option<bool>> {
        // SAFETY: the process handle is owned by this invocation.
        unsafe {
            match WaitForSingleObject(self.process.as_raw_handle(), 0) {
                WAIT_OBJECT_0 => {
                    self.sources.take();
                    let mut code = 0;
                    if GetExitCodeProcess(self.process.as_raw_handle(), &mut code) == 0 {
                        return Err(io::Error::last_os_error());
                    }
                    Ok(Some(code == 0))
                }
                WAIT_TIMEOUT => Ok(None),
                _ => Err(io::Error::last_os_error()),
            }
        }
    }

    pub(super) fn read_available(&mut self, buffer: &mut [u8]) -> io::Result<Option<usize>> {
        let mut available = 0;
        // SAFETY: the stdout pipe has a single reader and available receives output.
        if unsafe {
            PeekNamedPipe(
                self.stdout.as_raw_handle(),
                null_mut(),
                0,
                null_mut(),
                &mut available,
                null_mut(),
            )
        } == 0
        {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_BROKEN_PIPE as i32) {
                return Ok(Some(0));
            }
            return Err(error);
        }
        let count = buffer.len().min(available as usize);
        if count == 0 {
            return Ok(None);
        }
        self.stdout.read(&mut buffer[..count]).map(Some)
    }
}

impl Drop for PluginChild {
    fn drop(&mut self) {
        // SAFETY: the invocation owns both handles and the job has kill-on-close too.
        unsafe {
            TerminateJobObject(self.job.as_raw_handle(), 1);
            WaitForSingleObject(self.process.as_raw_handle(), 2_000);
        }
    }
}

fn pipe() -> io::Result<(OwnedHandle, OwnedHandle)> {
    let mut read: HANDLE = null_mut();
    let mut write: HANDLE = null_mut();
    // SAFETY: output pointers are valid; only the explicit handle list is inherited.
    if unsafe { CreatePipe(&mut read, &mut write, null::<SECURITY_ATTRIBUTES>(), 4096) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe {
        (
            OwnedHandle::from_raw_handle(read),
            OwnedHandle::from_raw_handle(write),
        )
    })
}

fn create_job() -> io::Result<OwnedHandle> {
    // SAFETY: the job has a unique owned handle and limits are passed with their actual size.
    unsafe {
        let raw = CreateJobObjectW(null(), null());
        if raw.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = OwnedHandle::from_raw_handle(raw);
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            raw,
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            size_of_val(&limits) as u32,
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(job)
    }
}

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain([0]).collect()
}

const ENTRY: &str = "plugins::host::windows::tests::windows_plugin_bootstrap_entry";

pub(in crate::plugins) fn run_if_requested() -> Option<i32> {
    std::env::var_os("TERMCRAFT_PLUGIN_IMAGE")?;
    if !std::env::args().skip(1).eq(["--exact", ENTRY]) {
        return None;
    }
    Some(if launch_plugin().is_ok() { 0 } else { 1 })
}

fn launch_plugin() -> io::Result<()> {
    use windows_sys::Win32::Foundation::{DUPLICATE_SAME_ACCESS, DuplicateHandle};
    let number = |key: &str| -> io::Result<usize> {
        std::env::var(key)
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|&v| v != 0)
            .ok_or_else(|| io::Error::other("Invalid plugin bootstrap"))
    };
    let program = std::env::var_os("TERMCRAFT_PLUGIN_IMAGE")
        .ok_or_else(|| io::Error::other("Missing plugin image"))?;
    let program = std::path::PathBuf::from(program);
    let pid = u32::try_from(number("TERMCRAFT_PLUGIN_PID")?).map_err(io::Error::other)?;
    unsafe {
        let mut in_job = 0;
        if IsProcessInJob(GetCurrentProcess(), null_mut(), &mut in_job) == 0 || in_job == 0 {
            return Err(io::Error::other(
                "Plugin bootstrap requires its invocation job",
            ));
        }
        let parent = OpenProcess(PROCESS_DUP_HANDLE, 0, pid);
        if parent.is_null() {
            return Err(io::Error::last_os_error());
        }
        let parent = OwnedHandle::from_raw_handle(parent);
        let mut ends = Vec::new();
        for key in [
            "TERMCRAFT_PLUGIN_STDIN",
            "TERMCRAFT_PLUGIN_STDOUT",
            "TERMCRAFT_PLUGIN_STDERR",
        ] {
            let mut remote = null_mut();
            if DuplicateHandle(
                parent.as_raw_handle(),
                number(key)? as HANDLE,
                GetCurrentProcess(),
                &mut remote,
                0,
                1,
                DUPLICATE_SAME_ACCESS,
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            ends.push(OwnedHandle::from_raw_handle(remote));
        }
        let inheritance: Vec<HANDLE> = ends.iter().map(AsRawHandle::as_raw_handle).collect();
        let mut size = 0;
        InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut size);
        let mut storage = vec![0usize; size.div_ceil(size_of::<usize>())];
        let list = storage.as_mut_ptr().cast();
        if InitializeProcThreadAttributeList(list, 1, 0, &mut size) == 0 {
            return Err(io::Error::last_os_error());
        }
        let attributes = Attributes(list);
        if UpdateProcThreadAttribute(
            attributes.0,
            0,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
            inheritance.as_ptr().cast(),
            size_of_val(inheritance.as_slice()),
            null_mut(),
            null(),
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let mut startup = STARTUPINFOEXW::default();
        startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = inheritance[0];
        startup.StartupInfo.hStdOutput = inheritance[1];
        startup.StartupInfo.hStdError = inheritance[2];
        startup.lpAttributeList = attributes.0;
        let mut info = PROCESS_INFORMATION::default();
        let image = wide(&program);
        let mut command: Vec<u16> = std::iter::once(b'"' as u16)
            .chain(program.as_os_str().encode_wide())
            .chain([b'"' as u16, 0])
            .collect();
        let environment = [0u16; 2];
        if CreateProcessW(
            image.as_ptr(),
            command.as_mut_ptr(),
            null(),
            null(),
            1,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT | CREATE_NO_WINDOW,
            environment.as_ptr().cast(),
            null(),
            &startup.StartupInfo,
            &mut info,
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let process = OwnedHandle::from_raw_handle(info.hProcess);
        drop(OwnedHandle::from_raw_handle(info.hThread));
        drop(ends);
        if WaitForSingleObject(process.as_raw_handle(), INFINITE) != WAIT_OBJECT_0 {
            return Err(io::Error::last_os_error());
        }
        let mut code = 1;
        if GetExitCodeProcess(process.as_raw_handle(), &mut code) == 0 || code != 0 {
            return Err(io::Error::other("Plugin exited unsuccessfully"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_plugin_bootstrap_entry() {
        if let Some(code) = run_if_requested() {
            std::process::exit(code);
        }
    }
    #[test]
    fn overlapping_child_cannot_inherit_plugin_pipes_and_stdout_reaches_eof() {
        use std::process::{Command, Stdio};
        use windows_sys::Win32::Foundation::{GetHandleInformation, HANDLE_FLAG_INHERIT};
        let system = std::env::var_os("SystemRoot").unwrap();
        let root = std::path::PathBuf::from(system).join("System32");
        let verified = File::open(root.join("whoami.exe")).unwrap();
        let fixture = tempfile::tempdir().unwrap();
        std::fs::create_dir(fixture.path().join("bin")).unwrap();
        std::fs::copy(
            root.join("whoami.exe"),
            fixture.path().join("bin/whoami.exe"),
        )
        .unwrap();
        let mut unrelated = None;
        let (mut plugin, input) =
            PluginChild::spawn_with(fixture.path(), "whoami.exe", &verified, |handles| {
                unrelated = Some(
                    Command::new(root.join("cmd.exe"))
                        .args(["/C", "ping -t 127.0.0.1 >NUL"])
                        .stdin(Stdio::piped())
                        .stdout(Stdio::piped())
                        .spawn()
                        .unwrap(),
                );
                for &handle in handles {
                    let mut flags = 0;
                    assert_ne!(unsafe { GetHandleInformation(handle, &mut flags) }, 0);
                    assert_eq!(
                        flags & HANDLE_FLAG_INHERIT,
                        0,
                        "main-process plugin end became inheritable"
                    );
                }
            })
            .unwrap();
        drop(input);
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut exited = false;
        let mut eof = false;
        while std::time::Instant::now() < until {
            exited |= plugin.exited().unwrap().is_some();
            eof |= plugin.read_available(&mut [0; 512]).unwrap() == Some(0);
            if exited && eof {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let mut unrelated = unrelated.unwrap();
        let still_live = unrelated.try_wait().unwrap().is_none();
        unrelated.kill().unwrap();
        unrelated.wait().unwrap();
        assert!(
            still_live,
            "overlap child exited before the ownership check"
        );
        assert!(exited && eof, "plugin pipe retained an unexpected writer");
    }
}
