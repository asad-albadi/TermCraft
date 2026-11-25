//! Private Windows provider. The unpatched upstream executable is never accepted.
use super::{
    XDisplayMode, XServerLease, XServerOptions,
    native_artifact::{VerifiedSnapshot, protect_directory},
    native_policy, native_transport,
};
use std::{
    ffi::OsString,
    net::{SocketAddr, TcpListener},
    path::Path,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
use windows_sys::Win32::{NetworkManagement::IpHelper::*, Networking::WinSock::AF_INET};
pub(super) fn bundled_runtime(root: &Path) -> Result<(std::path::PathBuf, String), String> {
    let pin = native_policy::windows_pin()?;
    let package = root.join("x11/windows-x86_64-private1");
    let _verified = VerifiedSnapshot::create(&package, pin.files)?;
    Ok((package.join(pin.executable), pin.version.into()))
}
pub(super) fn start(
    root: &Path,
    options: XServerOptions,
    flag: &AtomicBool,
) -> Result<XServerLease, String> {
    native_transport::cancelled(flag)?;
    if options.artifact_id != native_policy::WINDOWS_ARTIFACT {
        return Err("Unsupported packaged Windows X server artifact".into());
    }
    if options.monitor.is_some() {
        return Err(
            "Explicit monitor selection is not yet qualified for the private Windows X server"
                .into(),
        );
    }
    let pin = native_policy::windows_pin()?;
    launch(root, options, flag, pin)
}
fn launch(
    root: &Path,
    options: XServerOptions,
    flag: &AtomicBool,
    pin: &native_policy::NativePin<'_>,
) -> Result<XServerLease, String> {
    let image = VerifiedSnapshot::create(&root.join("x11/windows-x86_64-private1"), pin.files)?;
    let runtime = tempfile::Builder::new()
        .prefix("termcraft-x11-")
        .tempdir()
        .map_err(|e| e.to_string())?;
    let runtime_guard = protect_directory(runtime.path())?;
    // Let the OS choose one owned ephemeral port. No X display scanning or
    // deletion occurs. A race after release fails the engine's exclusive bind.
    let reservation = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let address = reservation.local_addr().map_err(|e| e.to_string())?;
    let display = address
        .port()
        .checked_sub(6000)
        .ok_or("Reserved X port is outside the display range")?;
    let authority_path = runtime.path().join("Xauthority");
    let mut cookie = zeroize::Zeroizing::new([0; 16]);
    getrandom::fill(cookie.as_mut()).map_err(|e| e.to_string())?;
    native_transport::write_authority(&authority_path, display, &cookie)?;
    let mut arguments: Vec<OsString> = [
        format!(":{display}"),
        "-noreset".into(),
        "-silent-dup-error".into(),
        "-nolisten".into(),
        "inet6".into(),
        "-nolisten".into(),
        "hyperv".into(),
        "-nopn".into(),
        "-listen".into(),
        "tcp".into(),
        "-iglx".into(),
        "-noclipboard".into(),
        "-auth".into(),
    ]
    .into_iter()
    .map(Into::into)
    .collect();
    arguments.push(authority_path.as_os_str().into());
    arguments.push("-logfile".into());
    arguments.push(runtime.path().join("server.log").into_os_string());
    if options.display_mode == XDisplayMode::Fullscreen {
        arguments.push("-fullscreen".into());
    }
    let mut environment = vec![
        ("PATH".into(), image.root().as_os_str().into()),
        ("TEMP".into(), runtime.path().as_os_str().into()),
        ("TMP".into(), runtime.path().as_os_str().into()),
        ("HOME".into(), runtime.path().as_os_str().into()),
    ];
    for name in ["SystemRoot", "WINDIR"] {
        if let Some(value) = std::env::var_os(name) {
            environment.push((name.into(), value));
        }
    }
    native_transport::cancelled(flag)?;
    let (mut process, thread) = super::engine::OwnedProcess::spawn_suspended(
        &image.root().join(pin.executable),
        &arguments,
        image.root(),
        &environment,
    )
    .map_err(|e| e.to_string())?;
    native_transport::cancelled(flag)?;
    drop(reservation);
    process.resume(&thread).map_err(|e| e.to_string())?;
    drop(thread);
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        native_transport::cancelled(flag)?;
        if !process.is_running().map_err(|e| e.to_string())? {
            return Err("Private Windows X server exited before readiness; inspect its private native build/dependencies".into());
        }
        if Instant::now() >= deadline {
            return Err("Private Windows X server timed out opening its reserved display".into());
        }
        if owns_listener(process.id().ok_or("Native X process exited")?, address)? {
            match native_transport::setup(address, None, flag) {
                Ok(true) => {
                    return Err(
                        "Private Windows X server allowed a client without Xauthority".into(),
                    );
                }
                Ok(false) => {
                    if native_transport::setup(address, Some(&cookie), flag)?
                        && owns_listener(process.id().ok_or("Native X process exited")?, address)?
                    {
                        break;
                    }
                }
                Err(_) => {}
            }
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    native_transport::cancelled(flag)?;
    Ok(XServerLease {
        alive: tokio::sync::watch::Sender::new(true),
        display: format!("127.0.0.1:{display}"),
        authority_path,
        process,
        _native_image: image,
        _runtime_guard: runtime_guard,
        _runtime: runtime,
    })
}

fn owns_listener(pid: u32, address: SocketAddr) -> Result<bool, String> {
    let mut size = 0;
    unsafe {
        GetExtendedTcpTable(
            std::ptr::null_mut(),
            &mut size,
            0,
            AF_INET as u32,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        );
    }
    for _ in 0..3 {
        if size as usize > 4 * 1024 * 1024 || size < 4 {
            return Err("Invalid native listener ownership table size".into());
        }
        let mut storage = vec![0u64; (size as usize).div_ceil(8)];
        let result = unsafe {
            GetExtendedTcpTable(
                storage.as_mut_ptr().cast(),
                &mut size,
                0,
                AF_INET as u32,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            )
        };
        if result == 122 {
            continue;
        }
        if result != 0 {
            return Err(format!(
                "Cannot prove native X listener ownership: {result}"
            ));
        }
        let table = storage.as_ptr().cast::<MIB_TCPTABLE_OWNER_PID>();
        let count = unsafe { (*table).dwNumEntries } as usize;
        if count > (storage.len() * 8 - 4) / size_of::<MIB_TCPROW_OWNER_PID>() {
            return Err("Invalid native listener ownership table".into());
        }
        let rows = unsafe {
            std::slice::from_raw_parts(
                std::ptr::addr_of!((*table).table).cast::<MIB_TCPROW_OWNER_PID>(),
                count,
            )
        };
        let mut found = false;
        for row in rows.iter().filter(|row| row.dwOwningPid == pid) {
            if row.dwLocalAddr != u32::from_ne_bytes([127, 0, 0, 1])
                || u16::from_be(row.dwLocalPort as u16) != address.port()
            {
                return Err("Private X engine opened an unowned or non-loopback listener".into());
            }
            found = true;
        }
        return Ok(found);
    }
    Err("Native listener ownership table kept changing".into())
}

#[cfg(test)]
mod candidate_qualification {
    use super::*;
    use std::path::PathBuf;
    use x11rb::{
        connection::Connection,
        protocol::xproto::{ConnectionExt, CreateWindowAux, MapState, WindowClass},
    };

    #[allow(dead_code)]
    mod ssh_fixture {
        use crate as harbor_workspace;
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/x11_ssh.rs"
        ));
    }

    #[test]
    #[ignore = "requires the privately rebuilt Windows X engine and its review inventory"]
    fn privately_built_engine_owns_listener_and_stops_cleanly() {
        let root = PathBuf::from(std::env::var_os("TERMCRAFT_TEST_WINDOWS_X11_ROOT").unwrap());
        let inventory =
            PathBuf::from(std::env::var_os("TERMCRAFT_TEST_WINDOWS_X11_INVENTORY").unwrap());
        let report: serde_json::Value =
            serde_json::from_reader(std::fs::File::open(inventory).unwrap()).unwrap();
        let files = report["files"].as_object().unwrap();
        let entries: Vec<_> = files
            .iter()
            .map(|(path, hash)| (path.as_str(), hash.as_str().unwrap()))
            .collect();
        let pin = native_policy::NativePin {
            files: &entries,
            executable: "vcxsrv.exe",
            version: "local qualification",
        };
        let options = XServerOptions {
            artifact_id: native_policy::WINDOWS_ARTIFACT.into(),
            ..Default::default()
        };
        let flag = AtomicBool::new(false);
        let lease = launch(&root, options, &flag, &pin).unwrap();
        let pid = lease.process_id().unwrap();
        let display: u16 = lease.display().split_once(':').unwrap().1.parse().unwrap();
        let address: SocketAddr = ([127, 0, 0, 1], display + 6000).into();
        let runtime = lease.authority_path().parent().unwrap().to_owned();
        assert!(owns_listener(pid, address).unwrap());
        assert!(!native_transport::setup(address, None, &flag).unwrap());
        assert!(!native_transport::setup(address, Some(&[0; 16]), &flag).unwrap());
        let fixture = ssh_fixture::Fixture::start();
        let (ssh, request) = fixture.connect(1, &lease);
        assert!(fixture.x_client(&request, false).is_err());
        let client = fixture.x_client(&request, true).unwrap();
        let screen = &client.setup().roots[0];
        let window = client.generate_id().unwrap();
        client
            .create_window(
                x11rb::COPY_DEPTH_FROM_PARENT,
                window,
                screen.root,
                24,
                24,
                150,
                80,
                0,
                WindowClass::INPUT_OUTPUT,
                screen.root_visual,
                &CreateWindowAux::new().background_pixel(screen.white_pixel),
            )
            .unwrap()
            .check()
            .unwrap();
        client.map_window(window).unwrap().check().unwrap();
        assert_eq!(
            client
                .get_window_attributes(window)
                .unwrap()
                .reply()
                .unwrap()
                .map_state,
            MapState::VIEWABLE
        );
        drop(ssh);
        lease.stop_owned().unwrap();
        assert!(!owns_listener(pid, address).unwrap());
        assert!(!runtime.exists());
    }
}
