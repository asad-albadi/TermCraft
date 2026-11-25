//! Private-key additions require the standard local pipe owned by the running SSH agent service.
use std::{os::windows::io::AsRawHandle, path::Path, time::Duration};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};
use windows_sys::Win32::{
    Foundation::ERROR_PIPE_BUSY,
    System::{Pipes::GetNamedPipeServerProcessId, Services::*},
};
pub(super) async fn private_service_pipe(path: &Path) -> Result<NamedPipeClient, String> {
    if !path
        .to_string_lossy()
        .eq_ignore_ascii_case(r"\\.\pipe\openssh-ssh-agent")
    {
        return Err("Private keys are added only to the verified Windows OpenSSH service, Pageant, or the TermCraft-owned agent.".into());
    }
    let pipe = loop {
        match ClientOptions::new().open(path) {
            Ok(pipe) => break pipe,
            Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
                tokio::time::sleep(Duration::from_millis(20)).await
            }
            Err(_) => return Err("Windows OpenSSH agent is unavailable.".into()),
        }
    };
    unsafe {
        let mut pid = 0;
        if GetNamedPipeServerProcessId(pipe.as_raw_handle(), &mut pid) == 0 {
            return Err("Could not verify agent pipe ownership.".into());
        }
        let scm = OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT);
        if scm.is_null() {
            return Err("Could not inspect the SSH agent service.".into());
        }
        let name = widestring::U16CString::from_str("ssh-agent").unwrap();
        let service = OpenServiceW(scm, name.as_ptr(), SERVICE_QUERY_STATUS);
        if service.is_null() {
            CloseServiceHandle(scm);
            return Err("Windows OpenSSH agent service is unavailable.".into());
        }
        let mut status: SERVICE_STATUS_PROCESS = std::mem::zeroed();
        let mut needed = 0;
        let ok = QueryServiceStatusEx(
            service,
            SC_STATUS_PROCESS_INFO,
            (&mut status as *mut SERVICE_STATUS_PROCESS).cast(),
            std::mem::size_of::<SERVICE_STATUS_PROCESS>() as u32,
            &mut needed,
        );
        CloseServiceHandle(service);
        CloseServiceHandle(scm);
        if ok == 0 || status.dwCurrentState != SERVICE_RUNNING || status.dwProcessId != pid {
            return Err("Agent pipe does not belong to the running Windows OpenSSH agent service; no private key was sent.".into());
        }
    }
    Ok(pipe)
}
