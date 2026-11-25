//! Optional, verified, owned PipeWire helper with bounded latest-frame IPC.
use super::{Cancel, Frame};
use std::{
    io::{self, Read},
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::{net::UnixStream, process::CommandExt},
    },
    process::{Child, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
#[derive(Default)]
struct Slot {
    frame: Option<Frame>,
    error: bool,
    revision: u64,
}
pub(super) struct Capture {
    slot: Arc<Mutex<Slot>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
    child: Child,
    last: Instant,
    revision: u64,
}
impl Capture {
    pub(super) fn prepare(root: Option<&std::path::Path>) -> io::Result<std::fs::File> {
        let current = std::env::current_exe()?;
        let root = root
            .or_else(|| current.parent())
            .ok_or_else(|| io::Error::other("No Wayland helper installation"))?;
        let (_command, image) = crate::runtime::wayland_command(root).map_err(io::Error::other)?;
        Ok(image)
    }
    pub(super) fn open(
        fd: OwnedFd,
        node: u32,
        serial: Option<u64>,
        cancel: Cancel,
        image: std::fs::File,
    ) -> io::Result<Self> {
        let (input, output) = UnixStream::pair()?;
        input.set_read_timeout(Some(Duration::from_millis(100)))?;
        let video = duplicate(fd.as_raw_fd())?;
        let ipc = duplicate(output.as_raw_fd())?;
        let executable = duplicate(image.as_raw_fd())?;
        // Keep a dedicated verified executable descriptor above the fixed protocol fds.
        let mut command =
            std::process::Command::new(format!("/proc/self/fd/{}", executable.as_raw_fd()));
        command
            .arg0("termcraft-wayland-engine")
            .arg(node.to_string())
            .arg(serial.map_or_else(|| "none".into(), |v| v.to_string()))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        unsafe {
            command.pre_exec(move || {
                if libc::dup2(video.as_raw_fd(), 3) < 0 || libc::dup2(ipc.as_raw_fd(), 4) < 0 {
                    return Err(io::Error::last_os_error());
                }
                if libc::fcntl(executable.as_raw_fd(), libc::F_SETFD, 0) < 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child=command.spawn().map_err(|_|io::Error::other("Could not start optional Wayland helper; check packaged helper and native libpipewire-0.3 ABI"))?;
        drop(output);
        drop(fd);
        drop(image);
        let stop = Arc::new(AtomicBool::new(false));
        let slot = Arc::new(Mutex::new(Slot::default()));
        let owned = slot.clone();
        let stopped = stop.clone();
        let worker = match thread::Builder::new()
            .name("vnc-portal-frame-reader".into())
            .spawn(move || {
                if read_frames(input, &owned, &stopped, &cancel).is_err() {
                    owned.lock().unwrap().error = true;
                }
            }) {
            Ok(w) => w,
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e);
            }
        };
        Ok(Self {
            slot,
            stop,
            worker: Some(worker),
            child,
            last: Instant::now(),
            revision: 0,
        })
    }
    pub(super) fn frame(&mut self) -> io::Result<Option<Frame>> {
        let slot = self.slot.lock().unwrap();
        if slot.error || self.child.try_wait()?.is_some() {
            return Err(io::Error::other(
                "Selected portal video helper ended; check optional PipeWire helper/native ABI",
            ));
        }
        if slot.revision != self.revision {
            self.revision = slot.revision;
            self.last = Instant::now();
        }
        if self.last.elapsed() > Duration::from_secs(5) {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Selected portal video stream timed out",
            ));
        }
        Ok(slot.frame.clone())
    }
}
fn duplicate(fd: i32) -> io::Result<OwnedFd> {
    let n = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 10) };
    if n < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { OwnedFd::from_raw_fd(n) })
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}
fn read_exact(
    input: &mut UnixStream,
    mut dst: &mut [u8],
    stop: &AtomicBool,
    cancel: &Cancel,
) -> io::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !dst.is_empty() {
        if stop.load(Ordering::Acquire) || cancel() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Video reader cancelled",
            ));
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Video frame IPC timed out",
            ));
        }
        match input.read(dst) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "Video helper ended",
                ));
            }
            Ok(n) => dst = &mut dst[n..],
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
fn read_frames(
    mut input: UnixStream,
    slot: &Arc<Mutex<Slot>>,
    stop: &AtomicBool,
    cancel: &Cancel,
) -> io::Result<()> {
    loop {
        let mut header = [0; 12];
        read_exact(&mut input, &mut header, stop, cancel)?;
        let width = u16::from_le_bytes([header[4], header[5]]);
        let height = u16::from_le_bytes([header[6], header[7]]);
        let size = u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
        let pixels = usize::from(width) * usize::from(height);
        if &header[..4] != b"TVF1"
            || pixels == 0
            || pixels > super::super::MAX_PIXELS
            || size != pixels * 4
        {
            return Err(io::Error::other("Invalid bounded video IPC frame"));
        }
        let mut rgba = vec![0; size];
        read_exact(&mut input, &mut rgba, stop, cancel)?;
        if stop.load(Ordering::Acquire) || cancel() {
            return Ok(());
        }
        let mut state = slot.lock().unwrap();
        state.frame = Some(Frame {
            width,
            height,
            rgba,
        });
        state.revision = state.revision.wrapping_add(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn ipc_rejects_malformed_header_before_pixel_allocation() {
        for header in [
            *b"BAD!\x01\x00\x01\x00\x04\x00\x00\x00",
            *b"TVF1\x00\x00\x01\x00\x00\x00\x00\x00",
            *b"TVF1\xff\xff\xff\xff\xfc\xff\xff\xff",
            *b"TVF1\x01\x00\x01\x00\x03\x00\x00\x00",
        ] {
            let (reader, mut writer) = UnixStream::pair().unwrap();
            writer.write_all(&header).unwrap();
            drop(writer);
            let state = Arc::new(Mutex::new(Slot::default()));
            assert!(
                read_frames(
                    reader,
                    &state,
                    &AtomicBool::new(false),
                    &(Arc::new(|| false) as Cancel)
                )
                .is_err()
            );
            assert!(state.lock().unwrap().frame.is_none());
        }
    }
    #[test]
    fn ipc_partial_body_stop_does_not_publish_stale_frame() {
        let (reader, mut writer) = UnixStream::pair().unwrap();
        reader
            .set_read_timeout(Some(Duration::from_millis(20)))
            .unwrap();
        writer
            .write_all(b"TVF1\x01\x00\x01\x00\x04\x00\x00\x00\x01\x02")
            .unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let state = Arc::new(Mutex::new(Slot::default()));
        let owned = state.clone();
        let worker = thread::spawn(move || {
            read_frames(reader, &owned, &stopped, &(Arc::new(|| false) as Cancel)).is_err()
        });
        thread::sleep(Duration::from_millis(30));
        stop.store(true, Ordering::Release);
        assert!(worker.join().unwrap());
        assert!(state.lock().unwrap().frame.is_none());
    }
    #[test]
    fn ipc_coalesces_complete_frames_into_one_latest_slot() {
        let (reader, mut writer) = UnixStream::pair().unwrap();
        for color in [1u8, 2] {
            writer
                .write_all(b"TVF1\x01\x00\x01\x00\x04\x00\x00\x00")
                .unwrap();
            writer.write_all(&[color, 0, 0, 255]).unwrap();
        }
        drop(writer);
        let state = Arc::new(Mutex::new(Slot::default()));
        assert!(
            read_frames(
                reader,
                &state,
                &AtomicBool::new(false),
                &(Arc::new(|| false) as Cancel)
            )
            .is_err()
        );
        let slot = state.lock().unwrap();
        assert_eq!(slot.revision, 2);
        assert_eq!(slot.frame.as_ref().unwrap().rgba, [2, 0, 0, 255]);
    }
    #[test]
    fn helper_resolution_requires_optional_manifest_hash_and_native_image() {
        let root = tempfile::tempdir().unwrap();
        assert!(crate::runtime::wayland_command(root.path()).is_err());
        let mut manifest = serde_json::json!({"schema_version":1,"platform":"linux","architecture":std::env::consts::ARCH,"rdp_helper_version":"legacy","files":{}});
        let path = root.path().join("PACKAGE-MANIFEST.json");
        std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert!(
            crate::runtime::wayland_command(root.path())
                .unwrap_err()
                .contains("optional")
        );
        manifest["wayland_helper_version"] = "0.1.0".into();
        manifest["files"]["termcraft-wayland-engine"] = "00".repeat(32).into();
        std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        std::fs::write(root.path().join("termcraft-wayland-engine"), b"untrusted").unwrap();
        assert!(crate::runtime::wayland_command(root.path()).is_err());
    }
}
