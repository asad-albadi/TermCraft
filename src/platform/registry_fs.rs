//! Bounded, typed native registry access. Values never enter workspace persistence or logs.
use std::sync::atomic::{AtomicBool, Ordering};
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RegistryPath {
    pub hive: Hive,
    pub key: String,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hive {
    CurrentUser,
    LocalMachine,
    ClassesRoot,
    Users,
    CurrentConfig,
}
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RegistryValue {
    pub kind: u32,
    pub bytes: Vec<u8>,
}
impl RegistryValue {
    pub fn string(value: &str) -> Self {
        Self {
            kind: 1,
            bytes: value
                .encode_utf16()
                .chain([0])
                .flat_map(u16::to_le_bytes)
                .collect(),
        }
    }
    pub fn parse_text(kind: u32, text: &str) -> Result<Self, String> {
        if text.len() > 393216 || text.contains('\0') {
            return Err("Value contains NUL or exceeds the editor limit".into());
        }
        let value = match kind {
            1 | 2 => {
                let mut value = Self::string(text);
                value.kind = kind;
                value
            }
            7 => {
                let parts: Vec<String> = serde_json::from_str(text)
                    .map_err(|_| "Enter a JSON array of strings, for example [\"one\",\"two\"]")?;
                if parts.iter().any(|s| s.is_empty() || s.contains('\0')) {
                    return Err("Multi-string entries must be nonempty and contain no NUL".into());
                }
                let mut units: Vec<u16> = parts
                    .iter()
                    .flat_map(|s| s.encode_utf16().chain([0]))
                    .chain([0])
                    .collect();
                if parts.is_empty() {
                    units.push(0);
                }
                Self {
                    kind,
                    bytes: units.into_iter().flat_map(u16::to_le_bytes).collect(),
                }
            }
            4 => Self {
                kind,
                bytes: text
                    .parse::<u32>()
                    .map_err(|_| "Enter an unsigned 32-bit decimal number")?
                    .to_le_bytes()
                    .to_vec(),
            },
            11 => Self {
                kind,
                bytes: text
                    .parse::<u64>()
                    .map_err(|_| "Enter an unsigned 64-bit decimal number")?
                    .to_le_bytes()
                    .to_vec(),
            },
            0 | 3 => Self {
                kind,
                bytes: text
                    .split_whitespace()
                    .map(|s| {
                        u8::from_str_radix(s, 16)
                            .map_err(|_| "Enter space-separated hexadecimal bytes")
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            },
            _ => return Err("Unknown registry types cannot be edited".into()),
        };
        if value.bytes.len() > 65536 {
            return Err("Value exceeds 64 KiB".into());
        }
        Ok(value)
    }
    /// Lossless editor representation. Malformed strings are displayed but never silently repaired.
    pub fn edit_text(&self) -> Result<String, String> {
        match self.kind {
            1 | 2 | 7 => {
                if self.bytes.len() % 2 != 0 {
                    return Err("Malformed UTF-16 value; inspect its bytes before editing".into());
                }
                let units: Vec<u16> = self
                    .bytes
                    .chunks_exact(2)
                    .map(|b| u16::from_le_bytes([b[0], b[1]]))
                    .collect();
                let text = String::from_utf16(&units)
                    .map_err(|_| "Malformed UTF-16 value cannot be edited as text")?;
                if self.kind == 7 {
                    let body = text
                        .strip_suffix("\0\0")
                        .ok_or("Multi-string value lacks its terminator")?;
                    let parts: Vec<&str> = if body.is_empty() {
                        vec![]
                    } else {
                        body.split('\0').collect()
                    };
                    if parts.iter().any(|s| s.is_empty()) {
                        return Err("Malformed empty multi-string entry".into());
                    }
                    serde_json::to_string(&parts)
                        .map_err(|_| "Cannot prepare multi-string editor".into())
                } else {
                    let body = text
                        .strip_suffix('\0')
                        .ok_or("String value lacks its terminator")?;
                    if body.contains('\0') {
                        return Err(
                            "String contains an embedded NUL; inspect bytes before editing".into(),
                        );
                    }
                    Ok(body.into())
                }
            }
            0 | 3 => Ok(self.display()),
            4 if self.bytes.len() == 4 => Ok(self.display()),
            11 if self.bytes.len() == 8 => Ok(self.display()),
            _ => Err("Unknown or malformed value type cannot be edited".into()),
        }
    }
    pub fn type_name(&self) -> &'static str {
        match self.kind {
            0 => "REG_NONE",
            1 => "REG_SZ",
            2 => "REG_EXPAND_SZ",
            3 => "REG_BINARY",
            4 => "REG_DWORD",
            7 => "REG_MULTI_SZ",
            11 => "REG_QWORD",
            _ => "Unknown type",
        }
    }
    pub fn display(&self) -> String {
        match self.kind {
            1 | 2 | 7 if self.bytes.len() % 2 == 0 => String::from_utf16_lossy(
                &self
                    .bytes
                    .chunks_exact(2)
                    .map(|b| u16::from_le_bytes([b[0], b[1]]))
                    .collect::<Vec<_>>(),
            )
            .trim_end_matches('\0')
            .replace('\0', " | "),
            4 if self.bytes.len() == 4 => {
                u32::from_le_bytes(self.bytes[..4].try_into().unwrap()).to_string()
            }
            11 if self.bytes.len() == 8 => {
                u64::from_le_bytes(self.bytes[..8].try_into().unwrap()).to_string()
            }
            _ => self.bytes.iter().map(|b| format!("{b:02x} ")).collect(),
        }
    }
}
#[derive(Clone, Debug)]
pub struct Listing {
    pub keys: Vec<String>,
    pub values: Vec<(String, RegistryValue)>,
}
#[derive(Clone)]
pub struct ConfirmedWrite {
    pub path: RegistryPath,
    pub name: String,
    pub expected: RegistryValue,
    pub replacement: RegistryValue,
}
impl RegistryPath {
    pub fn parse(input: &str) -> Result<Self, String> {
        let (hive, key) = input.split_once('\\').unwrap_or((input, ""));
        let hive = match hive {
            "HKCU" => Hive::CurrentUser,
            "HKLM" => Hive::LocalMachine,
            "HKCR" => Hive::ClassesRoot,
            "HKU" => Hive::Users,
            "HKCC" => Hive::CurrentConfig,
            _ => return Err("Use HKCU, HKLM, HKCR, HKU or HKCC".into()),
        };
        validate(key)?;
        Ok(Self {
            hive,
            key: key.into(),
        })
    }
}
fn validate(text: &str) -> Result<(), String> {
    if text.len() > 16384
        || text.chars().any(char::is_control)
        || text.split('\\').any(|s| s == ".." || s == ".")
    {
        Err("Invalid registry target".into())
    } else {
        Ok(())
    }
}
pub struct RegistryFs;
impl RegistryFs {
    pub fn enumerate(path: &RegistryPath, cancel: &AtomicBool) -> Result<Listing, String> {
        validate(&path.key)?;
        native::enumerate(path, cancel)
    }
    pub fn read(path: &RegistryPath, name: &str) -> Result<RegistryValue, String> {
        validate(name)?;
        native::read(path, name)
    }
    /// Caller must display the exact target, old value and replacement before constructing this request.
    pub fn write_confirmed(write: &ConfirmedWrite, cancel: &AtomicBool) -> Result<(), String> {
        validate(&write.path.key)?;
        validate(&write.name)?;
        if write.replacement.bytes.len() > 65536 {
            return Err("Value exceeds 64 KiB".into());
        }
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        native::write(write, cancel)
    }
}
#[cfg(not(windows))]
mod native {
    use super::*;
    pub fn enumerate(_: &RegistryPath, _: &AtomicBool) -> Result<Listing, String> {
        Err("Windows registry is unavailable on this platform".into())
    }
    pub fn read(_: &RegistryPath, _: &str) -> Result<RegistryValue, String> {
        Err("Windows registry is unavailable on this platform".into())
    }
    pub fn write(_: &ConfirmedWrite, _: &AtomicBool) -> Result<(), String> {
        Err("Windows registry is unavailable on this platform".into())
    }
}
#[cfg(windows)]
pub(crate) mod native {
    use super::*;
    use windows_sys::Win32::{
        Foundation::{ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, ERROR_SUCCESS},
        System::Registry::*,
    };
    pub fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain([0]).collect()
    }
    pub fn error(code: u32) -> String {
        format!(
            "Registry operation failed: {} (Windows error {code})",
            std::io::Error::from_raw_os_error(code as i32)
        )
    }
    pub struct Key(pub HKEY);
    impl Drop for Key {
        fn drop(&mut self) {
            unsafe {
                RegCloseKey(self.0);
            }
        }
    }
    pub fn root(hive: Hive) -> HKEY {
        match hive {
            Hive::CurrentUser => HKEY_CURRENT_USER,
            Hive::LocalMachine => HKEY_LOCAL_MACHINE,
            Hive::ClassesRoot => HKEY_CLASSES_ROOT,
            Hive::Users => HKEY_USERS,
            Hive::CurrentConfig => HKEY_CURRENT_CONFIG,
        }
    }
    pub fn open(path: &RegistryPath, access: u32) -> Result<Key, String> {
        let mut key = std::ptr::null_mut();
        let code = unsafe {
            RegOpenKeyExW(
                root(path.hive),
                wide(&path.key).as_ptr(),
                0,
                access,
                &mut key,
            )
        };
        if code != 0 {
            Err(error(code))
        } else {
            Ok(Key(key))
        }
    }
    pub fn read_key(key: HKEY, name: &str) -> Result<RegistryValue, String> {
        let mut kind = 0;
        let mut size = 0;
        let name = wide(name);
        let code = unsafe {
            RegQueryValueExW(
                key,
                name.as_ptr(),
                std::ptr::null(),
                &mut kind,
                std::ptr::null_mut(),
                &mut size,
            )
        };
        if code != 0 {
            return Err(error(code));
        }
        if size > 65536 {
            return Err("Value exceeds 64 KiB".into());
        }
        let mut bytes = vec![0; size as usize];
        let code = unsafe {
            RegQueryValueExW(
                key,
                name.as_ptr(),
                std::ptr::null(),
                &mut kind,
                bytes.as_mut_ptr(),
                &mut size,
            )
        };
        if code != 0 {
            return Err(error(code));
        }
        bytes.truncate(size as usize);
        Ok(RegistryValue { kind, bytes })
    }
    pub fn read(path: &RegistryPath, name: &str) -> Result<RegistryValue, String> {
        read_key(open(path, KEY_READ)?.0, name)
    }
    pub fn enumerate(path: &RegistryPath, cancel: &AtomicBool) -> Result<Listing, String> {
        let key = open(path, KEY_READ)?;
        let mut result = Listing {
            keys: vec![],
            values: vec![],
        };
        let mut total = 0usize;
        for index in 0..=2048 {
            if cancel.load(Ordering::Relaxed) {
                return Err("Cancelled".into());
            }
            let mut buf = vec![0u16; 16385];
            let mut len = 16384;
            let code = unsafe {
                RegEnumKeyExW(
                    key.0,
                    index,
                    buf.as_mut_ptr(),
                    &mut len,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };
            if code == ERROR_NO_MORE_ITEMS {
                break;
            }
            if code != ERROR_SUCCESS {
                return Err(error(code));
            }
            if index == 2048 {
                return Err("Key exceeds the 2048-child browsing limit".into());
            }
            result
                .keys
                .push(String::from_utf16(&buf[..len as usize]).map_err(|_| "Invalid Unicode key")?);
        }
        for index in 0..=2048 {
            if cancel.load(Ordering::Relaxed) {
                return Err("Cancelled".into());
            }
            let mut buf = vec![0u16; 16385];
            let mut len = 16384;
            let code = unsafe {
                RegEnumValueW(
                    key.0,
                    index,
                    buf.as_mut_ptr(),
                    &mut len,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };
            if code == ERROR_NO_MORE_ITEMS {
                break;
            }
            if code != 0 && code != ERROR_MORE_DATA {
                return Err(error(code));
            }
            if index == 2048 {
                return Err("Key exceeds the 2048-value browsing limit".into());
            }
            let name =
                String::from_utf16(&buf[..len as usize]).map_err(|_| "Invalid Unicode name")?;
            let value = read_key(key.0, &name)?;
            total += value.bytes.len();
            if total > 1048576 {
                return Err("Key exceeds the 1 MiB browsing limit".into());
            }
            result.values.push((name, value));
        }
        Ok(result)
    }
    pub fn set(key: HKEY, name: &str, value: &RegistryValue) -> Result<(), String> {
        let code = unsafe {
            RegSetValueExW(
                key,
                wide(name).as_ptr(),
                0,
                value.kind,
                value.bytes.as_ptr(),
                value.bytes.len() as u32,
            )
        };
        if code == 0 { Ok(()) } else { Err(error(code)) }
    }
    pub fn write(write: &ConfirmedWrite, cancel: &AtomicBool) -> Result<(), String> {
        let key = open(&write.path, KEY_QUERY_VALUE | KEY_SET_VALUE)?;
        if read_key(key.0, &write.name)? != write.expected {
            return Err("Value changed since preview; read again before confirming".into());
        }
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        set(key.0, &write.name, &write.replacement)
    }
}
