//! Android Keystore-backed credential vault.
//! Secret bytes are encrypted with an AES-256-GCM key generated and held by Android Keystore.
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::Mutex,
};

use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use jni::{
    objects::{JByteArray, JObject, JValue},
    JNIEnv, JavaVM,
};
use winit::platform::android::activity::AndroidApp;
use sha2::{Digest, Sha256};

use crate::core::network::SessionVault;
use crate::core::workspace::WORKSPACE_CIPHER_MAGIC;

fn valid_supabase_user_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 36
        && [8, 13, 18, 23].iter().all(|index| bytes[*index] == b'-')
        && bytes.iter().enumerate().all(|(index, byte)| {
            [8, 13, 18, 23].contains(&index) || byte.is_ascii_hexdigit()
        })
}

fn valid_workspace_relative_path(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.starts_with('/')
        && !value.contains('\\') && !value.contains('\n') && !value.contains('\r')
        && value.split('/').all(|part| !part.is_empty() && part != "." && part != "..")
}

pub struct AndroidSecretVault {
    app: AndroidApp,
    path: PathBuf,
    lock: Mutex<()>,
}

impl AndroidSecretVault {
    pub fn new(app: AndroidApp) -> Result<Self, String> {
        let root = app.internal_data_path().ok_or("Android private data directory is unavailable")?;
        let directory = root.join("cenai-vault");
        fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        Ok(Self { app, path: directory.join("secrets.json"), lock: Mutex::new(()) })
    }

    fn with_env<T>(&self, operation: impl FnOnce(&mut JNIEnv<'_>) -> jni::errors::Result<T>) -> Result<T, String> {
        let vm = unsafe { JavaVM::from_raw(self.app.vm_as_ptr().cast()) }.map_err(|error| error.to_string())?;
        let mut env = vm.attach_current_thread().map_err(|error| error.to_string())?;
        operation(&mut env).map_err(|error| error.to_string())
    }

    fn key<'local>(env: &mut JNIEnv<'local>) -> jni::errors::Result<JObject<'local>> {
        let store_class = env.find_class("java/security/KeyStore")?;
        let provider = env.new_string("AndroidKeyStore")?;
        let store = env.call_static_method(
            store_class,
            "getInstance",
            "(Ljava/lang/String;)Ljava/security/KeyStore;",
            &[JValue::Object(&provider)],
        )?.l()?;
        env.call_method(
            &store,
            "load",
            "(Ljava/security/KeyStore$LoadStoreParameter;)V",
            &[JValue::Object(&JObject::null())],
        )?;

        let alias = env.new_string("cenai_aes_gcm_v1")?;
        let key = env.call_method(
            &store,
            "getKey",
            "(Ljava/lang/String;[C)Ljava/security/Key;",
            &[JValue::Object(&alias), JValue::Object(&JObject::null())],
        )?.l()?;
        if !key.is_null() {
            return Ok(key);
        }

        let algorithm = env.new_string("AES")?;
        let provider = env.new_string("AndroidKeyStore")?;
        let generator = env.call_static_method(
            "javax/crypto/KeyGenerator",
            "getInstance",
            "(Ljava/lang/String;Ljava/lang/String;)Ljavax/crypto/KeyGenerator;",
            &[JValue::Object(&algorithm), JValue::Object(&provider)],
        )?.l()?;
        let builder = env.new_object(
            "android/security/keystore/KeyGenParameterSpec$Builder",
            "(Ljava/lang/String;I)V",
            &[JValue::Object(&alias), JValue::Int(3)],
        )?;

        let modes = env.new_object_array(1, "java/lang/String", JObject::null())?;
        let gcm = env.new_string("GCM")?;
        env.set_object_array_element(&modes, 0, gcm)?;
        let modes_object = JObject::from(modes);
        env.call_method(
            &builder,
            "setBlockModes",
            "([Ljava/lang/String;)Landroid/security/keystore/KeyGenParameterSpec$Builder;",
            &[JValue::Object(&modes_object)],
        )?;

