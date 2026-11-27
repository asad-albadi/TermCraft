//! Private, bounded printer process. Only the process touches platform spooler APIs.
//! Queue identity survives certificate retries; a failed/timed-out process is never reopened.
use super::{CANCELLED, Job, wide, word};
use crate::rdp_options::PrinterMapping;
use std::{
    io::{self, Read, Seek, Write},
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{mpsc, oneshot},
};
#[cfg(any(target_os = "macos", all(test, unix)))]
#[path = "printer_owner.rs"]
mod owner;
#[cfg(windows)]
#[path = "printer_windows.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "printer_macos.rs"]
mod platform;
#[cfg(windows)]
#[path = "printer_process_windows.rs"]
mod process_windows;

const MAX_FRAME: usize = 65537;
const TIMEOUT: u32 = 0xc00000b5;
const FAILED: u32 = 0xc0000001;
const LIMIT: usize = 16;
const DEADLINE: Duration = Duration::from_secs(10);
const INIT: u8 = 1;
const READY: u8 = 2;
const BEGIN: u8 = 3;
const WRITE: u8 = 4;
const FINISH: u8 = 5;
const ABORT: u8 = 6;
static LIVE: AtomicUsize = AtomicUsize::new(0);
struct Permit;
impl Permit {
    fn acquire() -> Result<Self, String> {
        LIVE.fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
            (n < LIMIT).then_some(n + 1)
        })
        .map_err(|_| "Too many native printer workers")?;
        Ok(Self)
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        LIVE.fetch_sub(1, Ordering::AcqRel);
    }
}
struct Request {
    bytes: Vec<u8>,
    reply: oneshot::Sender<Result<(), u32>>,
}
struct Agent {
    tx: mpsc::Sender<Request>,
    closed: Arc<AtomicBool>,
}
impl Drop for Agent {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Release);
    }
}
// Dropping a pending call poisons the retained helper instead of allowing the
// next request to consume a late reply or reopening a different printer.
struct Pending {
    closed: Arc<AtomicBool>,
    armed: bool,
}
impl Drop for Pending {
    fn drop(&mut self) {
        if self.armed {
            self.closed.store(true, Ordering::Release)
        }
    }
}
impl Agent {
    fn start(queue: String, command: std::process::Command) -> Result<Arc<Self>, String> {
        let permit = Permit::acquire()?;
        let (tx, rx) = mpsc::channel(1);
        let closed = Arc::new(AtomicBool::new(false));
        let stop = closed.clone();
        std::thread::Builder::new()
            .name("rdp-printer-process".into())
            .spawn(move || {
                let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                else {
                    return;
                };
                rt.block_on(run(queue, command, rx, stop, permit));
            })
            .map_err(|_| "Cannot create the native printer worker")?;
        Ok(Arc::new(Self { tx, closed }))
    }
    async fn call(&self, bytes: Vec<u8>) -> Result<(), u32> {
        if self.closed.load(Ordering::Acquire) {
            return Err(CANCELLED);
        }
        let mut pending = Pending {
            closed: self.closed.clone(),
            armed: true,
        };
        let (reply, rx) = oneshot::channel();
        // No unbounded producer backlog. Only the ordered printer frontend sends.
        self.tx
            .try_send(Request { bytes, reply })
            .map_err(|_| FAILED)?;
        let result = tokio::time::timeout(DEADLINE, rx)
            .await
            .map_err(|_| TIMEOUT)?
            .map_err(|_| FAILED)?;
        pending.armed = false;
        result
    }
}

