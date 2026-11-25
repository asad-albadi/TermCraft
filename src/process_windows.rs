//! Windows 10 1809+ pseudoconsole adapter. Must be validated on Windows.
use std::{
    fs::File,
    io::Read,
    os::windows::{ffi::OsStrExt, io::FromRawHandle},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::{Console::*, JobObjects::*, Pipes::CreatePipe, Threading::*},
};
struct Handle(HANDLE);
// A process/job handle names a kernel object and has exclusive Rust ownership.
// Moving it transfers the one CloseHandle responsibility to another thread.
unsafe impl Send for Handle {}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
struct Console(HPCON);
impl Drop for Console {
    fn drop(&mut self) {
        unsafe {
            ClosePseudoConsole(self.0);
        }
    }
}
pub struct Process {
    pub rx: Receiver<Vec<u8>>,
    input: super::InputQueue,
    process: Handle,
    console: Option<Console>,
    job: Option<Handle>,
    stop: Arc<AtomicBool>,
    reader: Option<std::thread::JoinHandle<()>>,
    pub exited: bool,
    pub exit_message: String,
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn child_environment(
    overrides: &[(std::ffi::OsString, std::ffi::OsString)],
) -> Result<Option<Vec<u16>>, String> {
    if overrides.is_empty() {
        return Ok(None);
    }
    // Windows environment names are case-insensitive and the block is ordered
    // by name. Keep inherited values as native strings, including Unicode paths.
    let key = |name: &std::ffi::OsStr| name.to_string_lossy().to_uppercase();
    let mut values: std::collections::BTreeMap<_, _> = std::env::vars_os()
        .map(|(name, value)| (key(&name), (name, value)))
        .collect();
    for (name, value) in overrides {
        let mut name_units = name.encode_wide();
        if name.is_empty()
            || name_units.any(|unit| unit == 0 || unit == u16::from(b'='))
            || value.encode_wide().any(|unit| unit == 0)
        {
            return Err("Invalid child environment variable".into());
        }
        values.insert(key(name), (name.clone(), value.clone()));
    }
    let mut block = Vec::new();
    for (name, value) in values.values() {
        block.extend(name.encode_wide());
        block.push(u16::from(b'='));
        block.extend(value.encode_wide());
        block.push(0);
    }
    block.push(0);
    Ok(Some(block))
}
fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    let mut back = 0;
    for c in s.chars() {
        if c == '\\' {
            back += 1;
        } else {
            if c == '"' {
                out.push_str(&"\\".repeat(back * 2 + 1));
            } else {
                out.push_str(&"\\".repeat(back));
            }
            back = 0;
            out.push(c);
        }
    }
    out.push_str(&"\\".repeat(back * 2));
    out.push('"');
    out
}
fn error() -> String {
    std::io::Error::last_os_error().to_string()
}
impl Process {
    pub fn spawn(
        program: &str,
        args: &[String],
        directory: Option<&Path>,
        cols: u16,
        rows: u16,
    ) -> Result<Self, String> {
        Self::spawn_with_environment(program, args, directory, cols, rows, &[])
    }

    pub fn spawn_with_environment_cancellable(
        program: &str,
        args: &[String],
        directory: Option<&Path>,
        cols: u16,
        rows: u16,
        environment: &[(std::ffi::OsString, std::ffi::OsString)],
        cancelled: &AtomicBool,
    ) -> Result<Self, String> {
        Self::spawn_inner(
            program,
            args,
            directory,
            cols,
            rows,
            environment,
            Some(cancelled),
        )
    }

    pub fn spawn_with_environment(
        program: &str,
        args: &[String],
        directory: Option<&Path>,
        cols: u16,
        rows: u16,
        environment: &[(std::ffi::OsString, std::ffi::OsString)],
    ) -> Result<Self, String> {
        Self::spawn_inner(program, args, directory, cols, rows, environment, None)
    }

