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
    objects::{JByteArray, JObject, JString, JValue},
    sys::jobject,
    JNIEnv, JavaVM,
};
use winit::platform::android::activity::AndroidApp;

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
        let vm = unsafe { JavaVM::from_raw(self.app.vm_as_ptr()) }.map_err(|error| error.to_string())?;
        let mut env = vm.attach_current_thread().map_err(|error| error.to_string())?;
        operation(&mut env).map_err(|error| error.to_string())
    }

    fn key(env: &mut JNIEnv<'_>) -> jni::errors::Result<JObject<'_>> {
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

    pub fn put_secret(&self, name: &str, value: &str) -> Result<(), String> {
        if name.is_empty() || name.len() > 100 { return Err("Secret name is invalid".into()); }
        let _guard = self.lock.lock().map_err(|_| "Vault lock poisoned")?;
        let mut values = self.read_map()?;
        if value.is_empty() {
            values.remove(name);
        } else {
            values.insert(name.to_owned(), STANDARD_NO_PAD.encode(self.encrypt(value)?));
        }
        self.write_map(&values)
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

    fn save_session(&self, access: &str, refresh: &str, email: &str) -> Result<(), String> {
        self.put_secret("supabase_access_token", access)?;
        self.put_secret("supabase_refresh_token", refresh)?;
        self.put_secret("supabase_account_email", email)
    }

    fn clear_session(&self) -> Result<(), String> {
        self.put_secret("supabase_access_token", "")?;
        self.put_secret("supabase_refresh_token", "")?;
        self.put_secret("supabase_account_email", "")
    }
}
