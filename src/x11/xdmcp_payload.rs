//! Cancellable loading of the pinned owned engine from an opened regular file.
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::OpenOptionsExt,
    },
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};
const MAX_PAYLOAD_BYTES: u64 = 16 * 1024 * 1024;
const PINNED_SHA256: [u8; 32] = [
    0xd7, 0xf1, 0xf4, 0x9e, 0xe4, 0x28, 0xfd, 0x06, 0xb2, 0x67, 0x07, 0xa7, 0x9b, 0xa6, 0xd6, 0x5b,
    0x4e, 0x9a, 0xda, 0x4e, 0x6b, 0x53, 0xac, 0xa4, 0x14, 0x6b, 0xaa, 0xa9, 0x2c, 0x79, 0xe5, 0x93,
];

pub(super) fn check_budget(cancelled: &AtomicBool, deadline: Instant) -> Result<(), String> {
    if cancelled.load(Ordering::Relaxed) {
        return Err("XDMCP payload/startup cancelled".into());
    }
    if Instant::now() >= deadline {
        return Err("XDMCP startup deadline expired".into());
    }
    Ok(())
}

pub(super) fn load_pinned(
    path: &Path,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<File, String> {
    load_engine(path, cancelled, deadline, PINNED_SHA256)
}
pub(super) fn load_ready_engine(
    path: &Path,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<File, String> {
    let digest = *include_bytes!("../../packaging/x11/xdmcp/readiness/executable.sha256.bin");
    load_engine(path, cancelled, deadline, digest)
}
fn load_engine(
    path: &Path,
    cancelled: &AtomicBool,
    deadline: Instant,
    expected: [u8; 32],
) -> Result<File, String> {
    check_budget(cancelled, deadline)?;
    // Validate the opened descriptor, not a racy path metadata lookup. Nonblocking
    // open also returns promptly for a FIFO without requiring any writer.
    let input = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|e| format!("Cannot open the pinned owned Xephyr payload: {e}"))?;
    check_budget(cancelled, deadline)?;
    let metadata = input.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("The owned Xephyr payload must be a regular file".into());
    }
    if metadata.len() > MAX_PAYLOAD_BYTES {
        return Err("The owned Xephyr payload exceeds its 16 MiB bound".into());
    }
    check_budget(cancelled, deadline)?;
    let fd = unsafe {
        libc::memfd_create(
            c"termcraft-xdmcp-engine".as_ptr(),
            libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let mut executable = unsafe { File::from_raw_fd(fd) };
    copy_verified(input, &mut executable, cancelled, deadline, expected)?;
    check_budget(cancelled, deadline)?;
    if unsafe {
        libc::fcntl(
            executable.as_raw_fd(),
            libc::F_ADD_SEALS,
            libc::F_SEAL_WRITE | libc::F_SEAL_SHRINK | libc::F_SEAL_GROW | libc::F_SEAL_SEAL,
        )
    } < 0
    {
        return Err(std::io::Error::last_os_error().to_string());
    }
    check_budget(cancelled, deadline)?;
    Ok(executable)
}

fn copy_verified(
    mut input: impl Read,
    output: &mut File,
    cancelled: &AtomicBool,
    deadline: Instant,
    expected: [u8; 32],
) -> Result<(), String> {
    let mut buffer = [0; 64 * 1024];
    let mut total = 0u64;
    let mut digest = Sha256::new();
    loop {
        check_budget(cancelled, deadline)?;
        let count = match input.read(&mut buffer) {
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(format!("Cannot read the owned Xephyr payload: {e}")),
        };
        check_budget(cancelled, deadline)?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > MAX_PAYLOAD_BYTES {
            return Err("The owned Xephyr payload exceeds its 16 MiB bound".into());
        }
        digest.update(&buffer[..count]);
        check_budget(cancelled, deadline)?;
        let mut copied = 0;
        while copied < count {
            check_budget(cancelled, deadline)?;
            match output.write(&buffer[copied..count]) {
                Ok(0) => return Err("Could not copy the owned Xephyr payload".into()),
                Ok(n) => copied += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e.to_string()),
            }
        }
    }
    check_budget(cancelled, deadline)?;
    let actual = digest.finalize();
    check_budget(cancelled, deadline)?;
    if actual.as_slice() != expected {
        return Err("The owned Xephyr engine failed its pinned SHA-256 verification".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        os::unix::fs::symlink,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
            mpsc,
        },
        time::{Duration, Instant},
    };
    #[test]
    fn fifo_directory_symlink_and_oversize_finish_without_an_x_display() {
        let root = tempfile::tempdir().unwrap();
        let fifo = root.path().join("fifo");
        let fifo_c = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) }, 0);
        let link = root.path().join("link");
        symlink(&fifo, &link).unwrap();
        let ordinary = root.path().join("ordinary");
        std::fs::write(&ordinary, b"not the pinned engine").unwrap();
        let regular_link = root.path().join("regular-link");
        symlink(&ordinary, &regular_link).unwrap();
        let huge = root.path().join("huge");
        std::fs::File::create(&huge)
            .unwrap()
            .set_len(MAX_PAYLOAD_BYTES + 1)
            .unwrap();
        for path in [fifo, root.path().to_path_buf(), link, regular_link, huge] {
            let (tx, rx) = mpsc::channel();
            let start = Instant::now();
            std::thread::spawn(move || {
                let _ = tx.send(
                    load_pinned(
                        &path,
                        &AtomicBool::new(false),
                        Instant::now() + Duration::from_secs(1),
                    )
                    .map(|_| ()),
                );
            });
            assert!(
                rx.recv_timeout(Duration::from_millis(500))
                    .expect("payload loader did not finish")
                    .is_err()
            );
            assert!(start.elapsed() < Duration::from_millis(500));
        }
    }
    #[test]
    fn cancellation_and_expired_deadline_prevent_payload_open() {
        let path = std::path::Path::new("/does-not-exist");
        assert!(
            load_pinned(
                path,
                &AtomicBool::new(true),
                Instant::now() + Duration::from_secs(1)
            )
            .unwrap_err()
            .contains("cancel")
        );
        assert!(
            load_pinned(path, &AtomicBool::new(false), Instant::now())
                .unwrap_err()
                .contains("deadline")
        );
    }
    struct CancelAfterRead {
        file: std::fs::File,
        cancelled: Arc<AtomicBool>,
    }
    impl Read for CancelAfterRead {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let n = self.file.read(buffer)?;
            self.cancelled.store(true, Ordering::Relaxed);
            Ok(n)
        }
    }
    #[test]
    fn cancellation_during_a_real_file_read_finishes_worker_before_copy() {
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(&vec![7u8; 128 * 1024]).unwrap();
        use std::io::{Seek, SeekFrom};
        file.seek(SeekFrom::Start(0)).unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut output = tempfile::tempfile().unwrap();
            let result = copy_verified(
                CancelAfterRead {
                    file,
                    cancelled: flag.clone(),
                },
                &mut output,
                &flag,
                Instant::now() + Duration::from_secs(1),
                [0; 32],
            );
            let _ = tx.send((result, output.metadata().unwrap().len()));
        });
        let (result, copied) = rx
            .recv_timeout(Duration::from_millis(500))
            .expect("cancelled load worker did not finish");
        assert!(result.unwrap_err().contains("cancel"));
        assert_eq!(copied, 0);
    }
    struct SlowFileRead(std::fs::File);
    impl Read for SlowFileRead {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let n = self.0.read(buffer)?;
            std::thread::sleep(Duration::from_millis(20));
            Ok(n)
        }
    }
    #[test]
    fn elapsed_deadline_stops_a_loading_worker_between_read_and_copy() {
        use std::io::{Seek, SeekFrom};
        let mut input = tempfile::tempfile().unwrap();
        input.write_all(&vec![9u8; 128 * 1024]).unwrap();
        input.seek(SeekFrom::Start(0)).unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut output = tempfile::tempfile().unwrap();
            let result = copy_verified(
                SlowFileRead(input),
                &mut output,
                &AtomicBool::new(false),
                Instant::now() + Duration::from_millis(10),
                [0; 32],
            );
            let _ = tx.send((result, output.metadata().unwrap().len()));
        });
        let (result, copied) = rx
            .recv_timeout(Duration::from_millis(500))
            .expect("expired loading worker did not finish");
        assert!(result.unwrap_err().contains("deadline"));
        assert_eq!(copied, 0);
    }
    #[test]
    fn regular_file_chunks_are_hashed_and_copied_without_relaxing_the_pin() {
        use std::io::{Seek, SeekFrom};
        let bytes = vec![6u8; 128 * 1024 + 17];
        let mut input = tempfile::tempfile().unwrap();
        input.write_all(&bytes).unwrap();
        input.seek(SeekFrom::Start(0)).unwrap();
        let expected: [u8; 32] = Sha256::digest(&bytes).as_slice().try_into().unwrap();
        let mut output = tempfile::tempfile().unwrap();
        copy_verified(
            &mut input,
            &mut output,
            &AtomicBool::new(false),
            Instant::now() + Duration::from_secs(1),
            expected,
        )
        .unwrap();
        output.seek(SeekFrom::Start(0)).unwrap();
        let mut actual = Vec::new();
        output.read_to_end(&mut actual).unwrap();
        assert_eq!(actual, bytes);
        input.seek(SeekFrom::Start(0)).unwrap();
        assert!(
            copy_verified(
                &mut input,
                &mut output,
                &AtomicBool::new(false),
                Instant::now() + Duration::from_secs(1),
                [0; 32]
            )
            .unwrap_err()
            .contains("SHA-256")
        );
    }
}
