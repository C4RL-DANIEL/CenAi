//! Authenticated access to the existing Supabase and CenAI Cloud endpoints.
use std::sync::Arc;

use base64::Engine as _;
use futures_util::StreamExt;
use reqwest::{header, Client, StatusCode};
use sha2::{Digest, Sha256};
use serde_json::{json, Value};

use crate::core::{config, multimodal::AttachmentStore, providers::{parse_openai_sse_event, validate_provider_endpoint, ChatRequest}, workspace::WorkspaceStore};

const MAX_STREAM_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const MAX_SSE_LINE_BYTES: usize = 256 * 1024;

fn jwt_has_active_aal2(token: &str, now_unix: i64) -> bool {
    let mut segments = token.split('.');
    let (Some(_header), Some(payload), Some(_signature), None) =
        (segments.next(), segments.next(), segments.next(), segments.next())
    else { return false; };
    if payload.is_empty() || payload.len() > 16 * 1024 { return false; }
    let Ok(decoded) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload) else { return false; };
    let Ok(claims) = serde_json::from_slice::<Value>(&decoded) else { return false; };
    claims.get("aal").and_then(Value::as_str) == Some("aal2")
        && claims.get("exp").and_then(Value::as_i64).is_some_and(|expires| expires > now_unix)
}

fn append_stream_delta(answer: &mut String, delta: &str) -> Result<(), String> {
    if answer.len().saturating_add(delta.len()) > MAX_STREAM_RESPONSE_BYTES {
        return Err("AI response exceeded the 2 MiB safety limit.".into());
    }
    answer.push_str(delta);
    Ok(())
}

fn valid_supabase_user_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 36 && [8, 13, 18, 23].iter().all(|index| bytes[*index] == b'-')
        && bytes.iter().enumerate().all(|(index, byte)| {
            [8, 13, 18, 23].contains(&index) || byte.is_ascii_hexdigit()
        })
}

fn is_non_public_provider_ip(address: std::net::IpAddr) -> bool {
    match address {
        std::net::IpAddr::V4(ip) => {
            let octets = ip.octets();
            ip.is_private() || ip.is_loopback() || ip.is_link_local() || ip.is_unspecified()
                || ip.is_multicast() || ip.is_broadcast() || octets[0] == 0 || octets[0] >= 240
                || (octets[0] == 100 && (64..=127).contains(&octets[1]))
                || (octets[0] == 192 && octets[1] == 0 && (octets[2] == 0 || octets[2] == 2))
                || (octets[0] == 198 && (octets[1] == 18 || octets[1] == 19 || (octets[1] == 51 && octets[2] == 100)))
                || (octets[0] == 203 && octets[1] == 0 && octets[2] == 113)
        }
        std::net::IpAddr::V6(ip) => {
            let segments = ip.segments();
            let octets = ip.octets();
            let mapped_v4 = segments[..5].iter().all(|segment| *segment == 0) && segments[5] == 0xffff;
            let compatible_v4 = segments[..6].iter().all(|segment| *segment == 0) && segments[6] != 0;
            if mapped_v4 || compatible_v4 {
                return is_non_public_provider_ip(std::net::IpAddr::V4(std::net::Ipv4Addr::new(
                    octets[12], octets[13], octets[14], octets[15]
                )));
            }
            ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local()
                || ip.is_unspecified() || ip.is_multicast()
                || (segments[0] == 0x2001 && (segments[1] == 0x0db8 || segments[1] == 0x0002))
                || (segments[0] == 0x3fff && segments[1] & 0xf000 == 0)
                || (segments[0] & 0xffc0 == 0xfec0)
        }
    }
}

async fn pinned_provider_client(endpoint: &str) -> Result<(Client, reqwest::Url), String> {
    let url = reqwest::Url::parse(endpoint).map_err(|_| "Provider endpoint is invalid")?;
    let host = url.host_str().ok_or("Provider endpoint host is missing")?;
    let port = url.port_or_known_default().ok_or("Provider endpoint port is missing")?;
    let addresses = tokio::net::lookup_host((host, port)).await
        .map_err(|error| format!("Provider host DNS lookup failed: {error}"))?
        .collect::<Vec<_>>();
    if addresses.is_empty() || addresses.iter().any(|address| is_non_public_provider_ip(address.ip())) {
        return Err("Provider hostname resolves to a private, reserved or local network address".into());
    }
    let client = Client::builder()
        .connect_timeout(std::time::Duration::from_secs(20))
        .timeout(std::time::Duration::from_secs(config::STREAM_TIMEOUT_SECONDS))
        .redirect(reqwest::redirect::Policy::none())
        .resolve_to_addrs(host, &addresses)
        .user_agent(concat!("CenAI/", env!("CARGO_PKG_VERSION")))
        .build().map_err(|error| error.to_string())?;
    Ok((client, url))
}

pub trait SessionVault: Send + Sync {
    fn access_token(&self) -> Result<String, String>;
    fn refresh_token(&self) -> Result<String, String>;
    fn account_email(&self) -> Result<String, String>;
    /// Immutable Supabase user UUID from the verified auth response.
    fn account_id(&self) -> Result<String, String>;
    fn save_session(&self, access: &str, refresh: &str, user_id: &str, email: &str) -> Result<(), String>;
    fn clear_session(&self) -> Result<(), String>;
    fn provider_key(&self, provider: &str) -> Result<String, String>;
    fn save_provider_key(&self, provider: &str, key: &str) -> Result<(), String>;
    fn email_verified(&self) -> Result<bool, String>;
    fn set_email_verified(&self, verified: bool) -> Result<(), String>;
    fn guardian_consent(&self) -> Result<bool, String>;
    fn set_guardian_consent(&self, enabled: bool) -> Result<(), String>;
    fn user_setting(&self, key: &str) -> Result<String, String>;
    fn set_user_setting(&self, key: &str, value: &str) -> Result<(), String>;
    /// Encrypts private workspace bytes with a device-held key and account/path binding.
    fn encrypt_workspace_bytes(&self, _user_identity: &str, _relative_path: &str, _plaintext: &[u8]) -> Result<Vec<u8>, String> {
        Err("This platform does not provide encrypted workspace storage".into())
    }
    /// Decrypts workspace bytes only for the matching authenticated account and relative path.
    fn decrypt_workspace_bytes(&self, _user_identity: &str, _relative_path: &str, _ciphertext: &[u8]) -> Result<Vec<u8>, String> {
        Err("This platform does not provide encrypted workspace storage".into())
    }
}

#[derive(Clone)]
pub struct CenAiApi {
    client: Client,
    provider_client: Client,
    voice_client: Client,
    vault: Arc<dyn SessionVault>,
    session_lock: Arc<tokio::sync::Mutex<()>>,
}

