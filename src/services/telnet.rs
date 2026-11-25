use super::{
    Auth,
    supervisor::{self, Context},
};
use crate::process::Process;
use std::{
    io::{self, Read},
    net::TcpStream,
    path::Path,
    sync::mpsc::{RecvTimeoutError, TryRecvError},
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

const IAC: u8 = 255;
const DONT: u8 = 254;
const DO: u8 = 253;
const WONT: u8 = 252;
const WILL: u8 = 251;
const SB: u8 = 250;
const SE: u8 = 240;
const ECHO: u8 = 1;
const SGA: u8 = 3;
const NAWS: u8 = 31;
const LOGIN_TIMEOUT: Duration = Duration::from_secs(30);
const IDLE: Duration = Duration::from_secs(300);
const MAX_SESSION: Duration = Duration::from_secs(3600);

pub(super) fn validate_shell(path: &Path) -> Result<(), String> {
    if !path.is_absolute() || path.to_str().is_none() {
        return Err("Choose an absolute path to the shell executable.".into());
    }
    let metadata = path
        .metadata()
        .map_err(|error| format!("Could not open the shell executable: {error}"))?;
    if !metadata.is_file() {
        return Err("Choose a shell executable file.".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err("The selected shell file is not executable.".into());
        }
    }
    Ok(())
}

pub(super) fn serve(
    mut stream: TcpStream,
    shell: &Path,
    auth: &Auth,
    context: &Context,
) -> io::Result<()> {
    let login = Instant::now() + LOGIN_TIMEOUT;
    supervisor::write(
        &mut stream,
        &[IAC, WILL, ECHO, IAC, WILL, SGA, IAC, DO, NAWS],
        context,
        login,
    )?;
    supervisor::write(&mut stream, b"Workspace login: ", context, login)?;
    let mut decoder = Decoder::default();
    let user = login_line(&mut stream, &mut decoder, context, login)?;
    supervisor::write(&mut stream, b"\r\nPassword: ", context, login)?;
    let password = login_line(&mut stream, &mut decoder, context, login)?;
    let accepted = auth.allows(&user, &password);
    drop(password);
    if !accepted {
        supervisor::write(
            &mut stream,
            b"\r\nAuthentication failed.\r\n",
            context,
            login,
        )?;
        return Ok(());
    }
    context.check(login)?;
    context.log("Telnet client authenticated.");
    // Construct and use the PTY in the owning worker, including on Windows.
    let (cols, rows) = decoder.window.unwrap_or((80, 24));
    let mut process = Process::spawn_with_environment(
        shell.to_str().ok_or(io::ErrorKind::InvalidInput)?,
        &[],
        None,
        cols,
        rows,
        &[],
    )
    .map_err(|_| io::Error::other("Could not launch configured shell"))?;
    supervisor::write(&mut stream, b"\r\n", context, login)?;
    bridge(&mut stream, &mut decoder, &mut process, context)
}

fn login_line(
    stream: &mut TcpStream,
    decoder: &mut Decoder,
    context: &Context,
    deadline: Instant,
) -> io::Result<Zeroizing<Vec<u8>>> {
    let mut line = Zeroizing::new(Vec::with_capacity(1024));
    let mut byte = Zeroizing::new([0; 1]);
    for _ in 0..4096 {
        if supervisor::read(stream, &mut *byte, context, deadline)? == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        match decoder.decode(byte[0], stream, context, deadline)? {
            Some(b'\r' | b'\n') => return Ok(line),
            Some(8 | 127) => {
                if let Some(last) = line.last_mut() {
                    *last = 0;
                    line.pop();
                }
            }
            Some(value) if !value.is_ascii_control() && line.len() < 1024 => line.push(value),
            Some(_) => return Err(io::ErrorKind::InvalidData.into()),
            None => {}
        }
    }
    Err(io::ErrorKind::InvalidData.into())
}

fn bridge(
    stream: &mut TcpStream,
    decoder: &mut Decoder,
    process: &mut Process,
    context: &Context,
) -> io::Result<()> {
    let session_end = Instant::now() + MAX_SESSION;
    let mut idle = Instant::now() + IDLE;
    let mut input = [0; 4096];
    let mut terminal_input = Vec::with_capacity(input.len());
    let mut output = Vec::new();
    let mut window = decoder.window;
    loop {
        let deadline = session_end.min(idle);
        context.check(deadline)?;
        for _ in 0..8 {
            let chunk = if process.exited {
                // PTY output can remain queued or unread after the shell exits.
                match process.rx.recv_timeout(supervisor::POLL) {
                    Ok(chunk) => chunk,
                    Err(RecvTimeoutError::Timeout) => break,
                    Err(RecvTimeoutError::Disconnected) => return Ok(()),
                }
            } else {
                match process.rx.try_recv() {
                    Ok(chunk) => chunk,
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return Ok(()),
                }
            };
            output.clear();
            for byte in chunk {
                match byte {
                    IAC => output.extend_from_slice(&[IAC, IAC]),
                    byte => output.push(byte),
                }
            }
            supervisor::write(stream, &output, context, deadline)?;
        }
        process.poll();
        if process.exited {
            continue;
        }
        if process.input_error().is_some() {
            return Err(io::Error::other("Shell input ended"));
        }
        match stream.read(&mut input) {
            Ok(0) => return Ok(()),
            Ok(count) => {
                terminal_input.clear();
                for &byte in &input[..count] {
                    if let Some(byte) = decoder.decode(byte, stream, context, deadline)? {
                        terminal_input.push(byte);
                    }
                }
                if decoder.window != window {
                    if let Some((cols, rows)) = decoder.window {
                        process.resize(cols, rows);
                    }
                    window = decoder.window;
                }
                if !terminal_input.is_empty() {
                    process.write(&terminal_input).map_err(io::Error::other)?;
                    idle = Instant::now() + IDLE;
                }
            }
            Err(error) if supervisor::would_wait(&error) => {}
            Err(error) => return Err(error),
        }
    }
}

#[derive(Default)]
enum Parse {
    #[default]
    Text,
    Escape,
    Negotiate(u8),
    SubOption,
    SubData {
        option: u8,
        escaped: bool,
    },
}

#[derive(Default)]
struct Decoder {
    parse: Parse,
    skip_cr_suffix: bool,
    window: Option<(u16, u16)>,
    sub_data: [u8; 4],
    sub_len: usize,
}

impl Decoder {
    fn decode(
        &mut self,
        byte: u8,
        stream: &mut TcpStream,
        context: &Context,
        deadline: Instant,
    ) -> io::Result<Option<u8>> {
        match std::mem::take(&mut self.parse) {
            Parse::Text if byte == IAC => self.parse = Parse::Escape,
            Parse::Text => return Ok(self.text(byte)),
            Parse::Escape => {
                self.parse = match byte {
                    IAC => return Ok(self.text(IAC)),
                    WILL | WONT | DO | DONT => Parse::Negotiate(byte),
                    SB => Parse::SubOption,
                    _ => Parse::Text,
                };
            }
            Parse::Negotiate(command) => {
                let reject = match command {
                    WILL if byte != NAWS => Some(DONT),
                    DO if !matches!(byte, ECHO | SGA) => Some(WONT),
                    _ => None,
                };
                if let Some(response) = reject {
                    supervisor::write(stream, &[IAC, response, byte], context, deadline)?;
                }
            }
            Parse::SubOption => {
                self.sub_len = 0;
                self.parse = Parse::SubData {
                    option: byte,
                    escaped: false,
                };
            }
            Parse::SubData { option, escaped } => {
                if escaped && byte == SE {
                    if option == NAWS && self.sub_len == 4 {
                        let [a, b, c, d] = self.sub_data;
                        let cols = u16::from_be_bytes([a, b]);
                        let rows = u16::from_be_bytes([c, d]);
                        if cols > 0 && rows > 0 {
                            self.window = Some((cols.min(500), rows.min(300)));
                        }
                    }
                } else if byte == IAC && !escaped {
                    self.parse = Parse::SubData {
                        option,
                        escaped: true,
                    };
                } else {
                    if self.sub_len < self.sub_data.len() {
                        self.sub_data[self.sub_len] = byte;
                    }
                    self.sub_len += 1;
                    if self.sub_len > 1024 {
                        return Err(io::ErrorKind::InvalidData.into());
                    }
                    self.parse = Parse::SubData {
                        option,
                        escaped: false,
                    };
                }
            }
        }
        Ok(None)
    }

    fn text(&mut self, byte: u8) -> Option<u8> {
        if std::mem::replace(&mut self.skip_cr_suffix, byte == b'\r') && matches!(byte, 0 | b'\n') {
            None
        } else {
            Some(byte)
        }
    }
}
