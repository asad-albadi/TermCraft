//! Private Windows provider. The unpatched upstream executable is never accepted.
#[cfg(target_arch = "x86_64")]
use super::xdmcp::{ClipboardPolicy, QueryMode, XdmcpConfig, windows_network};
use super::{
    XDisplayMode, XServerLease, XServerOptions,
    native_artifact::{VerifiedSnapshot, protect_directory},
    native_policy, native_transport,
    xdmcp::windows_monitor,
};
use std::{
    ffi::OsString,
    net::{Ipv4Addr, SocketAddr, TcpListener},
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
    let fullscreen = options.display_mode == XDisplayMode::Fullscreen;
    let monitor = windows_monitor(options.monitor.as_deref(), fullscreen)?;
    let pin = native_policy::windows_pin()?;
    let request = DisplayRequest {
        fullscreen,
        screen: monitor.map(|monitor| format!("@{monitor}")),
        ..DisplayRequest::local()
    };
    launch(root, request, flag, pin)
}

#[cfg(target_arch = "x86_64")]
pub(super) fn start_xdmcp(
    root: &Path,
    config: &XdmcpConfig,
    width: u32,
    height: u32,
    flag: &AtomicBool,
) -> Result<XServerLease, String> {
    native_transport::cancelled(flag)?;
    let request = DisplayRequest::xdmcp(config, width, height)?;
    let pin = native_policy::windows_pin()?;
    launch(root, request, flag, pin)
}

struct DisplayRequest {
    address: Ipv4Addr,
    display: Option<u16>,
    fullscreen: bool,
    clipboard: bool,
    screen: Option<String>,
    #[cfg(target_arch = "x86_64")]
    keyboard: String,
    extra: Vec<OsString>,
}

impl DisplayRequest {
    fn local() -> Self {
        Self {
            address: Ipv4Addr::LOCALHOST,
            display: None,
            fullscreen: false,
            clipboard: false,
            screen: None,
            #[cfg(target_arch = "x86_64")]
            keyboard: String::new(),
            extra: Vec::new(),
        }
    }

    #[cfg(target_arch = "x86_64")]
    fn xdmcp(config: &XdmcpConfig, width: u32, height: u32) -> Result<Self, String> {
        let (manager, address) = windows_network(config, width, height)?;
        let monitor = windows_monitor(config.monitor.as_deref(), config.fullscreen)?;
        let query = if config.mode == QueryMode::Indirect {
            "-indirect"
        } else {
            "-query"
        };
        Ok(Self {
            address,
            display: config.display,
            fullscreen: config.fullscreen,
            clipboard: config.clipboard == ClipboardPolicy::WindowsBidirectional,
            screen: Some(monitor.map_or_else(
                || format!("{width}x{height}"),
                |monitor| format!("@{monitor}"),
            )),
            keyboard: config.keyboard.clone(),
            extra: vec![
                "-from".into(),
                address.to_string().into(),
                "-port".into(),
                manager.port().to_string().into(),
                query.into(),
                manager.ip().to_string().into(),
            ],
        })
    }
}

