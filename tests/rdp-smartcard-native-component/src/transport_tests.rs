use super::native_worker;
use std::{process::Command, time::Duration};
const READER: &str = "Reader 日本語 🗝";
fn command(_case: &str) -> Command {
    Command::new(
        std::env::var_os("D02_NATIVE_FIXTURE_BINARY")
            .expect("Build and supply the controlled fixture binary"),
    )
}
#[test]
fn none_starts_no_process() {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap()
        .block_on(async {
            assert!(native_worker::Worker::start(None).await.unwrap().is_none());
        });
}
#[test]
fn owned_framed_worker_uses_common_session_and_reaps() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut launch = command("smartcard::transport_tests::native_fixture_child");
        launch.env("D02_NATIVE_FIXTURE", "1");
        let mut worker = native_worker::Worker::fixture(READER, launch)
            .await
            .unwrap();
        // Malformed assigned request still goes through the real owned pipe child,
        // common Session parser and zeroized framed completion transport.
        let mut packet = b"rDRI".to_vec();
        for word in [1u32, 0, 51, 14, 0, 1024, 0, 0x90014] {
            packet.extend(word.to_le_bytes());
        }
        packet.extend([0; 20]);
        assert!(worker.enqueue(&packet).unwrap().is_none());
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            let replies = worker.poll().unwrap();
            if !replies.is_empty() {
                assert_eq!(replies.len(), 1);
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        drop(worker);
    });
}
#[test]
fn stalled_readiness_is_killed_at_deadline() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut launch = command("smartcard::transport_tests::stalled_pipe_child");
        launch.env("D02_NATIVE_STALL", "1");
        let start = std::time::Instant::now();
        assert!(
            native_worker::Worker::fixture(READER, launch)
                .await
                .is_err()
        );
        assert!(start.elapsed() < Duration::from_secs(6));
    });
}

use native_worker::Worker;
const INVALID_HANDLE: u32 = 0x80100003;
fn value(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn words(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|word| word.to_le_bytes()).collect()
}

fn ndr(mut object: Vec<u8>) -> Vec<u8> {
    object.resize(object.len().next_multiple_of(8), 0);
    let mut wire = vec![1, 0x10, 8, 0, 0xcc, 0xcc, 0xcc, 0xcc];
    wire.extend(words(&[object.len() as u32, 0]));
    wire.extend(object);
    wire
}

fn packet(id: u32, code: u32, body: &[u8]) -> Vec<u8> {
    let mut wire = b"rDRI".to_vec();
    wire.extend(words(&[1, 0, id, 14, 0, 67_000, body.len() as u32, code]));
    wire.extend([0; 20]);
    wire.extend(body);
    wire
}

fn context_call(context: u32) -> Vec<u8> {
    ndr(words(&[4, 0x20000, 4, context]))
}

fn reader_string(name: &str) -> Vec<u8> {
    let utf16: Vec<_> = name.encode_utf16().chain([0]).collect();
    let mut bytes = words(&[utf16.len() as u32, 0, utf16.len() as u32]);
    bytes.extend(utf16.iter().flat_map(|v| v.to_le_bytes()));
    bytes.resize(bytes.len().next_multiple_of(4), 0);
    bytes
}

fn connection(context: u32, reader: &str) -> Vec<u8> {
    let mut bytes = words(&[0x20000, 4, 0x20004, 2, 3]);
    bytes.extend(reader_string(reader));
    bytes.extend(words(&[4, context]));
    ndr(bytes)
}

fn handle_call(context: u32, card: u32, extra: &[u32]) -> Vec<u8> {
    let mut bytes = words(&[4, 0x20000, 4, 0x20004]);
    bytes.extend(words(extra));
    bytes.extend(words(&[4, context, 4, card]));
    ndr(bytes)
}

