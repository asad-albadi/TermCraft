//! Length framing independent of native pipe capacity; no datagram size assumptions.
use super::ndr::MAX_PACKET;
use std::io::{self, Read, Write};
use zeroize::Zeroizing;
pub(super) fn read(input: &mut impl Read) -> io::Result<Zeroizing<Vec<u8>>> {
    let mut header = [0; 4];
    input.read_exact(&mut header)?;
    let len = u32::from_le_bytes(header) as usize;
    if len == 0 || len > MAX_PACKET {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid PC/SC frame length",
        ));
    }
    let mut bytes = Zeroizing::new(vec![0; len]);
    input.read_exact(&mut bytes)?;
    Ok(bytes)
}
pub(super) fn write(output: &mut impl Write, bytes: &[u8]) -> io::Result<()> {
    if bytes.is_empty() || bytes.len() > MAX_PACKET {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid PC/SC frame length",
        ));
    }
    output.write_all(&(bytes.len() as u32).to_le_bytes())?;
    output.write_all(bytes)?;
    output.flush()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_contract_and_fragmentation() {
        struct Fragmented(std::io::Cursor<Vec<u8>>);
        impl Read for Fragmented {
            fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
                let n = b.len().min(7);
                self.0.read(&mut b[..n])
            }
        }
        let input = Zeroizing::new(vec![0x6a; MAX_PACKET]);
        let mut wire = Vec::new();
        write(&mut wire, &input).unwrap();
        write(&mut wire, b"next").unwrap();
        let mut reader = Fragmented(std::io::Cursor::new(wire));
        assert_eq!(*read(&mut reader).unwrap(), *input);
        assert_eq!(&*read(&mut reader).unwrap(), b"next");
    }
    #[test]
    fn rejects_before_payload_and_truncated_frames() {
        for n in [0, MAX_PACKET as u32 + 1, u32::MAX] {
            assert_eq!(
                read(&mut &n.to_le_bytes()[..]).unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
        }
        assert!(read(&mut &[3, 0, 0, 0, 1][..]).is_err());
        assert!(write(&mut Vec::new(), &vec![0; MAX_PACKET + 1]).is_err());
    }
}
