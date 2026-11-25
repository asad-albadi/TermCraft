//! Optional Linux helper. Only inherited portal-fd input; no discovery or default remote.
use std::{
    io::{self, Write},
    os::fd::{FromRawFd, OwnedFd},
    os::unix::net::UnixStream,
    sync::Arc,
    time::Duration,
};
fn main() {
    if run().is_err() {
        std::process::exit(1);
    }
}
fn run() -> io::Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 2 {
        return Err(io::Error::other("Invalid helper arguments"));
    }
    let node = args[0]
        .parse()
        .map_err(|_| io::Error::other("Invalid node"))?;
    let serial = if args[1] == "none" {
        None
    } else {
        Some(
            args[1]
                .parse()
                .map_err(|_| io::Error::other("Invalid serial"))?,
        )
    };
    // FD 3 belongs to the portal remote; FD 4 is a private app-owned stream.
    let fd = unsafe { OwnedFd::from_raw_fd(3) };
    let mut output = unsafe { UnixStream::from_raw_fd(4) };
    output.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut capture =
        termcraft_wayland_engine::capture::Capture::open(fd, node, serial, Arc::new(|| false))?;
    loop {
        if let Some(frame) = capture.frame()? {
            output.write_all(b"TVF1")?;
            output.write_all(&frame.width.to_le_bytes())?;
            output.write_all(&frame.height.to_le_bytes())?;
            output.write_all(&(frame.rgba.len() as u32).to_le_bytes())?;
            output.write_all(&frame.rgba)?;
        }
        std::thread::sleep(Duration::from_millis(33));
    }
}