impl CenAiApi {
    pub fn new(vault: Arc<dyn SessionVault>) -> Result<Self, reqwest::Error> {
        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(20))
            .timeout(std::time::Duration::from_secs(config::REQUEST_TIMEOUT_SECONDS))
            .user_agent(concat!("CenAI/", env!("CARGO_PKG_VERSION")))
            .build()?;
        let voice_client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(20))
            .timeout(std::time::Duration::from_secs(240))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("CenAI-Voice/", env!("CARGO_PKG_VERSION")))
            .build()?;
        // Never forward provider credentials through redirects to another origin.
        let provider_client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(20))
            .timeout(std::time::Duration::from_secs(config::STREAM_TIMEOUT_SECONDS))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("CenAI/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self { client, provider_client, voice_client, vault, session_lock: Arc::new(tokio::sync::Mutex::new(())) })
    }

    pub fn session_email(&self) -> Result<String, String> {
        self.vault.account_email()
    }

    /// Checks the official CenAI GitHub releases feed using strict Rust metadata validation.
    pub async fn check_for_update(
        &self,
        current_version: &str,
        include_prerelease: bool,
    ) -> Result<Option<crate::core::updates::UpdateInfo>, String> {
        let response = self.client.get("https://api.github.com/repos/C4RL-DANIEL/CenAi/releases?per_page=30")
            .header(header::ACCEPT, "application/vnd.github+json")
            .header(header::USER_AGENT, concat!("CenAI/", env!("CARGO_PKG_VERSION")))
            .send().await.map_err(|error| format!("Update check failed: {error}"))?;
        if response.status() == StatusCode::NOT_FOUND { return Ok(None); }
        if !response.status().is_success() {
            return Err(format!("Update check failed (HTTP {}).", response.status().as_u16()));
        }
        let bytes = response.bytes().await.map_err(|error| format!("Update metadata download failed: {error}"))?;
        if bytes.len() > 1024 * 1024 { return Err("Update metadata exceeds the supported size limit.".into()); }
        let body = std::str::from_utf8(&bytes).map_err(|_| "Update metadata is not valid UTF-8")?;
        crate::core::updates::parse_latest_release(body, current_version, include_prerelease)
            .map_err(str::to_owned)
    }

    /// Streams an update to a private cache path, enforces a size bound and verifies
    /// SHA-256 before atomically making the APK available to the Android installer.
    pub async fn download_update_to(
        &self,
        info: &crate::core::updates::UpdateInfo,
        destination: &std::path::Path,
    ) -> Result<(), String> {
        use tokio::io::AsyncWriteExt;
        if !crate::core::updates::valid_release_download_url(&info.download_url) {
            return Err("Update download URL is not an official CenAI release asset.".into());
        }
        if info.sha256.len() != 64 || !info.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("Update SHA-256 digest is invalid.".into());
        }
        let temporary = destination.with_extension("apk.part");
        let _ = tokio::fs::remove_file(&temporary).await;
        let result = async {
            let response = self.client.get(&info.download_url).send().await
                .map_err(|error| format!("APK download failed: {error}"))?;
            if !response.status().is_success() {
                return Err(format!("APK download failed (HTTP {}).", response.status().as_u16()));
            }
            let final_url = response.url();
            let final_host = final_url.host_str().unwrap_or_default();
            if final_url.scheme() != "https" || !matches!(final_host,
                "github.com" | "release-assets.githubusercontent.com" | "objects.githubusercontent.com") {
                return Err("APK download redirected outside GitHub's trusted release hosts.".into());
            }
            if response.content_length().is_some_and(|size| size == 0 || size > crate::core::updates::MAX_UPDATE_APK_BYTES) {
                return Err("APK download size is outside the supported range.".into());
            }
            let mut file = tokio::fs::OpenOptions::new().write(true).create_new(true)
                .open(&temporary).await.map_err(|error| format!("Could not create update cache file: {error}"))?;
            let mut stream = response.bytes_stream();
            let mut digest = Sha256::new();
            let mut total = 0u64;
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|error| format!("APK download stream failed: {error}"))?;
                total = total.saturating_add(chunk.len() as u64);
                if total > crate::core::updates::MAX_UPDATE_APK_BYTES
                    || info.size.is_some_and(|expected| total > expected) {
                    return Err("APK download exceeded its declared or supported size.".into());
                }
                digest.update(&chunk);
                file.write_all(&chunk).await.map_err(|error| format!("Could not write update cache: {error}"))?;
            }
            if total == 0 || info.size.is_some_and(|expected| total != expected) {
                return Err("Downloaded APK size does not match the release metadata.".into());
            }
            file.sync_all().await.map_err(|error| format!("Could not sync update cache: {error}"))?;
            drop(file);
            let actual = digest.finalize();
            let actual_hex = actual.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
            if !actual_hex.eq_ignore_ascii_case(&info.sha256) {
                return Err("APK SHA-256 verification failed. The downloaded file was rejected.".into());
            }
            tokio::fs::rename(&temporary, destination).await
                .map_err(|error| format!("Could not finalize verified APK: {error}"))?;
            Ok(())
        }.await;
        if result.is_err() { let _ = tokio::fs::remove_file(&temporary).await; }
        result
    }

    /// Returns the immutable account UUID received from Supabase, never an email-derived identity.
    pub fn account_id(&self) -> Result<String, String> {
        self.vault.account_id()
    }

    /// Creates an account-bound workspace whose files are encrypted by the platform vault.
    pub fn workspace_store(&self, root: impl AsRef<std::path::Path>) -> Result<WorkspaceStore, String> {
        let account_id = self.account_id()?;
        WorkspaceStore::new_encrypted(root, &account_id, self.vault.clone())
    }

    /// Creates account-bound, encrypted storage for selected multimodal attachments.
    pub fn attachment_store(&self, root: impl AsRef<std::path::Path>) -> Result<AttachmentStore, String> {
        let account_id = self.account_id()?;
        AttachmentStore::new_encrypted(root, &account_id, self.vault.clone())
    }

    pub fn has_session(&self) -> bool {
        self.vault.access_token().map(|token| !token.is_empty()).unwrap_or(false)
    }

    /// Trusts only a non-expired Supabase-signed AAL2 claim from the stored session.
    pub fn session_has_aal2(&self) -> bool {
        self.vault.access_token().map(|token| jwt_has_active_aal2(&token, chrono::Utc::now().timestamp())).unwrap_or(false)
    }

    pub async fn sign_in(&self, email: &str, password: &str) -> Result<String, String> {
        let _session_guard = self.session_lock.lock().await;
        if !email.contains('@') || password.is_empty() {
            return Err("Enter your existing CenAI account email and password.".into());
        }
        let response = self.client.post(format!("{}/auth/v1/token?grant_type=password", config::SUPABASE_URL))
            .header("apikey", config::SUPABASE_PUBLISHABLE_KEY)
            .json(&json!({ "email": email.trim(), "password": password }))
            .send().await.map_err(|error| error.to_string())?;
        let data = Self::decode_response(response).await?;
        let access = data.get("access_token").and_then(Value::as_str).unwrap_or_default();
        let refresh = data.get("refresh_token").and_then(Value::as_str).unwrap_or_default();
        let account_email = data.pointer("/user/email").and_then(Value::as_str).unwrap_or(email).to_owned();
        let user_id = data.pointer("/user/id").and_then(Value::as_str).unwrap_or_default();
        if access.is_empty() || refresh.is_empty() || !valid_supabase_user_id(user_id) {
            return Err("The identity service returned an incomplete or invalid account identity.".into());
        }
        self.vault.save_session(access, refresh, user_id, &account_email)?;
        Ok(account_email)
    }

    pub fn guardian_consent(&self) -> bool {
        self.vault.guardian_consent().unwrap_or(false)
    }

    pub fn set_guardian_consent(&self, enabled: bool) -> Result<(), String> {
        self.vault.set_guardian_consent(enabled)
    }

    pub fn user_setting(&self, key: &str) -> Result<String, String> {
        self.vault.user_setting(key)
    }

    pub fn save_user_setting(&self, key: &str, value: &str) -> Result<(), String> {
        self.vault.set_user_setting(key, value)
    }

    pub fn email_verified(&self) -> bool {
        self.vault.email_verified().unwrap_or(false)
    }

    pub async fn send_email_otp(&self) -> Result<String, String> {
        let token = self.vault.access_token()?;
        if token.is_empty() { return Err("Sign in to your CenAI account first.".into()); }
        let response = self.client.post(config::WEBSITE_OTP_URL)
            .header("apikey", config::SUPABASE_PUBLISHABLE_KEY)
            .bearer_auth(token)
            .json(&json!({ "action": "send" }))
            .send().await.map_err(|error| error.to_string())?;
        let data = Self::decode_response(response).await?;
        Ok(data.get("message").and_then(Value::as_str).unwrap_or("Verification code sent.").to_owned())
    }

    pub async fn verify_email_otp(&self, code: &str) -> Result<String, String> {
        if code.len() != 6 || !code.chars().all(|character| character.is_ascii_digit()) {
            return Err("Enter the six-digit verification code.".into());
        }
        let account_id = self.vault.account_id()?;
        let token = self.vault.access_token()?;
        if token.is_empty() || account_id.is_empty() { return Err("Sign in to your CenAI account first.".into()); }
        let response = self.client.post(config::WEBSITE_OTP_URL)
            .header("apikey", config::SUPABASE_PUBLISHABLE_KEY)
            .bearer_auth(token)
            .json(&json!({ "action": "verify", "code": code }))
            .send().await.map_err(|error| error.to_string())?;
        let data = Self::decode_response(response).await?;
        if data.get("verified").and_then(Value::as_bool) != Some(true) {
            return Err("Email verification was not confirmed.".into());
        }
        let _session_guard = self.session_lock.lock().await;
        if self.vault.account_id()? != account_id {
            return Err("The active account changed before verification completed.".into());
        }
        self.vault.set_email_verified(true)?;
        Ok(data.get("message").and_then(Value::as_str).unwrap_or("Email verified.").to_owned())
    }

    /// Lists only the authenticated account's Supabase MFA factors.
    pub async fn mfa_factors(&self) -> Result<Value, String> {
        let token = self.vault.access_token()?;
        if token.is_empty() { return Err("Sign in before managing multi-factor authentication.".into()); }
        let response = self.client.get(format!("{}/auth/v1/factors", config::SUPABASE_URL))
            .header("apikey", config::SUPABASE_PUBLISHABLE_KEY)
            .bearer_auth(token)
            .send().await.map_err(|error| format!("MFA factor lookup failed: {error}"))?;
        Self::decode_response(response).await
    }

    /// Starts TOTP enrollment. The returned QR/secret must be shown only to the signed-in user.
    pub async fn mfa_enroll_totp(&self, friendly_name: &str) -> Result<Value, String> {
        let friendly_name = friendly_name.trim();
        if friendly_name.is_empty() || friendly_name.len() > 80 || friendly_name.chars().any(char::is_control) {
            return Err("Enter a valid name for this authenticator.".into());
        }
        let token = self.vault.access_token()?;
        if token.is_empty() { return Err("Sign in before enrolling an authenticator.".into()); }
        let response = self.client.post(format!("{}/auth/v1/factors", config::SUPABASE_URL))
            .header("apikey", config::SUPABASE_PUBLISHABLE_KEY)
            .bearer_auth(token)
            .json(&json!({ "factor_type": "totp", "friendly_name": friendly_name }))
            .send().await.map_err(|error| format!("MFA enrollment failed: {error}"))?;
        Self::decode_response(response).await
    }

    /// Creates a short-lived challenge for an already enrolled TOTP factor.
    pub async fn mfa_challenge(&self, factor_id: &str) -> Result<String, String> {
        if !valid_supabase_user_id(factor_id) { return Err("Authenticator factor identifier is invalid.".into()); }
        let token = self.vault.access_token()?;
        if token.is_empty() { return Err("Sign in before verifying an authenticator.".into()); }
        let response = self.client.post(format!("{}/auth/v1/factors/{factor_id}/challenge", config::SUPABASE_URL))
            .header("apikey", config::SUPABASE_PUBLISHABLE_KEY)
            .bearer_auth(token)
            .json(&json!({}))
            .send().await.map_err(|error| format!("MFA challenge failed: {error}"))?;
        let data = Self::decode_response(response).await?;
        let challenge_id = data.get("id").and_then(Value::as_str).unwrap_or_default();
        if !valid_supabase_user_id(challenge_id) { return Err("MFA service returned an invalid challenge identifier.".into()); }
        Ok(challenge_id.to_owned())
    }

    /// Verifies a TOTP challenge and stores the upgraded AAL2 session for the same account.
    pub async fn mfa_verify_totp(&self, factor_id: &str, challenge_id: &str, code: &str) -> Result<(), String> {
        if !valid_supabase_user_id(factor_id) || !valid_supabase_user_id(challenge_id) {
            return Err("Authenticator factor or challenge identifier is invalid.".into());
        }
        if code.len() != 6 || !code.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err("Enter the six-digit code from your authenticator app.".into());
        }
        let _session_guard = self.session_lock.lock().await;
        let account_id = self.vault.account_id()?;
        let email = self.vault.account_email()?;
        let token = self.vault.access_token()?;
        if token.is_empty() || !valid_supabase_user_id(&account_id) {
            return Err("Sign in before verifying an authenticator.".into());
        }
        let response = self.client.post(format!("{}/auth/v1/factors/{factor_id}/verify", config::SUPABASE_URL))
            .header("apikey", config::SUPABASE_PUBLISHABLE_KEY)
            .bearer_auth(token)
            .json(&json!({ "challenge_id": challenge_id, "code": code }))
            .send().await.map_err(|error| format!("MFA verification failed: {error}"))?;
        let data = Self::decode_response(response).await?;
        let access = data.get("access_token").and_then(Value::as_str).unwrap_or_default();
        let refresh = data.get("refresh_token").and_then(Value::as_str).unwrap_or_default();
        let returned_user = data.pointer("/user/id").and_then(Value::as_str)
            .ok_or("MFA verification response did not identify its authenticated user")?;
        if access.is_empty() || refresh.is_empty() || returned_user != account_id
            || self.vault.account_id()? != account_id {
            return Err("MFA verification did not return a valid upgraded session for the active account.".into());
        }
        self.vault.save_session(access, refresh, &account_id, &email)?;
        Ok(())
    }

    /// Removes a factor from the current account. Supabase enforces the required assurance level.
    pub async fn mfa_unenroll(&self, factor_id: &str) -> Result<(), String> {
        if !valid_supabase_user_id(factor_id) { return Err("Authenticator factor identifier is invalid.".into()); }
        let token = self.vault.access_token()?;
        if token.is_empty() { return Err("Sign in before removing an authenticator.".into()); }
        let response = self.client.delete(format!("{}/auth/v1/factors/{factor_id}", config::SUPABASE_URL))
            .header("apikey", config::SUPABASE_PUBLISHABLE_KEY)
            .bearer_auth(token)
            .send().await.map_err(|error| format!("MFA removal failed: {error}"))?;
        if !response.status().is_success() {
            return Self::decode_response(response).await.map(|_| ());
        }
        Ok(())
    }

    pub async fn sign_out(&self) -> Result<(), String> {
        let _session_guard = self.session_lock.lock().await;
        let token = self.vault.access_token()?;
        if !token.is_empty() {
            let response = self.client.post(format!("{}/auth/v1/logout", config::SUPABASE_URL))
                .header("apikey", config::SUPABASE_PUBLISHABLE_KEY)
                .bearer_auth(token)
                .send().await.map_err(|error| error.to_string())?;
            if !response.status().is_success() {
                log::warn!("Supabase logout returned HTTP {}", response.status());
            }
        }
        self.vault.clear_session()
    }

    async fn refresh_session(&self) -> Result<(), String> {
        let refresh = self.vault.refresh_token()?;
        if refresh.is_empty() { return Err("Your session has expired. Sign in again.".into()); }
        let response = self.client.post(format!("{}/auth/v1/token?grant_type=refresh_token", config::SUPABASE_URL))
            .header("apikey", config::SUPABASE_PUBLISHABLE_KEY)
            .json(&json!({ "refresh_token": refresh }))
            .send().await.map_err(|error| error.to_string())?;
        let data = Self::decode_response(response).await?;
        let access = data.get("access_token").and_then(Value::as_str).unwrap_or_default();
        let next_refresh = data.get("refresh_token").and_then(Value::as_str).unwrap_or_default();
        let email = data.pointer("/user/email").and_then(Value::as_str)
            .map(str::to_owned).unwrap_or(self.vault.account_email()?);
        let previous_user_id = self.vault.account_id()?;
        let user_id = data.pointer("/user/id").and_then(Value::as_str)
            .map(str::to_owned).unwrap_or(previous_user_id.clone());
        if access.is_empty() || next_refresh.is_empty() || !valid_supabase_user_id(&user_id)
            || (!previous_user_id.is_empty() && previous_user_id != user_id) {
            return Err("The refreshed session does not match the signed-in account.".into());
        }
        self.vault.save_session(access, next_refresh, &user_id, &email)
    }

    async fn post_authenticated(&self, url: &str, body: &Value) -> Result<Value, String> {
        let (mut token, account_id) = {
            let _session_guard = self.session_lock.lock().await;
            (self.vault.access_token()?, self.vault.account_id()?)
        };
        if token.is_empty() || account_id.is_empty() {
            return Err("Sign in to your CenAI account first.".into());
        }
        let mut response = self.client.post(url)
            .header("apikey", config::SUPABASE_PUBLISHABLE_KEY)
            .bearer_auth(&token)
            .header(header::ACCEPT, "application/json")
            .json(body)
            .send().await.map_err(|error| error.to_string())?;
        if response.status() == StatusCode::UNAUTHORIZED {
            let _session_guard = self.session_lock.lock().await;
            if self.vault.account_id()? != account_id {
                return Err("The active CenAI account changed while this request was running.".into());
            }
            let current_token = self.vault.access_token()?;
            if current_token == token {
                self.refresh_session().await?;
                token = self.vault.access_token()?;
            } else {
                token = current_token;
            }
            if token.is_empty() { return Err("Your CenAI session expired. Sign in again.".into()); }
            drop(_session_guard);
            response = self.client.post(url)
                .header("apikey", config::SUPABASE_PUBLISHABLE_KEY)
                .bearer_auth(&token)
                .header(header::ACCEPT, "application/json")
                .json(body)
                .send().await.map_err(|error| error.to_string())?;
        }
        let data = Self::decode_response(response).await?;
        if self.vault.account_id()? != account_id {
            return Err("The active CenAI account changed before the response completed.".into());
        }
        Ok(data)
    }

    async fn decode_response(response: reqwest::Response) -> Result<Value, String> {
        let status = response.status();
        let raw = response.text().await.map_err(|error| error.to_string())?;
        let data: Value = serde_json::from_str(&raw).unwrap_or_else(|_| json!({}));
        if !status.is_success() {
            let message = ["msg", "message", "error_description", "error"]
                .iter().find_map(|key| data.get(*key).and_then(Value::as_str))
                .unwrap_or("CenAI service request failed");
            return Err(format!("HTTP {}: {}", status.as_u16(), message));
        }
        Ok(data)
    }

    /// Checks the authenticated CenAI cloud voice service and GPU model readiness.
    pub async fn cloud_voice_status(&self) -> Result<Value, String> {
        self.post_authenticated(config::CLOUD_VOICE_STATUS_URL, &json!({})).await
    }

    /// Synthesizes speech through CenAI's authenticated cloud voice gateway.
    /// The model and voice reference remain on the server; no model weights are downloaded to Android.
    pub async fn synthesize_voice_cloud(
        &self,
        text: &str,
        language: &str,
        voice: &str,
    ) -> Result<Vec<u8>, String> {
        const MAX_TEXT_BYTES: usize = 32_000;
        const MAX_AUDIO_BYTES: usize = 32 * 1024 * 1024;
        let text = text.trim();
        if text.is_empty() || text.len() > MAX_TEXT_BYTES {
            return Err("Speech text is empty or exceeds the CenAI Voice limit.".into());
        }
        if language != "English" {
            return Err("CenAI Voice currently supports English synthesis only.".into());
        }
        if voice != "cenai-en-v1" {
            return Err("The requested CenAI Voice profile is unavailable.".into());
        }
        let payload = json!({ "input": text, "language": language, "voice": voice });
        let (mut token, account_id) = {
            let _session_guard = self.session_lock.lock().await;
            (self.vault.access_token()?, self.vault.account_id()?)
        };
        if token.is_empty() || !valid_supabase_user_id(&account_id) {
            return Err("Sign in to your CenAI account before using cloud voice.".into());
        }
        let send = |token: &str| self.voice_client.post(config::CLOUD_VOICE_URL)
            .header("apikey", config::SUPABASE_PUBLISHABLE_KEY)
            .bearer_auth(token)
            .header(header::ACCEPT, "audio/wav")
            .json(&payload);
        let mut response = send(&token).send().await
            .map_err(|error| format!("CenAI Voice connection failed: {error}"))?;
        if response.status() == StatusCode::UNAUTHORIZED {
            let _session_guard = self.session_lock.lock().await;
            if self.vault.account_id()? != account_id {
                return Err("The active CenAI account changed while voice synthesis was running.".into());
            }
            let current_token = self.vault.access_token()?;
            if current_token == token {
                self.refresh_session().await?;
                token = self.vault.access_token()?;
            } else {
                token = current_token;
            }
            if token.is_empty() { return Err("Your CenAI session expired. Sign in again.".into()); }
            drop(_session_guard);
            response = send(&token).send().await
                .map_err(|error| format!("CenAI Voice retry failed: {error}"))?;
        }
        if !response.status().is_success() {
            let status = response.status();
            let raw = response.text().await.unwrap_or_default();
            let data: Value = serde_json::from_str(&raw).unwrap_or_else(|_| json!({}));
            let message = data.get("error").and_then(Value::as_str)
                .or_else(|| data.get("message").and_then(Value::as_str))
                .unwrap_or("Cloud voice service is unavailable.");
            return Err(format!("CenAI Voice HTTP {}: {}", status.as_u16(), message));
        }
        let content_type = response.headers().get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()).unwrap_or("");
        if !content_type.starts_with("audio/wav") && !content_type.starts_with("audio/x-wav") {
            return Err("CenAI Voice returned an unsupported audio format.".into());
        }
        if response.content_length().is_some_and(|length| length > MAX_AUDIO_BYTES as u64) {
            return Err("CenAI Voice audio exceeded the 32 MB safety limit.".into());
        }
        let bytes = response.bytes().await
            .map_err(|error| format!("Could not receive CenAI Voice audio: {error}"))?;
        if bytes.len() < 44 || bytes.len() > MAX_AUDIO_BYTES || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
            return Err("CenAI Voice returned an invalid or oversized WAV file.".into());
        }
        if self.vault.account_id()? != account_id {
            return Err("The active CenAI account changed before voice playback completed.".into());
        }
        Ok(bytes.to_vec())
    }

    async fn personal_memory_action(&self, action: &str, mut payload: Value) -> Result<Value, String> {
        let object = payload.as_object_mut().ok_or("Request payload must be an object")?;
        object.insert("action".into(), Value::String(action.to_owned()));
        self.post_authenticated(config::PERSONAL_MEMORY_URL, &payload).await
    }

    pub async fn control_plane(&self, action: &str, mut payload: Value) -> Result<Value, String> {
        let object = payload.as_object_mut().ok_or("Request payload must be an object")?;
        object.insert("action".into(), Value::String(action.to_owned()));
        self.post_authenticated(config::CONTROL_PLANE_URL, &payload).await
    }

    /// Confirms that the authenticated account is eligible under the Discord invitation policy.
    /// A failed check is not treated as permission to continue.
    pub async fn registration_status(&self) -> Result<bool, String> {
        let status = self.control_plane("registration_status", json!({})).await?;
        status.get("registered").and_then(Value::as_bool)
            .ok_or_else(|| "CenAI registration status response was invalid.".to_owned())
    }

    /// Calls the per-user Ecosystem Engine. The server derives the account identity from the JWT.
    pub async fn ecosystem_action(&self, action: &str, mut payload: Value) -> Result<Value, String> {
        if !matches!(action, "list" | "register" | "grant" | "revoke" | "authorize") {
            return Err("Unsupported Ecosystem Engine action".into());
        }
        let object = payload.as_object_mut().ok_or("Request payload must be an object")?;
        object.insert("action".into(), Value::String(action.to_owned()));
        self.post_authenticated(config::ECOSYSTEM_URL, &payload).await
    }

    /// Uploads an already encrypted workspace envelope to the caller's private Storage path.
    pub async fn upload_encrypted_backup(&self, encrypted_backup: Vec<u8>) -> Result<(), String> {
        const MAX_BACKUP_BYTES: usize = 100 * 1024 * 1024;
        if encrypted_backup.is_empty() || encrypted_backup.len() > MAX_BACKUP_BYTES {
            return Err("Encrypted backup is empty or exceeds the 100 MB limit".into());
        }
        let user_id = self.account_id()?;
        if !valid_supabase_user_id(&user_id) { return Err("A verified account is required for backup upload".into()); }
        let token = self.vault.access_token()?;
        if token.is_empty() { return Err("Sign in before uploading a backup".into()); }
        let url = format!("{}/storage/v1/object/{}/{}/workspace.cenai", config::SUPABASE_URL, config::BACKUP_BUCKET, user_id);
        let response = self.provider_client.put(url)
            .header("apikey", config::SUPABASE_PUBLISHABLE_KEY)
            .bearer_auth(token)
            .header("content-type", "application/json")
            .header("x-upsert", "true")
            .body(encrypted_backup)
            .send().await.map_err(|error| format!("Encrypted backup upload failed: {error}"))?;
        if !response.status().is_success() {
            return Err(format!("Encrypted backup upload returned HTTP {}", response.status().as_u16()));
        }
        Ok(())
    }

    /// Downloads only the current user's private encrypted backup object.
    pub async fn download_encrypted_backup(&self) -> Result<Vec<u8>, String> {
        const MAX_BACKUP_BYTES: usize = 100 * 1024 * 1024;
        let user_id = self.account_id()?;
        if !valid_supabase_user_id(&user_id) { return Err("A verified account is required for backup download".into()); }
        let token = self.vault.access_token()?;
        if token.is_empty() { return Err("Sign in before downloading a backup".into()); }
        let url = format!("{}/storage/v1/object/authenticated/{}/{}/workspace.cenai", config::SUPABASE_URL, config::BACKUP_BUCKET, user_id);
        let response = self.provider_client.get(url)
            .header("apikey", config::SUPABASE_PUBLISHABLE_KEY)
            .bearer_auth(token)
            .send().await.map_err(|error| format!("Encrypted backup download failed: {error}"))?;
        if !response.status().is_success() {
            return Err(format!("Encrypted backup download returned HTTP {}", response.status().as_u16()));
        }
        let bytes = response.bytes().await.map_err(|error| format!("Encrypted backup download failed: {error}"))?;
        if bytes.is_empty() || bytes.len() > MAX_BACKUP_BYTES {
            return Err("Downloaded backup is empty or exceeds the 100 MB limit".into());
        }
        Ok(bytes.to_vec())
    }

    pub async fn ecosystem_list(&self) -> Result<Value, String> {
        self.ecosystem_action("list", json!({})).await
    }

    pub async fn ecosystem_register_app(&self, manifest: Value) -> Result<Value, String> {
        self.ecosystem_action("register", manifest).await
    }

    pub async fn ecosystem_grant(&self, app_id: &str, scopes: &[String]) -> Result<Value, String> {
        self.ecosystem_action("grant", json!({ "appId": app_id, "scopes": scopes })).await
    }

    pub async fn ecosystem_revoke(&self, app_id: &str) -> Result<Value, String> {
        self.ecosystem_action("revoke", json!({ "appId": app_id })).await
    }

    pub async fn ecosystem_authorize(&self, app_id: &str, capability: &str) -> Result<bool, String> {
        let data = self.ecosystem_action("authorize", json!({ "appId": app_id, "capability": capability })).await?;
        Ok(data.get("authorized").and_then(Value::as_bool) == Some(true))
    }

    pub async fn billing_checkout(&self, plan: &str) -> Result<String, String> {
        if !matches!(plan, "supporter" | "pro") { return Err("Choose a supported CenAI plan.".into()); }
        let data = self.post_authenticated(config::BILLING_URL, &json!({ "plan": plan })).await?;
        let url = data.get("checkoutUrl").and_then(Value::as_str).unwrap_or_default();
        if url.starts_with("https://checkout.stripe.com/") {
            Ok(url.to_owned())
        } else {
            Err("The billing service did not return a valid checkout link.".into())
        }
    }

    pub async fn paypal_billing(&self, action: &str, mut payload: Value) -> Result<Value, String> {
        let object = payload.as_object_mut().ok_or("Request payload must be an object")?;
        object.insert("action".into(), Value::String(action.to_owned()));
        self.post_authenticated(config::PAYPAL_BILLING_URL, &payload).await
    }

    pub async fn telemetry(&self, payload: Value) -> Result<Value, String> {
        self.post_authenticated(config::TELEMETRY_URL, &payload).await
    }


    pub async fn stream_openai_compatible<F>(
        &self,
        endpoint: &str,
        api_key: &str,
        request: &ChatRequest,
        mut on_delta: F,
    ) -> Result<String, String>
    where
        F: FnMut(&str) + Send,
    {
        validate_provider_endpoint(endpoint)?;
        request.validate()?;
        if api_key.trim().is_empty() { return Err("Provider API key is missing.".into()); }
        let base = endpoint.trim_end_matches('/');
        let endpoint = if base.ends_with("/chat/completions") { base.to_owned() } else { format!("{base}/chat/completions") };
        let (provider_client, parsed_endpoint) = pinned_provider_client(&endpoint).await?;
        let url = parsed_endpoint.as_str();
        let payload = serde_json::json!({
            "model": request.model,
            "messages": request.messages,
            "max_tokens": request.max_tokens,
            "stream": true
        });
        let response = provider_client.post(url)
            .bearer_auth(api_key)
            .header(header::ACCEPT, "text/event-stream")
            .json(&payload)
            .send().await.map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Self::decode_response(response).await.map(|_| String::new());
        }
        let mut stream = response.bytes_stream();
        let mut pending = String::new();
        let mut answer = String::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| error.to_string())?;
            pending.push_str(&String::from_utf8_lossy(&chunk));
            if pending.len() > MAX_SSE_LINE_BYTES { return Err("AI stream contained an oversized event line.".into()); }
            while let Some(newline) = pending.find('\n') {
                let line = pending[..newline].trim_end_matches('\r').to_owned();
                pending.drain(..=newline);
                if let Some(delta) = parse_openai_sse_event(&line) {
                    append_stream_delta(&mut answer, &delta)?;
                    on_delta(&delta);
                }
            }
        }
        if !pending.trim().is_empty() {
            if let Some(delta) = parse_openai_sse_event(&pending) {
                append_stream_delta(&mut answer, &delta)?;
                on_delta(&delta);
            }
        }
        Ok(answer)
    }

    /// Reads only this authenticated account's saved memories. The server enforces owner scoping.
    pub async fn personal_memories(&self) -> Result<Vec<Value>, String> {
        let status = self.personal_memory_action("memory_status", json!({})).await?;
        if status.get("enabled").and_then(Value::as_bool) != Some(true) {
            return Ok(Vec::new());
        }
        let data = self.personal_memory_action("memory_list", json!({})).await?;
        Ok(data.get("memories").and_then(Value::as_array).cloned().unwrap_or_default())
    }

    pub async fn set_personal_memory_enabled(&self, enabled: bool) -> Result<(), String> {
        let data = self.personal_memory_action("memory_set_enabled", json!({ "enabled": enabled })).await?;
        if data.get("enabled").and_then(Value::as_bool) != Some(enabled) {
            return Err("The server did not confirm the personal-memory setting.".into());
        }
        Ok(())
    }

    pub async fn save_personal_memory(&self, category: &str, content: &str, source: &str) -> Result<Value, String> {
        if !matches!(category, "preference" | "profile" | "project" | "workflow" | "fact")
            || !matches!(source, "user" | "imported")
            || content.trim().is_empty() || content.len() > 8000 {
            return Err("Personal memory category, source or content is invalid.".into());
        }
        let data = self.personal_memory_action("memory_save", json!({
            "category": category, "content": content.trim(), "source": source
        })).await?;
        data.get("memory").cloned().ok_or_else(|| "The memory service returned no saved record.".into())
    }

    pub async fn clear_personal_memories(&self) -> Result<(), String> {
        let data = self.personal_memory_action("memory_clear", json!({})).await?;
        if data.get("cleared").and_then(Value::as_bool) != Some(true) {
            return Err("The memory service did not confirm clearing personal memories.".into());
        }
        Ok(())
    }

    pub async fn delete_personal_memory(&self, memory_id: &str) -> Result<(), String> {
        if !valid_supabase_user_id(memory_id) {
            return Err("Invalid personal-memory identifier.".into());
        }
        let data = self.personal_memory_action("memory_delete", json!({ "memoryId": memory_id })).await?;
        if data.get("deleted").and_then(Value::as_bool) != Some(true) {
            return Err("The memory service did not confirm deletion.".into());
        }
        Ok(())
    }

    fn mcp_token_slot(server_id: &str) -> Result<String, String> {
        if server_id.trim().is_empty() || server_id.len() > 128 {
            return Err("MCP server identifier is invalid.".into());
        }
        let digest = Sha256::digest(server_id.as_bytes());
        let suffix = digest[..12].iter().map(|byte| format!("{byte:02x}")).collect::<String>();
        Ok(format!("mcp_{suffix}"))
    }

    pub fn mcp_server_token(&self, server_id: &str) -> Result<String, String> {
        self.vault.provider_key(&Self::mcp_token_slot(server_id)?)
    }

    pub fn save_mcp_server_token(&self, server_id: &str, token: &str) -> Result<(), String> {
        if token.len() > 4096 { return Err("MCP bearer token exceeds the supported limit.".into()); }
        self.vault.save_provider_key(&Self::mcp_token_slot(server_id)?, token)
    }

    pub fn provider_key(&self, provider: &str) -> Result<String, String> {
        self.vault.provider_key(provider)
    }

    pub fn save_provider_key(&self, provider: &str, key: &str) -> Result<(), String> {
        self.vault.save_provider_key(provider, key)
    }

    /// Starts Google's installed-app OAuth flow. PKCE verifier and state are stored
    /// only in the account-scoped Android Keystore vault.
    pub fn begin_google_gemini_oauth(&self, client_id: &str) -> Result<String, String> {
        let request = crate::core::google_oauth::create_authorization_request(client_id)
            .map_err(str::to_owned)?;
        self.vault.save_provider_key("gemini_oauth_client", client_id.trim())?;
        self.vault.save_provider_key("gemini_oauth_state", &request.state)?;
        self.vault.save_provider_key("gemini_oauth_verifier", &request.verifier)?;
        Ok(request.url)
    }

    /// Exchanges a validated one-time callback code and stores tokens encrypted.
    pub async fn complete_google_gemini_oauth(&self, callback: &str) -> Result<(), String> {
        let account_id = self.vault.account_id()?;
        if account_id.is_empty() { return Err("Sign in to CenAI before connecting Google.".into()); }
        let state = self.vault.provider_key("gemini_oauth_state")?;
        let code = crate::core::google_oauth::validate_callback(callback, &state)
            .map_err(str::to_owned)?;
        let client_id = self.vault.provider_key("gemini_oauth_client")?;
        let verifier = self.vault.provider_key("gemini_oauth_verifier")?;
        if client_id.is_empty() || verifier.is_empty() {
            return Err("Google OAuth setup expired. Start the connection again.".into());
        }
        let response = self.provider_client.post("https://oauth2.googleapis.com/token")
            .form(&[
                ("client_id", client_id.as_str()),
                ("code", code.as_str()),
                ("code_verifier", verifier.as_str()),
                ("redirect_uri", crate::core::google_oauth::REDIRECT_URI),
                ("grant_type", "authorization_code"),
            ])
            .send().await.map_err(|error| format!("Google token exchange failed: {error}"))?;
        if !response.status().is_success() {
            let status = response.status();
            let _ = response.bytes().await;
            return Err(format!("Google token exchange failed (HTTP {}).", status.as_u16()));
        }
        let token: Value = response.json().await.map_err(|_| "Google returned an invalid token response")?;
        let access = token.get("access_token").and_then(Value::as_str).unwrap_or_default();
        let refresh = token.get("refresh_token").and_then(Value::as_str).unwrap_or_default();
        let expires = token.get("expires_in").and_then(Value::as_i64).unwrap_or(3600);
        if access.is_empty() || access.len() > 16 * 1024 || !(60..=86_400).contains(&expires) {
            return Err("Google returned an incomplete or invalid token response.".into());
        }
        if self.vault.account_id()? != account_id {
            return Err("The active CenAI account changed during Google sign-in; tokens were not stored.".into());
        }
        self.vault.save_provider_key("gemini_oauth_access", access)?;
        if !refresh.is_empty() {
            if refresh.len() > 16 * 1024 { return Err("Google refresh token exceeds the supported limit.".into()); }
            self.vault.save_provider_key("gemini_oauth_refresh", refresh)?;
        }
        self.vault.save_provider_key("gemini_oauth_expiry", &(chrono::Utc::now().timestamp() + expires).to_string())?;
        self.vault.save_provider_key("gemini_oauth_state", "")?;
        self.vault.save_provider_key("gemini_oauth_verifier", "")?;
        Ok(())
    }

    /// Returns a usable OAuth access token, refreshing it before expiry when possible.
    pub async fn google_gemini_access_token(&self) -> Result<String, String> {
        let account_id = self.vault.account_id()?;
        if account_id.is_empty() { return Err("Sign in to CenAI before using Google Gemini.".into()); }
        let access = self.vault.provider_key("gemini_oauth_access")?;
        let expiry = self.vault.provider_key("gemini_oauth_expiry")?.parse::<i64>().unwrap_or(0);
        if !access.is_empty() && expiry > chrono::Utc::now().timestamp() + 60 {
            return Ok(access);
        }
        let refresh = self.vault.provider_key("gemini_oauth_refresh")?;
        let client_id = self.vault.provider_key("gemini_oauth_client")?;
        if refresh.is_empty() || client_id.is_empty() {
            return Err("Connect a Google account to use Gemini OAuth.".into());
        }
        let response = self.provider_client.post("https://oauth2.googleapis.com/token")
            .form(&[
                ("client_id", client_id.as_str()),
                ("refresh_token", refresh.as_str()),
                ("grant_type", "refresh_token"),
            ])
            .send().await.map_err(|error| format!("Google token refresh failed: {error}"))?;
        if !response.status().is_success() {
            let status = response.status();
            let _ = response.bytes().await;
            return Err(format!("Google token refresh failed (HTTP {}). Reconnect the account if needed.", status.as_u16()));
        }
        let token: Value = response.json().await.map_err(|_| "Google returned an invalid refresh response")?;
        let next = token.get("access_token").and_then(Value::as_str).unwrap_or_default();
        let expires = token.get("expires_in").and_then(Value::as_i64).unwrap_or(3600);
        if next.is_empty() || next.len() > 16 * 1024 || !(60..=86_400).contains(&expires) {
            return Err("Google returned an incomplete or invalid refresh response.".into());
        }
        if self.vault.account_id()? != account_id {
            return Err("The active CenAI account changed during Google token refresh.".into());
        }
        self.vault.save_provider_key("gemini_oauth_access", next)?;
        self.vault.save_provider_key("gemini_oauth_expiry", &(chrono::Utc::now().timestamp() + expires).to_string())?;
        Ok(next.to_owned())
    }

    pub fn has_google_gemini_oauth(&self) -> bool {
        self.vault.provider_key("gemini_oauth_refresh").map(|value| !value.is_empty()).unwrap_or(false)
    }

    pub fn disconnect_google_gemini_oauth(&self) -> Result<(), String> {
        for key in ["gemini_oauth_state", "gemini_oauth_verifier", "gemini_oauth_access",
            "gemini_oauth_refresh", "gemini_oauth_expiry"] {
            self.vault.save_provider_key(key, "")?;
        }
        Ok(())
    }

    pub async fn cloud_chat(&self, payload: &Value) -> Result<Value, String> {
        self.post_authenticated(config::CLOUD_CHAT_URL, payload).await
    }

    /// Streams the managed CenAI Cloud response, refreshing the Supabase session once on 401.
    /// Accepts both OpenAI-compatible SSE and the existing non-streaming JSON response shape.
    pub async fn stream_cloud_chat<F>(&self, payload: &Value, mut on_delta: F) -> Result<String, String>
    where
        F: FnMut(&str) + Send,
    {
        let (mut token, account_id) = {
            let _session_guard = self.session_lock.lock().await;
            (self.vault.access_token()?, self.vault.account_id()?)
        };
        if token.is_empty() || account_id.is_empty() { return Err("Sign in to your CenAI account first.".into()); }
        let mut response = self.client.post(config::CLOUD_CHAT_URL)
            .header("apikey", config::SUPABASE_PUBLISHABLE_KEY)
            .bearer_auth(&token)
            .header(header::ACCEPT, "text/event-stream, application/json")
            .timeout(std::time::Duration::from_secs(config::STREAM_TIMEOUT_SECONDS))
            .json(payload)
            .send().await.map_err(|error| error.to_string())?;
        if response.status() == StatusCode::UNAUTHORIZED {
            let _session_guard = self.session_lock.lock().await;
            if self.vault.account_id()? != account_id {
                return Err("The active CenAI account changed while this stream was running.".into());
            }
            let current_token = self.vault.access_token()?;
            if current_token == token {
                self.refresh_session().await?;
                token = self.vault.access_token()?;
            } else {
                token = current_token;
            }
            if token.is_empty() { return Err("Your CenAI session expired. Sign in again.".into()); }
            drop(_session_guard);
            response = self.client.post(config::CLOUD_CHAT_URL)
                .header("apikey", config::SUPABASE_PUBLISHABLE_KEY)
                .bearer_auth(&token)
                .header(header::ACCEPT, "text/event-stream, application/json")
                .timeout(std::time::Duration::from_secs(config::STREAM_TIMEOUT_SECONDS))
                .json(payload)
                .send().await.map_err(|error| error.to_string())?;
        }
        if !response.status().is_success() {
            return Self::decode_response(response).await.map(|_| String::new());
        }

        let content_type = response.headers().get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()).unwrap_or_default().to_owned();
        if !content_type.contains("text/event-stream") {
            let data: Value = response.json().await.map_err(|error| error.to_string())?;
            let answer = data.pointer("/choices/0/message/content").and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or("CenAI Cloud returned an empty response.")?.to_owned();
            if self.vault.account_id()? != account_id {
                return Err("The active CenAI account changed before the response completed.".into());
            }
            on_delta(&answer);
            return Ok(answer);
        }

        let mut stream = response.bytes_stream();
        let mut pending = String::new();
        let mut answer = String::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| error.to_string())?;
            pending.push_str(&String::from_utf8_lossy(&chunk));
            if pending.len() > MAX_SSE_LINE_BYTES { return Err("AI stream contained an oversized event line.".into()); }
            while let Some(newline) = pending.find('\n') {
                let line = pending[..newline].trim_end_matches('\r').to_owned();
                pending.drain(..=newline);
                if let Some(delta) = parse_openai_sse_event(&line) {
                    append_stream_delta(&mut answer, &delta)?;
                    on_delta(&delta);
                }
            }
        }
        if !pending.trim().is_empty() {
            if let Some(delta) = parse_openai_sse_event(&pending) {
                append_stream_delta(&mut answer, &delta)?;
                on_delta(&delta);
            }
        }
        if self.vault.account_id()? != account_id {
            return Err("The active CenAI account changed before the stream completed.".into());
        }
        if answer.is_empty() {
            return Err("CenAI Cloud returned an empty or unsupported streaming response.".into());
        }
        Ok(answer)
    }
    /// Streams Anthropic Messages API text deltas using the account's locally stored API key.
    pub async fn stream_anthropic<F>(&self, request: &ChatRequest, api_key: &str, mut on_delta: F) -> Result<String, String>
    where
        F: FnMut(&str) + Send,
    {
        request.validate()?;
        if api_key.trim().is_empty() { return Err("Anthropic API key is missing.".into()); }
        let system = request.messages.iter().filter(|message| message.role == "system")
            .map(|message| message.content.as_str()).collect::<Vec<_>>().join("\n\n");
        let mut messages = Vec::new();
        for message in &request.messages {
            match message.role.as_str() {
                "system" => {}
                "user" | "assistant" => messages.push(json!({ "role": message.role, "content": message.content })),
                _ => return Err("Anthropic Messages API accepts only user and assistant history messages.".into()),
            }
        }
        if messages.is_empty() { return Err("Anthropic requires at least one user or assistant message.".into()); }
        let mut payload = json!({
            "model": request.model,
            "max_tokens": request.max_tokens,
            "messages": messages,
            "stream": true
        });
        if !system.is_empty() { payload["system"] = Value::String(system); }
        let response = self.provider_client.post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header(header::ACCEPT, "text/event-stream")
            .json(&payload)
            .send().await.map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Self::decode_response(response).await.map(|_| String::new());
        }
        let mut stream = response.bytes_stream();
        let mut pending = String::new();
        let mut answer = String::new();
        while let Some(chunk) = stream.next().await {
            pending.push_str(&String::from_utf8_lossy(&chunk.map_err(|error| error.to_string())?));
            if pending.len() > MAX_SSE_LINE_BYTES { return Err("AI stream contained an oversized event line.".into()); }
            while let Some(newline) = pending.find('\n') {
                let line = pending[..newline].trim_end_matches('\r').to_owned();
                pending.drain(..=newline);
                if let Some(delta) = parse_anthropic_sse_event(&line)? {
                    append_stream_delta(&mut answer, &delta)?;
                    on_delta(&delta);
                }
            }
        }
        if !pending.trim().is_empty() {
            if let Some(delta) = parse_anthropic_sse_event(&pending)? {
                append_stream_delta(&mut answer, &delta)?;
                on_delta(&delta);
            }
        }
        if answer.is_empty() { return Err("Anthropic returned no text content.".into()); }
        Ok(answer)
    }

    /// Streams Gemini Developer API responses without placing the API key in the URL.
    /// Analyzes a validated image, PDF, or text attachment through Gemini multimodal input.
    /// The caller must obtain the bytes through an explicit user attachment action.
    pub async fn analyze_attachment_gemini(
        &self,
        prompt: &str,
        metadata: &crate::core::multimodal::AttachmentMetadata,
        bytes: &[u8],
    ) -> Result<String, String> {
        if prompt.trim().is_empty() || prompt.len() > 8_000 {
            return Err("Attachment analysis prompt must contain between 1 and 8,000 characters".into());
        }
        metadata.validate_content(bytes).map_err(str::to_owned)?;
        let api_key = self.vault.provider_key("gemini")?;
        let oauth = api_key.trim().is_empty();
        let credential = if oauth { self.google_gemini_access_token().await? } else { api_key };
        let mime = metadata.mime_type.to_ascii_lowercase();
        let content_part = match mime.as_str() {
            "text/plain" | "text/markdown" | "text/csv" => {
                if bytes.len() > 2 * 1024 * 1024 {
                    return Err("Text attachment exceeds the 2 MB inline analysis limit".into());
                }
                let text = std::str::from_utf8(bytes).map_err(|_| "Text attachment is not valid UTF-8")?;
                json!({ "text": format!("Attachment: {}\n\n{}", metadata.file_name, text) })
            }
            "audio/wav" | "audio/x-wav" => return self.transcribe_audio_gemini(bytes).await,
            "image/png" | "image/jpeg" | "image/webp" | "application/pdf" => {
                if bytes.len() > 20 * 1024 * 1024 {
                    return Err("Image and PDF attachments must be no larger than 20 MB for inline analysis".into());
                }
                json!({ "inline_data": {
                    "mime_type": mime,
                    "data": base64::engine::general_purpose::STANDARD.encode(bytes)
                } })
            }
            _ => return Err("This attachment type is not yet supported by the Gemini inline analyzer".into()),
        };
        let payload = json!({
            "contents": [{
                "parts": [
                    { "text": format!("Attachment name: {}\nTask: {}", metadata.file_name, prompt) },
                    content_part
                ]
            }],
            "generationConfig": { "temperature": 0.2, "maxOutputTokens": 4096 }
        });
        let mut request = self.provider_client.post("https://generativelanguage.googleapis.com/v1beta/models/gemini-3.8-flash:generateContent")
            .json(&payload);
        request = if oauth { request.bearer_auth(&credential) } else { request.header("x-goog-api-key", &credential) };
        let response = request.send().await.map_err(|error| format!("Gemini attachment analysis failed: {error}"))?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!("Gemini attachment analysis returned HTTP {}", status.as_u16()));
        }
        let value: Value = response.json().await.map_err(|error| format!("Gemini attachment response was invalid: {error}"))?;
        let answer = value.pointer("/candidates/0/content/parts").and_then(Value::as_array)
            .into_iter().flatten().filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>().join("\n").trim().to_owned();
        if answer.is_empty() { return Err("Gemini returned no attachment analysis".into()); }
        if answer.len() > MAX_STREAM_RESPONSE_BYTES { return Err("Attachment analysis exceeded the response limit".into()); }
        Ok(answer)
    }

    /// Transcribes a user-initiated WAV audio clip through Gemini's multimodal endpoint.
    /// The API key is sent in a header, never embedded in the URL or returned errors.
    pub async fn transcribe_audio_gemini(&self, wav_bytes: &[u8]) -> Result<String, String> {
        const MAX_AUDIO_BYTES: usize = 20 * 1024 * 1024;
        if wav_bytes.len() < 44 || wav_bytes.len() > MAX_AUDIO_BYTES
            || &wav_bytes[..4] != b"RIFF" || &wav_bytes[8..12] != b"WAVE"
        {
            return Err("Voice audio must be a valid WAV file under 20 MB".into());
        }
        let api_key = self.vault.provider_key("gemini")?;
        let oauth = api_key.trim().is_empty();
        let credential = if oauth { self.google_gemini_access_token().await? } else { api_key };
        let audio = base64::engine::general_purpose::STANDARD.encode(wav_bytes);
        let payload = json!({
            "contents": [{
                "parts": [
                    { "inline_data": { "mime_type": "audio/wav", "data": audio } },
                    { "text": "Transcribe the spoken words accurately. Return only the transcription, without commentary." }
                ]
            }],
            "generationConfig": { "temperature": 0, "maxOutputTokens": 2048 }
        });
        let mut request = self.provider_client.post("https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent")
            .json(&payload);
        request = if oauth { request.bearer_auth(&credential) } else { request.header("x-goog-api-key", &credential) };
        let response = request.send().await.map_err(|error| format!("Gemini voice request failed: {error}"))?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!("Gemini voice transcription returned HTTP {}", status.as_u16()));
        }
        let value: Value = response.json().await.map_err(|error| format!("Gemini voice response was invalid: {error}"))?;
        let transcript = value.pointer("/candidates/0/content/parts")
            .and_then(Value::as_array)
            .into_iter().flatten()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>().join("\n").trim().to_owned();
        if transcript.is_empty() {
            return Err("Gemini returned no speech transcription".into());
        }
        if transcript.len() > 16_000 {
            return Err("Voice transcription exceeded the supported text limit".into());
        }
        Ok(transcript)
    }

    pub async fn stream_gemini<F>(&self, request: &ChatRequest, api_key: &str, on_delta: F) -> Result<String, String>
    where F: FnMut(&str) + Send {
        self.stream_gemini_with_auth(request, api_key, false, on_delta).await
    }

    pub async fn stream_gemini_oauth<F>(&self, request: &ChatRequest, access_token: &str, on_delta: F) -> Result<String, String>
    where F: FnMut(&str) + Send {
        self.stream_gemini_with_auth(request, access_token, true, on_delta).await
    }

    async fn stream_gemini_with_auth<F>(&self, request: &ChatRequest, credential: &str, oauth: bool, mut on_delta: F) -> Result<String, String>
    where F: FnMut(&str) + Send {

        request.validate()?;
        if credential.trim().is_empty() { return Err("Gemini API key is missing.".into()); }
        if request.model.is_empty() || !request.model.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-') {
            return Err("Gemini model identifier is invalid.".into());
        }
        let mut system_parts = Vec::new();
        let mut contents = Vec::new();
        for message in &request.messages {
            match message.role.as_str() {
                "system" => system_parts.push(message.content.clone()),
                "user" => contents.push(json!({ "role": "user", "parts": [{ "text": message.content }] })),
                "assistant" => contents.push(json!({ "role": "model", "parts": [{ "text": message.content }] })),
                _ => return Err("Gemini chat history contains an unsupported role.".into()),
            }
        }
        if contents.is_empty() { return Err("Gemini requires at least one user or assistant message.".into()); }
        let mut payload = json!({
            "contents": contents,
            "generationConfig": { "maxOutputTokens": request.max_tokens }
        });
        if !system_parts.is_empty() {
            payload["systemInstruction"] = json!({ "parts": [{ "text": system_parts.join("\n\n") }] });
        }
        let url = format!("https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?alt=sse", request.model);
        let mut request_builder = self.provider_client.post(url)
            .header(header::ACCEPT, "text/event-stream")
            .json(&payload);
        request_builder = if oauth {
            request_builder.bearer_auth(credential)
        } else {
            request_builder.header("x-goog-api-key", credential)
        };
        let response = request_builder.send().await.map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Self::decode_response(response).await.map(|_| String::new());
        }
        let mut stream = response.bytes_stream();
        let mut pending = String::new();
        let mut answer = String::new();
        while let Some(chunk) = stream.next().await {
            pending.push_str(&String::from_utf8_lossy(&chunk.map_err(|error| error.to_string())?));
            if pending.len() > MAX_SSE_LINE_BYTES { return Err("AI stream contained an oversized event line.".into()); }
            while let Some(newline) = pending.find('\n') {
                let line = pending[..newline].trim_end_matches('\r').to_owned();
                pending.drain(..=newline);
                if let Some(delta) = parse_gemini_sse_event(&line)? {
                    append_stream_delta(&mut answer, &delta)?;
                    on_delta(&delta);
                }
            }
        }
        if !pending.trim().is_empty() {
            if let Some(delta) = parse_gemini_sse_event(&pending)? {
                append_stream_delta(&mut answer, &delta)?;
                on_delta(&delta);
            }
        }
        if answer.is_empty() { return Err("Gemini returned no text content.".into()); }
        Ok(answer)
    }
}


