use super::{
    CredentialBinding, CredentialId, Secret,
    format::{self, Header},
};
use crate::platform::private_file;
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use zeroize::Zeroizing;
#[derive(Clone)]
struct Entry {
    secret: Secret,
    binding: Option<CredentialBinding>,
}
#[derive(Serialize)]
struct OutputEntry<'a> {
    secret: &'a str,
    binding: &'a Option<CredentialBinding>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InputEntry {
    #[serde(deserialize_with = "secret")]
    secret: Secret,
    binding: Option<CredentialBinding>,
}
fn secret<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Secret, D::Error> {
    String::deserialize(deserializer).map(Secret::new)
}
#[derive(Clone, Debug)]
pub struct CredentialInfo {
    pub id: CredentialId,
    pub binding: Option<CredentialBinding>,
}
pub struct Vault {
    path: PathBuf,
    header: Header,
    key: Option<Zeroizing<[u8; 32]>>,
    entries: BTreeMap<CredentialId, Entry>,
    revision: [u8; 32],
}
fn mutex(path: &Path) -> Result<File, String> {
    let file = private_file::open(&path.with_extension("vault-lock"), true)?;
    file.try_lock()
        .map_err(|_| "Vault is busy in another operation; retry shortly.")?;
    Ok(file)
}
fn read(path: &Path) -> Result<Vec<u8>, String> {
    let file = private_file::open(path, false)?;
    let mut bytes = Vec::new();
    file.take((format::MAX_FILE + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read vault")?;
    if bytes.len() > format::MAX_FILE {
        return Err("Vault file is too large.".into());
    }
    Ok(bytes)
}
fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
fn write(path: &Path, bytes: &[u8], replace: bool) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|p| p.is_dir())
        .ok_or("Vault directory is unavailable.")?;
    let mut stage = tempfile::Builder::new()
        .prefix(".vault-")
        .make_in(parent, private_file::create_new)
        .map_err(|_| "Could not stage encrypted vault")?;
    stage
        .write_all(bytes)
        .map_err(|_| "Could not write encrypted vault")?;
    stage
        .as_file()
        .sync_all()
        .map_err(|_| "Could not synchronize encrypted vault")?;
    if replace {
        stage
            .persist(path)
            .map_err(|_| "Could not replace encrypted vault")?;
    } else {
        stage
            .persist_noclobber(path)
            .map_err(|_| "Vault already exists or could not be created")?;
    }
    // Commit has happened: failure to sync the directory cannot roll back bytes.
    // File data was flushed before rename; native power-loss behavior remains an evidence gap.
    #[cfg(unix)]
    if let Ok(dir) = File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(())
}
impl Vault {
    pub fn create(path: &Path, master: Secret) -> Result<Self, String> {
        let _lock = mutex(path)?;
        if path.exists() {
            return Err("Vault already exists; unlock it instead.".into());
        }
        let header = Header::new()?;
        let key = header.key(&master)?;
        drop(master);
        let entries = BTreeMap::new();
        let plain = serialize(&entries)?;
        let (header, bytes) = header.encrypt(&key, &plain)?;
        write(path, &bytes, false)?;
        Ok(Self {
            path: path.to_owned(),
            header,
            key: Some(key),
            entries,
            revision: digest(&bytes),
        })
    }
    pub fn unlock(path: &Path, master: Secret) -> Result<Self, String> {
        let _lock = mutex(path)?;
        let bytes = read(path)?;
        let (header, key, plain) = format::decrypt(&bytes, &master)?;
        drop(master);
        let input: BTreeMap<CredentialId, InputEntry> =
            serde_json::from_slice(&plain).map_err(|_| "Invalid encrypted vault contents")?;
        if input.len() > 1024 {
            return Err("Too many vault entries.".into());
        }
        let mut entries = BTreeMap::new();
        for (id, entry) in input {
            validate(&id, &entry.secret)?;
            if let Some(binding) = &entry.binding {
                binding.validate()?
            }
            entries.insert(
                id,
                Entry {
                    secret: entry.secret,
                    binding: entry.binding,
                },
            );
        }
        Ok(Self {
            path: path.to_owned(),
            header,
            key: Some(key),
            entries,
            revision: digest(&bytes),
        })
    }
    pub fn is_unlocked(&self) -> bool {
        self.key.is_some()
    }
    pub fn lock(&mut self) {
        self.entries.clear();
        self.key = None;
    }
    pub fn list(&self) -> Result<Vec<CredentialInfo>, String> {
        self.key.as_ref().ok_or("Vault is locked")?;
        Ok(self
            .entries
            .iter()
            .map(|(id, entry)| CredentialInfo {
                id: id.clone(),
                binding: entry.binding.clone(),
            })
            .collect())
    }
    pub fn put(&mut self, id: CredentialId, secret: Secret) -> Result<(), String> {
        self.put_entry(id, secret, None)
    }
    pub fn put_bound(
        &mut self,
        id: CredentialId,
        secret: Secret,
        binding: CredentialBinding,
    ) -> Result<(), String> {
        binding.validate()?;
        self.put_entry(id, secret, Some(binding))
    }
    fn put_entry(
        &mut self,
        id: CredentialId,
        secret: Secret,
        binding: Option<CredentialBinding>,
    ) -> Result<(), String> {
        validate(&id, &secret)?;
        let mut entries = self.entries.clone();
        entries.insert(id, Entry { secret, binding });
        if entries.len() > 1024 {
            return Err("Too many vault entries.".into());
        }
        self.commit(entries, None)
    }
    pub fn get(&self, id: &CredentialId) -> Result<Secret, String> {
        self.key.as_ref().ok_or("Vault is locked")?;
        self.entries
            .get(id)
            .map(|e| e.secret.clone())
            .ok_or("Credential not found".into())
    }
    pub fn get_bound(
        &self,
        id: &CredentialId,
        binding: &CredentialBinding,
    ) -> Result<Secret, String> {
        self.key.as_ref().ok_or("Vault is locked")?;
        let entry = self.entries.get(id).ok_or("Credential not found")?;
        if entry.binding.as_ref() != Some(binding) {
            return Err("Credential belongs to a different endpoint, profile, key or authentication method.".into());
        }
        Ok(entry.secret.clone())
    }
    pub fn delete(&mut self, id: &CredentialId) -> Result<(), String> {
        self.key.as_ref().ok_or("Vault is locked")?;
        let mut entries = self.entries.clone();
        if entries.remove(id).is_none() {
            return Err("Credential not found.".into());
        }
        self.commit(entries, None)
    }
    pub fn change_master(&mut self, master: Secret) -> Result<(), String> {
        self.key.as_ref().ok_or("Vault is locked")?;
        let header = Header::new()?;
        let key = header.key(&master)?;
        drop(master);
        self.commit(self.entries.clone(), Some((header, key)))
    }
    fn commit(
        &mut self,
        entries: BTreeMap<CredentialId, Entry>,
        replacement: Option<(Header, Zeroizing<[u8; 32]>)>,
    ) -> Result<(), String> {
        let existing = self.key.as_ref().ok_or("Vault is locked")?;
        let _lock = mutex(&self.path)?;
        if digest(&read(&self.path)?) != self.revision {
            return Err("Vault changed on disk; lock and unlock it again before saving.".into());
        }
        let (header, key) = match &replacement {
            Some((h, k)) => (h, k),
            None => (&self.header, existing),
        };
        let plain = serialize(&entries)?;
        let (header, bytes) = header.encrypt(key, &plain)?;
        write(&self.path, &bytes, true)?;
        self.entries = entries;
        self.header = header;
        self.revision = digest(&bytes);
        if let Some((_, key)) = replacement {
            self.key = Some(key)
        }
        Ok(())
    }
}
fn validate(id: &str, secret: &Secret) -> Result<(), String> {
    if !super::valid_id(id) || secret.expose().len() > 65536 {
        return Err("Credential ID or value exceeds its size limit.".into());
    }
    Ok(())
}
fn serialize(entries: &BTreeMap<CredentialId, Entry>) -> Result<Zeroizing<Vec<u8>>, String> {
    let borrowed = entries
        .iter()
        .map(|(id, entry)| {
            (
                id,
                OutputEntry {
                    secret: entry.secret.expose(),
                    binding: &entry.binding,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let plain = Zeroizing::new(
        serde_json::to_vec(&borrowed).map_err(|_| "Could not encode vault contents")?,
    );
    if plain.len() > format::MAX_FILE - 4096 {
        return Err("Vault contents are too large.".into());
    }
    Ok(plain)
}
