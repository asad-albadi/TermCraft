use super::Secret;
use argon2::{Algorithm, Argon2, Block, Params, Version};
use chacha20poly1305::{
    KeyInit, XChaCha20Poly1305, XNonce,
    aead::{Aead, Payload},
};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;
pub(super) const MAX_FILE: usize = 4 * 1024 * 1024;
const MAGIC: &[u8; 8] = b"TCVAULT1";
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Header {
    version: u8,
    memory_kib: u32,
    passes: u32,
    lanes: u32,
    salt: [u8; 32],
    nonce: [u8; 24],
}
fn random<const N: usize>() -> Result<[u8; N], String> {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes).map_err(|_| "Secure randomness is unavailable.")?;
    Ok(bytes)
}
impl Header {
    pub(super) fn new() -> Result<Self, String> {
        Ok(Self {
            version: 1,
            memory_kib: 65536,
            passes: 3,
            lanes: 4,
            salt: random()?,
            nonce: random()?,
        })
    }
    fn validate(&self) -> Result<(), String> {
        if self.version != 1 || self.memory_kib != 65536 || self.passes != 3 || self.lanes != 4 {
            Err("Unsupported vault format or KDF parameters.".into())
        } else {
            Ok(())
        }
    }
    pub(super) fn key(&self, master: &Secret) -> Result<Zeroizing<[u8; 32]>, String> {
        self.validate()?;
        if master.expose().is_empty() || master.expose().len() > 1024 {
            return Err("Master password must contain 1–1024 bytes.".into());
        }
        let params = Params::new(self.memory_kib, self.passes, self.lanes, Some(32))
            .map_err(|_| "Invalid vault KDF parameters")?;
        // The allocation helper inside hash_password_into does not wipe its memory.
        // Supply owned zeroizing blocks so both scratch memory and output are cleared.
        let mut blocks = Zeroizing::new(vec![Block::default(); params.block_count()]);
        let mut key = Zeroizing::new([0; 32]);
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password_into_with_memory(
                master.expose().as_bytes(),
                &self.salt,
                &mut key[..],
                &mut blocks[..],
            )
            .map_err(|_| "Vault key derivation failed")?;
        Ok(key)
    }
    pub(super) fn encrypt(&self, key: &[u8; 32], plain: &[u8]) -> Result<(Self, Vec<u8>), String> {
        if plain.len() > MAX_FILE - 4096 {
            return Err("Vault is too large.".into());
        }
        let mut header = self.clone();
        header.nonce = random()?;
        let encoded = serde_json::to_vec(&header).map_err(|_| "Could not encode vault header")?;
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&(encoded.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&encoded);
        let cipher = XChaCha20Poly1305::new_from_slice(key).map_err(|_| "Invalid vault key")?;
        let nonce: XNonce = header.nonce.into();
        let encrypted = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: plain,
                    aad: &bytes,
                },
            )
            .map_err(|_| "Vault encryption failed")?;
        bytes.extend(encrypted);
        Ok((header, bytes))
    }
}
pub(super) fn decrypt(
    bytes: &[u8],
    master: &Secret,
) -> Result<(Header, Zeroizing<[u8; 32]>, Zeroizing<Vec<u8>>), String> {
    if bytes.len() < 28 || bytes.len() > MAX_FILE || &bytes[..8] != MAGIC {
        return Err("Invalid or oversized vault file.".into());
    }
    let len = u32::from_be_bytes(bytes[8..12].try_into().unwrap()) as usize;
    if len > 2048 || 12 + len + 16 > bytes.len() {
        return Err("Invalid vault header size.".into());
    }
    let header: Header =
        serde_json::from_slice(&bytes[12..12 + len]).map_err(|_| "Invalid vault header")?;
    header.validate()?;
    let key = header.key(master)?;
    let cipher = XChaCha20Poly1305::new_from_slice(&key[..]).map_err(|_| "Invalid vault key")?;
    let nonce: XNonce = header.nonce.into();
    let plain = Zeroizing::new(
        cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: &bytes[12 + len..],
                    aad: &bytes[..12 + len],
                },
            )
            .map_err(|_| "Could not unlock vault: wrong password or damaged file.")?,
    );
    Ok((header, key, plain))
}
