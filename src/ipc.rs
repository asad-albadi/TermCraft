//! Same-user, workspace-scoped IPC. Receipts acknowledge queue acceptance, not completion.
use crate::cli::CliAction;
use interprocess::local_socket::{
    GenericNamespaced, ListenerNonblockingMode, ListenerOptions, Stream, prelude::*,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant},
};
const MAX_FRAME: usize = 65536;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum DispatchStatus {
    Accepted,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DispatchReceipt {
    pub id: String,
    pub status: DispatchStatus,
}
#[derive(Clone, Debug)]
pub struct DispatchRequest {
    pub id: String,
    pub action: CliAction,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u8,
    workspace: String,
    name: String,
    token: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    version: u8,
    workspace: String,
    token: String,
    id: String,
    action: CliAction,
}
#[derive(Serialize, Deserialize)]
struct Response {
    receipt: Option<DispatchReceipt>,
    error: Option<String>,
}
pub enum Binding {
    Primary(Primary),
    Secondary(InstanceBroker),
}
pub struct InstanceBroker {
    record: Record,
}
pub struct Primary {
    rx: Receiver<DispatchRequest>,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl Drop for Primary {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
impl Primary {
    pub fn try_recv(&mut self) -> Result<DispatchRequest, mpsc::TryRecvError> {
        self.rx.try_recv()
    }
    pub fn recv_timeout(
        &mut self,
        timeout: Duration,
    ) -> Result<DispatchRequest, mpsc::RecvTimeoutError> {
        self.rx.recv_timeout(timeout)
    }
}
fn ioerr(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn random() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(ioerr)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 256 && !id.chars().any(char::is_control)
}
impl InstanceBroker {
    pub fn bind(workspace: &Path) -> Result<Binding, String> {
        std::fs::create_dir_all(workspace).map_err(ioerr)?;
        let workspace = std::fs::canonicalize(workspace).map_err(ioerr)?;
        let identity = Sha256::digest(workspace.as_os_str().as_encoded_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let mut lock = crate::platform::private_file::open(&workspace.join("instance.lock"), true)?;
        match lock.try_lock() {
            Ok(()) => {
                let token = random()?;
                let name = format!("termcraft-{}", random()?);
                let name_os = name
                    .as_str()
                    .to_ns_name::<GenericNamespaced>()
                    .map_err(ioerr)?;
                let options = ListenerOptions::new()
                    .name(name_os)
                    .nonblocking(ListenerNonblockingMode::Both);
                #[cfg(unix)]
                let options = {
                    use interprocess::os::unix::local_socket::ListenerOptionsExt;
                    options.mode(0o600)
                };
                #[cfg(windows)]
                let options = {
                    use interprocess::os::windows::{
                        local_socket::ListenerOptionsExt, security_descriptor::SecurityDescriptor,
                    };
                    let descriptor =
                        widestring::U16CString::from_str("D:P(A;;GA;;;OW)").map_err(ioerr)?;
                    options.security_descriptor(
                        SecurityDescriptor::deserialize(&descriptor).map_err(ioerr)?,
                    )
                };
                let listener = options.create_sync().map_err(ioerr)?;
                let record = Record {
                    version: 1,
                    workspace: identity,
                    name,
                    token,
                };
                let data = serde_json::to_vec(&record).map_err(ioerr)?;
                lock.set_len(0).map_err(ioerr)?;
                lock.seek(SeekFrom::Start(0)).map_err(ioerr)?;
                lock.write_all(&data).map_err(ioerr)?;
                lock.sync_all().map_err(ioerr)?;
                let (tx, rx) = mpsc::sync_channel(32);
                let stop = Arc::new(AtomicBool::new(false));
                let stopped = stop.clone();
                let worker=std::thread::Builder::new().name("workspace-dispatch".into()).spawn(move||{
     let _lock=lock;let mut seen=std::collections::HashMap::<String,([u8;32],DispatchReceipt)>::new();
     while !stopped.load(Ordering::Acquire){
      let mut stream=match listener.accept(){Ok(s)=>s,Err(e)if e.kind()==std::io::ErrorKind::WouldBlock=>{std::thread::sleep(Duration::from_millis(5));continue},Err(_)=>break};
      if !peer_is_user(&stream){continue}
      let result=(||->Result<DispatchReceipt,String>{
       let bytes=read_frame(&mut stream,Duration::from_millis(400))?;
       let envelope:Envelope=serde_json::from_slice(&bytes).map_err(|_|"Malformed dispatch request.")?;
       use subtle::ConstantTimeEq;
       if envelope.version!=1||envelope.workspace!=record.workspace||!bool::from(envelope.token.as_bytes().ct_eq(record.token.as_bytes())){return Err("Dispatch authentication failed.".into())}
       if !valid_id(&envelope.id){return Err("Invalid dispatch request ID.".into())}envelope.action.validate()?;
       if matches!(envelope.action,CliAction::Help|CliAction::Screenshot{..}){return Err("This operation runs in a separate process.".into())}
       let digest:[u8;32]=Sha256::digest(serde_json::to_vec(&envelope.action).map_err(ioerr)?).into();
       if let Some((previous,receipt))=seen.get(&envelope.id){return if previous==&digest{Ok(receipt.clone())}else{Err("Request ID was already used for another action.".into())}}
       if seen.len()>=65536{return Err("Dispatch request limit reached; restart the workspace before submitting more commands.".into())}
       let receipt=DispatchReceipt{id:envelope.id.clone(),status:DispatchStatus::Accepted};
       tx.try_send(DispatchRequest{id:envelope.id.clone(),action:envelope.action}).map_err(|_|"Workspace command queue is full or closed; request was not accepted.")?;
       seen.insert(envelope.id,(digest,receipt.clone()));Ok(receipt)
      })();
      let response=match result{Ok(receipt)=>Response{receipt:Some(receipt),error:None},Err(error)=>Response{receipt:None,error:Some(error)}};
      if let Ok(bytes)=serde_json::to_vec(&response){let _=write_frame(&mut stream,&bytes,Duration::from_millis(200));}
     }
    }).map_err(ioerr)?;
                Ok(Binding::Primary(Primary {
                    rx,
                    stop,
                    worker: Some(worker),
                }))
            }
            Err(std::fs::TryLockError::WouldBlock) => {
                let end = Instant::now() + Duration::from_secs(2);
                loop {
                    lock.seek(SeekFrom::Start(0)).map_err(ioerr)?;
                    let mut bytes = Vec::new();
                    (&mut lock)
                        .take(16385)
                        .read_to_end(&mut bytes)
                        .map_err(ioerr)?;
                    if let Ok(record) = serde_json::from_slice::<Record>(&bytes) {
                        if record.version != 1
                            || record.workspace != identity
                            || record.token.len() != 64
                            || record.name.len() > 100
                        {
                            return Err("Invalid workspace dispatch metadata.".into());
                        }
                        return Ok(Binding::Secondary(Self { record }));
                    }
                    if Instant::now() >= end {
                        return Err("Existing workspace is still starting; retry shortly.".into());
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            Err(e) => Err(ioerr(e)),
        }
    }
    pub fn send(&self, action: CliAction) -> Result<DispatchReceipt, String> {
        self.send_with_id(&random()?, action)
    }
    pub fn send_with_id(&self, id: &str, mut action: CliAction) -> Result<DispatchReceipt, String> {
        action.absolute_paths(&std::env::current_dir().map_err(ioerr)?);
        if !valid_id(id) {
            return Err("Invalid request ID.".into());
        }
        action.validate()?;
        let envelope = Envelope {
            version: 1,
            workspace: self.record.workspace.clone(),
            token: self.record.token.clone(),
            id: id.into(),
            action,
        };
        let bytes = serde_json::to_vec(&envelope).map_err(ioerr)?;
        if bytes.len() > MAX_FRAME {
            return Err("Dispatch request is too large.".into());
        }
        let name = self
            .record
            .name
            .as_str()
            .to_ns_name::<GenericNamespaced>()
            .map_err(ioerr)?;
        let mut stream = Stream::connect(name)
            .map_err(|_| "Could not connect to the existing workspace.".to_string())?;
        if !peer_is_user(&stream) {
            return Err("Workspace peer belongs to another user.".into());
        }
        stream.set_nonblocking(true).map_err(ioerr)?;
        write_frame(&mut stream, &bytes, Duration::from_secs(2))?;
        let response:Response=serde_json::from_slice(&read_frame(&mut stream,Duration::from_secs(2)).map_err(|_|"Delivery acknowledgement was lost; completion is unknown. Do not automatically resubmit with a new request ID.")?).map_err(|_|"Invalid dispatch receipt.")?;
        if let Some(error) = response.error {
            return Err(error);
        }
        let receipt = response.receipt.ok_or("Missing dispatch receipt")?;
        if receipt.id != id {
            return Err("Dispatch receipt ID mismatch.".into());
        }
        Ok(receipt)
    }
}
fn peer_is_user(stream: &Stream) -> bool {
    #[cfg(unix)]
    {
        stream
            .peer_creds()
            .ok()
            .and_then(|c| c.euid())
            .is_some_and(|uid| uid == unsafe { libc::geteuid() })
    }
    #[cfg(windows)]
    {
        let _ = stream;
        true
    } // Owner-only pipe DACL plus private unguessable rendezvous record.
}
fn transfer(stream: &mut Stream, buf: &mut [u8], write: bool, end: Instant) -> Result<(), String> {
    let mut done = 0;
    while done < buf.len() {
        if Instant::now() >= end {
            return Err("Dispatch I/O timed out.".into());
        }
        let result = if write {
            stream.write(&buf[done..])
        } else {
            stream.read(&mut buf[done..])
        };
        match result {
            Ok(0) => return Err("Dispatch peer closed the connection.".into()),
            Ok(n) => done += n,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(2))
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return Err("Dispatch I/O failed.".into()),
        }
    }
    Ok(())
}
fn read_frame(stream: &mut Stream, timeout: Duration) -> Result<Vec<u8>, String> {
    let end = Instant::now() + timeout;
    let mut header = [0; 4];
    transfer(stream, &mut header, false, end)?;
    let len = u32::from_be_bytes(header) as usize;
    if len == 0 || len > MAX_FRAME {
        return Err("Invalid dispatch frame size.".into());
    }
    let mut bytes = vec![0; len];
    transfer(stream, &mut bytes, false, end)?;
    Ok(bytes)
}
fn write_frame(stream: &mut Stream, bytes: &[u8], timeout: Duration) -> Result<(), String> {
    if bytes.len() > MAX_FRAME {
        return Err("Dispatch frame is too large.".into());
    }
    let end = Instant::now() + timeout;
    let mut header = (bytes.len() as u32).to_be_bytes();
    transfer(stream, &mut header, true, end)?;
    let mut bytes = bytes.to_vec();
    transfer(stream, &mut bytes, true, end)
}