fn parse_anthropic_sse_event(line: &str) -> Result<Option<String>, String> {
    let Some(data) = line.trim().strip_prefix("data:").map(str::trim) else { return Ok(None); };
    if data.is_empty() || data == "[DONE]" { return Ok(None); }
    let value: Value = serde_json::from_str(data).map_err(|error| format!("Invalid Anthropic SSE event: {error}"))?;
    if let Some(error) = value.get("error") {
        return Err(error.get("message").and_then(Value::as_str).unwrap_or("Anthropic stream error").to_owned());
    }
    if value.get("type").and_then(Value::as_str) == Some("content_block_delta")
        && value.pointer("/delta/type").and_then(Value::as_str) == Some("text_delta") {
        return Ok(value.pointer("/delta/text").and_then(Value::as_str).map(str::to_owned));
    }
    Ok(None)
}

fn parse_gemini_sse_event(line: &str) -> Result<Option<String>, String> {
    let Some(data) = line.trim().strip_prefix("data:").map(str::trim) else { return Ok(None); };
    if data.is_empty() || data == "[DONE]" { return Ok(None); }
    let value: Value = serde_json::from_str(data).map_err(|error| format!("Invalid Gemini SSE event: {error}"))?;
    if let Some(error) = value.get("error") {
        return Err(error.get("message").and_then(Value::as_str).unwrap_or("Gemini stream error").to_owned());
    }
    let text = value.pointer("/candidates/0/content/parts").and_then(Value::as_array)
        .map(|parts| parts.iter().filter_map(|part| part.get("text").and_then(Value::as_str)).collect::<String>());
    Ok(text.filter(|text| !text.is_empty()))
}

