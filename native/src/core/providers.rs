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
        if self.model.trim().is_empty() {
            return Err("Model identifier is required");
        }
        if self.messages.is_empty() {
            return Err("At least one message is required");
        }
        if !(1..=32768).contains(&self.max_tokens) {
            return Err("Token limit is outside the supported range");
        }
        if self.messages.iter().any(|message| {
            !matches!(
                message.role.as_str(),
                "system" | "user" | "assistant" | "tool"
            ) || message.content.len() > 100_000
        }) {
            return Err("Message role or size is invalid");
        }
        Ok(())
    }
}

pub fn parse_openai_sse_event(line: &str) -> Option<String> {
    let data = line.trim().strip_prefix("data:")?.trim();
    if data.is_empty() || data == "[DONE]" {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(data).ok()?;
    value
        .pointer("/choices/0/delta/content")
        .or_else(|| value.get("response"))
        .or_else(|| value.get("generated_text"))
        .or_else(|| value.get("text"))?
        .as_str()
        .map(str::to_owned)
}

fn is_non_public_ip(address: std::net::IpAddr) -> bool {
    match address {
        std::net::IpAddr::V4(ip) => {
            let octets = ip.octets();
            ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_multicast()
                || ip.is_broadcast()
                || octets[0] == 0
                || octets[0] >= 240
                || (octets[0] == 100 && (64..=127).contains(&octets[1]))
                || (octets[0] == 192 && octets[1] == 0 && (octets[2] == 0 || octets[2] == 2))
                || (octets[0] == 198
                    && (octets[1] == 18
                        || octets[1] == 19
                        || (octets[1] == 51 && octets[2] == 100)))
                || (octets[0] == 203 && octets[1] == 0 && octets[2] == 113)
        }
        std::net::IpAddr::V6(ip) => {
            let segments = ip.segments();
            let octets = ip.octets();
            let mapped_v4 =
                segments[..5].iter().all(|segment| *segment == 0) && segments[5] == 0xffff;
            let compatible_v4 =
                segments[..6].iter().all(|segment| *segment == 0) && segments[6] != 0;
            if mapped_v4 || compatible_v4 {
                return is_non_public_ip(std::net::IpAddr::V4(std::net::Ipv4Addr::new(
                    octets[12], octets[13], octets[14], octets[15],
                )));
            }
            ip.is_loopback()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
                || ip.is_unspecified()
                || ip.is_multicast()
                || (segments[0] == 0x2001 && (segments[1] == 0x0db8 || segments[1] == 0x0002))
                || (segments[0] == 0x3fff && segments[1] & 0xf000 == 0)
                || (segments[0] & 0xffc0 == 0xfec0)
        }
    }
}

pub fn validate_provider_endpoint(endpoint: &str) -> Result<(), &'static str> {
    let trimmed = endpoint.trim();
    if trimmed.len() > 2048 || trimmed.is_empty() {
        return Err("Provider endpoint is invalid");
    }
    let parsed = reqwest::Url::parse(trimmed).map_err(|_| "Provider endpoint is invalid")?;
    if parsed.scheme() != "https" {
        return Err("Provider endpoints must use HTTPS");
    }
    if !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err("Provider endpoint cannot contain credentials, query parameters or fragments");
    }
    let host = parsed
        .host_str()
        .ok_or("Provider endpoint host is missing")?;
    let normalized = host.trim_end_matches('.').to_ascii_lowercase();
    if normalized == "localhost"
        || normalized.ends_with(".localhost")
        || normalized.ends_with(".local")
        || normalized.ends_with(".internal")
    {
        return Err("Provider endpoint cannot target a local or internal hostname");
    }
    let ip_literal = normalized.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = ip_literal.parse::<std::net::IpAddr>() {
        if is_non_public_ip(ip) {
            return Err("Provider endpoint cannot target a private, reserved or local IP address");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_request_rejects_invalid_roles_and_empty_models() {
        let mut request = ChatRequest {
            model: "model".into(),
            messages: vec![ChatMessage {
                role: "user".into(),
                content: "Hi".into(),
            }],
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
    fn custom_provider_endpoint_requires_safe_https_origin() {
        assert!(validate_provider_endpoint("https://api.example.com/v1").is_ok());
        assert!(validate_provider_endpoint("http://api.example.com").is_err());
        assert!(validate_provider_endpoint("https://localhost/v1").is_err());
        assert!(validate_provider_endpoint("https://127.0.0.1/v1").is_err());
        assert!(validate_provider_endpoint("https://10.0.0.8/v1").is_err());
        // Block non-routable and special-use literals before any network request.
        for address in [
            "0.0.0.1",
            "100.64.0.1",
            "192.0.2.1",
            "198.18.0.1",
            "198.51.100.1",
            "203.0.113.1",
            "224.0.0.1",
            "240.0.0.1",
            "[::]",
            "[ff02::1]",
            "[fc00::1]",
            "[fe80::1]",
            "[::ffff:127.0.0.1]",
            "[::ffff:10.0.0.1]",
            "[::ffff:192.0.2.1]",
        ] {
            assert!(
                validate_provider_endpoint(&format!("https://{address}/v1")).is_err(),
                "{address}"
            );
        }
        assert!(validate_provider_endpoint("https://8.8.8.8/v1").is_ok());
        assert!(validate_provider_endpoint("https://[2606:4700:4700::1111]/v1").is_ok());
        assert!(validate_provider_endpoint("https://[::1]/v1").is_err());
        assert!(validate_provider_endpoint("https://user:pass@api.example.com/v1").is_err());
        assert!(validate_provider_endpoint("https://api.example.com/v1?key=secret").is_err());
        assert!(validate_provider_endpoint("https://api.example.com/v1#fragment").is_err());
    }

    #[test]
    fn openai_sse_parser_extracts_text_deltas() {
        assert_eq!(
            parse_openai_sse_event(r#"data: {"choices":[{"delta":{"content":"hello"}}]}"#)
                .as_deref(),
            Some("hello")
        );
        assert_eq!(
            parse_openai_sse_event(r#"data: {"response":"managed chunk"}"#).as_deref(),
            Some("managed chunk")
        );
        assert_eq!(parse_openai_sse_event("data: [DONE]"), None);
    }
}