fn launch(
    root: &Path,
    request: DisplayRequest,
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
    let port = request
        .display
        .map_or(Some(0), |display| display.checked_add(6000))
        .ok_or("Display is outside the Windows X port range")?;
    let reservation = TcpListener::bind((request.address, port)).map_err(|e| {
        format!(
            "Cannot reserve the private X display on {}: {e}",
            request.address
        )
    })?;
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
        "-auth".into(),
    ]
    .into_iter()
    .map(Into::into)
    .collect();
    arguments.push(authority_path.as_os_str().into());
    arguments.push("-logfile".into());
    arguments.push(runtime.path().join("server.log").into_os_string());
    if request.clipboard {
        arguments.extend(["-clipboard".into(), "-noprimary".into()]);
    } else {
        arguments.push("-noclipboard".into());
    }
    if request.fullscreen {
        // Shadow GDI displays a fullscreen window without changing the host
        // monitor's display mode through exclusive DirectDraw.
        arguments.extend(["-fullscreen".into(), "-engine".into(), "1".into()]);
    }
    if let Some(screen) = request.screen {
        arguments.extend(["-screen".into(), "0".into(), screen.into()]);
    }
    #[cfg(target_arch = "x86_64")]
    if !request.keyboard.is_empty() {
        let catalog = std::fs::read_to_string(image.root().join("xkbdata/rules/base.lst"))
            .map_err(|error| {
                format!("Cannot read the private Windows XKB layout catalog: {error}")
            })?;
        if !super::xdmcp::registered_layout(&catalog, &request.keyboard) {
            return Err(format!(
                "XKB keyboard layout '{}' is absent from the private Windows XKB catalog",
                request.keyboard
            ));
        }
        arguments.extend([
            "-xkbrules".into(),
            "base".into(),
            "-xkblayout".into(),
            request.keyboard.into(),
        ]);
    }
    arguments.extend(request.extra);
    let mut environment = vec![
        ("PATH".into(), image.root().as_os_str().into()),
        ("TEMP".into(), runtime.path().as_os_str().into()),
        ("TMP".into(), runtime.path().as_os_str().into()),
        ("HOME".into(), runtime.path().as_os_str().into()),
    ];
    if request.address != Ipv4Addr::LOCALHOST {
        environment.push((
            "TERMCRAFT_X11_LISTEN_IPV4".into(),
            request.address.to_string().into(),
        ));
    }
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
            match native_transport::setup_owned_tcp(address, None, flag) {
                Ok(true) => {
                    return Err(
                        "Private Windows X server allowed a client without Xauthority".into(),
                    );
                }
                Ok(false) => {
                    if native_transport::setup_owned_tcp(address, Some(&cookie), flag)?
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
        display: format!("{}:{display}", request.address),
        authority_path,
        process,
        _native_image: image,
        _runtime_guard: runtime_guard,
        _runtime: runtime,
    })
}

