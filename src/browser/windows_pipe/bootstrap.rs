//! The helper is the only process that ever has inheritable browser handles.
use super::*;
use std::{fs::File, io::Read, os::windows::io::IntoRawHandle};

const CONTROL_PIPE: &str = "TERMCRAFT_BROWSER_CONTROL_PIPE";
const CONTROL_SERVER: &str = "TERMCRAFT_BROWSER_CONTROL_PID";
const BROWSER_IMAGE: &str = "TERMCRAFT_BROWSER_IMAGE";
const PROFILE_ROOT: &str = "TERMCRAFT_BROWSER_PROFILE_ROOT";
const ENTRY: &str = "windows_browser_bootstrap_entry";
const MAX_PROXY: usize = 8192;

pub(super) fn start(
    profile: &Path,
    chromium: &Path,
    security: &OwnerSecurity,
    read: &OwnedHandle,
    write: &OwnedHandle,
    proxy: &str,
    cancel: &AtomicBool,
) -> io::Result<Process> {
    let app = std::env::current_exe()?;
    let args = ["--exact".into(), ENTRY.into()];
    // Resolve known folders in the parent. Windows shell folder discovery can
    // fail once USERPROFILE and LOCALAPPDATA point to the private profile.
    let root = profile
        .parent()
        .ok_or_else(|| io::Error::other(START_ERROR))?;
    let paths = [
        (BROWSER_IMAGE, chromium.as_os_str().to_owned()),
        (PROFILE_ROOT, root.as_os_str().to_owned()),
    ];
    let (process, control) = connect(&app, &args, profile, security, &paths, cancel)?;
    if let Err(error) = send(&process, &control, read, write, proxy, cancel) {
        process.stop();
        return Err(error);
    }
    Ok(process)
}

pub(super) fn connect(
    app: &Path,
    args: &[OsString],
    profile: &Path,
    security: &OwnerSecurity,
    extra_env: &[(&str, OsString)],
    cancel: &AtomicBool,
) -> io::Result<(Process, Endpoint)> {
    if cancel.load(Ordering::Acquire) {
        return Err(io::Error::other(CANCELLED));
    }
    let name = format!(r"\\.\pipe\TermCraft-Browser-Control-{}", entropy()?);
    let name_wide = wide(OsStr::new(&name))?;
    let control = pipe_server(&name_wide, false, security)?;
    let mut environment = environment(profile)?;
    environment.extend([
        (CONTROL_PIPE, name.into()),
        (CONTROL_SERVER, std::process::id().to_string().into()),
    ]);
    environment.extend_from_slice(extra_env);
    let process = Process::launch(app, args, &environment, profile)?;
    if let Err(error) = await_helper(&control, &process, cancel) {
        process.stop();
        return Err(error);
    }
    Ok((process, control))
}

fn await_helper(control: &Endpoint, helper: &Process, cancel: &AtomicBool) -> io::Result<()> {
    let mut operation = OVERLAPPED {
        hEvent: control.event.as_raw_handle(),
        ..Default::default()
    };
    let pipe = control.file.as_raw_handle();
    unsafe {
        if ConnectNamedPipe(pipe, &mut operation) == 0 {
            match GetLastError() {
                ERROR_PIPE_CONNECTED => {}
                ERROR_IO_PENDING => {
                    let waits = [
                        control.event.as_raw_handle(),
                        helper.process.as_raw_handle(),
                    ];
                    let until = Instant::now() + Duration::from_secs(3);
                    loop {
                        match WaitForMultipleObjects(waits.len() as u32, waits.as_ptr(), 0, 15) {
                            WAIT_OBJECT_0 => break,
                            WAIT_TIMEOUT
                                if Instant::now() < until && !cancel.load(Ordering::Acquire) => {}
                            _ => {
                                CancelIoEx(pipe, &operation);
                                completed_transfer(pipe, &operation, true)?;
                                return Err(io::Error::other(START_ERROR));
                            }
                        }
                    }
                    completed_transfer(pipe, &operation, false)?;
                }
                error => return Err(io::Error::from_raw_os_error(error as i32)),
            }
        }
        let mut client = 0;
        if GetNamedPipeClientProcessId(pipe, &mut client) == 0
            || client != GetProcessId(helper.process.as_raw_handle())
        {
            return Err(io::Error::other(START_ERROR));
        }
    }
    Ok(())
}

pub(super) fn send(
    helper: &Process,
    control: &Endpoint,
    read: &OwnedHandle,
    write: &OwnedHandle,
    proxy: &str,
    cancel: &AtomicBool,
) -> io::Result<()> {
    if proxy.len() > MAX_PROXY {
        return Err(io::Error::other(START_ERROR));
    }
    let mut message = Vec::with_capacity(12 + proxy.len());
    for source in [read, write] {
        let mut remote = null_mut();
        if unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                source.as_raw_handle(),
                helper.process.as_raw_handle(),
                &mut remote,
                0,
                1,
                DUPLICATE_SAME_ACCESS,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        message.extend(
            u32::try_from(remote as usize)
                .map_err(|_| io::Error::other(START_ERROR))?
                .to_le_bytes(),
        );
    }
    message.extend((proxy.len() as u32).to_le_bytes());
    message.extend(proxy.as_bytes());
    let until = Instant::now() + Duration::from_millis(250);
    let mut remaining = message.as_slice();
    while !remaining.is_empty() {
        let written = control
            .transfer(IoBuffer::Write(remaining), until, cancel)?
            .filter(|&count| count != 0)
            .ok_or_else(|| io::Error::other(START_ERROR))?;
        remaining = &remaining[written..];
    }
    Ok(())
}

