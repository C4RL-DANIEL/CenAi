//! Portable workspace backup envelope using Argon2id and AES-256-GCM.
use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

const FORMAT_VERSION: u16 = 1;
const KDF_ID: &str = "argon2id-v1.3-m65536-t3-p1";
const AAD: &[u8] = b"CenAI::workspace-backup::v1";
const SALT_LENGTH: usize = 16;
const NONCE_LENGTH: usize = 12;
const MAX_PLAINTEXT_BYTES: usize = 70 * 1024 * 1024;
const MAX_CIPHERTEXT_BYTES: usize = MAX_PLAINTEXT_BYTES + 16;
const MAX_ENCODED_CIPHERTEXT_BYTES: usize = MAX_CIPHERTEXT_BYTES.div_ceil(3) * 4;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EncryptedBackup {
    pub format_version: u16,
    pub kdf: String,
    pub salt_base64: String,
    pub nonce_base64: String,
    pub ciphertext_base64: String,
}

impl EncryptedBackup {
    pub fn seal(plaintext: &[u8], passphrase: &str) -> Result<Self, String> {
        validate_passphrase(passphrase)?;
        if plaintext.is_empty() || plaintext.len() > MAX_PLAINTEXT_BYTES {
            return Err("Backup payload must be non-empty and no larger than 70 MB".into());
        }

        let mut salt = [0u8; SALT_LENGTH];
        let mut nonce_bytes = [0u8; NONCE_LENGTH];
        OsRng.fill_bytes(&mut salt);
        OsRng.fill_bytes(&mut nonce_bytes);

        let key = derive_key(passphrase, &salt)?;
        let cipher = Aes256Gcm::new_from_slice(&key[..]).map_err(|_| "Backup encryption could not initialize".to_owned())?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher.encrypt(nonce, Payload { msg: plaintext, aad: AAD })
            .map_err(|_| "Backup encryption failed".to_owned())?;

        Ok(Self {
            format_version: FORMAT_VERSION,
            kdf: KDF_ID.into(),
            salt_base64: STANDARD.encode(salt),
            nonce_base64: STANDARD.encode(nonce_bytes),
            ciphertext_base64: STANDARD.encode(ciphertext),
        })
    }

    pub fn open(&self, passphrase: &str) -> Result<Vec<u8>, String> {
        validate_passphrase(passphrase)?;
        if self.format_version != FORMAT_VERSION || self.kdf != KDF_ID {
            return Err("Backup format or key-derivation parameters are unsupported".into());
        }
        if self.salt_base64.len() > 64 || self.nonce_base64.len() > 64
            || self.ciphertext_base64.len() > MAX_ENCODED_CIPHERTEXT_BYTES {
            return Err("Backup envelope contains oversized encoded fields".into());
        }
        let salt = STANDARD.decode(&self.salt_base64).map_err(|_| "Backup salt is malformed".to_owned())?;
        let nonce = STANDARD.decode(&self.nonce_base64).map_err(|_| "Backup nonce is malformed".to_owned())?;
        let ciphertext = STANDARD.decode(&self.ciphertext_base64).map_err(|_| "Backup ciphertext is malformed".to_owned())?;
        if salt.len() != SALT_LENGTH || nonce.len() != NONCE_LENGTH
            || ciphertext.len() < 16 || ciphertext.len() > MAX_CIPHERTEXT_BYTES {
            return Err("Backup envelope contains invalid cryptographic lengths".into());
        }

        let key = derive_key(passphrase, &salt)?;
        let cipher = Aes256Gcm::new_from_slice(&key[..]).map_err(|_| "Backup decryption could not initialize".to_owned())?;
        cipher.decrypt(Nonce::from_slice(&nonce), Payload { msg: &ciphertext, aad: AAD })
            .map_err(|_| "Backup could not be authenticated; the passphrase may be wrong or the file was modified".into())
    }

