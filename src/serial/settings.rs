use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Parity {
    #[default]
    None,
    Even,
    Odd,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlowControl {
    #[default]
    None,
    Software,
    Hardware,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SerialLine {
    pub data_bits: u8,
    pub parity: Parity,
    pub stop_bits: u8,
    pub flow_control: FlowControl,
}
impl Default for SerialLine {
    fn default() -> Self {
        Self {
            data_bits: 8,
            parity: Parity::None,
            stop_bits: 1,
            flow_control: FlowControl::None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SerialSettings {
    pub baud: u32,
    pub data_bits: u8,
    pub parity: Parity,
    pub stop_bits: u8,
    pub flow_control: FlowControl,
}
impl SerialLine {
    pub fn settings(self, baud: u32) -> SerialSettings {
        SerialSettings {
            baud,
            data_bits: self.data_bits,
            parity: self.parity,
            stop_bits: self.stop_bits,
            flow_control: self.flow_control,
        }
    }
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}
impl Default for SerialSettings {
    fn default() -> Self {
        SerialLine::default().settings(9600)
    }
}
impl SerialSettings {
    pub fn validate(&self) -> Result<(), String> {
        if ![
            50, 75, 110, 300, 600, 1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200, 230400,
        ]
        .contains(&self.baud)
            || !(5..=8).contains(&self.data_bits)
            || ![1, 2].contains(&self.stop_bits)
            || (self.data_bits == 5 && self.stop_bits == 2)
        {
            return Err("Choose a supported baud rate, 5–8 data bits, and 1 or 2 stop bits (5 data bits supports 1 stop bit)".into());
        }
        Ok(())
    }
    #[cfg(unix)]
    pub fn apply_termios(&self, t: &mut libc::termios) -> Result<(), String> {
        self.validate()?;
        let speed = match self.baud {
            50 => libc::B50,
            75 => libc::B75,
            110 => libc::B110,
            300 => libc::B300,
            600 => libc::B600,
            1200 => libc::B1200,
            2400 => libc::B2400,
            4800 => libc::B4800,
            9600 => libc::B9600,
            19200 => libc::B19200,
            38400 => libc::B38400,
            57600 => libc::B57600,
            115200 => libc::B115200,
            230400 => libc::B230400,
            _ => unreachable!(),
        };
        unsafe {
            libc::cfmakeraw(t);
            if libc::cfsetispeed(t, speed) != 0 || libc::cfsetospeed(t, speed) != 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
        }
        t.c_cflag &= !(libc::CSIZE | libc::PARENB | libc::PARODD | libc::CSTOPB | libc::CRTSCTS);
        t.c_cflag |= libc::CLOCAL
            | libc::CREAD
            | match self.data_bits {
                5 => libc::CS5,
                6 => libc::CS6,
                7 => libc::CS7,
                _ => libc::CS8,
            };
        match self.parity {
            Parity::None => {}
            Parity::Even => t.c_cflag |= libc::PARENB,
            Parity::Odd => t.c_cflag |= libc::PARENB | libc::PARODD,
        }
        if self.stop_bits == 2 {
            t.c_cflag |= libc::CSTOPB;
        }
        t.c_iflag &=
            !(libc::IXON | libc::IXOFF | libc::IXANY | libc::INPCK | libc::IGNPAR | libc::PARMRK);
        if self.parity != Parity::None {
            t.c_iflag |= libc::INPCK;
        }
        match self.flow_control {
            FlowControl::None => {}
            FlowControl::Software => {
                t.c_iflag |= libc::IXON | libc::IXOFF;
                t.c_cc[libc::VSTART] = 0x11;
                t.c_cc[libc::VSTOP] = 0x13;
            }
            FlowControl::Hardware => t.c_cflag |= libc::CRTSCTS,
        }
        t.c_cc[libc::VMIN] = 0;
        t.c_cc[libc::VTIME] = 1;
        Ok(())
    }
    #[cfg(windows)]
    pub fn apply_dcb(
        &self,
        d: &mut windows_sys::Win32::Devices::Communication::DCB,
    ) -> Result<(), String> {
        use windows_sys::Win32::Devices::Communication::*;
        self.validate()?;
        d.BaudRate = self.baud;
        d.ByteSize = self.data_bits;
        d.Parity = match self.parity {
            Parity::None => NOPARITY,
            Parity::Even => EVENPARITY,
            Parity::Odd => ODDPARITY,
        };
        d.StopBits = if self.stop_bits == 2 {
            TWOSTOPBITS
        } else {
            ONESTOPBIT
        };
        // DCB packed flags: binary, parity, CTS/DSR, DTR, XON/XOFF, RTS.
        // Reset inherited modem/software handshake state deterministically.
        d._bitfield =
            1 | if self.parity != Parity::None {
                1 << 1
            } else {
                0
            } | (1 << 4)
                | (1 << 12);
        match self.flow_control {
            FlowControl::None => {}
            FlowControl::Software => {
                d._bitfield |= (1 << 8) | (1 << 9);
                d.XonChar = 0x11;
                d.XoffChar = 0x13;
                d.XonLim = 128;
                d.XoffLim = 128;
            }
            FlowControl::Hardware => {
                d._bitfield &= !(3 << 12);
                d._bitfield |= (1 << 2) | (2 << 12);
            }
        }
        Ok(())
    }
}
