//! Windows Chromium transport. The browser receives synchronous handles and
//! the worker holds the corresponding deadline-bound overlapped endpoints.

use std::{
    ffi::{OsStr, OsString, c_void},
    io,
    mem::{size_of, size_of_val},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use serde_json::Value;
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, *},
    Storage::FileSystem::*,
    System::{
        IO::*, JobObjects::*, Pipes::*, SystemInformation::GetSystemWindowsDirectoryW, Threading::*,
    },
};

use crate::runtime::{RuntimeCatalog, ToolId};

#[path = "windows_pipe/bootstrap.rs"]
pub(super) mod bootstrap;

const MAX_MESSAGE: usize = 12 * 1024 * 1024;
const CONNECTION_ERROR: &str = "Private Chromium connection failed.";
const CANCELLED: &str = "Browser closed.";
const START_ERROR: &str = "Could not launch Chromium in its private Windows process job.";

pub(super) struct Pipe {
    process: Process,
    writer: Endpoint,
    reader: Endpoint,
    pending: Vec<u8>,
    profile: Profile,
}

impl Pipe {
    pub(super) fn start_with_runtime(
        proxy: &str,
        cancel: &AtomicBool,
        runtime: super::Runtime,
    ) -> Result<Self, String> {
        let catalog = RuntimeCatalog::for_current_executable()?;
        let security =
            OwnerSecurity::new().map_err(|_| "Could not secure the private browser profile.")?;
        let profile =
            Profile::new(&security).map_err(|_| "Could not create a private browser profile.")?;
        let chromium = match runtime {
            super::Runtime::SystemChromium => {
                catalog.resolve(ToolId::SystemChromium)?.executable()?
            }
            super::Runtime::BundledChromium => {
                catalog.stage_bundled_browser(&profile.path.join("runtime"), cancel)?
            }
        };
        let (writer, browser_reader) = pipe_pair(false, &security).map_err(|_| CONNECTION_ERROR)?;
        let (reader, browser_writer) = pipe_pair(true, &security).map_err(|_| CONNECTION_ERROR)?;
        let process = bootstrap::start(
            &profile.path,
            &chromium,
            &security,
            &browser_reader,
            &browser_writer,
            proxy,
            cancel,
        )
        .map_err(|_| START_ERROR)?;
        Ok(Self {
            process,
            writer,
            reader,
            pending: Vec::new(),
            profile,
        })
    }

    pub(super) fn send(&mut self, message: &impl serde::Serialize) -> Result<(), String> {
        let mut bytes = zeroize::Zeroizing::new(
            serde_json::to_vec(message).map_err(|_| "Invalid browser command.")?,
        );
        if bytes.len() >= MAX_MESSAGE {
            return Err("Browser command exceeded the size limit.".into());
        }
        bytes.push(0);
        let until = Instant::now() + Duration::from_millis(100);
        let cancel = AtomicBool::new(false);
        let mut offset = 0;
        while offset < bytes.len() {
            let count = self
                .writer
                .transfer(
                    IoBuffer::Write(&bytes[offset..(offset + 64 * 1024).min(bytes.len())]),
                    until,
                    &cancel,
                )
                .map_err(|_| CONNECTION_ERROR)?
                .filter(|&count| count != 0)
                .ok_or(CONNECTION_ERROR)?;
            offset += count;
        }
        Ok(())
    }

    pub(super) fn receive(
        &mut self,
        until: Instant,
        cancel: &AtomicBool,
    ) -> Result<Option<Value>, String> {
        let mut checked = 0;
        loop {
            if cancel.load(Ordering::Acquire) {
                return Err(CANCELLED.into());
            }
            if let Some(index) = self.pending[checked..].iter().position(|&byte| byte == 0) {
                let end = checked + index;
                let message = serde_json::from_slice(&self.pending[..end])
                    .map_err(|_| "Invalid browser engine response.")?;
                self.pending.drain(..=end);
                return Ok(Some(message));
            }
            checked = self.pending.len();
            if checked >= MAX_MESSAGE {
                return Err("Browser engine response exceeded the size limit.".into());
            }
            let mut bytes = [0u8; 32 * 1024];
            let limit = bytes.len().min(MAX_MESSAGE - checked);
            let count = self
                .reader
                .transfer(IoBuffer::Read(&mut bytes[..limit]), until, cancel)
                .map_err(|_| {
                    if cancel.load(Ordering::Acquire) {
                        CANCELLED
                    } else {
                        CONNECTION_ERROR
                    }
                })?;
            match count {
                None => return Ok(None),
                Some(0) => return Err("The Chromium process exited. Check the selected browser and sandbox configuration.".into()),
                Some(count) => self.pending.extend_from_slice(&bytes[..count]),
            }
        }
    }
}

