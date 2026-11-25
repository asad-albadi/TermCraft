//! Driver calls occur solely in a killable owned child. Parent pipe brokers only move bounded frames.
use super::{
    framed,
    ndr::{self, BUSY, INVALID, MAX_PACKET, word_at},
    pcsc::Api,
    session::Session,
};
use ironrdp_core::{Encode, EncodeResult, WriteCursor};
use ironrdp_svc::{SvcEncode, SvcMessage};
use std::{
    collections::BTreeSet,
    ffi::CString,
    io::{Read, Write},
    sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    thread::JoinHandle,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;
#[cfg(windows)]
#[path = "process_windows.rs"]
mod process;
#[cfg(any(
    target_os = "macos",
    all(unix, any(feature = "pcsc-test-macos", feature = "pcsc-test-windows"))
))]
#[path = "process_macos.rs"]
mod process;
const READY: &[u8] = b"PCSC ready";
const HEARTBEAT: &[u8] = b"\0";
const TIMEOUT: Duration = Duration::from_secs(4);
const TICK: Duration = Duration::from_millis(100);
const LIMIT: usize = 16;
type Packet = Zeroizing<Vec<u8>>;
struct SensitivePdu(Packet);
impl Encode for SensitivePdu {
    fn encode(&self, dst: &mut WriteCursor<'_>) -> EncodeResult<()> {
        ironrdp_core::ensure_size!(in:dst,size:self.size());
        dst.write_slice(&self.0);
        Ok(())
    }
    fn name(&self) -> &'static str {
        "RDP smartcard completion"
    }
    fn size(&self) -> usize {
        self.0.len()
    }
}
impl SvcEncode for SensitivePdu {}
struct Broker {
    tx: Option<SyncSender<Packet>>,
    rx: Option<Receiver<Packet>>,
    threads: Vec<JoinHandle<()>>,
}
impl Broker {
    fn new(mut input: impl Read + Send + 'static, mut output: impl Write + Send + 'static) -> Self {
        let (tx, requests) = mpsc::sync_channel::<Packet>(LIMIT);
        let (responses, rx) = mpsc::sync_channel::<Packet>(LIMIT + 1);
        let reader = std::thread::spawn(move || {
            while let Ok(frame) = framed::read(&mut input) {
                if responses.send(frame).is_err() {
                    break;
                }
            }
        });
        let writer = std::thread::spawn(move || {
            while let Ok(frame) = requests.recv() {
                if framed::write(&mut output, &frame).is_err() {
                    break;
                }
            }
        });
        Self {
            tx: Some(tx),
            rx: Some(rx),
            threads: vec![reader, writer],
        }
    }
    fn close(&mut self) {
        self.tx.take();
        self.rx.take();
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}
pub struct Worker {
    process: process::Process,
    broker: Broker,
    pending: BTreeSet<u32>,
    last_seen: Instant,
}
impl Worker {
    pub async fn start(reader: Option<&str>) -> Result<Option<Self>, String> {
        let Some(reader) = reader else {
            return Ok(None);
        };
        if reader.is_empty() || reader.len() >= 128 || reader.chars().any(char::is_control) {
            return Err("Invalid selected PC/SC reader".into());
        }
        let (process, input, output) =
            process::Process::spawn().map_err(|_| {
                if cfg!(windows) { "Cannot start the owned PC/SC worker; Windows 10 or later with private Job launch support is required" }
                else { "Cannot start the owned PC/SC worker" }
            })?;
        Self::open(reader, process, input, output).await.map(Some)
    }
    #[cfg(all(
        test,
        unix,
        any(feature = "pcsc-test-macos", feature = "pcsc-test-windows")
    ))]
    pub(super) async fn fixture(
        reader: &str,
        command: std::process::Command,
    ) -> Result<Self, String> {
        let (process, input, output) =
            process::Process::fixture(command).map_err(|_| "Cannot start fixture worker")?;
        Self::open(reader, process, input, output).await
    }
    async fn open(
        reader: &str,
        process: process::Process,
        input: impl Read + Send + 'static,
        output: impl Write + Send + 'static,
    ) -> Result<Self, String> {
        let mut worker = Self {
            process,
            broker: Broker::new(input, output),
            pending: BTreeSet::new(),
            last_seen: Instant::now(),
        };
        worker
            .broker
            .tx
            .as_ref()
            .unwrap()
            .try_send(Zeroizing::new(reader.as_bytes().to_vec()))
            .map_err(|_| "Cannot configure the PC/SC worker")?;
        let deadline = Instant::now() + TIMEOUT;
        loop {
            match worker.broker.rx.as_ref().unwrap().try_recv() {
                Ok(frame) if frame.as_slice() == READY => {
                    worker.last_seen = Instant::now();
                    return Ok(worker);
                }
                Ok(_) | Err(TryRecvError::Disconnected) => {
                    return Err(
                        "The selected PC/SC reader or local PC/SC service is unavailable".into(),
                    );
                }
                Err(TryRecvError::Empty) => {}
            }
            if Instant::now() >= deadline {
                return Err("The local PC/SC readiness check timed out".into());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
    #[cfg(all(unix, any(feature = "pcsc-test-macos", feature = "pcsc-test-windows")))]
    pub(super) fn pid(&self) -> u32 {
        self.process.pid()
    }
    pub fn enqueue(&mut self, packet: &[u8]) -> Result<Option<SvcMessage>, String> {
        if ndr::unassigned_control(packet) {
            return Ok(None);
        }
        let reject = |status| Ok(Some(SensitivePdu(ndr::fail(packet, status)).into()));
        if packet.len() > MAX_PACKET {
            return reject(INVALID);
        }
        let id = word_at(packet, 12).map_err(|_| "Invalid smartcard completion ID")?;
        if self.pending.contains(&id) {
            return Err("Duplicate in-flight smartcard completion ID".into());
        }
        if self.pending.len() >= LIMIT {
            return reject(BUSY);
        }
        match self
            .broker
            .tx
            .as_ref()
            .unwrap()
            .try_send(Zeroizing::new(packet.to_vec()))
        {
            Ok(()) => {
                if self.pending.is_empty() {
                    self.last_seen = Instant::now();
                }
                self.pending.insert(id);
                Ok(None)
            }
            Err(TrySendError::Full(_)) => reject(BUSY),
            Err(TrySendError::Disconnected(_)) => Err("The owned PC/SC worker stopped".into()),
        }
    }
    pub fn poll(&mut self) -> Result<Vec<SvcMessage>, String> {
        let mut replies = Vec::new();
        for _ in 0..=LIMIT {
            let bytes = match self.broker.rx.as_ref().unwrap().try_recv() {
                Ok(v) => v,
                Err(TryRecvError::Empty) => break,
                Err(_) => return Err("The owned PC/SC worker stopped".into()),
            };
            self.last_seen = Instant::now();
            if bytes.as_slice() == HEARTBEAT {
                continue;
            }
            let id = word_at(&bytes, 8).map_err(|_| "Invalid PC/SC worker response")?;
            if !bytes.starts_with(b"rDCI")
                || bytes.len() > MAX_PACKET
                || word_at(&bytes, 16).map_err(|_| "Invalid PC/SC worker response")? as usize
                    != bytes.len() - 20
                || !self.pending.remove(&id)
            {
                return Err("Invalid PC/SC worker response".into());
            }
            replies.push(SensitivePdu(bytes).into());
        }
        if !self.pending.is_empty() && self.last_seen.elapsed() > TIMEOUT {
            return Err("A local PC/SC operation timed out".into());
        }
        Ok(replies)
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.process.stop();
        self.broker.close();
    }
}

pub fn child() -> Result<(), ()> {
    child_with_api(Api::open)
}
#[cfg(any(feature = "pcsc-test-macos", feature = "pcsc-test-windows"))]
pub(super) fn fixture_child() -> Result<(), ()> {
    child_with_api(|| Ok(Api::fixture()))
}
fn child_with_api(open_api: impl FnOnce() -> Result<Api, ()>) -> Result<(), ()> {
    validate_inherited_pipes()?;
    #[cfg(any(
        target_os = "macos",
        all(unix, any(feature = "pcsc-test-macos", feature = "pcsc-test-windows"))
    ))]
    {
        // Darwin has no Linux PDEATHSIG. An independent local watchdog can end
        // this owned process group even when a vendor call blocks the worker.
        let parent = unsafe { libc::getppid() };
        let own = unsafe { libc::getpid() };
        if parent <= 1 || unsafe { libc::getpgrp() } != own {
            return Err(());
        }
        std::thread::Builder::new()
            .name("pcsc-parent-watch".into())
            .spawn(move || {
                loop {
                    std::thread::sleep(TICK);
                    if unsafe { libc::getppid() } != parent {
                        // The live group leader anchors identity; never signal a service
                        // or a process selected by an unowned/reused PID.
                        unsafe {
                            libc::kill(-own, libc::SIGKILL);
                            libc::_exit(1);
                        }
                    }
                }
            })
            .map_err(|_| ())?;
    }
    let (sender, requests) = mpsc::sync_channel::<Packet>(LIMIT);
    let _reader = std::thread::spawn(move || {
        let mut input = std::io::stdin().lock();
        while let Ok(packet) = framed::read(&mut input) {
            if sender.send(packet).is_err() {
                break;
            }
        }
    });
    let name = requests.recv_timeout(TIMEOUT).map_err(|_| ())?;
    if name.len() >= 128 {
        return Err(());
    }
    let name = std::str::from_utf8(&name).map_err(|_| ())?;
    if name.trim().is_empty() || name.chars().any(char::is_control) {
        return Err(());
    }
    let mut session =
        Session::open(CString::new(name).map_err(|_| ())?, open_api()?).map_err(|_| ())?;
    let mut output = std::io::stdout().lock();
    framed::write(&mut output, READY).map_err(|_| ())?;
    let mut heartbeat = Instant::now();
    loop {
        match requests.recv_timeout(TICK) {
            Ok(packet) => {
                for reply in session.process(&packet) {
                    framed::write(&mut output, &reply).map_err(|_| ())?;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
        }
        if heartbeat.elapsed() >= TICK {
            if session.has_watches() {
                for reply in session.poll_watches() {
                    framed::write(&mut output, &reply).map_err(|_| ())?;
                }
                framed::write(&mut output, HEARTBEAT).map_err(|_| ())?;
            }
            heartbeat = Instant::now();
        }
    }
}
fn validate_inherited_pipes() -> Result<(), ()> {
    #[cfg(any(
        target_os = "macos",
        all(unix, any(feature = "pcsc-test-macos", feature = "pcsc-test-windows"))
    ))]
    {
        use std::os::fd::AsRawFd;
        for fd in [std::io::stdin().as_raw_fd(), std::io::stdout().as_raw_fd()] {
            let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
            if unsafe { libc::fstat(fd, stat.as_mut_ptr()) } != 0 {
                return Err(());
            }
            if unsafe { stat.assume_init() }.st_mode & libc::S_IFMT != libc::S_IFIFO {
                return Err(());
            }
        }
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::{
            Storage::FileSystem::{FILE_TYPE_PIPE, GetFileType},
            System::Console::{GetStdHandle, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE},
        };
        for id in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE] {
            if unsafe { GetFileType(GetStdHandle(id)) } != FILE_TYPE_PIPE {
                return Err(());
            }
        }
    }
    Ok(())
}