        let paddings = env.new_object_array(1, "java/lang/String", JObject::null())?;
        let no_padding = env.new_string("NoPadding")?;
        env.set_object_array_element(&paddings, 0, no_padding)?;
        let paddings_object = JObject::from(paddings);
        env.call_method(
            &builder,
            "setEncryptionPaddings",
            "([Ljava/lang/String;)Landroid/security/keystore/KeyGenParameterSpec$Builder;",
            &[JValue::Object(&paddings_object)],
        )?;
        env.call_method(
            &builder,
            "setRandomizedEncryptionRequired",
            "(Z)Landroid/security/keystore/KeyGenParameterSpec$Builder;",
            &[JValue::Bool(1)],
        )?;
        let specification = env.call_method(&builder, "build", "()Landroid/security/keystore/KeyGenParameterSpec;", &[])?.l()?;
        env.call_method(
            &generator,
            "init",
            "(Ljava/security/spec/AlgorithmParameterSpec;)V",
            &[JValue::Object(&specification)],
        )?;
        env.call_method(&generator, "generateKey", "()Ljavax/crypto/SecretKey;", &[])?.l()
    }

    fn encrypt_bytes(&self, value: &[u8]) -> Result<Vec<u8>, String> {
        self.with_env(|env| {
            let key = Self::key(env)?;
            let algorithm = env.new_string("AES/GCM/NoPadding")?;
            let cipher = env.call_static_method("javax/crypto/Cipher", "getInstance", "(Ljava/lang/String;)Ljavax/crypto/Cipher;", &[JValue::Object(&algorithm)])?.l()?;
            env.call_method(&cipher, "init", "(ILjava/security/Key;)V", &[JValue::Int(1), JValue::Object(&key)])?;
            let plain = env.byte_array_from_slice(value)?;
            let plain_object = JObject::from(plain);
            let encrypted = env.call_method(&cipher, "doFinal", "([B)[B", &[JValue::Object(&plain_object)])?.l()?;
            let iv = env.call_method(&cipher, "getIV", "()[B", &[])?.l()?;
            let mut packed = env.convert_byte_array(JByteArray::from(iv))?;
            packed.extend(env.convert_byte_array(JByteArray::from(encrypted))?);
            Ok(packed)
        })
    }

    fn encrypt(&self, value: &str) -> Result<Vec<u8>, String> {
        self.encrypt_bytes(value.as_bytes())
    }

    fn decrypt_bytes(&self, packed: &[u8]) -> Result<Vec<u8>, String> {
        if packed.len() <= 12 { return Err("Encrypted secret is truncated".into()); }
        self.with_env(|env| {
            let key = Self::key(env)?;
            let algorithm = env.new_string("AES/GCM/NoPadding")?;
            let cipher = env.call_static_method("javax/crypto/Cipher", "getInstance", "(Ljava/lang/String;)Ljavax/crypto/Cipher;", &[JValue::Object(&algorithm)])?.l()?;
            let iv = env.byte_array_from_slice(&packed[..12])?;
            let iv_object = JObject::from(iv);
            let parameters = env.new_object("javax/crypto/spec/GCMParameterSpec", "(I[B)V", &[JValue::Int(128), JValue::Object(&iv_object)])?;
            env.call_method(&cipher, "init", "(ILjava/security/Key;Ljava/security/spec/AlgorithmParameterSpec;)V", &[JValue::Int(2), JValue::Object(&key), JValue::Object(&parameters)])?;
            let ciphertext = env.byte_array_from_slice(&packed[12..])?;
            let ciphertext_object = JObject::from(ciphertext);
            let plain = env.call_method(&cipher, "doFinal", "([B)[B", &[JValue::Object(&ciphertext_object)])?.l()?;
            env.convert_byte_array(JByteArray::from(plain)).map_err(|error| error.to_string())
        })
    }

    fn decrypt(&self, packed: &[u8]) -> Result<String, String> {
        let bytes = self.decrypt_bytes(packed)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
    fn read_map(&self) -> Result<BTreeMap<String, String>, String> {
        if !self.path.exists() { return Ok(BTreeMap::new()); }
        let bytes = fs::read(&self.path).map_err(|error| error.to_string())?;
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())
    }

    fn write_map(&self, values: &BTreeMap<String, String>) -> Result<(), String> {
        let bytes = serde_json::to_vec(values).map_err(|error| error.to_string())?;
        let temporary = self.path.with_extension("tmp");
        let mut file = OpenOptions::new().write(true).create(true).truncate(true).open(&temporary)
            .map_err(|error| error.to_string())?;
        file.write_all(&bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        fs::rename(temporary, &self.path).map_err(|error| error.to_string())
    }

    fn put_secrets(&self, entries: &[(&str, &str)]) -> Result<(), String> {
        if entries.iter().any(|(name, _)| name.is_empty() || name.len() > 120) {
            return Err("Secret name is invalid".into());
        }
        let _guard = self.lock.lock().map_err(|_| "Vault lock poisoned")?;
        let mut values = self.read_map()?;
        for (name, value) in entries {
            if value.is_empty() {
                values.remove(*name);
            } else {
                values.insert((*name).to_owned(), STANDARD_NO_PAD.encode(self.encrypt(value)?));
            }
        }
        self.write_map(&values)
    }

    pub fn put_secret(&self, name: &str, value: &str) -> Result<(), String> {
        self.put_secrets(&[(name, value)])
    }

    fn scoped_secret_name(&self, namespace: &str, key: &str) -> Result<String, String> {
        let account_id = self.get_secret("supabase_account_id")?;
        if account_id.is_empty() {
            return Err("Sign in to the CenAI account before accessing account-scoped settings.".into());
        }
        let digest = Sha256::digest(account_id.as_bytes());
        let account_scope = digest.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
        Ok(format!("{namespace}_{account_scope}_{key}"))
    }

    pub fn get_secret(&self, name: &str) -> Result<String, String> {
        let _guard = self.lock.lock().map_err(|_| "Vault lock poisoned")?;
        let values = self.read_map()?;
        let Some(encoded) = values.get(name) else { return Ok(String::new()); };
        let packed = STANDARD_NO_PAD.decode(encoded).map_err(|error| error.to_string())?;
        self.decrypt(&packed)
    }
}