impl Drop for Pipe {
    fn drop(&mut self) {
        // Keep the private directory if Windows fails to confirm the job is empty.
        self.profile.remove = self.process.stop();
    }
}

struct Endpoint {
    file: OwnedHandle,
    event: OwnedHandle,
}

enum IoBuffer<'a> {
    Read(&'a mut [u8]),
    Write(&'a [u8]),
}

impl Endpoint {
    fn transfer(
        &self,
        buffer: IoBuffer<'_>,
        until: Instant,
        cancel: &AtomicBool,
    ) -> io::Result<Option<usize>> {
        if cancel.load(Ordering::Acquire) || Instant::now() >= until {
            return Ok(None);
        }
        let file = self.file.as_raw_handle();
        let event = self.event.as_raw_handle();
        let mut operation = OVERLAPPED {
            hEvent: event,
            ..Default::default()
        };
        // The buffer and OVERLAPPED stay live until this exact operation completes,
        // including when cancellation and a successful transfer race.
        unsafe {
            if ResetEvent(event) == 0 {
                return Err(io::Error::last_os_error());
            }
            let completed = match buffer {
                IoBuffer::Read(bytes) => ReadFile(
                    file,
                    bytes.as_mut_ptr(),
                    bytes.len() as u32,
                    null_mut(),
                    &mut operation,
                ),
                IoBuffer::Write(bytes) => WriteFile(
                    file,
                    bytes.as_ptr(),
                    bytes.len() as u32,
                    null_mut(),
                    &mut operation,
                ),
            };
            if completed != 0 {
                return completed_transfer(file, &operation, false);
            }
            let error = GetLastError();
            if error != ERROR_IO_PENDING {
                return pipe_completion(error);
            }
            loop {
                let remaining = until.saturating_duration_since(Instant::now());
                if remaining.is_zero() || cancel.load(Ordering::Acquire) {
                    break;
                }
                let interval = (remaining.as_millis() as u32).clamp(1, 15);
                match WaitForSingleObject(event, interval) {
                    WAIT_TIMEOUT => continue,
                    WAIT_OBJECT_0 => return completed_transfer(file, &operation, false),
                    _ => break,
                }
            }
            CancelIoEx(file, &operation);
            // CancelIoEx only requests cancellation. NPFS completion must be
            // collected before the kernel stops borrowing our stack and buffer.
            completed_transfer(file, &operation, true)
        }
    }
}

fn pipe_completion(error: u32) -> io::Result<Option<usize>> {
    match error {
        ERROR_OPERATION_ABORTED => Ok(None),
        ERROR_BROKEN_PIPE | ERROR_NO_DATA | ERROR_PIPE_NOT_CONNECTED => Ok(Some(0)),
        _ => Err(io::Error::from_raw_os_error(error as i32)),
    }
}

fn completed_transfer(
    file: HANDLE,
    operation: &OVERLAPPED,
    wait: bool,
) -> io::Result<Option<usize>> {
    let mut count = 0;
    // Called only after observing completion or after requesting cancellation.
    unsafe {
        if GetOverlappedResult(file, operation, &mut count, wait.into()) != 0 {
            Ok(Some(count as usize))
        } else {
            pipe_completion(GetLastError())
        }
    }
}

fn pipe_pair(parent_reads: bool, security: &OwnerSecurity) -> io::Result<(Endpoint, OwnedHandle)> {
    let name = wide(OsStr::new(&format!(
        "\\\\.\\pipe\\TermCraft-Browser-{}",
        entropy()?
    )))?;
    let client_access = if parent_reads {
        GENERIC_WRITE
    } else {
        GENERIC_READ
    };
    let client_security = security.non_inheritable_attributes();
    let server = pipe_server(&name, parent_reads, security)?;
    unsafe {
        let client = handle(CreateFileW(
            name.as_ptr(),
            client_access,
            0,
            &client_security,
            OPEN_EXISTING,
            SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
            null_mut(),
        ))?;
        // CreateFile connected this local client before ConnectNamedPipe.
        // ERROR_PIPE_CONNECTED confirms the documented already-connected path.
        let mut connection = OVERLAPPED {
            hEvent: server.event.as_raw_handle(),
            ..Default::default()
        };
        if ConnectNamedPipe(server.file.as_raw_handle(), &mut connection) == 0 {
            let error = GetLastError();
            if error == ERROR_IO_PENDING {
                CancelIoEx(server.file.as_raw_handle(), &connection);
                let mut count = 0;
                GetOverlappedResult(server.file.as_raw_handle(), &connection, &mut count, 1);
                return Err(io::Error::other(CONNECTION_ERROR));
            }
            if error != ERROR_PIPE_CONNECTED {
                return Err(io::Error::from_raw_os_error(error as i32));
            }
        }
        Ok((server, client))
    }
}

fn pipe_server(name: &[u16], parent_reads: bool, security: &OwnerSecurity) -> io::Result<Endpoint> {
    let access = if parent_reads {
        PIPE_ACCESS_INBOUND
    } else {
        PIPE_ACCESS_OUTBOUND
    };
    let security = security.non_inheritable_attributes();
    unsafe {
        let file = handle(CreateNamedPipeW(
            name.as_ptr(),
            access | FILE_FLAG_FIRST_PIPE_INSTANCE | FILE_FLAG_OVERLAPPED,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            64 * 1024,
            64 * 1024,
            0,
            &security,
        ))?;
        let event = handle(CreateEventW(null(), 1, 0, null()))?;
        Ok(Endpoint { file, event })
    }
}

fn open_null() -> io::Result<OwnedHandle> {
    let name = wide(OsStr::new("NUL"))?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: null_mut(),
        bInheritHandle: 1,
    };
    unsafe {
        handle(CreateFileW(
            name.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            &attributes,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            null_mut(),
        ))
    }
}