fn owns_listener(pid: u32, address: SocketAddr) -> Result<bool, String> {
    let SocketAddr::V4(expected) = address else {
        return Err("The private Windows X listener requires an IPv4 address".into());
    };
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
            if row.dwLocalAddr != u32::from_ne_bytes(expected.ip().octets())
                || u16::from_be(row.dwLocalPort as u16) != address.port()
            {
                return Err(
                    "Private X engine opened a listener outside its reserved address and port"
                        .into(),
                );
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

    fn fixture_lease(request: DisplayRequest) -> XServerLease {
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
        launch(&root, request, &AtomicBool::new(false), &pin).unwrap()
    }

    #[test]
    #[ignore = "requires the privately rebuilt Windows X engine and its review inventory"]
    fn privately_built_engine_owns_listener_and_stops_cleanly() {
        let flag = AtomicBool::new(false);
        let lease = fixture_lease(DisplayRequest::local());
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

    #[test]
    #[ignore = "requires the private Windows X candidate, native xauth and a selected OpenSSH client"]
    fn installed_windows_openssh_forwards_authenticated_x11() {
        use crate::process::Process;
        use std::process::Command;

        let lease = fixture_lease(DisplayRequest::local());
        let fixture = ssh_fixture::Fixture::start();
        let config = fixture.config(1);
        let scratch = tempfile::tempdir().unwrap();
        let empty_config = scratch.path().join("ssh_config");
        std::fs::write(&empty_config, "").unwrap();
        let known_hosts = scratch.path().join("known_hosts");
        std::fs::copy(config.app_known_hosts, &known_hosts).unwrap();
        let system_root = PathBuf::from(std::env::var_os("SystemRoot").unwrap());
        let ssh = PathBuf::from(std::env::var_os("TERMCRAFT_TEST_WINDOWS_OPENSSH").unwrap());
        assert!(ssh.is_absolute() && ssh.is_file());
        let xauth = PathBuf::from(std::env::var_os("TERMCRAFT_TEST_WINDOWS_XAUTH").unwrap());
        assert!(xauth.is_absolute() && xauth.is_file());
        let authority = scratch.path().join(".Xauthority");
        std::fs::copy(lease.authority_path(), &authority).unwrap();
        let listing = Command::new(&xauth)
            .arg("list")
            .arg(lease.display())
            .env("XAUTHORITY", &authority)
            .output()
            .unwrap();
        assert!(listing.status.success() && !listing.stdout.is_empty());
        let xauth_root = xauth.parent().unwrap();
        let path =
            std::env::join_paths([xauth_root.to_owned(), system_root.join("System32")]).unwrap();
        let args = vec![
            "-vv".into(),
            "-Y".into(),
            "-T".into(),
            "-F".into(),
            empty_config.to_str().unwrap().to_owned(),
            "-oBatchMode=yes".into(),
            "-oIdentityFile=none".into(),
            "-oIdentityAgent=none".into(),
            "-oPreferredAuthentications=none".into(),
            "-oGlobalKnownHostsFile=none".into(),
            "-oStrictHostKeyChecking=yes".into(),
            format!(
                "-oUserKnownHostsFile=\"{}\"",
                known_hosts.display().to_string().replace('\\', "/")
            ),
            format!(
                "-oXAuthLocation=\"{}\"",
                xauth.display().to_string().replace('\\', "/")
            ),
            "-p".into(),
            config.session.port.to_string(),
            "fixture@127.0.0.1".into(),
        ];
        let environment = [
            (
                "ComSpec".into(),
                system_root.join("System32/cmd.exe").into_os_string(),
            ),
            ("PATH".into(), path),
            ("HOME".into(), scratch.path().into()),
            ("USERPROFILE".into(), scratch.path().into()),
            ("TEMP".into(), scratch.path().into()),
            ("TMP".into(), scratch.path().into()),
            ("DISPLAY".into(), lease.display().into()),
            ("XAUTHORITY".into(), authority.into_os_string()),
        ];
        let mut child = Process::spawn_with_environment(
            ssh.to_str().unwrap(),
            &args,
            Some(xauth_root),
            120,
            35,
            &environment,
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut log = Vec::new();
        let connection = loop {
            if let Some(request) = fixture.take_request() {
                break fixture.x_client(&request, true).is_ok_and(|client| {
                    client
                        .get_input_focus()
                        .is_ok_and(|request| request.reply().is_ok())
                });
            }
            child.poll();
            if child.exited || Instant::now() >= deadline {
                break false;
            }
            if let Ok(chunk) = child.rx.recv_timeout(Duration::from_millis(15)) {
                log.extend(chunk);
            }
        };
        while let Ok(chunk) = child.rx.try_recv() {
            log.extend(chunk);
        }
        drop(child);
        assert!(
            connection,
            "Installed OpenSSH X11 probe failed:\n{}",
            String::from_utf8_lossy(&log)
        );
        lease.stop_owned().unwrap();
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    #[ignore = "requires the private Windows XDMCP candidate and its review inventory"]
    fn private_xdmcp_binds_only_the_requested_interface_and_accepts_the_manager() {
        use std::net::{TcpStream, UdpSocket};
        use windows_sys::{
            Win32::{
                Foundation::{LPARAM, RECT},
                Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO},
            },
            core::BOOL,
        };
        use x11rb::rust_connection::{DefaultStream, RustConnection};

        unsafe extern "system" fn first_monitor(
            monitor: HMONITOR,
            _dc: HDC,
            _rect: *mut RECT,
            data: LPARAM,
        ) -> BOOL {
            unsafe { GetMonitorInfoW(monitor, data as *mut MONITORINFO) };
            0
        }

        fn receive(peer: &UdpSocket, opcode: u8) -> (Vec<u8>, SocketAddr) {
            let mut packet = [0; 1024];
            let (size, sender) = peer.recv_from(&mut packet).unwrap();
            assert!(size >= 6);
            assert_eq!(&packet[..4], &[0, 1, 0, opcode]);
            assert_eq!(
                u16::from_be_bytes([packet[4], packet[5]]) as usize,
                size - 6
            );
            (packet[6..size].to_vec(), sender)
        }

        fn send(peer: &UdpSocket, target: SocketAddr, opcode: u8, data: &[u8]) {
            let mut packet = vec![0, 1, 0, opcode];
            packet.extend_from_slice(&(data.len() as u16).to_be_bytes());
            packet.extend_from_slice(data);
            peer.send_to(&packet, target).unwrap();
        }

        let interface = Ipv4Addr::new(127, 0, 0, 2);
        let mut monitor = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        unsafe {
            EnumDisplayMonitors(
                std::ptr::null_mut(),
                std::ptr::null(),
                Some(first_monitor),
                std::ptr::addr_of_mut!(monitor) as LPARAM,
            );
        }
        let work_area = monitor.rcWork;
        assert!(work_area.right > work_area.left && work_area.bottom > work_area.top);
        let peer = UdpSocket::bind((interface, 0)).unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let config = XdmcpConfig {
            selected_endpoint: Some(peer.local_addr().unwrap()),
            advertise_address: Some(interface),
            allow_network_listener: true,
            keyboard: "de".into(),
            fullscreen: true,
            monitor: Some("1".into()),
            ..Default::default()
        };
        let lease = fixture_lease(DisplayRequest::xdmcp(&config, 800, 600).unwrap());
        let pid = lease.process_id().unwrap();
        let (host, display) = lease.display().split_once(':').unwrap();
        assert_eq!(host, interface.to_string());
        let display: u16 = display.parse().unwrap();
        let address = (interface, display + 6000).into();
        let flag = AtomicBool::new(false);
        assert!(owns_listener(pid, address).unwrap());

        let (_, target) = receive(&peer, 2);
        assert_eq!(target.ip(), interface);
        send(&peer, target, 5, b"\0\0\0\x07fixture\0\x05ready");
        let (request, sender) = receive(&peer, 7);
        assert_eq!(sender, target);
        assert_eq!(&request[..2], &display.to_be_bytes());
        let cookie = [41; 16];
        let mut accepted = b"\0\0\0\x11\0\0\0\0\0\x12MIT-MAGIC-COOKIE-1\0\x10".to_vec();
        accepted.extend(cookie);
        send(&peer, target, 8, &accepted);
        let (manage, sender) = receive(&peer, 10);
        assert_eq!(sender, target);
        assert_eq!(&manage[..4], &17u32.to_be_bytes());
        assert_eq!(&manage[4..6], &display.to_be_bytes());
        assert!(!native_transport::setup_owned_tcp(address, None, &flag).unwrap());
        let stream = TcpStream::connect_timeout(&address, Duration::from_secs(3)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let (stream, _) = DefaultStream::from_tcp_stream(stream).unwrap();
        let x = RustConnection::connect_to_stream_with_auth_info(
            stream,
            0,
            b"MIT-MAGIC-COOKIE-1".to_vec(),
            cookie.to_vec(),
        )
        .unwrap();
        let screen = &x.setup().roots[0];
        assert_eq!(
            i32::from(screen.width_in_pixels),
            work_area.right - work_area.left
        );
        assert_eq!(
            i32::from(screen.height_in_pixels),
            work_area.bottom - work_area.top
        );
        // VcXsrv's PC keycode 29 is the top-row Y/Z key.
        let mapping = x.get_keyboard_mapping(29, 1).unwrap().reply().unwrap();
        assert_eq!(mapping.keysyms.first(), Some(&u32::from(b'z')));
        lease.stop_owned().unwrap();
        assert!(!owns_listener(pid, address).unwrap());
    }
}