pub(super) fn receive() -> io::Result<(OwnedHandle, OwnedHandle, String)> {
    let name = std::env::var_os(CONTROL_PIPE).ok_or_else(|| io::Error::other(START_ERROR))?;
    let server: u32 = std::env::var(CONTROL_SERVER)
        .ok()
        .and_then(|pid| pid.parse().ok())
        .filter(|&pid| pid != 0 && pid != std::process::id())
        .ok_or_else(|| io::Error::other(START_ERROR))?;
    let name = wide(&name)?;
    // Restrict even a manually invoked entry to the local control namespace.
    let prefix: Vec<u16> = r"\\.\pipe\TermCraft-Browser-Control-"
        .encode_utf16()
        .collect();
    if !name.starts_with(&prefix)
        || name.len() != prefix.len() + 33
        || !name[prefix.len()..name.len() - 1]
            .iter()
            .all(|&ch| u8::try_from(ch).is_ok_and(|byte| byte.is_ascii_hexdigit()))
    {
        return Err(io::Error::other(START_ERROR));
    }
    let mut control = unsafe {
        let mut in_job = 0;
        if IsProcessInJob(GetCurrentProcess(), null_mut(), &mut in_job) == 0 || in_job == 0 {
            return Err(io::Error::other(START_ERROR));
        }
        let handle = handle(CreateFileW(
            name.as_ptr(),
            GENERIC_READ,
            0,
            null(),
            OPEN_EXISTING,
            SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
            null_mut(),
        ))?;
        let mut actual_server = 0;
        if GetNamedPipeServerProcessId(handle.as_raw_handle(), &mut actual_server) == 0
            || actual_server != server
        {
            return Err(io::Error::other(START_ERROR));
        }
        File::from_raw_handle(handle.into_raw_handle())
    };
    let mut header = [0u8; 12];
    control.read_exact(&mut header)?;
    let read = u32::from_le_bytes(header[0..4].try_into().unwrap()) as usize as HANDLE;
    let write = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize as HANDLE;
    let length = u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
    if read == write || length > MAX_PROXY || [read, write].contains(&control.as_raw_handle()) {
        return Err(io::Error::other(START_ERROR));
    }
    for raw in [read, write] {
        let mut flags = 0;
        if unsafe { GetFileType(raw) } != FILE_TYPE_PIPE
            || unsafe { GetHandleInformation(raw, &mut flags) } == 0
            || flags & HANDLE_FLAG_INHERIT == 0
        {
            return Err(io::Error::other(START_ERROR));
        }
    }
    // These two values identify distinct, validated handles duplicated into
    // this helper alone. Ownership is transferred to the returned handles.
    let (read, write) = unsafe {
        (
            OwnedHandle::from_raw_handle(read),
            OwnedHandle::from_raw_handle(write),
        )
    };
    let mut proxy = vec![0; length];
    control.read_exact(&mut proxy)?;
    let proxy = std::str::from_utf8(&proxy).map_err(io::Error::other)?;
    let proxy = crate::browser::validated_proxy(proxy).map_err(io::Error::other)?;
    Ok((read, write, proxy))
}

pub(in crate::browser) fn run_if_requested() -> Option<i32> {
    std::env::var_os(CONTROL_PIPE)?;
    if !std::env::args_os()
        .skip(1)
        .eq([OsStr::new("--exact"), OsStr::new(ENTRY)])
    {
        return None;
    }
    Some(if launch_browser().is_ok() { 0 } else { 1 })
}

fn launch_browser() -> io::Result<()> {
    let (read, write, proxy) = receive()?;
    let profile = std::env::current_dir()?;
    let local = std::env::var_os(PROFILE_ROOT)
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::other(START_ERROR))?;
    let name = profile
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or_default();
    let valid_name = name
        .strip_prefix("TermCraft-Browser-")
        .is_some_and(|suffix| {
            suffix.len() == 32 && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
        });
    if !valid_name
        || profile.parent().and_then(|path| path.canonicalize().ok()) != Some(local.canonicalize()?)
        || profile.symlink_metadata()?.file_type().is_symlink()
    {
        return Err(io::Error::other(START_ERROR));
    }
    let chromium = std::env::var_os(BROWSER_IMAGE)
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::other(START_ERROR))?;
    super::super::native_windows::verified_install_image(&chromium)?;
    let stdio = open_null()?;
    let read = read.as_raw_handle();
    let write = write.as_raw_handle();
    let mut args: Vec<OsString> = [
        "--headless",
        "--force-device-scale-factor=1",
        "--remote-debugging-pipe",
        "--no-first-run",
        "--no-default-browser-check",
        "--disable-background-networking",
        "--disable-component-update",
        "--disable-sync",
        "--disable-default-apps",
        "--disable-extensions",
        "--disable-features=MediaRouter,OptimizationHints",
        "--disable-breakpad",
        "--disable-crash-reporter",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    args.push(
        format!(
            "--remote-debugging-io-pipes={},{}",
            read as u32, write as u32
        )
        .into(),
    );
    let mut profile_arg = OsString::from("--user-data-dir=");
    profile_arg.push(&profile);
    args.push(profile_arg);
    if proxy.is_empty() {
        args.push("--no-proxy-server".into());
    } else {
        args.push(format!("--proxy-server={proxy}").into());
        args.push("--proxy-bypass-list=<-loopback>".into());
    }
    args.push("about:blank".into());
    let environment = environment(&profile)?;
    let directory = chromium
        .parent()
        .ok_or_else(|| io::Error::other(START_ERROR))?;
    spawn(
        &chromium,
        &args,
        &environment,
        directory,
        None,
        Some((stdio.as_raw_handle(), &[read, write, stdio.as_raw_handle()])),
    )?;
    Ok(())
}