#[cfg(test)]
mod identity_tests {
    use super::{append_stream_delta, parse_anthropic_sse_event, parse_gemini_sse_event, valid_supabase_user_id, CenAiApi, MAX_STREAM_RESPONSE_BYTES};

    #[test]
    fn anthropic_stream_parser_extracts_only_text_deltas() {
        assert_eq!(
            parse_anthropic_sse_event(r#"data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"hello"}}"#).unwrap().as_deref(),
            Some("hello")
        );
        assert_eq!(
            parse_anthropic_sse_event(r#"data: {"type":"content_block_delta","delta":{"type":"input_json_delta","partial_json":"{}"}}"#).unwrap(),
            None
        );
        assert!(parse_anthropic_sse_event(r#"data: {"type":"error","error":{"message":"overloaded"}}"#).is_err());
    }

    #[test]
    fn gemini_stream_parser_extracts_candidate_text_and_errors() {
        assert_eq!(
            parse_gemini_sse_event(r#"data: {"candidates":[{"content":{"parts":[{"text":"hello"}]}}]}"#).unwrap().as_deref(),
            Some("hello")
        );
        assert_eq!(
            parse_gemini_sse_event(r#"data: {"candidates":[{"finishReason":"STOP"}]}"#).unwrap(),
            None
        );
        assert!(parse_gemini_sse_event(r#"data: {"error":{"message":"quota exceeded"}}"#).is_err());
    }

    #[test]
    fn mcp_token_slots_are_stable_and_do_not_expose_server_ids() {
        let first = CenAiApi::mcp_token_slot("org.example.private-tools").unwrap();
        let second = CenAiApi::mcp_token_slot("org.example.private-tools").unwrap();
        assert_eq!(first, second);
        assert!(first.starts_with("mcp_"));
        assert!(!first.contains("private-tools"));
        assert!(CenAiApi::mcp_token_slot("").is_err());
    }

    #[test]
    fn streamed_provider_output_is_bounded_without_partial_overflow() {
        let mut answer = String::new();
        append_stream_delta(&mut answer, "hello").unwrap();
        assert_eq!(answer, "hello");
        let overflow = "x".repeat(MAX_STREAM_RESPONSE_BYTES);
        assert!(append_stream_delta(&mut answer, &overflow).is_err());
        assert_eq!(answer, "hello");
    }

    #[test]
    fn account_scope_requires_a_supabase_uuid() {
        assert!(valid_supabase_user_id("550e8400-e29b-41d4-a716-446655440000"));
        assert!(!valid_supabase_user_id("user@example.com"));
        assert!(!valid_supabase_user_id("550e8400-e29b-41d4-a716-44665544000z"));
        assert!(!valid_supabase_user_id("550e8400e29b41d4a716446655440000"));
    }
}
#[cfg(test)]
mod assurance_claim_tests {
    use super::*;

    #[test]
    fn only_non_expired_aal2_jwt_claims_satisfy_step_up() {
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
            br#"{"aal":"aal2","exp":2000000000}"#
        );
        let token = format!("header.{payload}.signature");
        assert!(jwt_has_active_aal2(&token, 1_900_000_000));
        assert!(!jwt_has_active_aal2(&token, 2_000_000_000));
        let aal1 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
            br#"{"aal":"aal1","exp":2000000000}"#
        );
        assert!(!jwt_has_active_aal2(&format!("header.{aal1}.signature"), 1_900_000_000));
        assert!(!jwt_has_active_aal2("not-a-jwt", 1_900_000_000));
    }
}