fn handle(raw: HANDLE) -> io::Result<OwnedHandle> {
    if raw.is_null() || raw == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // Only successful owning handles from Win32 creation APIs reach this point.
    Ok(unsafe { OwnedHandle::from_raw_handle(raw) })
}

struct OwnerSecurity(PSECURITY_DESCRIPTOR);

impl OwnerSecurity {
    fn new() -> io::Result<Self> {
        unsafe {
            let mut token = null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return Err(io::Error::last_os_error());
            }
            let token = handle(token)?;
            let mut size = 0;
            GetTokenInformation(token.as_raw_handle(), TokenUser, null_mut(), 0, &mut size);
            let mut user = vec![0usize; (size as usize).div_ceil(size_of::<usize>())];
            if GetTokenInformation(
                token.as_raw_handle(),
                TokenUser,
                user.as_mut_ptr().cast(),
                size,
                &mut size,
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            let mut sid = null_mut();
            if ConvertSidToStringSidW((*user.as_ptr().cast::<TOKEN_USER>()).User.Sid, &mut sid) == 0
            {
                return Err(io::Error::last_os_error());
            }
            let len = (0..).take_while(|&i| *sid.add(i) != 0).count();
            let mut sddl: Vec<u16> = "D:P(A;OICI;FA;;;".encode_utf16().collect();
            sddl.extend_from_slice(std::slice::from_raw_parts(sid, len));
            LocalFree(sid.cast());
            sddl.extend([')' as u16, 0]);
            let mut descriptor = null_mut();
            if ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                null_mut(),
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(Self(descriptor))
        }
    }

    fn non_inheritable_attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0,
            bInheritHandle: 0,
        }
    }
}

impl Drop for OwnerSecurity {
    fn drop(&mut self) {
        unsafe { LocalFree(self.0) };
    }
}

struct Profile {
    path: PathBuf,
    directory: Option<OwnedHandle>,
    remove: bool,
}

