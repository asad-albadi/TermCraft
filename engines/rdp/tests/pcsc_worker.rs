#![cfg(target_os = "linux")]
#[path = "../src/smartcard.rs"]
#[allow(dead_code, unused_imports)]
mod smartcard;

use smartcard::Worker;
use std::{
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

const READER: &str = "Fixture Reader 00 00";
const INVALID_HANDLE: u32 = 0x80100003;

#[test]
fn provider_requires_its_private_socket() {
    let output = worker_binary()
        .arg("--pcsc-worker")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
}

fn worker_binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_termcraft-rdp-engine"))
}

fn fixture(directory: &Path) -> Command {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pcsc.c");
    let status = Command::new("cc")
        .args(["-shared", "-fPIC", "-Wall", "-Wextra", "-Werror", "-o"])
        .arg(directory.join("libpcsclite.so.1"))
        .arg(source)
        .status()
        .unwrap();
    assert!(status.success());
    let mut process = Command::new(std::env::current_exe().unwrap());
    process.args(["--exact", "fixture_worker_entry", "--nocapture"]);
    process
        .env(
            "TERMCRAFT_TEST_PCSC_LIBRARY",
            directory.join("libpcsclite.so.1"),
        )
        .env("LD_LIBRARY_PATH", "")
        .env("LD_PRELOAD", "")
        .env("DYLD_FALLBACK_LIBRARY_PATH", "")
        .env("LIBPCSCLITE_DELEGATE", "")
        .env("TERMCRAFT_OTHER_SETTING", "present");
    process
}

// Only this cfg(test) executable reads the explicitly passed fixture path.
#[test]
fn fixture_worker_entry() {
    let Some(library) = std::env::var_os("TERMCRAFT_TEST_PCSC_LIBRARY") else {
        return;
    };
    for name in [
        "LD_LIBRARY_PATH",
        "LD_PRELOAD",
        "DYLD_FALLBACK_LIBRARY_PATH",
        "LIBPCSCLITE_DELEGATE",
    ] {
        assert!(
            std::env::var_os(name).is_none(),
            "loader override reached provider child"
        );
    }
    assert!(std::env::var_os("TERMCRAFT_OTHER_SETTING").is_some());
    smartcard::fixture_child(Path::new(&library)).unwrap();
}

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
    bytes.extend([0xfa, 0xce, 0x21, 0]);
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
async fn selected_reader_probes_and_exchanges_card_bytes_over_real_worker_transport() {
    let directory = tempfile::tempdir().unwrap();
    let absent = Worker::fixture("Missing reader", fixture(directory.path())).await;
    assert!(matches!(absent, Err(message) if message.contains("unavailable")));
    let mut worker = Worker::fixture(READER, fixture(directory.path()))
        .await
        .unwrap();
    assert_eq!(value(&call(&mut worker, 1, 0x900e0, &[0; 4]).await, 36), 0);
    let (context, card) = handles(&mut worker).await;
    let listed = call(
        &mut worker,
        4,
        0x9002c,
        &ndr(words(&[4, 0x20000, 0, 0, 0, u32::MAX, 4, context])),
    )
    .await;
    assert_eq!(value(&listed, 36), 0);
    let selected = reader_string(READER);
    let expected = &selected[12..12 + (READER.len() + 1) * 2];
    assert_eq!(&listed[52..52 + expected.len()], expected);
    assert_eq!(value(&listed, 40) as usize, (READER.len() + 2) * 2);
    let unselected = call(
        &mut worker,
        5,
        0x900b0,
        &connection(context, "Unselected Reader 01 00"),
    )
    .await;
    assert_eq!(value(&unselected, 36), 0x80100009);
    let status = call(
        &mut worker,
        6,
        0x900cc,
        &handle_call(context, card, &[0, u32::MAX, 32]),
    )
    .await;
    assert_eq!(value(&status, 36), 0);
    assert_eq!(value(&status, 48), 6); // Linux native bits translated to Windows SCARD_SPECIFIC.
    assert_eq!(&status[56..58], &[0x3b, 0x01]);
    let ready = call(&mut worker, 7, 0x900a4, &watch(context, 0, READER, 0)).await;
    assert_eq!(value(&ready, 56), 0x22);
    assert_eq!(&ready[64..66], &[0x3b, 0x01]);

    let payload = transmit(context, card, 3, true);
    let result = call(&mut worker, 8, 0x900d0, &payload).await;
    assert_eq!(value(&result, 36), 0);
    assert_eq!(value(&result, 52), 2);
    assert_eq!(&result[68..71], &[0x2a, 0x90, 0x00]);

    let malformed = transmit(context, card, 4, true);
    let error = call(&mut worker, 9, 0x900d0, &malformed).await;
    assert_eq!(value(&error, 12), 0xc000000d);
    assert_eq!(value(&error, 16), 0);
    let other_context = value(&call(&mut worker, 16, 0x90014, &ndr(words(&[0]))).await, 52);
    let wrong_context = call(
        &mut worker,
        10,
        0x900d0,
        &transmit(other_context, card, 3, false),
    )
    .await;
    assert_eq!(value(&wrong_context, 36), INVALID_HANDLE);
    let valid = call(&mut worker, 17, 0x9001c, &context_call(other_context)).await;
    assert_eq!(value(&valid, 36), 0);

    let disconnected = call(&mut worker, 11, 0x900b8, &handle_call(context, card, &[0])).await;
    assert_eq!(value(&disconnected, 36), 0);
    let stale = call(&mut worker, 12, 0x900d0, &payload).await;
    assert_eq!(value(&stale, 36), INVALID_HANDLE);
    let released = call(&mut worker, 13, 0x90018, &context_call(context)).await;
    assert_eq!(value(&released, 36), 0);
    let invalid = call(&mut worker, 14, 0x9001c, &context_call(context)).await;
    assert_eq!(value(&invalid, 36), INVALID_HANDLE);
    let unsupported = call(&mut worker, 15, 0x900d4, &ndr(words(&[0]))).await;
    assert_eq!(value(&unsupported, 12), 0xc00000bb);
}

