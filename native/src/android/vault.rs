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

    fn encrypt(&self, value: &str) -> Result<Vec<u8>, String> {
        self.with_env(|env| {
            let key = Self::key(env)?;
            let algorithm = env.new_string("AES/GCM/NoPadding")?;
            let cipher = env.call_static_method(
                "javax/crypto/Cipher",
                "getInstance",
                "(Ljava/lang/String;)Ljavax/crypto/Cipher;",
                &[JValue::Object(&algorithm)],
            )?.l()?;
            env.call_method(&cipher, "init", "(ILjava/security/Key;)V", &[JValue::Int(1), JValue::Object(&key)])?;
            let plain = env.byte_array_from_slice(value.as_bytes())?;
            let plain_object = JObject::from(plain);
            let encrypted = env.call_method(&cipher, "doFinal", "([B)[B", &[JValue::Object(&plain_object)])?.l()?;
            let iv = env.call_method(&cipher, "getIV", "()[B", &[])?.l()?;
            let iv_bytes = env.convert_byte_array(JByteArray::from(iv))?;
            let encrypted_bytes = env.convert_byte_array(JByteArray::from(encrypted))?;
            let mut packed = iv_bytes;
            packed.extend(encrypted_bytes);
            Ok(packed)
        })
    }

    fn decrypt(&self, packed: &[u8]) -> Result<String, String> {
        if packed.len() <= 12 { return Err("Encrypted secret is truncated".into()); }
        self.with_env(|env| {
            let key = Self::key(env)?;
            let algorithm = env.new_string("AES/GCM/NoPadding")?;
            let cipher = env.call_static_method(
                "javax/crypto/Cipher",
                "getInstance",
                "(Ljava/lang/String;)Ljavax/crypto/Cipher;",
                &[JValue::Object(&algorithm)],
            )?.l()?;
            let iv = env.byte_array_from_slice(&packed[..12])?;
            let iv_object = JObject::from(iv);
            let parameters = env.new_object(
                "javax/crypto/spec/GCMParameterSpec",
                "(I[B)V",
                &[JValue::Int(128), JValue::Object(&iv_object)],
            )?;
            env.call_method(
                &cipher,
                "init",
                "(ILjava/security/Key;Ljava/security/spec/AlgorithmParameterSpec;)V",
                &[JValue::Int(2), JValue::Object(&key), JValue::Object(&parameters)],
            )?;
            let ciphertext = env.byte_array_from_slice(&packed[12..])?;
            let ciphertext_object = JObject::from(ciphertext);
            let plain = env.call_method(&cipher, "doFinal", "([B)[B", &[JValue::Object(&ciphertext_object)])?.l()?;
            let bytes = env.convert_byte_array(JByteArray::from(plain))?;
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        })
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

    fn save_session(&self, access: &str, refresh: &str, user_id: &str, email: &str) -> Result<(), String> {
        if user_id.trim().is_empty() { return Err("A verified account UUID is required.".into()); }
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
        if !provider.chars().all(|character| character.is_ascii_alphanumeric() || character == '_' || character == '-') {
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

    fn save_provider_key(&self, provider: &str, key: &str) -> Result<(), String> {
        if !provider.chars().all(|character| character.is_ascii_alphanumeric() || character == '_' || character == '-') {
            return Err("Provider identifier is invalid".into());
        }
        let name = self.scoped_secret_name("provider_api_key", provider)?;
        self.put_secret(&name, key)
    }
}
