const IAC: u8 = 255;
const DONT: u8 = 254;
const DO: u8 = 253;
const WONT: u8 = 252;
const WILL: u8 = 251;
const SB: u8 = 250;
const SE: u8 = 240;
const BINARY: u8 = 0;
const ECHO: u8 = 1;
const SUPPRESS_GO_AHEAD: u8 = 3;
const TERMINAL_TYPE: u8 = 24;
const NAWS: u8 = 31;
const NEW_ENVIRON: u8 = 39;
const IS: u8 = 0;
const SEND: u8 = 1;

#[derive(Default)]
enum State {
    #[default]
    Data,
    Command,
    Negotiate(u8),
    Suboption,
    Subdata(u8),
    Subcommand(u8),
}

pub(super) struct Telnet {
    user: String,
    size: (u16, u16),
    local: [bool; 256],
    remote: [bool; 256],
    state: State,
    sub: Vec<u8>,
    discard_sub: bool,
    saw_cr: bool,
}

impl Telnet {
    pub(super) fn new(user: String, size: (u16, u16)) -> Self {
        Self {
            user,
            size,
            local: [false; 256],
            remote: [false; 256],
            state: State::default(),
            sub: Vec::new(),
            discard_sub: false,
            saw_cr: false,
        }
    }

    pub(super) fn receive(&mut self, bytes: &[u8], wire: &mut Vec<u8>, display: &mut Vec<u8>) {
        for &byte in bytes {
            self.state = match (std::mem::take(&mut self.state), byte) {
                (State::Data, IAC) => State::Command,
                (State::Data, _) | (State::Command, IAC) => {
                    if !(self.saw_cr && byte == 0) {
                        display.push(byte);
                    }
                    self.saw_cr = byte == b'\r' && !self.remote[BINARY as usize];
                    State::Data
                }
                (State::Command, DO | DONT | WILL | WONT) => State::Negotiate(byte),
                (State::Command, SB) => State::Suboption,
                (State::Command, _) => State::Data,
                (State::Negotiate(command), _) => {
                    self.negotiate(command, byte, wire);
                    State::Data
                }
                (State::Suboption, _) => {
                    self.sub.clear();
                    self.discard_sub = false;
                    State::Subdata(byte)
                }
                (State::Subdata(option), IAC) => State::Subcommand(option),
                (State::Subdata(option), _) | (State::Subcommand(option), IAC) => {
                    if self.sub.len() < 4096 {
                        self.sub.push(byte);
                    } else {
                        self.discard_sub = true;
                    }
                    State::Subdata(option)
                }
                (State::Subcommand(option), SE) => {
                    if !self.discard_sub {
                        self.subnegotiation(option, wire);
                    }
                    State::Data
                }
                (State::Subcommand(option), _) => {
                    self.discard_sub = true;
                    State::Subdata(option)
                }
            };
        }
    }

    fn negotiate(&mut self, command: u8, option: u8, wire: &mut Vec<u8>) {
        let index = option as usize;
        match command {
            DO => {
                let supported = match option {
                    BINARY | SUPPRESS_GO_AHEAD | TERMINAL_TYPE | NAWS => true,
                    NEW_ENVIRON => !self.user.is_empty(),
                    _ => false,
                };
                if !supported {
                    wire.extend([IAC, WONT, option]);
                } else if !std::mem::replace(&mut self.local[index], true) {
                    wire.extend([IAC, WILL, option]);
                    if option == NAWS {
                        self.resize(self.size, wire);
                    }
                }
            }
            DONT => {
                if std::mem::take(&mut self.local[index]) {
                    wire.extend([IAC, WONT, option]);
                }
            }
            WILL => {
                if matches!(option, BINARY | ECHO | SUPPRESS_GO_AHEAD) {
                    if !std::mem::replace(&mut self.remote[index], true) {
                        wire.extend([IAC, DO, option]);
                    }
                } else {
                    wire.extend([IAC, DONT, option]);
                }
            }
            WONT => {
                if std::mem::take(&mut self.remote[index]) {
                    wire.extend([IAC, DONT, option]);
                }
            }
            _ => unreachable!("the parser only enters negotiation for WILL/WONT/DO/DONT"),
        }
    }

