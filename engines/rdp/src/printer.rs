//! Legacy RDP printer redirection to one explicitly selected local queue.
use super::{INVALID, MAX_IO, UNSUPPORTED, completion_header, status, u32_at, wide, word};
#[path = "printer_ipp.rs"]
mod ipp;
use std::{
    collections::BTreeMap,
    fs::File,
    io::Write,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
#[cfg(target_os = "linux")]
#[path = "printer_linux.rs"]
mod linux;
#[cfg(target_os = "linux")]
pub(super) use linux::Binding;
#[cfg(any(windows, target_os = "macos", test))]
#[path = "printer_native.rs"]
mod native;
#[cfg(any(windows, target_os = "macos"))]
pub(super) use native::Binding;
#[cfg(any(windows, target_os = "macos"))]
pub(crate) fn child() -> Result<(), String> {
    native::child()
}

const MAX_JOB_BYTES: u32 = 32 * 1024 * 1024;
const MAX_JOBS: u32 = 4;
const CANCELLED: u32 = 0xc0000120;
const INVALID_HANDLE: u32 = 0xc0000008;

async fn wait_cancelled(closed: &AtomicBool) {
    while !closed.load(Ordering::Acquire) {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

struct Job {
    file: File,
    bytes: u32,
}

pub(super) struct Printer {
    binding: Binding,
    device: u32,
    jobs: BTreeMap<u32, Job>,
    next_file_id: u32,
}

impl Printer {
    pub(super) fn new(binding: Binding, device: u32) -> Self {
        Self {
            binding,
            device,
            jobs: BTreeMap::new(),
            next_file_id: 0,
        }
    }

    pub(super) fn contains(&self, device: u32) -> bool {
        self.device == device
    }

    pub(super) fn process(
        &mut self,
        request: &[u8],
        closed: &AtomicBool,
    ) -> super::DeviceCompletion {
        let mut notice = None;
        let file = u32_at(request, 8).unwrap();
        let major = u32_at(request, 16).unwrap();
        let minor = u32_at(request, 20).unwrap();
        let result = if minor != 0 {
            if matches!(major, 2 | 4) {
                self.jobs.remove(&file);
            }
            Err(INVALID)
        } else {
            self.operation(file, major, &request[24..], closed, &mut notice)
        };
        (response(request, result), notice)
    }

    fn operation(
        &mut self,
        file: u32,
        major: u32,
        body: &[u8],
        closed: &AtomicBool,
        notice: &mut Option<&'static str>,
    ) -> Result<u32, u32> {
        match major {
            0 => {
                if file != 0 || body.len() != 32 || u32_at(body, 28)? != 0 {
                    return Err(INVALID);
                }
                if self.jobs.len() >= MAX_JOBS as usize {
                    return Err(0xc000009a);
                }
                // A delayed close may still refer to a job discarded by a failed write.
                let id = self.next_file_id;
                let next = id.checked_add(1).ok_or(0xc000009au32)?;
                let file = tempfile::tempfile().map_err(status)?;
                self.next_file_id = next;
                self.jobs.insert(id, Job { file, bytes: 0 });
                Ok(id)
            }
            4 => {
                let job = self.jobs.get_mut(&file).ok_or(INVALID_HANDLE)?;
                // Printer writes are a stream. Windows repeats the offset for each chunk.
                let Some(length) = u32_at(body, 0)
                    .ok()
                    .map(|n| n as usize)
                    .filter(|&length| length <= MAX_IO && body.len() == 32 + length)
                else {
                    self.jobs.remove(&file);
                    return Err(INVALID);
                };
                let total = job.bytes + length as u32;
                if total > MAX_JOB_BYTES {
                    self.jobs.remove(&file);
                    return Err(0xc000007f); // STATUS_DISK_FULL
                }
                if let Err(error) = job.file.write_all(&body[32..]) {
                    self.jobs.remove(&file);
                    return Err(status(error));
                }
                job.bytes = total;
                Ok(length as u32)
            }
            2 => {
                let job = self.jobs.remove(&file).ok_or(INVALID_HANDLE)?;
                if body.len() != 32 {
                    return Err(INVALID);
                }
                if job.bytes != 0
                    && let Err(error) = self.binding.submit(job, closed)
                {
                    *notice = Some(
                        "The local spooler did not confirm the print job. Check the selected queue before retrying.",
                    );
                    return Err(error);
                }
                Ok(0)
            }
            _ => Err(UNSUPPORTED),
        }
    }
}

pub(super) fn response(request: &[u8], result: Result<u32, u32>) -> Vec<u8> {
    let (status, count) = match result {
        Ok(value) => (0, value),
        Err(error) => (error, 0),
    };
    let mut packet = completion_header(request, status);
    word(&mut packet, count);
    if u32_at(request, 16) == Ok(4) {
        packet.push(0);
    }
    packet
}

// Discard legacy cache notifications. Names and driver blobs never affect the binding.
pub(super) fn discard_cache(packet: &[u8]) -> Result<(), String> {
    let valid_shape = || -> Option<bool> {
        if !packet.starts_with(b"RPCP") || packet.len() > MAX_IO + 128 {
            return None;
        }
        let event = u32_at(packet, 4).ok()?;
        let lengths: &[usize] = match event {
            1 => &[16, 20, 24, 28],
            2 | 4 => &[8, 12],
            3 => &[8],
            _ => return None,
        };
        let start = lengths.last()? + 4;
        let end = lengths.iter().try_fold(start, |total, &at| {
            total.checked_add(u32_at(packet, at).ok()? as usize)
        })?;
        Some(end == packet.len())
    };
    if valid_shape() != Some(true) {
        return Err("Invalid RDP printer cache event".into());
    }
    Ok(())
}
