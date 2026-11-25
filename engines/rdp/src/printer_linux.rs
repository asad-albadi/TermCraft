//! Linux retained CUPS socket provider. Kept separate from native platform providers.
use super::{CANCELLED, Job, ipp, status, wait_cancelled, wide, word};
use crate::rdp_options::PrinterMapping;
use std::{
    fs::File,
    io::{self, Read, Seek},
    os::{fd::AsRawFd, unix::fs::FileTypeExt},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{io::AsyncWriteExt, net::UnixStream};
#[derive(Clone)]
pub(crate) struct Binding {
    mapping: PrinterMapping,
    socket: Arc<File>,
    uid: u32,
    local_user: String,
    timeout: Duration,
}

impl Binding {
    pub(crate) fn new(mapping: &PrinterMapping) -> Result<Self, String> {
        Self::open(mapping, Path::new("/run/cups/cups.sock"), 0)
    }

    #[cfg(test)]
    pub(crate) fn at(mapping: &PrinterMapping, socket: &Path) -> Result<Self, String> {
        use std::os::unix::fs::MetadataExt;
        let uid = std::fs::symlink_metadata(socket)
            .map_err(|_| "Cannot inspect the local CUPS socket")?
            .uid();
        let mut binding = Self::open(mapping, socket, uid)?;
        binding.timeout = Duration::from_millis(500);
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| "Cannot start the printer readiness check")?
            .block_on(binding.check_ready())?;
        Ok(binding)
    }

    fn open(mapping: &PrinterMapping, path: &Path, uid: u32) -> Result<Self, String> {
        use rustix::fs::{Mode, OFlags, open};
        use std::os::unix::fs::MetadataExt;
        let socket = File::from(
            open(
                path,
                OFlags::PATH | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|_| "The local CUPS socket /run/cups/cups.sock is unavailable")?,
        );
        let metadata = socket
            .metadata()
            .map_err(|_| "Cannot inspect the local CUPS socket")?;
        if !metadata.file_type().is_socket() || metadata.uid() != uid {
            return Err("The local CUPS socket has an unexpected type or owner".into());
        }
        // This is a local identity hint. The CUPS server enforces its own socket policy.
        let local_user = std::env::var("USER")
            .ok()
            .filter(|s| !s.is_empty() && s.len() <= 127 && !s.chars().any(char::is_control))
            .unwrap_or_else(|| "termcraft-rdp".into());
        Ok(Self {
            mapping: mapping.clone(),
            socket: Arc::new(socket),
            uid,
            local_user,
            timeout: Duration::from_secs(10),
        })
    }

    pub(crate) fn announcement(&self) -> Vec<u8> {
        let driver = wide(&format!("{}\0", self.mapping.driver));
        let printer = wide(&format!("{}\0", self.mapping.queue));
        let mut data = Vec::new();
        // Flags, code page, PnP name length, driver length, name length, cache length.
        for field in [0, 0, 0, driver.len() as u32, printer.len() as u32, 0] {
            word(&mut data, field);
        }
        data.extend(driver);
        data.extend(printer);
        data
    }

    pub(super) fn submit(&self, mut job: Job, closed: &AtomicBool) -> Result<(), u32> {
        if closed.load(Ordering::Acquire) {
            return Err(CANCELLED);
        }
        job.file.rewind().map_err(status)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .map_err(status)?;
        runtime.block_on(async {
            tokio::select! {
                biased;
                _ = wait_cancelled(closed) => Err(CANCELLED),
                result = tokio::time::timeout(self.timeout, self.print_job(job, closed)) => {
                    match result {
                        Ok(reply) => reply.map_err(status),
                        Err(_) => Err(0xc00000b5), // STATUS_IO_TIMEOUT
                    }
                }
            }
        })
    }

    pub(crate) async fn check_ready(&self) -> Result<(), String> {
        let error = |reason| format!("CUPS queue {} {reason}", self.mapping.queue);
        let resource = self.resource();
        let check = async {
            let stream = self
                .start_request(ipp::GET_PRINTER_ATTRIBUTES, &resource, 0)
                .await
                .map_err(|_| "could not be reached on the pinned local CUPS socket")?;
            let body = ipp::read_response(stream)
                .await
                .map_err(|_| "did not return a valid queue response")?;
            let reply = ipp::Response::parse(&body, ipp::GET_PRINTER_ATTRIBUTES)
                .map_err(|_| "did not return a valid IPP queue response")?;
            reply.check_queue(&resource)
        };
        tokio::time::timeout(self.timeout, check)
            .await
            .map_err(|_| error("readiness check timed out"))?
            .map_err(error)
    }

    fn resource(&self) -> String {
        format!("/printers/{}", self.mapping.queue)
    }

    async fn start_request(
        &self,
        operation: u16,
        resource: &str,
        document_len: usize,
    ) -> io::Result<UnixStream> {
        // Connect to the originally opened socket even if its pathname is replaced.
        let pinned = PathBuf::from(format!("/proc/self/fd/{}", self.socket.as_raw_fd()));
        let mut stream = UnixStream::connect(pinned).await?;
        if stream.peer_cred()?.uid() != self.uid {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Unexpected CUPS socket peer",
            ));
        }
        let ipp = ipp::request(operation, resource, &self.local_user);
        let length = ipp.len() + document_len;
        let header = format!(
            "POST {resource} HTTP/1.0\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/ipp\r\nContent-Length: {length}\r\n\r\n"
        );
        stream.write_all(header.as_bytes()).await?;
        stream.write_all(&ipp).await?;
        Ok(stream)
    }

    async fn print_job(&self, mut job: Job, closed: &AtomicBool) -> io::Result<()> {
        let resource = self.resource();
        let mut stream = self
            .start_request(ipp::PRINT_JOB, &resource, job.bytes as usize)
            .await?;
        let mut chunk = [0; 32 * 1024];
        let mut remaining = job.bytes as usize;
        while remaining != 0 {
            if closed.load(Ordering::Acquire) {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "Print job cancelled",
                ));
            }
            let count = remaining.min(chunk.len());
            job.file.read_exact(&mut chunk[..count])?;
            stream.write_all(&chunk[..count]).await?;
            remaining -= count;
            tokio::task::consume_budget().await;
        }
        let body = ipp::read_response(stream).await?;
        ipp::Response::parse(&body, ipp::PRINT_JOB)?.confirm_job(&resource)
    }
}