impl SessionVault for AndroidSecretVault {
    fn access_token(&self) -> Result<String, String> { self.get_secret("supabase_access_token") }
    fn refresh_token(&self) -> Result<String, String> { self.get_secret("supabase_refresh_token") }
    fn account_email(&self) -> Result<String, String> { self.get_secret("supabase_account_email") }
    fn account_id(&self) -> Result<String, String> { self.get_secret("supabase_account_id") }

    fn encrypt_workspace_bytes(&self, user_identity: &str, relative_path: &str, plaintext: &[u8]) -> Result<Vec<u8>, String> {
        if !valid_supabase_user_id(user_identity) || !valid_workspace_relative_path(relative_path) || plaintext.len() > 110 * 1024 * 1024 {
            return Err("Account-file encryption identity, path, or payload size is invalid".into());
        }
        let mut envelope = format!("CENAI-ACCOUNT-FILE-V1\n{user_identity}\n{relative_path}\n").into_bytes();
        envelope.extend_from_slice(plaintext);
        let encrypted = self.encrypt_bytes(&envelope)?;
        let mut output = WORKSPACE_CIPHER_MAGIC.to_vec();
        output.extend_from_slice(&encrypted);
        Ok(output)
    }

    fn decrypt_workspace_bytes(&self, user_identity: &str, relative_path: &str, ciphertext: &[u8]) -> Result<Vec<u8>, String> {
        if !valid_supabase_user_id(user_identity) || !valid_workspace_relative_path(relative_path)
            || !ciphertext.starts_with(WORKSPACE_CIPHER_MAGIC) || ciphertext.len() > 120 * 1024 * 1024 {
            return Err("Encrypted account-file identity, path, or envelope is invalid".into());
        }
        let envelope = self.decrypt_bytes(&ciphertext[WORKSPACE_CIPHER_MAGIC.len()..])?;
        let prefix = format!("CENAI-ACCOUNT-FILE-V1\n{user_identity}\n{relative_path}\n");
        if !envelope.starts_with(prefix.as_bytes()) {
            return Err("Encrypted file belongs to a different account or storage path".into());
        }
        let plaintext = envelope[prefix.len()..].to_vec();
        if plaintext.len() > 110 * 1024 * 1024 { return Err("Encrypted account file exceeds the storage limit".into()); }
        Ok(plaintext)
    }

