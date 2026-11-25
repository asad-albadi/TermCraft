//! Bounded eligibility check for native macOS browser executables.
//! This checks architecture/container bounds; macOS remains responsible for loading and signing policy.
use std::io::{self, Read, Seek, SeekFrom};

pub(crate) fn verify(image: &mut (impl Read + Seek), cpu: u32) -> io::Result<()> {
    let invalid = || {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Browser executable does not contain a valid native macOS image",
        )
    };
    if !matches!(cpu, 0x01000007 | 0x0100000c) {
        return Err(invalid());
    }
    let length = image.seek(SeekFrom::End(0))?;
    image.seek(SeekFrom::Start(0))?;
    let mut header = [0; 32];
    image.read_exact(&mut header)?;
    let mut selected = (0, length);
    if header[..4] == [0xca, 0xfe, 0xba, 0xbe] {
        let count = u32::from_be_bytes(header[4..8].try_into().unwrap()) as usize;
        if count == 0 || count > 32 {
            return Err(invalid());
        }
        let table_end = 8 + count as u64 * 20;
        if table_end > length {
            return Err(invalid());
        }
        image.seek(SeekFrom::Start(8))?;
        let mut entries = vec![0; count * 20];
        image.read_exact(&mut entries)?;
        let mut native = None;
        let mut ranges = Vec::with_capacity(count);
        for entry in entries.chunks_exact(20) {
            let word = |at| u32::from_be_bytes(entry[at..at + 4].try_into().unwrap());
            let offset = u64::from(word(8));
            let size = u64::from(word(12));
            let alignment = word(16);
            let end = offset.checked_add(size).ok_or_else(invalid)?;
            if alignment > 31
                || offset < table_end
                || size < 32
                || end > length
                || offset % (1u64 << alignment) != 0
            {
                return Err(invalid());
            }
            ranges.push((offset, end));
            if word(0) == cpu {
                if native.is_some() {
                    return Err(invalid());
                }
                native = Some((offset, size));
            }
        }
        ranges.sort_unstable();
        if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
            return Err(invalid());
        }
        selected = native.ok_or_else(invalid)?;
        image.seek(SeekFrom::Start(selected.0))?;
        image.read_exact(&mut header)?;
    }
    let word = |at| u32::from_le_bytes(header[at..at + 4].try_into().unwrap());
    if header[..4] != [0xcf, 0xfa, 0xed, 0xfe]
        || word(4) != cpu
        || word(12) != 2
        || 32 + u64::from(word(20)) > selected.1
    {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    fn thin(cpu: u32) -> Vec<u8> {
        let mut data = vec![0; 64];
        data[..4].copy_from_slice(&[0xcf, 0xfa, 0xed, 0xfe]);
        data[4..8].copy_from_slice(&cpu.to_le_bytes());
        data[12..16].copy_from_slice(&2u32.to_le_bytes());
        data
    }
    fn universal() -> Vec<u8> {
        let mut data = vec![0; 12288];
        data[..4].copy_from_slice(&[0xca, 0xfe, 0xba, 0xbe]);
        data[4..8].copy_from_slice(&2u32.to_be_bytes());
        for (i, cpu) in [0x01000007u32, 0x0100000c].into_iter().enumerate() {
            let entry = 8 + i * 20;
            let offset = (i + 1) * 4096;
            data[entry..entry + 4].copy_from_slice(&cpu.to_be_bytes());
            data[entry + 8..entry + 12].copy_from_slice(&(offset as u32).to_be_bytes());
            data[entry + 12..entry + 16].copy_from_slice(&64u32.to_be_bytes());
            data[entry + 16..entry + 20].copy_from_slice(&12u32.to_be_bytes());
            data[offset..offset + 64].copy_from_slice(&thin(cpu));
        }
        data
    }
    #[test]
    fn accepts_intel_and_arm_thin_images_but_rejects_wrong_cpu_and_libraries() {
        let mut data = thin(0x01000007);
        assert!(super::verify(&mut Cursor::new(&data), 0x01000007).is_ok());
        assert!(super::verify(&mut Cursor::new(&data), 0x0100000c).is_err());
        data[12..16].copy_from_slice(&6u32.to_le_bytes());
        assert!(super::verify(&mut Cursor::new(&data), 0x01000007).is_err());
        assert!(super::verify(&mut Cursor::new(thin(0x0100000c)), 0x0100000c).is_ok());
    }
    #[test]
    fn selects_and_checks_the_actual_native_slice_in_a_universal_application() {
        let mut data = universal();
        assert!(super::verify(&mut Cursor::new(&data), 0x01000007).is_ok());
        assert!(super::verify(&mut Cursor::new(&data), 0x0100000c).is_ok());
        data[8192 + 4..8192 + 8].copy_from_slice(&0x01000007u32.to_le_bytes());
        assert!(super::verify(&mut Cursor::new(&data), 0x0100000c).is_err());
    }
    #[test]
    fn rejects_truncated_huge_or_out_of_file_universal_tables() {
        for length in 0..32 {
            assert!(super::verify(&mut Cursor::new(vec![0; length]), 0x0100000c).is_err());
        }
        let mut data = universal();
        data[4..8].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(super::verify(&mut Cursor::new(&data), 0x01000007).is_err());
        let mut data = universal();
        data[8 + 8..8 + 12].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(super::verify(&mut Cursor::new(&data), 0x01000007).is_err());
    }
}