#[tokio::test]
async fn cancelling_a_watch_retires_only_that_owned_context() {
    let directory = tempfile::tempdir().unwrap();
    let mut worker = Worker::fixture(READER, fixture(directory.path()))
        .await
        .unwrap();
    let setup = ndr(words(&[0]));
    let first = value(&call(&mut worker, 1, 0x90014, &setup).await, 52);
    let second = value(&call(&mut worker, 2, 0x90014, &setup).await, 52);
    enqueue(
        &mut worker,
        3,
        0x900a4,
        &watch(first, 0x20, READER, u32::MAX),
    );
    enqueue(
        &mut worker,
        4,
        0x900a4,
        &watch(second, 0x20, READER, u32::MAX),
    );
    assert_eq!(
        value(&call(&mut worker, 5, 0x900a8, &context_call(900)).await, 36),
        INVALID_HANDLE
    );
    for (id, cancelled, context) in [(6, 3, first), (7, 4, second)] {
        enqueue(&mut worker, id, 0x900a8, &context_call(context));
        let results = replies(&mut worker, 2).await;
        assert_eq!(value(&results[0], 8), id);
        assert_eq!(value(&results[0], 36), 0);
        assert_eq!(value(&results[1], 8), cancelled);
        assert_eq!(value(&results[1], 36), 0x80100002);
    }
}

