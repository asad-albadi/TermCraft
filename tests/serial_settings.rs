use harbor_workspace::{
    model::Session,
    serial::{FlowControl, Parity, SerialLine, SerialSettings},
};
#[test]
fn old_serial_session_stays_9600_8n1_and_new_settings_roundtrip() {
    let s: Session = serde_json::from_str(r#"{"protocol":"Serial"}"#).unwrap();
    assert_eq!(s.baud, 9600);
    assert_eq!(s.serial_line, SerialLine::default());
    let mut s = s;
    s.baud = 19200;
    s.serial_line = SerialLine {
        data_bits: 7,
        parity: Parity::Even,
        stop_bits: 2,
        flow_control: FlowControl::Hardware,
    };
    let decoded: Session = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
    assert_eq!(decoded.serial_line, s.serial_line);
    assert_eq!(decoded.baud, 19200);
}
#[test]
fn invalid_serial_settings_fail_before_device_open() {
    for settings in [
        SerialSettings {
            data_bits: 5,
            stop_bits: 2,
            ..Default::default()
        },
        SerialSettings {
            data_bits: 9,
            ..Default::default()
        },
        SerialSettings {
            baud: 0,
            ..Default::default()
        },
        SerialSettings {
            stop_bits: 3,
            ..Default::default()
        },
    ] {
        assert!(settings.validate().is_err());
        assert!(
            harbor_workspace::serial::Serial::open_with_settings("/does/not/exist", settings)
                .unwrap_err_message()
                .contains("Choose")
        );
    }
}
trait ErrorMessage {
    fn unwrap_err_message(self) -> String;
}
impl ErrorMessage for Result<harbor_workspace::serial::Serial, String> {
    fn unwrap_err_message(self) -> String {
        match self {
            Ok(_) => panic!("invalid settings opened a device"),
            Err(e) => e,
        }
    }
}
#[cfg(unix)]
#[test]
fn serial_settings_map_to_native_flags() {
    let settings = SerialSettings {
        baud: 19200,
        data_bits: 7,
        parity: Parity::Even,
        stop_bits: 2,
        flow_control: FlowControl::Hardware,
    };
    let mut t: libc::termios = unsafe { std::mem::zeroed() };
    settings.apply_termios(&mut t).unwrap();
    assert_eq!(t.c_cflag & libc::CSIZE, libc::CS7);
    assert_ne!(t.c_cflag & libc::PARENB, 0);
    assert_eq!(t.c_cflag & libc::PARODD, 0);
    assert_ne!(t.c_cflag & libc::CSTOPB, 0);
    assert_ne!(t.c_cflag & libc::CRTSCTS, 0);
    assert_eq!(t.c_iflag & (libc::IXON | libc::IXOFF), 0);
    SerialSettings {
        flow_control: FlowControl::Software,
        ..Default::default()
    }
    .apply_termios(&mut t)
    .unwrap();
    assert_eq!(t.c_cflag & libc::CRTSCTS, 0);
    assert_eq!(t.c_cflag & libc::PARENB, 0);
    assert_eq!(t.c_iflag & libc::INPCK, 0);
    assert_eq!(
        t.c_iflag & (libc::IXON | libc::IXOFF),
        libc::IXON | libc::IXOFF
    );
    assert_eq!(t.c_cc[libc::VSTART], 0x11);
}

#[cfg(unix)]
#[path = "support/serial.rs"]
mod serial_fixture;
#[cfg(unix)]
#[test]
fn idle_serial_close_stops_reader_and_real_pty_carries_bytes() {
    use std::io::Write;
    use std::time::{Duration, Instant};
    let mut pty = serial_fixture::SerialPty::new();
    let mut serial =
        harbor_workspace::serial::Serial::open_with_settings(&pty.path, SerialSettings::default())
            .unwrap();
    pty.master.write_all(b"SERIAL_OK").unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut data = Vec::new();
    while data.len() < 9 {
        assert!(Instant::now() < deadline);
        if let Ok(part) = serial.rx.recv_timeout(Duration::from_millis(50)) {
            data.extend(part);
        }
    }
    assert_eq!(data, b"SERIAL_OK");
    serial.close();
    let deadline = Instant::now() + Duration::from_millis(500);
    while !serial.is_finished() {
        assert!(Instant::now() < deadline, "idle reader failed to stop");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(serial.write(b"late").is_err());
}

#[cfg(unix)]
#[test]
fn serial_full_output_queue_still_cancels_and_unplug_reports_an_error() {
    use std::io::Write;
    use std::os::fd::AsRawFd;
    use std::time::{Duration, Instant};
    let mut pty = serial_fixture::SerialPty::new();
    let mut serial = harbor_workspace::serial::Serial::open(&pty.path, 9600).unwrap();
    let fd = pty.master.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    assert!(flags >= 0);
    assert_eq!(
        unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) },
        0
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut total = 0;
    while Instant::now() < deadline {
        match pty.master.write(&[7u8; 4096]) {
            Ok(n) => total += n,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(1))
            }
            other => panic!("{other:?}"),
        }
    }
    assert!(total >= 256, "PTY fixture produced too little data");
    serial.close();
    let deadline = Instant::now() + Duration::from_millis(500);
    while !serial.is_finished() {
        assert!(
            Instant::now() < deadline,
            "full queue trapped reader after close"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        serial.rx.try_iter().count(),
        256,
        "output queue capacity changed or silently lost data before close"
    );
    let pty = serial_fixture::SerialPty::new();
    let mut serial = harbor_workspace::serial::Serial::open(&pty.path, 9600).unwrap();
    drop(pty.master);
    let deadline = Instant::now() + Duration::from_secs(1);
    while serial.input_error().is_none() {
        assert!(Instant::now() < deadline, "unplug error was not surfaced");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(serial.disconnected());
    serial.close();
}

#[cfg(target_os = "linux")]
#[test]
fn close_releases_all_serial_handles_while_session_remains_alive_under_write_pressure() {
    use std::{
        ffi::CStr,
        fs::File,
        io::{ErrorKind, Read},
        os::fd::FromRawFd,
        time::{Duration, Instant},
    };
    // Keep only the master so Linux reports EIO after all slave handles close.
    let mut path = [0i8; 1024];
    let fd = unsafe { libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY | libc::O_NONBLOCK) };
    assert!(fd >= 0, "{}", std::io::Error::last_os_error());
    let mut master = unsafe { File::from_raw_fd(fd) };
    assert_eq!(unsafe { libc::grantpt(fd) }, 0);
    assert_eq!(unsafe { libc::unlockpt(fd) }, 0);
    assert_eq!(
        unsafe { libc::ptsname_r(fd, path.as_mut_ptr(), path.len()) },
        0
    );
    let path = unsafe { CStr::from_ptr(path.as_ptr()) }.to_str().unwrap();
    let mut serial = harbor_workspace::serial::Serial::open(path, 9600).unwrap();
    let chunk = [0x5a; 32768];
    let mut queued = 0;
    loop {
        match serial.write(&chunk) {
            Ok(()) => queued += chunk.len(),
            Err(e) if e.contains("queue full") => break,
            Err(e) => panic!("{e}"),
        }
        assert!(queued < 64 * chunk.len(), "PTY did not exert backpressure");
    }
    assert!(queued > chunk.len());
    serial.close();

    let deadline = Instant::now() + Duration::from_secs(1);
    while !serial.is_finished() {
        assert!(Instant::now() < deadline, "serial workers did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
    let mut drained = 0;
    let mut buffer = [0u8; 4096];
    loop {
        match master.read(&mut buffer) {
            Ok(n) => drained += n,
            Err(e) if e.raw_os_error() == Some(libc::EIO) => break,
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "finished session retained a serial handle"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => panic!("{e}"),
        }
        assert!(drained < queued, "closed session flushed its queued writes");
    }
    assert!(serial.write(b"after close").is_err());
    assert!(serial.input_error().is_none());
}
