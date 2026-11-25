//! Locate direct Windows Chromium payloads and validate their PE machine type.

use std::io::{self, Read, Seek, SeekFrom};

pub(crate) fn verify(image: &mut (impl Read + Seek), machine: u16) -> io::Result<()> {
    let file_len = image.seek(SeekFrom::End(0))?;
    image.seek(SeekFrom::Start(0))?;
    let mut dos = [0u8; 64];
    image.read_exact(&mut dos)?;
    let offset = u32::from_le_bytes(dos[60..64].try_into().unwrap()) as u64;
    if dos[..2] != *b"MZ" || offset < dos.len() as u64 || offset + 26 > file_len {
        return Err(ineligible());
    }

    image.seek(SeekFrom::Start(offset))?;
    let mut pe = [0u8; 26];
    image.read_exact(&mut pe)?;
    let actual_machine = u16::from_le_bytes(pe[4..6].try_into().unwrap());
    let sections = u16::from_le_bytes(pe[6..8].try_into().unwrap());
    let optional_len = u16::from_le_bytes(pe[20..22].try_into().unwrap());
    let characteristics = u16::from_le_bytes(pe[22..24].try_into().unwrap());
    let magic = u16::from_le_bytes(pe[24..26].try_into().unwrap());
    if pe[..4] != *b"PE\0\0"
        || actual_machine != machine
        || !(1..=96).contains(&sections)
        || optional_len < 112
        || characteristics & 0x0002 == 0
        || characteristics & 0x2000 != 0
        || magic != 0x020b
        || offset + 24 + u64::from(optional_len) + u64::from(sections) * 40 > file_len
    {
        return Err(ineligible());
    }
    Ok(())
}

fn ineligible() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Unsupported browser executable image",
    )
}

#[cfg(windows)]
use platform::{fixed_drive, known_folder};
#[cfg(windows)]
use std::path::{Path, PathBuf};
#[cfg(windows)]
use windows_sys::Win32::UI::Shell::{
    FOLDERID_LocalAppData, FOLDERID_ProgramFiles, FOLDERID_ProgramFilesX64,
    FOLDERID_ProgramFilesX86,
};

#[cfg(windows)]
pub(super) fn local_app_data() -> io::Result<PathBuf> {
    known_folder(&FOLDERID_LocalAppData)
}

#[cfg(windows)]
pub(crate) fn installed() -> Option<PathBuf> {
    [
        &FOLDERID_ProgramFiles,
        &FOLDERID_ProgramFilesX64,
        &FOLDERID_ProgramFilesX86,
        &FOLDERID_LocalAppData,
    ]
    .into_iter()
    .filter_map(|folder| known_folder(folder).ok())
    .flat_map(|root| {
        [
            root.join("Google/Chrome/Application/chrome.exe"),
            root.join("Chromium/Application/chrome.exe"),
        ]
    })
    .find(|path| verified_install_image(path).is_ok())
}

#[cfg(windows)]
pub(super) fn verified_install_image(path: &Path) -> io::Result<()> {
    let machine = match std::env::consts::ARCH {
        "x86_64" => 0x8664,
        "aarch64" => 0xaa64,
        _ => return Err(ineligible()),
    };
    // Reject install files redirected away from the selected local directory.
    let image = path.canonicalize()?;
    let directory = path.parent().ok_or_else(ineligible)?.canonicalize()?;
    if !fixed_drive(&image) || image.parent() != Some(directory.as_path()) {
        return Err(ineligible());
    }
    verify(&mut std::fs::File::open(image)?, machine)
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::{
        ffi::OsString,
        os::windows::ffi::OsStringExt,
        path::{Component, Path, Prefix},
        ptr::null_mut,
    };
    use windows_sys::{
        Win32::{
            Storage::FileSystem::GetDriveTypeW,
            System::Com::CoTaskMemFree,
            UI::Shell::{KF_FLAG_DEFAULT, SHGetKnownFolderPath},
        },
        core::GUID,
    };

    pub(super) fn known_folder(id: &GUID) -> io::Result<PathBuf> {
        let mut raw = null_mut();
        let result =
            unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT as u32, null_mut(), &mut raw) };
        if result < 0 || raw.is_null() {
            unsafe { CoTaskMemFree(raw.cast()) };
            return Err(io::Error::other(
                "Required Windows known folder is unavailable",
            ));
        }
        let path = unsafe {
            let length = (0..).take_while(|&index| *raw.add(index) != 0).count();
            let path = PathBuf::from(OsString::from_wide(std::slice::from_raw_parts(raw, length)));
            CoTaskMemFree(raw.cast());
            path
        };
        if !fixed_drive(&path) || !fixed_drive(&path.canonicalize()?) {
            return Err(ineligible());
        }
        Ok(path)
    }

    pub(super) fn fixed_drive(path: &Path) -> bool {
        const DRIVE_FIXED: u32 = 3;
        if !path.is_absolute() {
            return false;
        }
        let Some(Component::Prefix(prefix)) = path.components().next() else {
            return false;
        };
        let drive = match prefix.kind() {
            Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => drive,
            _ => return false,
        };
        let root = [u16::from(drive), b':' as u16, b'\\' as u16, 0];
        unsafe { GetDriveTypeW(root.as_ptr()) == DRIVE_FIXED }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn pe(machine: u16) -> Vec<u8> {
        let mut image = vec![0; 512];
        image[..2].copy_from_slice(b"MZ");
        image[60..64].copy_from_slice(&64u32.to_le_bytes());
        image[64..68].copy_from_slice(b"PE\0\0");
        image[68..70].copy_from_slice(&machine.to_le_bytes());
        image[70..72].copy_from_slice(&1u16.to_le_bytes());
        image[84..86].copy_from_slice(&240u16.to_le_bytes());
        image[86..88].copy_from_slice(&2u16.to_le_bytes());
        image[88..90].copy_from_slice(&0x020bu16.to_le_bytes());
        image
    }

    #[test]
    fn accepts_correct_machine_and_rejects_wrong_machine() {
        for machine in [0x8664, 0xaa64] {
            assert!(verify(&mut Cursor::new(pe(machine)), machine).is_ok());
        }
        assert!(verify(&mut Cursor::new(pe(0xaa64)), 0x8664).is_err());
        assert!(verify(&mut Cursor::new(pe(0x014c)), 0x8664).is_err());
    }

    #[test]
    fn rejects_library_and_truncated_or_contradictory_headers() {
        let mut image = pe(0x8664);
        assert!(verify(&mut Cursor::new(&image[..88]), 0x8664).is_err());
        assert!(verify(&mut Cursor::new(&image[..200]), 0x8664).is_err());
        image[86..88].copy_from_slice(&0x2002u16.to_le_bytes());
        assert!(verify(&mut Cursor::new(&image), 0x8664).is_err());
        image[86..88].copy_from_slice(&2u16.to_le_bytes());
        image[88..90].copy_from_slice(&0x010bu16.to_le_bytes());
        assert!(verify(&mut Cursor::new(&image), 0x8664).is_err());
        image[60..64].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(verify(&mut Cursor::new(&image), 0x8664).is_err());
    }
}
