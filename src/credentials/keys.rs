//! Bounded key inspection and atomic publication of new key-pair bundles.
use super::Secret;
use russh::keys::ssh_key::{
    self, Algorithm, EcdsaCurve, HashAlg, Kdf, LineEnding, PrivateKey,
    rand_core::{TryCryptoRng, TryRng, UnwrapErr},
};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
use zeroize::Zeroizing;
const MAX_KEY: usize = 1024 * 1024;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyAlgorithm {
    #[default]
    Ed25519,
    EcdsaP256,
    EcdsaP384,
    EcdsaP521,
    Rsa3072,
    Rsa4096,
}
#[derive(Default)]
pub struct KeySettings {
    pub algorithm: KeyAlgorithm,
    pub comment: String,
    pub passphrase: Option<Secret>,
}
#[derive(Clone, Debug)]
pub struct KeyInfo {
    pub private: bool,
    pub fingerprint: String,
    pub algorithm: String,
    pub comment: String,
    pub encrypted: bool,
    pub public_key: String,
}
#[derive(Clone, Debug)]
pub struct KeyPairPaths {
    pub private: PathBuf,
    pub public: PathBuf,
    pub fingerprint: String,
}
pub struct KeyManager;
struct OsRandom;
impl TryRng for OsRandom {
    type Error = std::io::Error;
    fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
        let mut b = [0; 4];
        getrandom::fill(&mut b)
            .map_err(|_| std::io::Error::other("OS randomness is unavailable"))?;
        Ok(u32::from_le_bytes(b))
    }
    fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
        let mut b = [0; 8];
        getrandom::fill(&mut b)
            .map_err(|_| std::io::Error::other("OS randomness is unavailable"))?;
        Ok(u64::from_le_bytes(b))
    }
    fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), Self::Error> {
        getrandom::fill(dst).map_err(|_| std::io::Error::other("OS randomness is unavailable"))
    }
}
impl TryCryptoRng for OsRandom {}
fn canceled(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) {
        Err("Key operation canceled before publication.".into())
    } else {
        Ok(())
    }
}
fn text(path: &Path) -> Result<Zeroizing<String>, String> {
    let metadata = fs::metadata(path).map_err(|_| "Key file is unavailable")?;
    if !metadata.is_file() || metadata.len() > MAX_KEY as u64 {
        return Err("Choose a regular key file up to 1 MiB.".into());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY);
    }
    let file = options.open(path).map_err(|_| "Could not open key file")?;
    if !file
        .metadata()
        .map_err(|_| "Could not inspect key")?
        .is_file()
    {
        return Err("Key is not a regular file.".into());
    }
    let mut bytes = Zeroizing::new(Vec::new());
    file.take((MAX_KEY + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read key file")?;
    if bytes.len() > MAX_KEY {
        return Err("Key file exceeds 1 MiB.".into());
    }
    let value =
        std::str::from_utf8(&bytes).map_err(|_| "Key file must be UTF-8/OpenSSH/PEM/PPK text")?;
    Ok(Zeroizing::new(value.to_owned()))
}
fn policy(value: &str) -> Result<(), String> {
    if value
        .trim_start()
        .starts_with("-----BEGIN ENCRYPTED PRIVATE KEY-----")
    {
        return Err("Convert encrypted PKCS#8 keys to OpenSSH before import/use; this path does not accept an unbounded legacy KDF.".into());
    }
    if value.trim_start().starts_with("PuTTY-User-Key-File-") {
        let mut memory = 0u64;
        let mut passes = 0u64;
        for line in value.lines() {
            if let Some((name, v)) = line.split_once(':') {
                if matches!(
                    name,
                    "Argon2-Memory" | "Argon2-Passes" | "Argon2-Parallelism"
                ) {
                    let n = v
                        .trim()
                        .parse::<u64>()
                        .map_err(|_| "Invalid PPK KDF parameters")?;
                    match name {
                        "Argon2-Memory" => memory = n,
                        "Argon2-Passes" => passes = n,
                        _ => {
                            if n == 0 || n > 16 {
                                return Err("PPK KDF parallelism exceeds supported limits.".into());
                            }
                        }
                    }
                }
            }
        }
        if memory > 262144 || passes > 256 || memory.saturating_mul(passes) > 2097152 {
            return Err("PPK KDF cost exceeds supported import limits.".into());
        }
    }
    Ok(())
}
fn parse(value: &str, passphrase: Option<&Secret>, decrypt: bool) -> Result<PrivateKey, String> {
    if passphrase.is_some_and(|p| p.expose().len() > 65536) {
        return Err("Key passphrase exceeds 64 KiB.".into());
    }
    policy(value)?;
    if value
        .trim_start()
        .starts_with("-----BEGIN OPENSSH PRIVATE KEY-----")
    {
        let key = PrivateKey::from_openssh(value).map_err(|_| "Invalid OpenSSH private key")?;
        match key.kdf() {
            Kdf::None => {}
            Kdf::Bcrypt { rounds, .. } if (1..=256).contains(rounds) => {}
            _ => return Err("OpenSSH key KDF cost exceeds supported limits.".into()),
        }
        if key.is_encrypted() && (decrypt || passphrase.is_some()) {
            return key
                .decrypt(passphrase.ok_or("A key passphrase is required")?.expose())
                .map_err(|_| "Could not unlock private key".to_owned());
        }
        return Ok(key);
    }
    russh::keys::decode_secret_key(value, passphrase.map(Secret::expose))
        .map_err(|_| "Could not decode key; check its format and passphrase.".into())
}
fn comment(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .take(1024)
        .collect()
}
fn info(key: &PrivateKey, encrypted: bool) -> Result<KeyInfo, String> {
    let mut public = key.public_key().clone();
    public.set_comment(comment(key.comment().as_str_lossy()));
    Ok(KeyInfo {
        private: true,
        fingerprint: public.fingerprint(HashAlg::Sha256).to_string(),
        algorithm: public.algorithm().to_string(),
        comment: comment(key.comment().as_str_lossy()),
        encrypted,
        public_key: public
            .to_openssh()
            .map_err(|_| "Could not encode public key")?,
    })
}
impl KeyManager {
    pub fn inspect(path: &Path, passphrase: Option<&Secret>) -> Result<KeyInfo, String> {
        let value = text(path)?;
        if !value.trim_start().starts_with("-----BEGIN")
            && !value.trim_start().starts_with("PuTTY-User-Key-File-")
        {
            let mut public = ssh_key::PublicKey::from_openssh(value.trim())
                .map_err(|_| "Unsupported public/private key format")?;
            public.set_comment(comment(public.comment().as_str_lossy()));
            return Ok(KeyInfo {
                private: false,
                fingerprint: public.fingerprint(HashAlg::Sha256).to_string(),
                algorithm: public.algorithm().to_string(),
                comment: comment(public.comment().as_str_lossy()),
                encrypted: false,
                public_key: public.to_openssh().map_err(|_| "Invalid public key")?,
            });
        }
        let encrypted = is_encrypted(&value);
        let key = parse(&value, passphrase, false)?;
        info(&key, encrypted)
    }
    pub fn load_private(path: &Path, passphrase: Option<&Secret>) -> Result<PrivateKey, String> {
        parse(&text(path)?, passphrase, true)
    }
    pub fn generate(destination: &Path, settings: KeySettings) -> Result<KeyPairPaths, String> {
        Self::generate_cancelable(destination, settings, &AtomicBool::new(false))
    }
    pub fn generate_cancelable(
        destination: &Path,
        settings: KeySettings,
        cancel: &AtomicBool,
    ) -> Result<KeyPairPaths, String> {
        if destination.exists() {
            return Err("Destination exists; choose a new key-bundle folder.".into());
        }
        canceled(cancel)?;
        if settings
            .passphrase
            .as_ref()
            .is_some_and(|p| p.expose().len() > 65536)
        {
            return Err("Key passphrase exceeds 64 KiB.".into());
        }
        if settings.comment.len() > 4096 || settings.comment.chars().any(char::is_control) {
            return Err("Key comments must be bounded text without control characters.".into());
        }
        let mut key = std::panic::catch_unwind(|| {
            let mut rng = UnwrapErr(OsRandom);
            match settings.algorithm {
                KeyAlgorithm::Rsa3072 | KeyAlgorithm::Rsa4096 => {
                    let bits = if settings.algorithm == KeyAlgorithm::Rsa3072 {
                        3072
                    } else {
                        4096
                    };
                    let pair = ssh_key::private::RsaKeypair::random(&mut rng, bits)?;
                    PrivateKey::new(pair.into(), settings.comment.clone())
                }
                algorithm => {
                    let algorithm = match algorithm {
                        KeyAlgorithm::Ed25519 => Algorithm::Ed25519,
                        KeyAlgorithm::EcdsaP256 => Algorithm::Ecdsa {
                            curve: EcdsaCurve::NistP256,
                        },
                        KeyAlgorithm::EcdsaP384 => Algorithm::Ecdsa {
                            curve: EcdsaCurve::NistP384,
                        },
                        KeyAlgorithm::EcdsaP521 => Algorithm::Ecdsa {
                            curve: EcdsaCurve::NistP521,
                        },
                        _ => unreachable!(),
                    };
                    PrivateKey::random(&mut rng, algorithm)
                }
            }
        })
        .map_err(|_| "Secure key generation failed")?
        .map_err(|_| "Key generation failed")?;
        key.set_comment(settings.comment);
        canceled(cancel)?;
        if let Some(passphrase) = settings.passphrase {
            if !passphrase.expose().is_empty() {
                key = key
                    .encrypt(&mut OsRandom, passphrase.expose())
                    .map_err(|_| "Private-key encryption failed")?
            }
        }
        let metadata = info(&key, key.is_encrypted())?;
        let encoded = key
            .to_openssh(LineEnding::LF)
            .map_err(|_| "Could not encode private key")?;
        publish(destination, encoded.as_bytes(), metadata, cancel)
    }
    pub fn import(
        source: &Path,
        destination: &Path,
        passphrase: Option<Secret>,
        cancel: &AtomicBool,
    ) -> Result<KeyPairPaths, String> {
        if destination.exists() {
            return Err("Destination exists; choose a new key-bundle folder.".into());
        }
        let value = text(source)?;
        canceled(cancel)?;
        let key = parse(&value, passphrase.as_ref(), false)?;
        let source_openssh = value
            .trim_start()
            .starts_with("-----BEGIN OPENSSH PRIVATE KEY-----");
        if source_openssh {
            return publish(
                destination,
                value.as_bytes(),
                info(&key, is_encrypted(&value))?,
                cancel,
            );
        }
        let key = if let Some(passphrase) = passphrase {
            if !passphrase.expose().is_empty() {
                key.encrypt(&mut OsRandom, passphrase.expose())
                    .map_err(|_| "Could not protect converted key")?
            } else {
                key
            }
        } else {
            key
        };
        let metadata = info(&key, key.is_encrypted())?;
        let encoded = key
            .to_openssh(LineEnding::LF)
            .map_err(|_| "Could not convert key to OpenSSH")?;
        publish(destination, encoded.as_bytes(), metadata, cancel)
    }
}
fn is_encrypted(value: &str) -> bool {
    if value
        .trim_start()
        .starts_with("-----BEGIN OPENSSH PRIVATE KEY-----")
    {
        PrivateKey::from_openssh(value)
            .map(|k| k.is_encrypted())
            .unwrap_or(false)
    } else {
        value.contains("ENCRYPTED")
            || value
                .lines()
                .any(|l| l.starts_with("Encryption:") && !l.trim_end().ends_with("none"))
    }
}
fn publish(
    destination: &Path,
    private: &[u8],
    metadata: KeyInfo,
    cancel: &AtomicBool,
) -> Result<KeyPairPaths, String> {
    canceled(cancel)?;
    let parent = destination
        .parent()
        .filter(|p| p.is_dir())
        .ok_or("Choose a key-bundle folder inside an existing directory.")?;
    let stage = tempfile::Builder::new()
        .prefix(".termcraft-key-")
        .tempdir_in(parent)
        .map_err(|_| "Could not stage key bundle")?;
    for (name, bytes) in [
        ("private_key", private),
        ("public_key.pub", metadata.public_key.as_bytes()),
    ] {
        canceled(cancel)?;
        let mut file = crate::platform::private_file::create_new(&stage.path().join(name))
            .map_err(|_| "Could not create private key-bundle file")?;
        file.write_all(bytes)
            .map_err(|_| "Could not write key-bundle file")?;
        file.sync_all()
            .map_err(|_| "Could not synchronize key-bundle file")?;
    }
    canceled(cancel)?;
    crate::platform::publish_no_replace(stage.path(),destination).map_err(|_|"Key bundle was not published; destination exists or filesystem does not support no-replace publication.")?;
    let _ = stage.keep();
    #[cfg(unix)]
    if let Ok(dir) = File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(KeyPairPaths {
        private: destination.join("private_key"),
        public: destination.join("public_key.pub"),
        fingerprint: metadata.fingerprint,
    })
}