impl Profile {
    fn new(security: &OwnerSecurity) -> io::Result<Self> {
        const FILE_PERSISTENT_ACLS: u32 = 8;
        let root = super::native_windows::local_app_data()?;
        let path = root.join(format!("TermCraft-Browser-{}", entropy()?));
        let name = wide(path.as_os_str())?;
        let attributes = security.non_inheritable_attributes();
        if unsafe { CreateDirectoryW(name.as_ptr(), &attributes) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut profile = Self {
            path,
            directory: None,
            remove: true,
        };
        let directory = unsafe {
            handle(CreateFileW(
                name.as_ptr(),
                FILE_READ_ATTRIBUTES,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                null_mut(),
            ))?
        };
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        if unsafe { GetFileInformationByHandle(directory.as_raw_handle(), &mut info) } == 0
            || info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0
        {
            profile.remove = false;
            return Err(io::Error::other(
                "Browser profile was replaced during creation.",
            ));
        }
        let mut flags = 0;
        if unsafe {
            GetVolumeInformationByHandleW(
                directory.as_raw_handle(),
                null_mut(),
                0,
                null_mut(),
                null_mut(),
                &mut flags,
                null_mut(),
                0,
            )
        } == 0
            || flags & FILE_PERSISTENT_ACLS == 0
        {
            return Err(io::Error::other(
                "Browser profiles require a local volume supporting ACLs.",
            ));
        }
        profile.directory = Some(directory);
        Ok(profile)
    }
}

impl Drop for Profile {
    fn drop(&mut self) {
        drop(self.directory.take());
        if self.remove {
            let until = Instant::now() + Duration::from_millis(400);
            loop {
                match std::fs::remove_dir_all(&self.path) {
                    Ok(()) => break,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => break,
                    Err(_) if Instant::now() < until => {
                        std::thread::sleep(Duration::from_millis(25))
                    }
                    Err(_) => break,
                }
            }
        }
    }
}

fn entropy() -> io::Result<String> {
    let mut random = [0; 16];
    getrandom::fill(&mut random).map_err(|error| io::Error::other(error.to_string()))?;
    Ok(format!("{:032x}", u128::from_ne_bytes(random)))
}

struct Process {
    job: OwnedHandle,
    process: OwnedHandle,
}

struct AttributeList(LPPROC_THREAD_ATTRIBUTE_LIST);

impl Drop for AttributeList {
    fn drop(&mut self) {
        unsafe { DeleteProcThreadAttributeList(self.0) };
    }
}

impl Process {
    fn launch(
        program: &Path,
        args: &[OsString],
        environment: &[(&str, OsString)],
        directory: &Path,
    ) -> io::Result<Self> {
        let job = unsafe { handle(CreateJobObjectW(null(), null()))? };
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if unsafe {
            SetInformationJobObject(
                job.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of_val(&limits) as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let process = spawn(program, args, environment, directory, Some(&job), None)?;
        Ok(Self { job, process })
    }

    fn stop(&self) -> bool {
        unsafe {
            let job = self.job.as_raw_handle();
            TerminateJobObject(job, 1);
            let until = Instant::now() + Duration::from_secs(2);
            loop {
                let mut accounting = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
                if QueryInformationJobObject(
                    job,
                    JobObjectBasicAccountingInformation,
                    (&mut accounting as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                    size_of_val(&accounting) as u32,
                    null_mut(),
                ) != 0
                    && accounting.ActiveProcesses == 0
                    && WaitForSingleObject(self.process.as_raw_handle(), 0) == WAIT_OBJECT_0
                {
                    return true;
                }
                if Instant::now() >= until {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

fn spawn(
    program: &Path,
    args: &[OsString],
    environment: &[(&str, OsString)],
    directory: &Path,
    job: Option<&OwnedHandle>,
    inherited: Option<(HANDLE, &[HANDLE])>,
) -> io::Result<OwnedHandle> {
    let image = wide(program.as_os_str())?;
    let directory = wide(directory.as_os_str())?;
    let mut command = Vec::new();
    for arg in std::iter::once(program.as_os_str()).chain(args.iter().map(OsString::as_os_str)) {
        if !command.is_empty() {
            command.push(b' ' as u16);
        }
        append_quoted(&mut command, arg)?;
    }
    command.push(0);
    let mut environment = environment.to_vec();
    environment.sort_unstable_by_key(|(key, _)| key.to_ascii_uppercase());
    let mut block = Vec::new();
    for (key, value) in environment {
        block.extend(key.encode_utf16());
        block.push(b'=' as u16);
        block.extend(wide(&value)?);
    }
    block.push(0);
    unsafe {
        let mut size = 0;
        let count = u32::from(job.is_some()) + u32::from(inherited.is_some());
        InitializeProcThreadAttributeList(null_mut(), count, 0, &mut size);
        let mut storage = vec![0usize; size.div_ceil(size_of::<usize>())];
        let list = storage.as_mut_ptr().cast();
        let jobs = [job.map_or(null_mut(), AsRawHandle::as_raw_handle)];
        if InitializeProcThreadAttributeList(list, count, 0, &mut size) == 0 {
            return Err(io::Error::last_os_error());
        }
        let attributes = AttributeList(list);
        let mut startup = STARTUPINFOEXW::default();
        startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        startup.lpAttributeList = attributes.0;
        if let Some((stdio, handles)) = inherited {
            if UpdateProcThreadAttribute(
                attributes.0,
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                handles.as_ptr().cast(),
                size_of_val(handles),
                null_mut(),
                null(),
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
            startup.StartupInfo.hStdInput = stdio;
            startup.StartupInfo.hStdOutput = stdio;
            startup.StartupInfo.hStdError = stdio;
        }
        // The main app assigns the helper at creation. Chromium then
        // automatically joins the helper's job as its descendant.
        if job.is_some()
            && UpdateProcThreadAttribute(
                attributes.0,
                0,
                PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
                jobs.as_ptr().cast(),
                size_of_val(&jobs),
                null_mut(),
                null(),
            ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let mut info = PROCESS_INFORMATION::default();
        if CreateProcessW(
            image.as_ptr(),
            command.as_mut_ptr(),
            null(),
            null(),
            inherited.is_some().into(),
            EXTENDED_STARTUPINFO_PRESENT
                | CREATE_SUSPENDED
                | CREATE_UNICODE_ENVIRONMENT
                | CREATE_NO_WINDOW,
            block.as_ptr().cast::<c_void>(),
            directory.as_ptr(),
            &startup.StartupInfo,
            &mut info,
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let process = handle(info.hProcess)?;
        let thread = handle(info.hThread)?;
        if ResumeThread(thread.as_raw_handle()) == u32::MAX {
            let error = io::Error::last_os_error();
            TerminateProcess(process.as_raw_handle(), 1);
            WaitForSingleObject(process.as_raw_handle(), 2_000);
            return Err(error);
        }
        Ok(process)
    }
}

fn environment(profile: &Path) -> io::Result<Vec<(&'static str, OsString)>> {
    use std::os::windows::ffi::OsStringExt;
    let mut root = [0u16; 260];
    let len = unsafe { GetSystemWindowsDirectoryW(root.as_mut_ptr(), root.len() as u32) } as usize;
    if len == 0 || len >= root.len() {
        return Err(io::Error::last_os_error());
    }
    let root = PathBuf::from(OsString::from_wide(&root[..len]));
    let mut environment = vec![
        ("SystemRoot", root.as_os_str().to_owned()),
        ("WINDIR", root.as_os_str().to_owned()),
        ("PATH", root.join("System32").into_os_string()),
    ];
    for key in ["APPDATA", "LOCALAPPDATA", "TEMP", "TMP", "USERPROFILE"] {
        environment.push((key, profile.as_os_str().to_owned()));
    }
    Ok(environment)
}

fn wide(value: &OsStr) -> io::Result<Vec<u16>> {
    let mut units: Vec<u16> = value.encode_wide().collect();
    if units.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Unexpected NUL in Windows path or argument.",
        ));
    }
    units.push(0);
    Ok(units)
}

fn append_quoted(command: &mut Vec<u16>, arg: &OsStr) -> io::Result<()> {
    const QUOTE: u16 = b'"' as u16;
    const SLASH: u16 = b'\\' as u16;
    command.push(QUOTE);
    let mut slashes = 0;
    for unit in arg.encode_wide() {
        match unit {
            0 => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Unexpected NUL in Windows argument.",
                ));
            }
            SLASH => slashes += 1,
            _ => {
                let escaped = if unit == QUOTE {
                    slashes * 2 + 1
                } else {
                    slashes
                };
                command.extend(std::iter::repeat_n(SLASH, escaped));
                command.push(unit);
                slashes = 0;
            }
        }
    }
    command.extend(std::iter::repeat_n(SLASH, slashes * 2));
    command.push(QUOTE);
    Ok(())
}

#[cfg(test)]
#[path = "windows_pipe/tests.rs"]
mod tests;