    fn spawn_inner(
        program: &str,
        args: &[String],
        directory: Option<&Path>,
        cols: u16,
        rows: u16,
        environment: &[(std::ffi::OsString, std::ffi::OsString)],
        cancelled: Option<&AtomicBool>,
    ) -> Result<Self, String> {
        if cancelled.is_some_and(|cancelled| cancelled.load(Ordering::Acquire)) {
            return Err("Terminal startup cancelled".into());
        }
        let environment = child_environment(environment)?;
        unsafe {
            let (mut ir, mut iw, mut or, mut ow) = (
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            if CreatePipe(&mut ir, &mut iw, std::ptr::null(), 0) == 0 {
                return Err(error());
            }
            let input_read = Handle(ir);
            let input_write = Handle(iw);
            if CreatePipe(&mut or, &mut ow, std::ptr::null(), 0) == 0 {
                return Err(error());
            }
            let output_read = Handle(or);
            let output_write = Handle(ow);
            let mut hpc = 0;
            let hr = CreatePseudoConsole(
                COORD {
                    X: cols as i16,
                    Y: rows as i16,
                },
                input_read.0,
                output_write.0,
                0,
                &mut hpc,
            );
            if hr < 0 {
                return Err(format!("Could not create Windows pseudoconsole: {hr:#x}"));
            }
            let console = Console(hpc);
            drop(input_read);
            drop(output_write);
            let mut size = 0;
            InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut size);
            let mut buffer = vec![0usize; size.div_ceil(std::mem::size_of::<usize>())];
            let list = buffer.as_mut_ptr() as LPPROC_THREAD_ATTRIBUTE_LIST;
            if InitializeProcThreadAttributeList(list, 1, 0, &mut size) == 0 {
                return Err(error());
            }
            if UpdateProcThreadAttribute(
                list,
                0,
                PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
                hpc as *const _,
                std::mem::size_of::<HPCON>(),
                std::ptr::null_mut(),
                std::ptr::null(),
            ) == 0
            {
                DeleteProcThreadAttributeList(list);
                return Err(error());
            }
            let mut startup: STARTUPINFOEXW = std::mem::zeroed();
            startup.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
            startup.lpAttributeList = list;
            let command = std::iter::once(program.to_string())
                .chain(args.iter().cloned())
                .map(|s| quote(&s))
                .collect::<Vec<_>>()
                .join(" ");
            let mut command = wide(&command);
            let cwd = directory.map(|path| {
                path.as_os_str()
                    .encode_wide()
                    .chain(Some(0))
                    .collect::<Vec<_>>()
            });
            if cancelled.is_some_and(|cancelled| cancelled.load(Ordering::Acquire)) {
                DeleteProcThreadAttributeList(list);
                return Err("Terminal startup cancelled".into());
            }
            let mut info: PROCESS_INFORMATION = std::mem::zeroed();
            let ok = CreateProcessW(
                std::ptr::null(),
                command.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                EXTENDED_STARTUPINFO_PRESENT | CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT,
                environment
                    .as_ref()
                    .map_or(std::ptr::null(), |block| block.as_ptr().cast()),
                cwd.as_ref().map_or(std::ptr::null(), |p| p.as_ptr()),
                &startup.StartupInfo,
                &mut info,
            );
            let failure = error();
            DeleteProcThreadAttributeList(list);
            if ok == 0 {
                return Err(format!("Could not start {program}: {failure}"));
            }
            let process = Handle(info.hProcess);
            let thread_handle = Handle(info.hThread);
            let job = Handle(CreateJobObjectW(std::ptr::null(), std::ptr::null()));
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if job.0.is_null()
                || SetInformationJobObject(
                    job.0,
                    JobObjectExtendedLimitInformation,
                    &limits as *const _ as *const _,
                    std::mem::size_of_val(&limits) as u32,
                ) == 0
                || AssignProcessToJobObject(job.0, process.0) == 0
            {
                let e = error();
                TerminateProcess(process.0, 1);
                return Err(format!("Could not manage terminal process: {e}"));
            }
            if cancelled.is_some_and(|cancelled| cancelled.load(Ordering::Acquire)) {
                TerminateProcess(process.0, 1);
                return Err("Terminal startup cancelled".into());
            }
            if ResumeThread(thread_handle.0) == u32::MAX {
                TerminateProcess(process.0, 1);
                return Err(error());
            }
            drop(thread_handle);

            let writer = File::from_raw_handle(input_write.0);
            std::mem::forget(input_write);
            let input = super::InputQueue::new(writer)?;
            let mut reader = File::from_raw_handle(output_read.0);
            std::mem::forget(output_read);
            let (tx, rx) = mpsc::sync_channel(256);
            let stop = Arc::new(AtomicBool::new(false));
            let flag = stop.clone();
            let reader_handle = thread::spawn(move || {
                let mut buf = [0u8; 32768];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if flag.load(Ordering::Acquire) {
                                continue;
                            }
                            let mut bytes = buf[..n].to_vec();
                            loop {
                                match tx.try_send(bytes) {
                                    Ok(()) => break,
                                    Err(mpsc::TrySendError::Disconnected(_)) => return,
                                    Err(mpsc::TrySendError::Full(v)) => {
                                        bytes = v;
                                        if flag.load(Ordering::Acquire) {
                                            break;
                                        }
                                        thread::sleep(std::time::Duration::from_millis(1));
                                    }
                                }
                            }
                        }
                    }
                }
            });
            Ok(Self {
                rx,
                input,
                process,
                console: Some(console),
                job: Some(job),
                stop,
                reader: Some(reader_handle),
                exited: false,
                exit_message: String::new(),
            })
        }
    }
    pub fn input_error(&self) -> Option<String> {
        self.input.error()
    }
    pub fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.input.submit(bytes)
    }
    pub fn poll(&mut self) {
        if !self.exited {
            let mut code = 259;
            unsafe {
                if GetExitCodeProcess(self.process.0, &mut code) != 0 && code != 259 {
                    self.exited = true;
                    self.exit_message = format!("Process ended with code {code}");
                }
            }
        }
    }
    pub fn resize(&self, cols: u16, rows: u16) {
        unsafe {
            ResizePseudoConsole(
                self.console.as_ref().unwrap().0,
                COORD {
                    X: cols as i16,
                    Y: rows as i16,
                },
            );
        }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        drop(self.job.take());
        unsafe {
            TerminateProcess(self.process.0, 1);
        }
        drop(self.console.take());
        if let Some(h) = self.reader.take() {
            let _ = h.join();
        }
    }
}
