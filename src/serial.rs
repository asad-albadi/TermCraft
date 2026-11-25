pub mod settings;
pub use settings::{FlowControl, Parity, SerialLine, SerialSettings};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
#[cfg(unix)]
use std::{fs::File, os::fd::AsRawFd};
use std::{
    fs::OpenOptions,
    io::Read,
    sync::mpsc::{self, Receiver},
    thread,
};
pub struct Serial {
    pub rx: Receiver<Vec<u8>>,
    input: Option<crate::process::InputQueue>,
    reader_closed: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    reader_error: Arc<std::sync::Mutex<Option<String>>>,
    reader_thread: Option<std::thread::JoinHandle<()>>,
    #[cfg(unix)]
    probe: Option<File>,
}
impl Serial {
    pub fn open(device: &str, baud: u32) -> Result<Self, String> {
        Self::open_with_settings(device, SerialLine::default().settings(baud))
    }
    pub fn open_with_settings(device: &str, settings: SerialSettings) -> Result<Self, String> {
        settings.validate()?;
        #[cfg(unix)]
        let file = {
            use std::os::{fd::AsRawFd, unix::fs::OpenOptionsExt};
            let f = OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(libc::O_NOCTTY | libc::O_NONBLOCK)
                .open(device)
                .map_err(|e| e.to_string())?;
            unsafe {
                let mut t: libc::termios = std::mem::zeroed();
                if libc::tcgetattr(f.as_raw_fd(), &mut t) != 0 {
                    return Err(std::io::Error::last_os_error().to_string());
                }
                settings.apply_termios(&mut t)?;
                if libc::tcsetattr(f.as_raw_fd(), libc::TCSANOW, &t) != 0 {
                    return Err(std::io::Error::last_os_error().to_string());
                }
            }
            f
        };
        #[cfg(windows)]
        let file = {
            use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
            use windows_sys::Win32::Devices::Communication::*;
            let device = if device.starts_with("\\\\.\\") {
                device.to_string()
            } else {
                format!("\\\\.\\{device}")
            };
            let f = OpenOptions::new()
                .read(true)
                .write(true)
                .share_mode(0)
                .open(device)
                .map_err(|e| e.to_string())?;
            unsafe {
                let h = f.as_raw_handle();
                let mut d: DCB = std::mem::zeroed();
                d.DCBlength = std::mem::size_of::<DCB>() as u32;
                if GetCommState(h, &mut d) == 0 {
                    return Err(std::io::Error::last_os_error().to_string());
                }
                settings.apply_dcb(&mut d)?;
                if SetCommState(h, &d) == 0 {
                    return Err(std::io::Error::last_os_error().to_string());
                }
                let t = COMMTIMEOUTS {
                    ReadIntervalTimeout: 50,
                    ReadTotalTimeoutMultiplier: 0,
                    ReadTotalTimeoutConstant: 100,
                    WriteTotalTimeoutMultiplier: 0,
                    WriteTotalTimeoutConstant: 1000,
                };
                if SetCommTimeouts(h, &t) == 0 {
                    return Err(std::io::Error::last_os_error().to_string());
                }
            }
            f
        };
        let mut reader = file.try_clone().map_err(|e| e.to_string())?;
        #[cfg(unix)]
        let probe = file.try_clone().map_err(|e| e.to_string())?;
        let input = crate::process::InputQueue::new(file)?;
        let (tx, rx) = mpsc::sync_channel(256);
        let reader_closed = Arc::new(AtomicBool::new(false));
        let closed = reader_closed.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        let reader_error = Arc::new(std::sync::Mutex::new(None));
        let error = reader_error.clone();
        let reader_thread = thread::spawn(move || {
            let mut buf = [0u8; 4096];
            while !stop.load(Ordering::Acquire) {
                #[cfg(unix)]
                let hung_up = {
                    use std::os::fd::AsRawFd;
                    let mut fd = libc::pollfd {
                        fd: reader.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    };
                    let ready = unsafe { libc::poll(&mut fd, 1, 100) };
                    if ready < 0 {
                        if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted
                        {
                            continue;
                        }
                        *error.lock().unwrap_or_else(|p| p.into_inner()) =
                            Some(std::io::Error::last_os_error().to_string());
                        break;
                    }
                    let hung_up =
                        fd.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0;
                    if hung_up && fd.revents & libc::POLLIN == 0 {
                        *error.lock().unwrap_or_else(|p| p.into_inner()) =
                            Some("Serial device disconnected".into());
                        break;
                    }
                    hung_up
                };
                #[cfg(not(unix))]
                let hung_up = false;
                match reader.read(&mut buf) {
                    Ok(0) if hung_up => {
                        *error.lock().unwrap_or_else(|p| p.into_inner()) =
                            Some("Serial device disconnected".into());
                        break;
                    }
                    Ok(0) => {
                        thread::sleep(std::time::Duration::from_millis(30));
                    }
                    Ok(n) => {
                        let mut data = buf[..n].to_vec();
                        loop {
                            match tx.try_send(data) {
                                Ok(()) => break,
                                Err(mpsc::TrySendError::Full(value)) => {
                                    if stop.load(Ordering::Acquire) {
                                        break;
                                    }
                                    data = value;
                                    thread::sleep(std::time::Duration::from_millis(10));
                                }
                                Err(_) => {
                                    stop.store(true, Ordering::Release);
                                    break;
                                }
                            }
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(std::time::Duration::from_millis(30));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) => {
                        *error.lock().unwrap_or_else(|p| p.into_inner()) = Some(e.to_string());
                        break;
                    }
                }
            }
            closed.store(true, Ordering::Release);
        });
        Ok(Self {
            rx,
            input: Some(input),
            reader_closed,
            cancel,
            reader_error,
            reader_thread: Some(reader_thread),
            #[cfg(unix)]
            probe: Some(probe),
        })
    }
    pub fn close(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(input) = &self.input {
            input.cancel();
        }
        #[cfg(unix)]
        {
            self.probe = None;
        }
    }
    pub fn is_finished(&self) -> bool {
        self.reader_closed.load(Ordering::Acquire)
            && self
                .reader_thread
                .as_ref()
                .is_none_or(|thread| thread.is_finished())
            && self.input.as_ref().is_none_or(|input| input.is_finished())
    }
    pub fn disconnected(&self) -> bool {
        if self.reader_closed.load(Ordering::Acquire) {
            return true;
        }
        #[cfg(unix)]
        {
            // Readiness probe only; never read or wait on the UI thread.
            let mut fd = libc::pollfd {
                fd: match self.probe.as_ref() {
                    Some(probe) => probe.as_raw_fd(),
                    None => return true,
                },
                events: 0,
                revents: 0,
            };
            let ready = unsafe { libc::poll(&mut fd, 1, 0) };
            if ready > 0 && fd.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0 {
                return true;
            }
        }
        false
    }
    pub fn input_error(&self) -> Option<String> {
        self.input
            .as_ref()
            .and_then(|input| input.error())
            .or_else(|| {
                self.reader_error
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .clone()
            })
    }
    pub fn write(&mut self, data: &[u8]) -> Result<(), String> {
        if self.cancel.load(Ordering::Acquire) {
            return Err("Serial session is closed".into());
        }
        self.input
            .as_ref()
            .ok_or("Serial session is closed")?
            .submit(data)
    }
}

impl Drop for Serial {
    fn drop(&mut self) {
        self.close();
        // Native read timeouts and queue backpressure both observe cancellation.
        // Do not join here: GUI close remains nonblocking.
        let _ = self.reader_thread.take();
    }
}
