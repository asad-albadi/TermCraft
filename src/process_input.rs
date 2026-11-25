//! Bounded background input for local PTYs, ConPTY pipes and serial devices.
use std::{
    fs::File,
    io::Write,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};
pub struct InputQueue {
    tx: SyncSender<Vec<u8>>,
    stop: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
    worker: Option<thread::JoinHandle<()>>,
}
impl InputQueue {
    pub fn new(mut file: File) -> Result<Self, String> {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            let fd = file.as_raw_fd();
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
            {
                return Err(std::io::Error::last_os_error().to_string());
            }
        }
        let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(32);
        let stop = Arc::new(AtomicBool::new(false));
        let error = Arc::new(Mutex::new(None));
        let flag = stop.clone();
        let failed = error.clone();
        let worker = thread::spawn(move || {
            while !flag.load(Ordering::Acquire) {
                let bytes = match rx.recv_timeout(Duration::from_millis(20)) {
                    Ok(bytes) => bytes,
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(_) => break,
                };
                let mut offset = 0;
                while offset < bytes.len() && !flag.load(Ordering::Acquire) {
                    match file.write(&bytes[offset..]) {
                        Ok(0) => {
                            *failed.lock().unwrap_or_else(|e| e.into_inner()) =
                                Some("Terminal input closed while writing.".into());
                            return;
                        }
                        Ok(n) => offset += n,
                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(1))
                        }
                        Err(e) => {
                            if !flag.load(Ordering::Acquire) {
                                *failed.lock().unwrap_or_else(|e| e.into_inner()) =
                                    Some(format!("Terminal input failed: {e}"));
                            }
                            return;
                        }
                    }
                }
            }
        });
        Ok(Self {
            tx,
            stop,
            error,
            worker: Some(worker),
        })
    }
    pub(crate) fn cancel(&self) {
        self.stop.store(true, Ordering::Release);
        #[cfg(windows)]
        if let Some(worker) = &self.worker {
            use std::os::windows::io::AsRawHandle;
            unsafe {
                windows_sys::Win32::System::IO::CancelSynchronousIo(worker.as_raw_handle() as _);
            }
        }
    }
    pub(crate) fn is_finished(&self) -> bool {
        self.worker
            .as_ref()
            .is_none_or(|worker| worker.is_finished())
    }
    pub fn error(&self) -> Option<String> {
        self.error
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .or_else(|| {
                (!self.stop.load(Ordering::Acquire) && self.is_finished())
                    .then(|| "Terminal input worker has closed.".into())
            })
    }
    pub fn submit(&self, bytes: &[u8]) -> Result<(), String> {
        if let Some(error) = self.error.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            return Err(error);
        }
        if bytes.len() > 32768 {
            return Err(
                "Terminal input chunk exceeds 32 KiB; use paced paste for larger text.".into(),
            );
        }
        self.tx.try_send(bytes.to_vec()).map_err(|e| {
            match e {
                mpsc::TrySendError::Full(_) => {
                    "Terminal input queue full; retry when the application reads input."
                }
                mpsc::TrySendError::Disconnected(_) => "Terminal input worker has closed.",
            }
            .into()
        })
    }
}
impl Drop for InputQueue {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let deadline = Instant::now() + Duration::from_millis(200);
            while !worker.is_finished() && Instant::now() < deadline {
                #[cfg(windows)]
                {
                    use std::os::windows::io::AsRawHandle;
                    unsafe {
                        windows_sys::Win32::System::IO::CancelSynchronousIo(
                            worker.as_raw_handle() as _
                        );
                    }
                }
                thread::sleep(Duration::from_millis(1));
            }
            // An unresponsive native driver must not freeze the UI during close.
            // The worker owns its handle until the cancelled kernel operation returns.
            if worker.is_finished() {
                let _ = worker.join();
            }
        }
    }
}
