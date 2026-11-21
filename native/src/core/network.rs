//! Authenticated access to the existing Supabase and CenAI Cloud endpoints.
use std::sync::Arc;

use futures_util::StreamExt;
use reqwest::{header, Client, StatusCode};
use sha2::{Digest, Sha256};
use serde_json::{json, Value};

use crate::core::{config, providers::{parse_openai_sse_event, validate_provider_endpoint, ChatRequest}};

const MAX_STREAM_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const MAX_SSE_LINE_BYTES: usize = 256 * 1024;

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
}

#[derive(Clone)]
pub struct CenAiApi {
    client: Client,
    provider_client: Client,
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
        // Never forward provider credentials through redirects to another origin.
        let provider_client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(20))
            .timeout(std::time::Duration::from_secs(config::STREAM_TIMEOUT_SECONDS))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("CenAI/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self { client, provider_client, vault, session_lock: Arc::new(tokio::sync::Mutex::new(())) })
    }

    pub fn session_email(&self) -> Result<String, String> {
        self.vault.account_email()
    }

    /// Returns the immutable account UUID received from Supabase, never an email-derived identity.
    pub fn account_id(&self) -> Result<String, String> {
        self.vault.account_id()
    }

    pub fn has_session(&self) -> bool {
        self.vault.access_token().map(|token| !token.is_empty()).unwrap_or(false)
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
    pub async fn stream_gemini<F>(&self, request: &ChatRequest, api_key: &str, mut on_delta: F) -> Result<String, String>
    where
        F: FnMut(&str) + Send,
    {
        request.validate()?;
        if api_key.trim().is_empty() { return Err("Gemini API key is missing.".into()); }
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
        let response = self.provider_client.post(url)
            .header("x-goog-api-key", api_key)
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
