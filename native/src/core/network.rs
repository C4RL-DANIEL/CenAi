//! Authenticated access to the existing Supabase and CenAI Cloud endpoints.
use std::sync::Arc;

use futures_util::StreamExt;
use reqwest::{header, Client, StatusCode};
use serde_json::{json, Value};

use crate::core::{config, providers::{parse_openai_sse_event, validate_provider_endpoint, ChatRequest}};

pub trait SessionVault: Send + Sync {
    fn access_token(&self) -> Result<String, String>;
    fn refresh_token(&self) -> Result<String, String>;
    fn account_email(&self) -> Result<String, String>;
    fn save_session(&self, access: &str, refresh: &str, email: &str) -> Result<(), String>;
    fn clear_session(&self) -> Result<(), String>;
    fn provider_key(&self, provider: &str) -> Result<String, String>;
    fn save_provider_key(&self, provider: &str, key: &str) -> Result<(), String>;
}

#[derive(Clone)]
pub struct CenAiApi {
    client: Client,
    vault: Arc<dyn SessionVault>,
}

impl CenAiApi {
    pub fn new(vault: Arc<dyn SessionVault>) -> Result<Self, reqwest::Error> {
        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(20))
            .timeout(std::time::Duration::from_secs(config::REQUEST_TIMEOUT_SECONDS))
            .user_agent(concat!("CenAI/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self { client, vault })
    }

    pub fn session_email(&self) -> Result<String, String> {
        self.vault.account_email()
    }

    pub fn has_session(&self) -> bool {
        self.vault.access_token().map(|token| !token.is_empty()).unwrap_or(false)
    }

    pub async fn sign_in(&self, email: &str, password: &str) -> Result<String, String> {
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
        if access.is_empty() || refresh.is_empty() {
            return Err("The identity service returned an incomplete session.".into());
        }
        self.vault.save_session(access, refresh, &account_email)?;
        Ok(account_email)
    }

    pub async fn sign_out(&self) -> Result<(), String> {
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
        if access.is_empty() || next_refresh.is_empty() {
            return Err("The identity service returned an incomplete session.".into());
        }
        self.vault.save_session(access, next_refresh, &email)
    }

    async fn post_authenticated(&self, url: &str, body: &Value) -> Result<Value, String> {
        let mut token = self.vault.access_token()?;
        if token.is_empty() { return Err("Sign in to your CenAI account first.".into()); }
        let mut response = self.client.post(url)
            .header("apikey", config::SUPABASE_PUBLISHABLE_KEY)
            .bearer_auth(&token)
            .header(header::ACCEPT, "application/json")
            .json(body)
            .send().await.map_err(|error| error.to_string())?;
        if response.status() == StatusCode::UNAUTHORIZED {
            self.refresh_session().await?;
            token = self.vault.access_token()?;
            response = self.client.post(url)
                .header("apikey", config::SUPABASE_PUBLISHABLE_KEY)
                .bearer_auth(token)
                .header(header::ACCEPT, "application/json")
                .json(body)
                .send().await.map_err(|error| error.to_string())?;
        }
        Self::decode_response(response).await
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
        let url = if base.ends_with("/chat/completions") { base.to_owned() } else { format!("{base}/chat/completions") };
        let payload = serde_json::json!({
            "model": request.model,
            "messages": request.messages,
            "max_tokens": request.max_tokens,
            "stream": true
        });
        let response = self.client.post(url)
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
            while let Some(newline) = pending.find('\n') {
                let line = pending[..newline].trim_end_matches('\r').to_owned();
                pending.drain(..=newline);
                if let Some(delta) = parse_openai_sse_event(&line) {
                    answer.push_str(&delta);
                    on_delta(&delta);
                }
            }
        }
        if !pending.trim().is_empty() {
            if let Some(delta) = parse_openai_sse_event(&pending) {
                answer.push_str(&delta);
                on_delta(&delta);
            }
        }
        Ok(answer)
    }

    pub fn provider_key(&self, provider: &str) -> Result<String, String> {
        self.vault.provider_key(provider)
    }

    pub fn save_provider_key(&self, provider: &str, key: &str) -> Result<(), String> {
        self.vault.save_provider_key(provider, key)
    }

    pub async fn cloud_chat(&self, payload: &Value) -> Result<Value, String> {
        let token = self.vault.access_token()?;
        if token.is_empty() { return Err("Sign in to your CenAI account first.".into()); }
        let response = self.client.post(config::CLOUD_CHAT_URL)
            .bearer_auth(token)
            .header(header::ACCEPT, "application/json")
            .json(payload)
            .send().await.map_err(|error| error.to_string())?;
        Self::decode_response(response).await
    }

    pub async fn stream_cloud_chat<F>(&self, payload: &Value, mut on_delta: F) -> Result<String, String>
    where
        F: FnMut(&str) + Send,
    {
        let token = self.vault.access_token()?;
        if token.is_empty() { return Err("Sign in to your CenAI account first.".into()); }
        let response = self.client.post(config::CLOUD_CHAT_URL)
            .bearer_auth(token)
            .timeout(std::time::Duration::from_secs(config::STREAM_TIMEOUT_SECONDS))
            .json(payload)
            .send().await.map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Self::decode_response(response).await.map(|_| String::new());
        }
        let data: Value = response.json().await.map_err(|error| error.to_string())?;
        let answer = data.pointer("/choices/0/message/content").and_then(Value::as_str).unwrap_or_default();
        if answer.is_empty() { return Err("CenAI Cloud returned an empty response.".into()); }
        on_delta(answer);
        Ok(answer.to_owned())
    }
}