#[tokio::test]
async fn closing_worker_reaps_stalled_native_transmit() {
    let directory = tempfile::tempdir().unwrap();
    let entered_native = directory.path().join("entered-native");
    let mut command = fixture(directory.path());
    command.env("TERMCRAFT_PCSC_FIXTURE_STALL", &entered_native);
    let mut worker = Worker::fixture(READER, command).await.unwrap();
    let pid = worker.pid() as libc::pid_t;
    let (context, card) = handles(&mut worker).await;
    enqueue(&mut worker, 4, 0x900d0, &transmit(context, card, 3, false));
    tokio::time::timeout(Duration::from_secs(2), async {
        while !entered_native.exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("Native transmit was not entered");
    assert!(worker.poll().unwrap().is_empty());
    let deadline = tokio::time::timeout(Duration::from_secs(6), async {
        loop {
            match worker.poll() {
                Err(message) => break message,
                Ok(replies) => assert!(replies.is_empty()),
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("The stalled native operation did not time out");
    assert_eq!(deadline, "A local PC/SC operation timed out");
    let started = std::time::Instant::now();
    drop(worker);
    assert!(started.elapsed() < Duration::from_secs(1));
    // SAFETY: Passing null for the optional status is valid. The owned PID should be reaped.
    assert_eq!(
        unsafe { libc::waitpid(pid, std::ptr::null_mut(), libc::WNOHANG) },
        -1
    );
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ECHILD)
    );
}

#[tokio::test]
async fn inconsistent_transmit_capacity_has_no_native_side_effect() {
    let directory = tempfile::tempdir().unwrap();
    let calls = directory.path().join("calls");
    let mut command = fixture(directory.path());
    command.env("TERMCRAFT_PCSC_FIXTURE_CALLS", &calls);
    let mut worker = Worker::fixture(READER, command).await.unwrap();
    let (context, card) = handles(&mut worker).await;
    for (pci, exact) in [(false, 56u32), (true, 64)] {
        let before = std::fs::read(&calls).unwrap_or_default().len();
        for (id, limit) in [(20, exact - 1), (21, exact)] {
            let mut wire = packet(id, 0x900d0, &transmit(context, card, 3, pci));
            wire[24..28].copy_from_slice(&limit.to_le_bytes());
            assert!(worker.enqueue(&wire).unwrap().is_none());
            let reply = replies(&mut worker, 1).await.remove(0);
            if limit < exact {
                assert_eq!(value(&reply, 12), 0xc0000023);
                assert_eq!(std::fs::read(&calls).unwrap_or_default().len(), before);
            } else {
                assert_eq!(value(&reply, 12), 0);
                assert_eq!(std::fs::read(&calls).unwrap().len(), before + 1);
            }
        }
    }
}

#[tokio::test]
async fn failed_native_cancel_or_release_keeps_live_watch() {
    for (operation, flag) in [
        (0x900a8, "TERMCRAFT_PCSC_FIXTURE_CANCEL_FAIL"),
        (0x90018, "TERMCRAFT_PCSC_FIXTURE_RELEASE_FAIL"),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let changed = directory.path().join("changed");
        let mut command = fixture(directory.path());
        command
            .env(flag, "1")
            .env("TERMCRAFT_PCSC_FIXTURE_CHANGED", &changed);
        let mut worker = Worker::fixture(READER, command).await.unwrap();
        let (context, _) = handles(&mut worker).await;
        enqueue(
            &mut worker,
            20,
            0x900a4,
            &watch(context, 0x20, READER, u32::MAX),
        );
        let error = call(&mut worker, 21, operation, &context_call(context)).await;
        assert_eq!(value(&error, 36), 0x80100016);
        std::fs::write(changed, []).unwrap();
        let status = replies(&mut worker, 1).await.remove(0);
        assert_eq!(value(&status, 8), 20);
        assert_eq!(value(&status, 36), 0);
        assert_eq!(value(&status, 56), 0x22);
    }
}

#[tokio::test]
async fn unassigned_controls_drop_without_consuming_the_worker_queue() {
    let directory = tempfile::tempdir().unwrap();
    let mut worker = Worker::fixture(READER, fixture(directory.path()))
        .await
        .unwrap();
    for id in 100..300 {
        enqueue(&mut worker, id, 0x90010, &ndr(words(&[0])));
    }
    let valid = call(&mut worker, 100, 0x90014, &ndr(words(&[0]))).await;
    assert_eq!(value(&valid, 36), 0);
    let unsupported = call(&mut worker, 301, 0x900d4, &ndr(words(&[0]))).await;
    assert_eq!(value(&unsupported, 12), 0xc00000bb);
}
