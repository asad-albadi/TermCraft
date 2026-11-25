#![cfg(unix)]
use harbor_workspace::process::InputQueue;
use std::{
    fs::File,
    io::Read,
    os::{
        fd::{FromRawFd, IntoRawFd},
        unix::net::UnixStream,
    },
    sync::mpsc,
    time::Duration,
};
fn file(stream: UnixStream) -> File {
    unsafe { File::from_raw_fd(stream.into_raw_fd()) }
}
#[test]
fn full_input_queue_applies_backpressure_and_drop_does_not_wait_for_reader() {
    let (writer, peer) = UnixStream::pair().unwrap();
    let (done, rx) = mpsc::channel();
    let thread = std::thread::spawn(move || {
        let queue = InputQueue::new(file(writer)).unwrap();
        let mut accepted = 0;
        for _ in 0..128 {
            match queue.submit(&vec![b'x'; 32768]) {
                Ok(()) => accepted += 1,
                Err(e) => {
                    assert!(e.contains("full"), "{e}");
                    break;
                }
            }
        }
        assert!(accepted > 0 && accepted < 128);
        drop(queue);
        done.send(accepted).unwrap();
    });
    let result = rx.recv_timeout(Duration::from_secs(3));
    drop(peer);
    thread.join().unwrap();
    assert!(result.is_ok(), "closing a full queue blocked on its reader");
}
#[test]
fn queued_partial_writes_preserve_bytes_and_order() {
    let (writer, mut reader) = UnixStream::pair().unwrap();
    let queue = InputQueue::new(file(writer)).unwrap();
    let expected = [vec![b'a'; 32768], vec![b'b'; 32768]].concat();
    let read = std::thread::spawn(move || {
        let mut bytes = vec![0; 65536];
        reader.read_exact(&mut bytes).unwrap();
        bytes
    });
    queue.submit(&expected[..32768]).unwrap();
    queue.submit(&expected[32768..]).unwrap();
    assert_eq!(read.join().unwrap(), expected);
}
#[test]
fn worker_failure_is_observable_without_another_submit() {
    let (writer, reader) = UnixStream::pair().unwrap();
    let queue = InputQueue::new(file(writer)).unwrap();
    drop(reader);
    queue.submit(b"accepted before failure").unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while queue.error().is_none() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        queue.error().is_some(),
        "an accepted final write failure must be observable without later input"
    );
}