    fn subnegotiation(&self, option: u8, wire: &mut Vec<u8>) {
        if !self.local[option as usize] || self.sub.first() != Some(&SEND) {
            return;
        }
        match option {
            TERMINAL_TYPE => subnegotiation(wire, option, b"\0XTERM-256COLOR"),
            NEW_ENVIRON if requests_user(&self.sub[1..]) => {
                let mut value = b"\0\0USER\x01".to_vec();
                for byte in self.user.bytes() {
                    if byte <= 3 {
                        value.push(2); // NEW-ENVIRON ESC
                    }
                    value.push(byte);
                }
                subnegotiation(wire, option, &value);
            }
            NEW_ENVIRON => subnegotiation(wire, option, &[IS]),
            _ => {}
        }
    }

    pub(super) fn resize(&mut self, size: (u16, u16), wire: &mut Vec<u8>) {
        self.size = size;
        if self.local[NAWS as usize] {
            let [col_high, col_low] = size.0.to_be_bytes();
            let [row_high, row_low] = size.1.to_be_bytes();
            subnegotiation(wire, NAWS, &[col_high, col_low, row_high, row_low]);
        }
    }

    pub(super) fn enter(&self, wire: &mut Vec<u8>, display: &mut Vec<u8>) {
        wire.push(b'\r');
        if !self.local[BINARY as usize] {
            wire.push(b'\n');
        }
        if !self.remote[ECHO as usize] {
            display.extend(b"\r\n");
        }
    }

    pub(super) fn echo_input(&self, bytes: &[u8], display: &mut Vec<u8>) {
        if self.remote[ECHO as usize]
            || bytes
                .iter()
                .any(|byte| matches!(byte, 0..=7 | 11..=12 | 14..=31))
        {
            return;
        }
        // Only echo text and basic editing. Escape sequences (mouse reports,
        // terminal replies, arrow keys) must not execute locally.
        for &byte in bytes {
            match byte {
                b'\x08' | b'\x7f' => display.extend(b"\x08 \x08"),
                b'\r' => {}
                b'\n' => display.extend(b"\r\n"),
                _ => display.push(byte),
            }
        }
    }

    pub(super) fn input(&self, bytes: &[u8], wire: &mut Vec<u8>) {
        let mut bytes = bytes.iter().copied().peekable();
        while let Some(byte) = bytes.next() {
            match byte {
                IAC => wire.extend([IAC, IAC]),
                b'\r' if !self.local[BINARY as usize] && bytes.peek() != Some(&b'\n') => {
                    wire.extend([b'\r', 0]);
                }
                b'\n' if !self.local[BINARY as usize] && !wire.ends_with(b"\r") => {
                    wire.extend(*b"\r\n");
                }
                _ => wire.push(byte),
            }
        }
    }
}

fn subnegotiation(wire: &mut Vec<u8>, option: u8, value: &[u8]) {
    wire.extend([IAC, SB, option]);
    for &byte in value {
        wire.push(byte);
        if byte == IAC {
            wire.push(IAC);
        }
    }
    wire.extend([IAC, SE]);
}

fn requests_user(mut request: &[u8]) -> bool {
    if request.is_empty() {
        return true;
    }
    while let Some((&kind, rest)) = request.split_first() {
        if !matches!(kind, 0 | 3) {
            return false;
        }
        let mut name = Vec::new();
        request = rest;
        while let Some((&byte, rest)) = request.split_first() {
            match byte {
                0 | 3 => break,
                2 => {
                    let Some((&escaped, rest)) = rest.split_first() else {
                        return false;
                    };
                    name.push(escaped);
                    request = rest;
                }
                _ => {
                    name.push(byte);
                    request = rest;
                }
            }
        }
        if kind == 0 && (name.is_empty() || name == b"USER") {
            return true;
        }
    }
    false
}
