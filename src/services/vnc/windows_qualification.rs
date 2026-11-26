use super::*;
use crate::{
    credentials::Secret,
    services::{ServiceState, ServiceSupervisor, VncServiceConfig, VncSource},
};
use std::{
    io::{Read, Write},
    net::TcpStream,
    thread,
    time::{Duration, Instant},
};

fn read<const N: usize>(stream: &mut TcpStream) -> [u8; N] {
    let mut bytes = [0; N];
    stream.read_exact(&mut bytes).unwrap();
    bytes
}

#[test]
#[ignore = "requires an unlocked Windows desktop; reads one native pixel over loopback"]
fn windows_vnc_authenticated_view_only_pixel_and_stop() {
    let mut random = [0; 4];
    getrandom::fill(&mut random).unwrap();
    let password = Secret::new(format!("{:08x}", u32::from_ne_bytes(random)));
    let mut config = VncServiceConfig::local(VncSource::WindowsPrimaryDesktop, password.clone());
    assert!(!config.allow_control && !config.allow_clipboard);
    config.allow_unencrypted = true;
    let supervisor = ServiceSupervisor::default();
    let id = supervisor.start_vnc(config).unwrap();
    let status = supervisor.status(id).unwrap();
    assert_eq!(status.state, ServiceState::Running);
    let address = status.address.unwrap();
    assert!(address.ip().is_loopback());

    let timeout = Duration::from_secs(3);
    let mut stream = TcpStream::connect_timeout(&address, timeout).unwrap();
    stream.set_read_timeout(Some(timeout)).unwrap();
    stream.set_write_timeout(Some(timeout)).unwrap();
    assert_eq!(&read::<12>(&mut stream), b"RFB 003.008\n");
    stream.write_all(b"RFB 003.008\n").unwrap();
    assert_eq!(read::<2>(&mut stream), [1, 2]);
    stream.write_all(&[2]).unwrap();
    let challenge = read::<16>(&mut stream);
    let response = super::super::auth_response(&password, &challenge);
    stream.write_all(&*response).unwrap();
    assert_eq!(read::<4>(&mut stream), [0; 4]);
    stream.write_all(&[1]).unwrap();
    let init = read::<24>(&mut stream);
    let width = u16::from_be_bytes([init[0], init[1]]);
    let height = u16::from_be_bytes([init[2], init[3]]);
    assert!(width > 0 && height > 0);
    assert_eq!(init[4], 32);
    let name_length = u32::from_be_bytes(init[20..24].try_into().unwrap()) as usize;
    assert!((1..=256).contains(&name_length));
    let mut name = [0; 256];
    stream.read_exact(&mut name[..name_length]).unwrap();

    // Request one raw pixel only. Its content depends on the current desktop,
    // so consume it without displaying, recording, or asserting RGB values.
    stream.write_all(&[3, 0, 0, 0, 0, 0, 0, 1, 0, 1]).unwrap();
    assert_eq!(read::<4>(&mut stream), [0, 0, 0, 1]);
    assert_eq!(
        read::<12>(&mut stream),
        [0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 0]
    );
    let mut pixel = Zeroizing::new([0u8; 4]);
    stream.read_exact(&mut *pixel).unwrap();
    drop(pixel);

    supervisor.stop(id).unwrap();
    let deadline = Instant::now() + Duration::from_secs(6);
    loop {
        let status = supervisor.status(id).unwrap();
        if status.state == ServiceState::Stopped {
            assert!(status.error.is_none());
            break;
        }
        assert!(Instant::now() < deadline, "VNC service did not stop");
        thread::sleep(Duration::from_millis(20));
    }
    let mut remaining = [0; 1];
    match stream.read(&mut remaining) {
        Ok(0) => {}
        Err(error) if error.kind() == io::ErrorKind::ConnectionReset => {}
        _ => panic!("VNC client stream stayed open after stop"),
    }
    assert!(TcpStream::connect_timeout(&address, timeout).is_err());
}

