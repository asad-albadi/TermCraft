//! Bounded MS-RPCE Type Serialization Version 1 for MS-RDPESC call packets.
use zeroize::Zeroizing;

pub(super) const MAX_APDU: usize = 66_560;
pub(super) const MAX_PACKET: usize = MAX_APDU + 2048;
pub(super) const INVALID: u32 = 0xc000000d;
pub(super) const SMALL: u32 = 0xc0000023;
pub(super) const UNSUPPORTED: u32 = 0xc00000bb;
pub(super) const BUSY: u32 = 0x80000011;

// MS-RDPESC section 3.1.4 lists the assigned function ranges.
pub(super) fn unassigned_control(bytes: &[u8]) -> bool {
    if word_at(bytes, 16) != Ok(0xe) {
        return false;
    }
    word_at(bytes, 32).is_ok_and(|code| {
        code.checked_sub(0x0009_0000).is_none_or(|offset| {
            offset % 4 != 0 || !matches!(offset / 4, 5..=11 | 20..=31 | 38..=66)
        })
    })
}

pub(super) fn word_at(bytes: &[u8], offset: usize) -> Result<u32, u32> {
    let bytes = bytes.get(offset..offset + 4).ok_or(INVALID)?;
    Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
}

pub(super) struct In<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> In<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Self, u32> {
        if !bytes.starts_with(&[1, 0x10, 8, 0])
            || word_at(bytes, 8)? as usize != bytes.len().saturating_sub(16)
        {
            return Err(INVALID);
        }
        let bytes = bytes.get(16..).ok_or(INVALID)?;
        if bytes.len() % 8 != 0 {
            return Err(INVALID);
        }
        Ok(Self { bytes, pos: 0 })
    }

    pub fn word(&mut self) -> Result<u32, u32> {
        self.align(4)?;
        let value = word_at(self.bytes, self.pos)?;
        self.pos += 4;
        Ok(value)
    }

    pub fn take(&mut self, len: usize) -> Result<&'a [u8], u32> {
        let end = self.pos.checked_add(len).ok_or(INVALID)?;
        let data = self.bytes.get(self.pos..end).ok_or(INVALID)?;
        self.pos = end;
        Ok(data)
    }

    fn align(&mut self, width: usize) -> Result<(), u32> {
        self.take(self.pos.next_multiple_of(width) - self.pos)
            .map(|_| ())
    }

    pub fn boolean(&mut self) -> Result<bool, u32> {
        match self.word()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(INVALID),
        }
    }

    pub fn pointer(&mut self) -> Result<bool, u32> {
        Ok(self.word()? != 0)
    }

    pub fn token_header(&mut self) -> Result<(), u32> {
        if self.word()? != 4 || !self.pointer()? {
            return Err(INVALID);
        }
        Ok(())
    }

    pub fn token(&mut self) -> Result<u32, u32> {
        if self.word()? != 4 {
            return Err(INVALID);
        }
        self.word()
    }

    pub fn card_header(&mut self) -> Result<(), u32> {
        self.token_header()?;
        self.token_header()
    }

    pub fn card(&mut self) -> Result<(u32, u32), u32> {
        Ok((self.token()?, self.token()?))
    }

    pub fn count(&mut self, max: u32) -> Result<usize, u32> {
        let value = self.word()?;
        if value > max {
            return Err(INVALID);
        }
        Ok(value as usize)
    }

    pub fn conformant(&mut self, len: usize) -> Result<&'a [u8], u32> {
        if self.word()? as usize != len {
            return Err(INVALID);
        }
        self.take(len)
    }

    pub fn reader(&mut self, wide: bool) -> Result<String, u32> {
        let maximum = self.count(256)?;
        let offset = self.word()?;
        let length = self.count(256)?;
        if offset != 0 || length == 0 || length > maximum {
            return Err(INVALID);
        }
        let name = self.take(length * if wide { 2 } else { 1 })?;
        if wide {
            let units: Vec<u16> = name
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect();
            let units = units.strip_suffix(&[0]).ok_or(INVALID)?;
            if units.contains(&0) {
                return Err(INVALID);
            }
            String::from_utf16(units).map_err(|_| INVALID)
        } else {
            let name = name.strip_suffix(&[0]).ok_or(INVALID)?;
            if !name.is_ascii() || name.contains(&0) {
                return Err(INVALID);
            }
            Ok(String::from_utf8(name.to_vec()).map_err(|_| INVALID)?)
        }
    }

    pub fn finish(self) -> Result<(), u32> {
        if self.pos.next_multiple_of(8) == self.bytes.len() {
            Ok(())
        } else {
            Err(INVALID)
        }
    }
}

pub(super) struct Out {
    // Contains the complete RDPDR response, including any card data.
    bytes: Zeroizing<Vec<u8>>,
    referent: u32,
}

impl Out {
    pub fn response(request: &[u8], capacity: usize) -> Self {
        // Reserve the final capacity before copying any potentially sensitive bytes.
        let mut bytes = Zeroizing::new(Vec::with_capacity(36 + capacity.next_multiple_of(8)));
        bytes.extend_from_slice(b"rD");
        bytes.extend_from_slice(&0x4943u16.to_le_bytes());
        bytes.extend_from_slice(&request[4..8]);
        bytes.extend_from_slice(&request[12..16]);
        bytes.extend_from_slice(&[0; 8]); // IoStatus and OutputBufferLength
        bytes.extend_from_slice(&[1, 0x10, 8, 0, 0xcc, 0xcc, 0xcc, 0xcc]);
        bytes.extend_from_slice(&[0; 8]); // ObjectBufferLength and reserved field
        Self {
            bytes,
            referent: 0x0002_0000,
        }
    }

    pub fn word(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub fn pointer(&mut self, present: bool) {
        if present {
            self.word(self.referent);
            self.referent += 4;
        } else {
            self.word(0);
        }
    }

    pub fn token_header(&mut self, present: bool) {
        self.word(if present { 4 } else { 0 });
        self.pointer(present);
    }

    pub fn token(&mut self, token: u32) {
        self.word(4);
        self.word(token);
    }

    pub fn bytes(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    pub fn finish(mut self, max_output: usize) -> Zeroizing<Vec<u8>> {
        // 20 bytes of device completion framing precede the Type Serialization stream.
        let object_len = (self.bytes.len() - 36).next_multiple_of(8);
        if object_len + 16 > max_output {
            self.bytes.truncate(20);
            self.bytes[12..16].copy_from_slice(&SMALL.to_le_bytes());
        } else {
            self.bytes.resize(36 + object_len, 0);
            self.bytes[16..20].copy_from_slice(&((object_len + 16) as u32).to_le_bytes());
            self.bytes[28..32].copy_from_slice(&(object_len as u32).to_le_bytes());
        }
        self.bytes
    }
}

pub(super) fn fail(request: &[u8], code: u32) -> Zeroizing<Vec<u8>> {
    let mut response = Zeroizing::new(Vec::with_capacity(20));
    response.extend_from_slice(b"rDCI");
    response.extend_from_slice(&request[4..8]);
    response.extend_from_slice(&request[12..16]);
    response.extend_from_slice(&code.to_le_bytes());
    response.extend_from_slice(&[0; 4]);
    response
}
