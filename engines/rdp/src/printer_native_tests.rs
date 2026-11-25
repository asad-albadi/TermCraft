use super::*;
use std::{cell::RefCell, io::Cursor};
thread_local! {static EVENTS:RefCell<Vec<String>>=const{RefCell::new(Vec::new())};}
struct Fake;
fn event(s: impl Into<String>) {
    EVENTS.with(|v| v.borrow_mut().push(s.into()))
}
impl Backend for Fake {
    fn open(queue: &str) -> Result<Self, u32> {
        event(format!("open:{queue}"));
        if queue == "Denied" {
            Err(0xc0000022)
        } else {
            Ok(Self)
        }
    }
    fn ready(&mut self) -> Result<(), u32> {
        event("ready");
        Ok(())
    }
    fn begin(&mut self, n: u32) -> Result<(), u32> {
        event(format!("begin:{n}"));
        Ok(())
    }
    fn write(&mut self, p: &[u8]) -> Result<(), u32> {
        event(format!("write:{}", String::from_utf8_lossy(p)));
        Ok(())
    }
    fn finish(&mut self) -> Result<(), u32> {
        event("finish");
        Ok(())
    }
    fn abort(&mut self) -> Result<(), u32> {
        event("abort-owned");
        Ok(())
    }
}
fn packets(ps: &[&[u8]]) -> Vec<u8> {
    let mut p = Vec::new();
    for b in ps {
        p.extend((b.len() as u32).to_le_bytes());
        p.extend(*b)
    }
    p
}
fn codes(ps: &[&[u8]]) -> (Vec<u32>, Vec<String>) {
    EVENTS.with(|v| v.borrow_mut().clear());
    let mut output = Vec::new();
    serve::<Fake>(Cursor::new(packets(ps)), &mut output).unwrap();
    let result = output
        .chunks_exact(8)
        .map(|p| {
            assert_eq!(&p[..4], &4u32.to_le_bytes());
            u32::from_le_bytes(p[4..].try_into().unwrap())
        })
        .collect();
    (result, EVENTS.with(|v| v.borrow().clone()))
}
#[test]
fn explicit_queue_raw_chunks_and_owned_completion() {
    let (result, events) = codes(&[
        b"\x01Office",
        b"\x02",
        b"\x03\x06\0\0\0",
        b"\x04abc",
        b"\x04def",
        b"\x05",
        b"\x06",
    ]);
    assert_eq!(result, vec![0; 7]);
    assert_eq!(
        events,
        vec![
            "open:Office",
            "ready",
            "ready",
            "begin:6",
            "write:abc",
            "write:def",
            "finish",
            "abort-owned"
        ]
    );
}
#[test]
fn truncated_document_cannot_finish_and_is_aborted() {
    let (result, events) = codes(&[b"\x01Office", b"\x03\x06\0\0\0", b"\x04abc", b"\x05"]);
    assert_eq!(result.last(), Some(&super::super::INVALID));
    assert!(!events.iter().any(|s| s == "finish"));
    assert_eq!(events.last().unwrap(), "abort-owned");
}
#[test]
fn native_permission_failure_is_not_success() {
    let (result, events) = codes(&[b"\x01Denied", b"\x02"]);
    assert_eq!(result, vec![0xc0000022]);
    assert_eq!(events, vec!["open:Denied"]);
}
#[test]
fn no_default_or_server_queue_names() {
    for q in [
        "",
        "\\\\host\\printer",
        "ipp://host/queue",
        "Queue\0Other",
        "Queue\nOther",
    ] {
        assert!(validate_queue(q).is_err())
    }
    assert!(validate_queue("Office Printer").is_ok());
}
#[test]
fn malformed_and_oversized_frames_are_bounded() {
    for n in [0, MAX_FRAME as u32 + 1, u32::MAX] {
        assert!(frame(&mut Cursor::new(n.to_le_bytes())).is_err())
    }
    let (_, events) = codes(&[b"\x01Office", b"\x03\x01\0\0\0", b"\x04ab"]);
    assert!(!events.iter().any(|s| s.starts_with("write:")));
    assert_eq!(events.last().unwrap(), "abort-owned");
}
#[test]
fn cancellation_poisoning_kills_and_reaps_hung_owned_process() {
    let dir = tempfile::tempdir().unwrap();
    let pid = dir.path().join("pid");
    let mut command = std::process::Command::new("python3");
    // Controlled protocol fixture: initialize, then wait forever in readiness.
    command.arg("-c").arg("import sys,struct,os,time\nopen(sys.argv[1],'w').write(str(os.getpid()))\nn=struct.unpack('<I',sys.stdin.buffer.read(4))[0]\nsys.stdin.buffer.read(n)\nsys.stdout.buffer.write(struct.pack('<II',4,0));sys.stdout.buffer.flush()\ntime.sleep(60)").arg(&pid);
    let agent = Agent::start("Office".into(), command).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        assert!(
            tokio::time::timeout(Duration::from_millis(150), agent.call(vec![READY]))
                .await
                .is_err()
        );
        assert!(agent.closed.load(Ordering::Acquire));
        assert_eq!(agent.call(vec![READY]).await, Err(CANCELLED));
    });
    let owned: i32 = std::fs::read_to_string(&pid).unwrap().parse().unwrap();
    let until = std::time::Instant::now() + Duration::from_secs(4);
    while unsafe { libc::kill(owned, 0) } == 0 && std::time::Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20))
    }
    assert_eq!(
        unsafe { libc::kill(owned, 0) },
        -1,
        "controlled child was not reaped"
    );
}
#[test]
fn initialization_permission_status_survives_real_pipe_transport() {
    let mut command = std::process::Command::new("python3");
    command.arg("-c").arg("import sys,struct\nn=struct.unpack('<I',sys.stdin.buffer.read(4))[0]\nsys.stdin.buffer.read(n)\nsys.stdout.buffer.write(struct.pack('<II',4,0xc0000022));sys.stdout.buffer.flush()");
    let agent = Agent::start("Denied".into(), command).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    assert_eq!(rt.block_on(agent.call(vec![READY])), Err(0xc0000022));
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while !agent.closed.load(Ordering::Acquire) && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10))
    }
    assert!(agent.closed.load(Ordering::Acquire));
}

#[test]
fn wait_errors_and_timeouts_never_prove_reaping() {
    let normal: Result<Result<(), ()>, ()> = Ok(Ok(()));
    let wait_error: Result<Result<(), ()>, ()> = Ok(Err(()));
    let timeout: Result<Result<(), ()>, ()> = Err(());
    assert_eq!(wait_evidence(&normal), WaitEvidence::Reaped);
    assert_eq!(wait_evidence(&wait_error), WaitEvidence::Unknown);
    assert_eq!(wait_evidence(&timeout), WaitEvidence::Pending);
    // A nonzero child exit status still proves the process was reaped.
    let failed_exit: Result<Result<i32, ()>, ()> = Ok(Ok(1));
    assert_eq!(wait_evidence(&failed_exit), WaitEvidence::Reaped);
}