    fn save_session(&self, access: &str, refresh: &str, user_id: &str, email: &str) -> Result<(), String> {
        if !valid_supabase_user_id(user_id) || access.trim().is_empty() || refresh.trim().is_empty() {
            return Err("A valid Supabase user UUID and complete authenticated session are required.".into());
        }
        self.put_secrets(&[
            ("supabase_access_token", access),
            ("supabase_refresh_token", refresh),
            ("supabase_account_id", user_id),
            ("supabase_account_email", email),
        ])
    }

    fn clear_session(&self) -> Result<(), String> {
        self.put_secrets(&[
            ("supabase_access_token", ""),
            ("supabase_refresh_token", ""),
            ("supabase_account_email", ""),
            ("supabase_account_id", ""),
        ])
    }
    fn provider_key(&self, provider: &str) -> Result<String, String> {
        if provider.len() > 32 || !provider.chars().all(|character| character.is_ascii_alphanumeric() || character == '_' || character == '-') {
            return Err("Provider identifier is invalid".into());
        }
        let name = self.scoped_secret_name("provider_api_key", provider)?;
        self.get_secret(&name)
    }

    fn email_verified(&self) -> Result<bool, String> {
        let name = self.scoped_secret_name("cenai", "email_verified")?;
        Ok(self.get_secret(&name)? == "true")
    }

    fn set_email_verified(&self, verified: bool) -> Result<(), String> {
        let name = self.scoped_secret_name("cenai", "email_verified")?;
        self.put_secret(&name, if verified { "true" } else { "false" })
    }

    fn guardian_consent(&self) -> Result<bool, String> {
        let name = self.scoped_secret_name("cenai", "guardian_telemetry_consent")?;
        Ok(self.get_secret(&name)? == "true")
    }

    fn set_guardian_consent(&self, enabled: bool) -> Result<(), String> {
        let name = self.scoped_secret_name("cenai", "guardian_telemetry_consent")?;
        self.put_secret(&name, if enabled { "true" } else { "false" })
    }

    fn user_setting(&self, key: &str) -> Result<String, String> {
        if !matches!(key, "light_mode" | "reduced_motion") {
            return Err("Unsupported account preference".into());
        }
        let name = self.scoped_secret_name("settings", key)?;
        self.get_secret(&name)
    }

    fn set_user_setting(&self, key: &str, value: &str) -> Result<(), String> {
        if !matches!(key, "light_mode" | "reduced_motion") || !matches!(value, "true" | "false") {
            return Err("Unsupported account preference value".into());
        }
        let name = self.scoped_secret_name("settings", key)?;
        self.put_secret(&name, value)
    }

    fn save_provider_key(&self, provider: &str, key: &str) -> Result<(), String> {
        if provider.len() > 32 || !provider.chars().all(|character| character.is_ascii_alphanumeric() || character == '_' || character == '-') {
            return Err("Provider identifier is invalid".into());
        }
        let name = self.scoped_secret_name("provider_api_key", provider)?;
        self.put_secret(&name, key)
    }
}


#[cfg(test)]
mod identity_tests {
    use super::valid_supabase_user_id;

    #[test]
    fn only_canonical_supabase_uuid_identities_are_accepted() {
        assert!(valid_supabase_user_id("550e8400-e29b-41d4-a716-446655440000"));
        assert!(!valid_supabase_user_id(""));
        assert!(!valid_supabase_user_id("user@example.com"));
        assert!(!valid_supabase_user_id("550e8400e29b41d4a716446655440000"));
        assert!(!valid_supabase_user_id("550e8400-e29b-41d4-a716-44665544000z"));
    }
}