fn transmit(context: u32, card: u32, conformant: u32, pci: bool) -> Vec<u8> {
    let mut bytes = words(&[
        4,
        0x20000,
        4,
        0x20004,
        2,
        0,
        0,
        3,
        0x20008,
        if pci { 0x2000c } else { 0 },
        0,
        16,
        4,
        context,
        4,
        card,
        conformant,
    ]);
    bytes.extend([0, 0xa4, 4, 0]);
    if pci {
        bytes.extend(words(&[0, 0, 0]));
    }
    ndr(bytes)
}

fn watch(context: u32, current: u32, name: &str, timeout: u32) -> Vec<u8> {
    let mut bytes = words(&[
        4, 0x20000, timeout, 1, 0x20004, 4, context, 1, 0x20008, current, 0, 0,
    ]);
    bytes.extend([0; 36]);
    bytes.extend(reader_string(name));
    ndr(bytes)
}

fn enqueue(worker: &mut Worker, id: u32, code: u32, payload: &[u8]) {
    assert!(
        worker
            .enqueue(&packet(id, code, payload))
            .unwrap()
            .is_none()
    );
}

async fn replies(worker: &mut Worker, count: usize) -> Vec<Vec<u8>> {
    tokio::time::timeout(Duration::from_secs(2), async {
        let mut replies = Vec::new();
        while replies.len() < count {
            for response in worker.poll().unwrap() {
                let bytes = response.encode_unframed_pdu().unwrap();
                assert_eq!(&bytes[..4], b"rDCI");
                if value(&bytes, 12) == 0 {
                    assert_eq!(value(&bytes, 16) as usize, bytes.len() - 20);
                    assert_eq!(value(&bytes, 28) as usize, bytes.len() - 36);
                }
                replies.push(bytes);
            }
            if replies.len() < count {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }
        assert_eq!(replies.len(), count);
        replies
    })
    .await
    .expect("PC/SC worker did not reply")
}

async fn call(worker: &mut Worker, id: u32, code: u32, payload: &[u8]) -> Vec<u8> {
    enqueue(worker, id, code, payload);
    let result = replies(worker, 1).await.remove(0);
    assert_eq!(value(&result, 8), id);
    result
}

async fn handles(worker: &mut Worker) -> (u32, u32) {
    let context = call(worker, 2, 0x90014, &ndr(words(&[0]))).await;
    assert_eq!(value(&context, 36), 0);
    let context = value(&context, 52);
    let card = call(worker, 3, 0x900b0, &connection(context, READER)).await;
    assert_eq!(value(&card, 36), 0);
    assert_eq!(value(&card, 56), 2);
    assert_eq!(value(&card, 64), context);
    (context, value(&card, 72))
}

#[tokio::test]
async fn framed_transport_unicode_cards_watch_cancel_and_stale_handles() {
    assert!(
        Worker::fixture("Missing reader", command(""))
            .await
            .is_err()
    );
    let mut worker = Worker::fixture(READER, command("")).await.unwrap();
    assert_eq!(value(&call(&mut worker, 1, 0x900e0, &[0; 4]).await, 36), 0);
    let (context, card) = handles(&mut worker).await;
    let listed = call(
        &mut worker,
        4,
        0x9002c,
        &ndr(words(&[4, 0x20000, 0, 0, 0, u32::MAX, 4, context])),
    )
    .await;
    let expected: Vec<_> = READER
        .encode_utf16()
        .chain([0, 0])
        .flat_map(u16::to_le_bytes)
        .collect();
    assert_eq!(&listed[52..52 + expected.len()], expected);
    let rejected = call(
        &mut worker,
        5,
        0x900b0,
        &connection(context, "Unselected reader"),
    )
    .await;
    assert_eq!(value(&rejected, 36), 0x80100009);
    let status = call(
        &mut worker,
        6,
        0x900cc,
        &handle_call(context, card, &[0, u32::MAX, 32]),
    )
    .await;
    assert_eq!(value(&status, 48), 6);
    assert_eq!(status[56], 0x3b);
    let ready = call(&mut worker, 7, 0x900a4, &watch(context, 0, READER, 0)).await;
    assert_eq!(value(&ready, 56), 0x22);
    let result = call(&mut worker, 8, 0x900d0, &transmit(context, card, 3, true)).await;
    assert_eq!(value(&result, 36), 0);
    assert_eq!(&result[68..70], [0x90, 0]);
    let malformed = call(&mut worker, 9, 0x900d0, &transmit(context, card, 4, true)).await;
    assert_eq!(value(&malformed, 12), 0xc000000d);
    let second = value(&call(&mut worker, 10, 0x90014, &ndr(words(&[0]))).await, 52);
    let wrong = call(&mut worker, 11, 0x900d0, &transmit(second, card, 3, false)).await;
    assert_eq!(value(&wrong, 36), INVALID_HANDLE);
    enqueue(
        &mut worker,
        12,
        0x900a4,
        &watch(context, 0x20, READER, u32::MAX),
    );
    enqueue(
        &mut worker,
        13,
        0x900a4,
        &watch(second, 0x20, READER, u32::MAX),
    );
    for (id, cancelled, ctx) in [(14, 12, context), (15, 13, second)] {
        enqueue(&mut worker, id, 0x900a8, &context_call(ctx));
        let result = replies(&mut worker, 2).await;
        assert_eq!(value(&result[0], 8), id);
        assert_eq!(value(&result[1], 8), cancelled);
        assert_eq!(value(&result[1], 36), 0x80100002);
    }
    assert_eq!(
        value(
            &call(&mut worker, 16, 0x900b8, &handle_call(context, card, &[0])).await,
            36
        ),
        0
    );
    assert_eq!(
        value(
            &call(&mut worker, 17, 0x900d0, &transmit(context, card, 3, false)).await,
            36
        ),
        INVALID_HANDLE
    );
    assert_eq!(
        value(
            &call(&mut worker, 18, 0x90018, &context_call(context)).await,
            36
        ),
        0
    );
    assert_eq!(
        value(
            &call(&mut worker, 19, 0x9001c, &context_call(context)).await,
            36
        ),
        INVALID_HANDLE
    );
}
#[tokio::test]
async fn hung_native_callback_times_out_and_reaps_only_owned_process() {
    struct Unrelated(std::process::Child);
    impl Drop for Unrelated {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut unrelated = Unrelated(
        command("")
            .env("D02_NATIVE_STALL", "1")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    let marker = std::env::temp_dir().join(format!(
        "d02-smartcard-{}-{}",
        std::process::id(),
        if cfg!(feature = "pcsc-test-macos") {
            "mac"
        } else {
            "win"
        }
    ));
    let _ = std::fs::remove_file(&marker);
    let mut launch = command("");
    launch.env("D02_NATIVE_TRANSMIT_STALL", &marker);
    let mut worker = Worker::fixture(READER, launch).await.unwrap();
    let pid = worker.pid() as libc::pid_t;
    let (context, card) = handles(&mut worker).await;
    enqueue(&mut worker, 4, 0x900d0, &transmit(context, card, 3, false));
    let wait = std::time::Instant::now();
    while !marker.exists() {
        assert!(wait.elapsed() < Duration::from_secs(2));
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let wait = std::time::Instant::now();
    loop {
        match worker.poll() {
            Err(message) => {
                assert_eq!(message, "A local PC/SC operation timed out");
                break;
            }
            Ok(replies) => assert!(replies.is_empty()),
        }
        assert!(wait.elapsed() < Duration::from_secs(6));
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let start = std::time::Instant::now();
    drop(worker);
    assert!(start.elapsed() < Duration::from_secs(1));
    assert_eq!(
        unsafe { libc::waitpid(pid, std::ptr::null_mut(), libc::WNOHANG) },
        -1
    );
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ECHILD)
    );
    assert!(
        unrelated.0.try_wait().unwrap().is_none(),
        "Unrelated child was affected"
    );
    std::fs::remove_file(marker).unwrap();
}

#[test]
fn parent_death_watchdog_reaps_in_disposable_subreaper() {
    let result = command("")
        .arg("--fixture-watchdog-audit")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