    pub fn to_json(&self) -> Result<Vec<u8>, String> {
        self.validate_envelope()?;
        serde_json::to_vec(self).map_err(|error| error.to_string())
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self, String> {
        if bytes.is_empty() || bytes.len() > MAX_ENCODED_CIPHERTEXT_BYTES + 4096 {
            return Err("Backup envelope file is empty or exceeds the supported size".into());
        }
        let backup: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        backup.validate_envelope()?;
        Ok(backup)
    }

    fn validate_envelope(&self) -> Result<(), String> {
        if self.format_version != FORMAT_VERSION || self.kdf != KDF_ID {
            return Err("Backup format or key-derivation parameters are unsupported".into());
        }
        if self.salt_base64.len() > 64 || self.nonce_base64.len() > 64
            || self.ciphertext_base64.len() > MAX_ENCODED_CIPHERTEXT_BYTES {
            return Err("Backup envelope contains oversized encoded fields".into());
        }
        let salt = STANDARD.decode(&self.salt_base64).map_err(|_| "Backup salt is malformed".to_owned())?;
        let nonce = STANDARD.decode(&self.nonce_base64).map_err(|_| "Backup nonce is malformed".to_owned())?;
        let ciphertext = STANDARD.decode(&self.ciphertext_base64).map_err(|_| "Backup ciphertext is malformed".to_owned())?;
        if salt.len() != SALT_LENGTH || nonce.len() != NONCE_LENGTH
            || ciphertext.len() < 16 || ciphertext.len() > MAX_CIPHERTEXT_BYTES {
            return Err("Backup envelope contains invalid cryptographic lengths".into());
        }
        Ok(())
    }
}

fn validate_passphrase(passphrase: &str) -> Result<(), String> {
    if passphrase.len() < 12 || passphrase.len() > 1024 || passphrase.chars().any(char::is_control) {
        return Err("Backup passphrase must be 12–1024 UTF-8 bytes and contain no control characters".into());
    }
    Ok(())
}

fn derive_key(passphrase: &str, salt: &[u8]) -> Result<Zeroizing<[u8; 32]>, String> {
    let params = Params::new(65_536, 3, 1, Some(32)).map_err(|error| error.to_string())?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = Zeroizing::new([0u8; 32]);
    argon2.hash_password_into(passphrase.as_bytes(), salt, &mut key[..])
        .map_err(|error| error.to_string())?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PASSPHRASE: &str = "correct horse battery staple 2026";

    #[test]
    fn encrypted_backup_round_trips_and_rejects_wrong_passphrases() {
        let plaintext = br#"{"schema":1,"private":"workspace data"}"#;
        let backup = EncryptedBackup::seal(plaintext, PASSPHRASE).unwrap();
        assert_eq!(backup.open(PASSPHRASE).unwrap(), plaintext);
        assert!(backup.open("incorrect horse battery staple").is_err());
        let json = backup.to_json().unwrap();
        let restored = EncryptedBackup::from_json(&json).unwrap();
        assert_eq!(restored.open(PASSPHRASE).unwrap(), plaintext);
    }

    #[test]
    fn encrypted_backup_authenticates_ciphertext_and_format_metadata() {
        let mut backup = EncryptedBackup::seal(b"private", PASSPHRASE).unwrap();
        let mut ciphertext = STANDARD.decode(&backup.ciphertext_base64).unwrap();
        ciphertext[0] ^= 0x80;
        backup.ciphertext_base64 = STANDARD.encode(ciphertext);
        assert!(backup.open(PASSPHRASE).is_err());

        let mut unsupported = EncryptedBackup::seal(b"private", PASSPHRASE).unwrap();
        unsupported.format_version = 99;
        assert!(unsupported.open(PASSPHRASE).is_err());
    }

    #[test]
    fn encrypted_backup_rejects_oversized_base64_before_decoding() {
        let mut backup = EncryptedBackup::seal(b"payload", PASSPHRASE).unwrap();
        backup.ciphertext_base64 = "A".repeat(MAX_ENCODED_CIPHERTEXT_BYTES + 1);
        assert!(backup.validate_envelope().is_err());
    }

    #[test]
    fn encrypted_backup_rejects_weak_passphrases_and_empty_payloads() {
        assert!(EncryptedBackup::seal(b"data", "short").is_err());
        assert!(EncryptedBackup::seal(b"", PASSPHRASE).is_err());
    }
}
