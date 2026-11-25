use super::{HOST_PROTOCOL, Result, invalid};
use crate::packages::cancelled;
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::Write,
    path::Path,
    sync::atomic::AtomicBool,
    thread,
    time::{Duration, Instant},
};

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;
#[cfg(unix)]
use unix::PluginChild;
#[cfg(windows)]
use windows::PluginChild;

const DEADLINE: Duration = Duration::from_secs(5);
const MAX_RESPONSE: usize = 4096;

#[derive(Serialize)]
struct Request<'a> {
    protocol: u16,
    entrypoint: &'a str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Response {
    protocol: u16,
    ok: bool,
}

pub(super) struct Invocation {
    child: PluginChild,
    until: Instant,
}

impl Invocation {
    pub(super) fn start(
        directory: &Path,
        executable: &str,
        verified: &File,
        entrypoint: &str,
    ) -> Result<Self> {
        let mut request = serde_json::to_vec(&Request {
            protocol: HOST_PROTOCOL,
            entrypoint,
        })
        .map_err(std::io::Error::other)?;
        request.push(b'\n');
        let (child, mut stdin) = PluginChild::spawn(directory, executable, verified)
            .map_err(|_| invalid("Could not launch the plugin executable"))?;
        // This single sub-128-byte write fits in an empty anonymous pipe.
        stdin
            .write_all(&request)
            .map_err(|_| invalid("Plugin exited before accepting its request"))?;
        Ok(Self {
            child,
            until: Instant::now() + DEADLINE,
        })
    }

    pub(super) fn finish(mut self, cancel: &AtomicBool) -> Result<()> {
        let mut collected = Vec::with_capacity(MAX_RESPONSE);
        loop {
            cancelled(cancel)?;
            if Instant::now() >= self.until {
                return Err(invalid("Plugin response timed out"));
            }
            self.drain(&mut collected)?;
            if let Some(succeeded) = self.child.exited()? {
                if !succeeded {
                    return Err(invalid("Plugin exited unsuccessfully"));
                }
                // A final write can race the first drain and exit observation.
                self.drain(&mut collected)?;
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let packet = collected
            .strip_suffix(b"\n")
            .ok_or_else(|| invalid("Missing plugin IPC response"))?;
        if packet.contains(&b'\n') {
            return Err(invalid("Expected one plugin IPC response"));
        }
        let response: Response =
            serde_json::from_slice(packet).map_err(|_| invalid("Malformed plugin IPC response"))?;
        if response.protocol != HOST_PROTOCOL {
            return Err(invalid("Plugin response protocol is incompatible"));
        }
        if !response.ok {
            return Err(invalid("Plugin reported failure"));
        }
        Ok(())
    }

    fn drain(&mut self, collected: &mut Vec<u8>) -> Result<()> {
        let mut buffer = [0_u8; 512];
        while let Some(count) = self.child.read_available(&mut buffer)? {
            if count == 0 {
                break;
            }
            if collected.len() + count > MAX_RESPONSE {
                return Err(invalid("Plugin response exceeds the IPC size limit"));
            }
            collected.extend_from_slice(&buffer[..count]);
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
pub(super) use unix::{finish_verification, verification_target};

#[cfg(windows)]
pub(super) use windows::run_if_requested;