#[test]
#[ignore = "requires Windows desktop-object creation rights"]
fn windows_vnc_refuses_capture_and_control_from_an_inactive_desktop() {
    struct PrivateDesktop {
        previous: HDESK,
        handle: HDESK,
    }

    impl Drop for PrivateDesktop {
        fn drop(&mut self) {
            // Return only this dedicated thread to its original desktop.
            unsafe {
                SetThreadDesktop(self.previous);
                CloseDesktop(self.handle);
            }
        }
    }

    thread::spawn(|| {
        let mut random = [0; 8];
        getrandom::fill(&mut random).unwrap();
        let name: Vec<u16> = format!("TermCraftVncTest{:016x}\0", u64::from_ne_bytes(random))
            .encode_utf16()
            .collect();
        let previous = unsafe {
            GetThreadDesktop(windows_sys::Win32::System::Threading::GetCurrentThreadId())
        };
        assert!(!previous.is_null(), "{}", fail());
        let handle = unsafe {
            CreateDesktopW(
                name.as_ptr(),
                ptr::null(),
                ptr::null(),
                0,
                DESKTOP_CREATEWINDOW | DESKTOP_READOBJECTS | DESKTOP_WRITEOBJECTS,
                ptr::null(),
            )
        };
        assert!(!handle.is_null(), "{}", fail());
        let desktop = PrivateDesktop { previous, handle };
        // SetThreadDesktop never changes which desktop receives system input.
        assert_ne!(unsafe { SetThreadDesktop(desktop.handle) }, 0, "{}", fail());
        // Exercise an already-created provider even when the test runner began
        // on a disconnected desktop and cannot start a live share.
        let mut source = Desktop {
            width: 1,
            height: 1,
            control: true,
            buttons: 0,
            held: Vec::new(),
            temporary: Vec::new(),
            clipboard_enabled: false,
            clipboard: None,
        };
        let diagnosis = active_desktop().unwrap_err().to_string();
        assert!(Desktop::open(true).is_err());
        assert_eq!(source.capture().err().unwrap().to_string(), diagnosis);
        // There are no held inputs to release. No SendInput or clipboard path
        // can run even if the active-desktop check regresses.
        assert_eq!(
            source.input(Input::Release).unwrap_err().to_string(),
            diagnosis
        );
        drop(source);
        drop(desktop);
    })
    .join()
    .unwrap();
}

#[test]
#[ignore = "uses an owned input window on a disposable GitHub-hosted Windows VM"]
fn windows_vnc_control_reaches_only_the_disposable_desktop_fixture() {
    use windows_sys::Win32::Foundation::{HWND, POINT};

    assert!(
        std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true")
            && std::env::var("RUNNER_ENVIRONMENT").as_deref() == Ok("github-hosted")
            && std::env::var("TERMCRAFT_VNC_DISPOSABLE_CONTROL").as_deref() == Ok("1"),
        "Windows VNC control requires explicit opt-in on a disposable GitHub-hosted VM"
    );

    struct FixtureWindow(HWND);
    impl Drop for FixtureWindow {
        fn drop(&mut self) {
            unsafe { DestroyWindow(self.0) };
        }
    }

    thread::spawn(|| unsafe {
        active_desktop().expect("The hosted runner needs an active input desktop");
        let (width, height) = metrics().unwrap();
        assert!(width >= 320 && height >= 240);
        let x = width / 2;
        let y = height / 2;
        let point = POINT {
            x: i32::from(x),
            y: i32::from(y),
        };
        let window = CreateWindowExW(
            0,
            windows_sys::w!("EDIT"),
            ptr::null(),
            WS_POPUP | WS_VISIBLE | WS_BORDER,
            point.x - 80,
            point.y - 40,
            160,
            80,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null(),
        );
        assert!(!window.is_null(), "{}", fail());
        let fixture = FixtureWindow(window);
        BringWindowToTop(fixture.0);
        SetForegroundWindow(fixture.0);
        SetFocus(fixture.0);

        let verify_target = || {
            active_desktop().unwrap();
            assert_eq!(GetForegroundWindow(), fixture.0);
            assert_eq!(GetFocus(), fixture.0);
            assert_eq!(WindowFromPoint(point), fixture.0);
        };
        let mut source = Desktop::open(true).unwrap();
        for (operation, event) in [
            (
                "Unicode key down",
                Input::Key {
                    down: true,
                    keysym: 0x010003a9,
                },
            ),
            (
                "Unicode key up",
                Input::Key {
                    down: false,
                    keysym: 0x010003a9,
                },
            ),
            ("left mouse down", Input::Pointer { buttons: 1, x, y }),
            ("left mouse up", Input::Pointer { buttons: 0, x, y }),
        ] {
            verify_target();
            source
                .input(event)
                .unwrap_or_else(|e| panic!("{operation}: {e}"));
        }

        let (mut text, mut pressed, mut released) = (false, false, false);
        let deadline = Instant::now() + Duration::from_secs(3);
        while !(text && pressed && released) && Instant::now() < deadline {
            let mut message = MSG::default();
            while PeekMessageW(&mut message, fixture.0, 0, 0, PM_REMOVE) != 0 {
                match message.message {
                    WM_CHAR => text |= message.wParam == '\u{3a9}' as usize,
                    WM_LBUTTONDOWN => pressed = true,
                    WM_LBUTTONUP => released = true,
                    _ => {}
                }
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(
            (text && pressed && released),
            "Fixture events: {text}/{pressed}/{released}"
        );
        verify_target();
        drop(source);
    })
    .join()
    .unwrap();
}
