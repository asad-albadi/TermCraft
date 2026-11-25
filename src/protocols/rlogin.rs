use tokio::net::TcpStream;

pub(super) fn greeting(local: &str, remote: &str) -> Vec<u8> {
    format!("\0{local}\0{remote}\0xterm-256color/38400\0").into_bytes()
}

#[derive(Default)]
pub(super) struct Rlogin {
    send_window: bool,
    discard_until_mark: bool,
}

impl Rlogin {
    pub(super) fn resize(&self, (cols, rows): (u16, u16), wire: &mut Vec<u8>) {
        if self.send_window {
            wire.extend([255, 255, b's', b's']);
            wire.extend(rows.to_be_bytes());
            wire.extend(cols.to_be_bytes());
            wire.extend([0; 4]); // Pixel size unknown.
        }
    }

    pub(super) fn receive_control(
        &mut self,
        stream: &TcpStream,
        size: (u16, u16),
        wire: &mut Vec<u8>,
    ) -> Result<bool, String> {
        let Some(flags) = urgent_byte(stream)? else {
            return Ok(false);
        };
        if flags & 0x80 != 0 && !std::mem::replace(&mut self.send_window, true) {
            self.resize(size, wire);
        }
        let flush = flags & 0x02 != 0;
        self.discard_until_mark |= flush;
        Ok(flush)
    }

    pub(super) fn discard_before_mark(&mut self, stream: &TcpStream) -> Result<bool, String> {
        if self.discard_until_mark && at_mark(stream)? {
            self.discard_until_mark = false;
        }
        Ok(self.discard_until_mark)
    }
}

#[cfg(unix)]
fn urgent_byte(stream: &TcpStream) -> Result<Option<u8>, String> {
    use std::{io, os::fd::AsRawFd};
    let mut flag = 0u8;
    // The Tokio socket is nonblocking. Rlogin sends its control flags using
    // TCP urgent data, with the flag byte separate from terminal output.
    let count = unsafe {
        libc::recv(
            stream.as_raw_fd(),
            (&mut flag as *mut u8).cast(),
            1,
            libc::MSG_OOB,
        )
    };
    if count == 1 {
        return Ok(Some(flag));
    }
    if count == 0 {
        return Ok(None);
    }
    let error = io::Error::last_os_error();
    if matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::InvalidInput | io::ErrorKind::Interrupted
    ) {
        return Ok(None);
    }
    Err(format!("Read Rlogin urgent control data: {error}"))
}

#[cfg(windows)]
fn urgent_byte(stream: &TcpStream) -> Result<Option<u8>, String> {
    use std::os::windows::io::AsRawSocket;

    #[link(name = "Ws2_32")]
    unsafe extern "system" {
        fn recv(socket: usize, buf: *mut u8, len: i32, flags: i32) -> i32;
        fn WSAGetLastError() -> i32;
    }

    const MSG_OOB: i32 = 1;
    const WSAEWOULDBLOCK: i32 = 10035;
    const WSAEINVAL: i32 = 10022;
    const WSAEINTR: i32 = 10004;
    let mut flag = 0;
    match unsafe { recv(stream.as_raw_socket() as usize, &mut flag, 1, MSG_OOB) } {
        1 => Ok(Some(flag)),
        0 => Ok(None),
        _ => match unsafe { WSAGetLastError() } {
            WSAEWOULDBLOCK | WSAEINVAL | WSAEINTR => Ok(None),
            code => Err(format!(
                "Read Rlogin urgent control data: {}",
                std::io::Error::from_raw_os_error(code)
            )),
        },
    }
}

// With out-of-band reception, a normal TCP read stops at the urgent mark.
// Check the mark before the next read so bytes after a flush remain visible.
#[cfg(unix)]
fn at_mark(stream: &TcpStream) -> Result<bool, String> {
    use std::{io, os::fd::AsRawFd};

    unsafe extern "C" {
        fn sockatmark(socket: libc::c_int) -> libc::c_int;
    }

    match unsafe { sockatmark(stream.as_raw_fd()) } {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(format!(
            "Check Rlogin urgent mark: {}",
            io::Error::last_os_error()
        )),
    }
}

#[cfg(windows)]
fn at_mark(stream: &TcpStream) -> Result<bool, String> {
    use std::{io, os::windows::io::AsRawSocket};

    #[link(name = "Ws2_32")]
    unsafe extern "system" {
        fn ioctlsocket(socket: usize, command: i32, value: *mut u32) -> i32;
        fn WSAGetLastError() -> i32;
    }

    const SIOCATMARK: i32 = 0x40047307;
    let mut mark = 0;
    match unsafe { ioctlsocket(stream.as_raw_socket() as usize, SIOCATMARK, &mut mark) } {
        0 => Ok(mark != 0),
        _ => Err(format!(
            "Check Rlogin urgent mark: {}",
            io::Error::from_raw_os_error(unsafe { WSAGetLastError() })
        )),
    }
}