#[derive(Clone)]
pub(crate) struct Binding {
    mapping: PrinterMapping,
    agent: Arc<Agent>,
}
impl Binding {
    pub(crate) fn new(mapping: &PrinterMapping) -> Result<Self, String> {
        // Validate independently of profile/UI checks, before process creation.
        validate_queue(&mapping.queue)?;
        let mut command = std::process::Command::new(
            std::env::current_exe().map_err(|_| "Cannot locate the native printer helper")?,
        );
        command.arg("--printer-worker");
        Ok(Self {
            mapping: mapping.clone(),
            agent: Agent::start(mapping.queue.clone(), command)?,
        })
    }
    pub(crate) fn announcement(&self) -> Vec<u8> {
        let driver = wide(&format!("{}\0", self.mapping.driver));
        let name = wide(&format!("{}\0", self.mapping.queue));
        let mut p = Vec::new();
        for v in [0, 0, 0, driver.len() as u32, name.len() as u32, 0] {
            word(&mut p, v)
        }
        p.extend(driver);
        p.extend(name);
        p
    }
    pub(crate) async fn check_ready(&self) -> Result<(), String> {
        self.agent.call(vec![READY]).await.map_err(|s|
            format!("Selected printer {} is unavailable, not ready for raw jobs, or not permitted (0x{s:08x})",self.mapping.queue))
    }
    pub(super) fn submit(&self, mut job: Job, closed: &AtomicBool) -> Result<(), u32> {
        job.file.rewind().map_err(super::status)?;
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(super::status)?;
        let result=rt.block_on(async{
            tokio::select!{biased;
                _=super::wait_cancelled(closed)=>{self.agent.closed.store(true,Ordering::Release);Err(CANCELLED)},
                result=tokio::time::timeout(DEADLINE,async{
                    let mut begin=vec![BEGIN];begin.extend(job.bytes.to_le_bytes());
                    self.agent.call(begin).await?;
                    let mut remaining=job.bytes as usize;
                    while remaining>0 {
                        let n=remaining.min(MAX_FRAME-1);
                        let mut p=vec![WRITE];p.resize(n+1,0);
                        job.file.read_exact(&mut p[1..]).map_err(super::status)?;
                        self.agent.call(p).await?;remaining-=n;
                    }
                    self.agent.call(vec![FINISH]).await
                })=>match result{Ok(r)=>r,Err(_)=>{self.agent.closed.store(true,Ordering::Release);Err(TIMEOUT)}}
            }
        });
        if result.is_err() {
            self.agent.closed.store(true, Ordering::Release)
        }
        result
    }
}
fn validate_queue(queue: &str) -> Result<(), String> {
    // No default lookup, server syntax, URI or UNC namespace. A selected local
    // installed queue may itself have a network port, under user authority.
    if queue.is_empty()
        || queue.len() > 255
        || queue
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':' | '\0'))
    {
        return Err("Select one local installed printer queue by name".into());
    }
    Ok(())
}
async fn exchange(child: &mut tokio::process::Child, bytes: &[u8]) -> io::Result<u32> {
    let input = child
        .stdin
        .as_mut()
        .ok_or_else(|| io::Error::other("Printer stdin closed"))?;
    input.write_u32_le(bytes.len() as u32).await?;
    input.write_all(bytes).await?;
    input.flush().await?;
    let output = child
        .stdout
        .as_mut()
        .ok_or_else(|| io::Error::other("Printer stdout closed"))?;
    if output.read_u32_le().await? != 4 {
        return Err(io::Error::other("Invalid printer completion"));
    }
    output.read_u32_le().await
}
async fn stopped(closed: &AtomicBool) {
    while !closed.load(Ordering::Acquire) {
        tokio::time::sleep(Duration::from_millis(10)).await
    }
}
async fn run(
    queue: String,
    command: std::process::Command,
    mut rx: mpsc::Receiver<Request>,
    closed: Arc<AtomicBool>,
    permit: Permit,
) {
    let mut command = tokio::process::Command::from(command);
    // Pass the known owner at spawn. The child must not infer ownership after
    // a scheduling delay in which buffered INIT can outlive this process.
    command.arg(std::process::id().to_string());
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let Ok(mut child) = command.spawn() else {
        return;
    };
    #[cfg(windows)]
    let _ownership = match process_windows::Ownership::assign(&child) {
        Ok(v) => v,
        Err(_) => {
            closed.store(true, Ordering::Release);
            if !stop_owned(&mut child).await {
                std::mem::forget(child);
                std::mem::forget(permit);
            }
            return;
        }
    };
    let work = async {
        let mut init = vec![INIT];
        init.extend(queue.as_bytes());
        let code = exchange(&mut child, &init).await?;
        if code != 0 {
            if let Some(request) = rx.recv().await {
                let _ = request.reply.send(Err(code));
            }
            return Err(io::Error::other("Printer initialization failed"));
        }
        loop {
            let Some(request) = rx.recv().await else {
                return Ok(());
            };
            let code = exchange(&mut child, &request.bytes).await?;
            let _ = request
                .reply
                .send(if code == 0 { Ok(()) } else { Err(code) });
            if code != 0 {
                return Err(io::Error::other("Native printer operation failed"));
            }
        }
    };
    tokio::select! {_ = stopped(&closed)=>{},_ = tokio::time::timeout(Duration::from_secs(3600*24),work)=>{}}
    closed.store(true, Ordering::Release);
    // EOF asks an idle helper to abort its current owned document. Never append
    // another frame after a cancelled partial pipe write: it could be interpreted
    // as document bytes. A blocked native call gets process termination instead.
    if stop_owned(&mut child).await {
        return;
    }
    // No successful wait proved reaping. Retain the budget AND process resources.
    // Dropping Child would invoke kill-on-drop against an uncertain Unix PID;
    // the Windows JobObject ownership handle must remain retained as well.
    std::mem::forget(child);
    std::mem::forget(permit);
    #[cfg(windows)]
    std::mem::forget(_ownership);
}

