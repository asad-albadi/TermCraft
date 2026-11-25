//! Darwin System CUPS: retained local connection, IPP raw bytes, no discovery.
//! Uses Darwin getpeereid via Tokio peer_cred, never Linux O_PATH or /proc.
use super::super::ipp;
use super::Backend;
use std::{
    io,
    os::unix::fs::{FileTypeExt, MetadataExt},
    time::Duration,
};
use tokio::{io::AsyncWriteExt, net::UnixStream, runtime::Runtime};
pub(super) struct Native {
    runtime: Runtime,
    stream: UnixStream,
    resource: String,
    user: String,
    uuid: Option<Vec<u8>>,
    remaining: Option<u32>,
}
fn failure(_: impl std::fmt::Debug) -> u32 {
    0xc0000001
}
fn local_socket() -> io::Result<()> {
    // System namespace is protected from the caller's unprivileged user. No
    // per-user CUPS config/environment, TCP discovery, symlinks or alternate path.
    for path in [
        "/private",
        "/private/var",
        "/private/var/run",
        "/private/var/run/cupsd",
    ] {
        let m = std::fs::symlink_metadata(path)?;
        if m.uid() != 0
            || m.file_type().is_symlink()
            || (path.ends_with("cupsd") && !m.file_type().is_socket())
            || (!path.ends_with("cupsd") && (!m.is_dir() || m.mode() & 0o022 != 0))
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Unprotected System CUPS namespace",
            ));
        }
    }
    Ok(())
}
impl Native {
    async fn request(
        stream: &mut UnixStream,
        resource: &str,
        user: &str,
        op: u16,
        bytes: u32,
    ) -> io::Result<()> {
        let body = ipp::request(op, resource, user);
        // Retain this exact connection until helper termination. A close or a
        // daemon restart fails closed; it cannot redirect a certificate retry.
        let header = format!(
            "POST {resource} HTTP/1.1\r\nHost: localhost\r\nConnection: keep-alive\r\nContent-Type: application/ipp\r\nContent-Length: {}\r\n\r\n",
            body.len() + bytes as usize
        );
        stream.write_all(header.as_bytes()).await?;
        stream.write_all(&body).await
    }
}
impl Backend for Native {
    fn open(queue: &str) -> Result<Self, u32> {
        if !queue
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
        {
            return Err(0xc000000d);
        }
        local_socket().map_err(failure)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(failure)?;
        let stream = runtime
            .block_on(async {
                let stream = tokio::time::timeout(
                    Duration::from_secs(3),
                    UnixStream::connect("/private/var/run/cupsd"),
                )
                .await??;
                if stream.peer_cred()?.uid() != 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "CUPS peer is not root",
                    ));
                }
                Ok::<_, io::Error>(stream)
            })
            .map_err(failure)?;
        // getpwuid_r, rather than USER env, is used only as a local hint; the
        // system service enforces socket/IPP authorization. Never solicit auth.
        let user = unsafe {
            let mut pwd: libc::passwd = std::mem::zeroed();
            let mut out = std::ptr::null_mut();
            let mut buf = vec![0; 16384];
            if libc::getpwuid_r(
                libc::geteuid(),
                &mut pwd,
                buf.as_mut_ptr().cast(),
                buf.len(),
                &mut out,
            ) != 0
                || out.is_null()
                || pwd.pw_name.is_null()
            {
                return Err(0xc0000022);
            }
            std::ffi::CStr::from_ptr(pwd.pw_name)
                .to_str()
                .map_err(failure)?
                .to_owned()
        };
        let mut this = Self {
            runtime,
            stream,
            resource: format!("/printers/{queue}"),
            user,
            uuid: None,
            remaining: None,
        };
        this.ready()?;
        Ok(this)
    }
    fn ready(&mut self) -> Result<(), u32> {
        if self.remaining.is_some() {
            return Err(0xc000000d);
        }
        let uuid = self
            .runtime
            .block_on(async {
                Self::request(
                    &mut self.stream,
                    &self.resource,
                    &self.user,
                    ipp::GET_PRINTER_ATTRIBUTES,
                    0,
                )
                .await?;
                let body = ipp::read_response(&mut self.stream).await?;
                let reply = ipp::Response::parse(&body, ipp::GET_PRINTER_ATTRIBUTES)?;
                reply
                    .check_queue(&self.resource)
                    .map_err(io::Error::other)?;
                Ok::<_, io::Error>(reply.queue_uuid()?.to_vec())
            })
            .map_err(failure)?;
        if self.uuid.as_ref().is_some_and(|p| *p != uuid) {
            return Err(0xc0000034);
        }
        self.uuid = Some(uuid);
        Ok(())
    }
    fn begin(&mut self, bytes: u32) -> Result<(), u32> {
        if self.remaining.is_some() {
            return Err(0xc000000d);
        }
        self.runtime
            .block_on(Self::request(
                &mut self.stream,
                &self.resource,
                &self.user,
                ipp::PRINT_JOB,
                bytes,
            ))
            .map_err(failure)?;
        self.remaining = Some(bytes);
        Ok(())
    }
    fn write(&mut self, bytes: &[u8]) -> Result<(), u32> {
        let left = self.remaining.ok_or(0xc0000008u32)?;
        if bytes.len() > left as usize {
            return Err(0xc000000d);
        }
        self.runtime
            .block_on(self.stream.write_all(bytes))
            .map_err(failure)?;
        self.remaining = Some(left - bytes.len() as u32);
        Ok(())
    }
    fn finish(&mut self) -> Result<(), u32> {
        if self.remaining != Some(0) {
            return Err(0xc000000d);
        }
        self.runtime
            .block_on(async {
                let body = ipp::read_response(&mut self.stream).await?;
                ipp::Response::parse(&body, ipp::PRINT_JOB)?.confirm_job(&self.resource)
            })
            .map_err(failure)?;
        self.remaining = None;
        Ok(())
    }
    fn abort(&mut self) -> Result<(), u32> {
        // Close only our unfinished upload. After a successful/ambiguous full
        // upload the service may own/print it; never cancel a recycled CUPS ID.
        self.remaining = None;
        self.runtime
            .block_on(self.stream.shutdown())
            .map_err(failure)
    }
}
