//! Provider-neutral conversation contracts and safe endpoint validation.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    CenAiCloud,
    OpenAiCompatible,
    Anthropic,
    Gemini,
    Custom,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: u32,
    pub stream: bool,
}

impl ChatRequest {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.model.trim().is_empty() { return Err("Model identifier is required"); }
        if self.messages.is_empty() { return Err("At least one message is required"); }
        if !(1..=32768).contains(&self.max_tokens) { return Err("Token limit is outside the supported range"); }
        if self.messages.iter().any(|message| {
            !matches!(message.role.as_str(), "system" | "user" | "assistant" | "tool")
                || message.content.len() > 100_000
        }) {
            return Err("Message role or size is invalid");
        }
        Ok(())
    }
}

pub fn parse_openai_sse_event(line: &str) -> Option<String> {
    let data = line.trim().strip_prefix("data:")?.trim();
    if data.is_empty() || data == "[DONE]" { return None; }
    let value: serde_json::Value = serde_json::from_str(data).ok()?;
    value.pointer("/choices/0/delta/content")?.as_str().map(str::to_owned)
}

pub fn validate_provider_endpoint(endpoint: &str) -> Result<(), &'static str> {
    let trimmed = endpoint.trim();
    if !trimmed.starts_with("https://") { return Err("Provider endpoints must use HTTPS"); }
    if trimmed.len() > 2048 || trimmed.contains('@') { return Err("Provider endpoint is invalid"); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_request_rejects_invalid_roles_and_empty_models() {
        let mut request = ChatRequest {
            model: "model".into(),
            messages: vec![ChatMessage { role: "user".into(), content: "Hi".into() }],
            max_tokens: 1024,
            stream: true,
        };
        assert!(request.validate().is_ok());
        request.messages[0].role = "root".into();
        assert!(request.validate().is_err());
        request.messages[0].role = "user".into();
        request.model.clear();
        assert!(request.validate().is_err());
    }

    #[test]
    fn custom_provider_endpoint_requires_tls() {
        assert!(validate_provider_endpoint("https://api.example.com/v1").is_ok());
        assert!(validate_provider_endpoint("http://api.example.com").is_err());
    }

    #[test]
    fn openai_sse_parser_extracts_text_deltas() {
        assert_eq!(parse_openai_sse_event(r#"data: {"choices":[{"delta":{"content":"hello"}}]}"#).as_deref(), Some("hello"));
        assert_eq!(parse_openai_sse_event("data: [DONE]"), None);
    }
}