async fn stop_owned(child: &mut tokio::process::Child) -> bool {
    drop(child.stdin.take());
    match wait_evidence(&tokio::time::timeout(Duration::from_millis(250), child.wait()).await) {
        WaitEvidence::Reaped => return true,
        WaitEvidence::Unknown => {}
        WaitEvidence::Pending => match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) => {
                // An owned-child wait confirms that this identity still exists.
                // Do not signal a PID after an ambiguous wait error.
                let _ = child.start_kill();
                if wait_evidence(&tokio::time::timeout(Duration::from_secs(2), child.wait()).await)
                    == WaitEvidence::Reaped
                {
                    return true;
                }
            }
            Err(_) => {}
        },
    }
    false
}

#[derive(Debug, PartialEq, Eq)]
enum WaitEvidence {
    Reaped,
    Pending,
    Unknown,
}
fn wait_evidence<T, E, W>(result: &Result<Result<T, E>, W>) -> WaitEvidence {
    match result {
        Ok(Ok(_)) => WaitEvidence::Reaped,
        Ok(Err(_)) => WaitEvidence::Unknown,
        Err(_) => WaitEvidence::Pending,
    }
}

/// Child-only native methods. No implementation may reopen a selected queue.
trait Backend: Sized {
    fn open(queue: &str) -> Result<Self, u32>;
    fn ready(&mut self) -> Result<(), u32>;
    fn begin(&mut self, bytes: u32) -> Result<(), u32>;
    fn write(&mut self, bytes: &[u8]) -> Result<(), u32>;
    fn finish(&mut self) -> Result<(), u32>;
    fn abort(&mut self) -> Result<(), u32>;
}
fn frame(input: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut length = [0; 4];
    input.read_exact(&mut length)?;
    let n = u32::from_le_bytes(length) as usize;
    if !(1..=MAX_FRAME).contains(&n) {
        return Err(io::Error::other("Invalid printer frame"));
    }
    let mut p = vec![0; n];
    input.read_exact(&mut p)?;
    Ok(p)
}
fn serve<B: Backend>(mut input: impl Read, mut output: impl Write) -> io::Result<()> {
    let init = frame(&mut input)?;
    let queue =
        std::str::from_utf8(&init[1..]).map_err(|_| io::Error::other("Invalid printer queue"))?;
    if init[0] != INIT || validate_queue(queue).is_err() {
        return Err(io::Error::other("Invalid printer initialization"));
    }
    let mut backend = B::open(queue);
    reply(&mut output, backend.as_ref().map(|_| ()).map_err(|s| *s))?;
    let Ok(ref mut backend) = backend else {
        return Ok(());
    };
    let mut remaining = None;
    while let Ok(p) = frame(&mut input) {
        let result = match (p[0], p.len(), remaining) {
            (READY, 1, None) => backend.ready(),
            (BEGIN, 5, None) => {
                let bytes = u32::from_le_bytes(p[1..5].try_into().unwrap());
                if bytes == 0 || bytes > 32 * 1024 * 1024 {
                    Err(super::INVALID)
                } else {
                    backend
                        .ready()
                        .and_then(|()| backend.begin(bytes))
                        .map(|()| remaining = Some(bytes))
                }
            }
            (WRITE, n, Some(left)) if n > 1 && (n - 1) <= left as usize => backend
                .write(&p[1..])
                .map(|()| remaining = Some(left - (n - 1) as u32)),
            (FINISH, 1, Some(0)) => backend.finish().map(|()| remaining = None),
            (ABORT, 1, _) => {
                let result = backend.abort();
                reply(&mut output, result)?;
                return Ok(());
            }
            _ => Err(super::INVALID),
        };
        reply(&mut output, result)?;
        if result.is_err() {
            let _ = backend.abort();
            return Ok(());
        }
    }
    let _ = backend.abort();
    Ok(())
}
fn reply(output: &mut impl Write, result: Result<(), u32>) -> io::Result<()> {
    output.write_all(&4u32.to_le_bytes())?;
    output.write_all(&result.err().unwrap_or(0).to_le_bytes())?;
    output.flush()
}
#[cfg(any(windows, target_os = "macos"))]
pub(crate) fn child() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let argument = std::env::args_os().nth(2);
        let expected = owner::expected_owner(argument.as_deref())?;
        owner::watch(expected)?;
    }
    serve::<platform::Native>(io::stdin().lock(), io::stdout().lock())
        .map_err(|_| "Native printer worker closed".into())
}

#[cfg(all(test, target_os = "linux"))]
#[path = "printer_native_tests.rs"]
mod tests;
