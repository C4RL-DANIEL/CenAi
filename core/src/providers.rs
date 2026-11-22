//! Provider-neutral message transformation and provider payload construction.

use serde_json::{json, Value};

pub fn build_provider_payload(
    provider: &str,
    model: &str,
    max_tokens: i32,
    messages_json: &str,
    custom_params_json: &str,
) -> Result<String, String> {
    if model.trim().is_empty() {
        return Err("Enter a model ID in Settings.".to_owned());
    }
    let parsed: Value = serde_json::from_str(messages_json)
        .map_err(|_| "Conversation messages could not be decoded.".to_owned())?;
    let messages = parsed.as_array()
        .ok_or_else(|| "Conversation messages must be a JSON array.".to_owned())?;
    if messages.is_empty() {
        return Err("A conversation must contain at least one message.".to_owned());
    }
    let mut normalized = Vec::with_capacity(messages.len());
    for message in messages {
        let role = message.get("role").and_then(Value::as_str)
            .ok_or_else(|| "A conversation message is missing its role.".to_owned())?;
        if !matches!(role, "system" | "user" | "assistant" | "tool") {
            return Err(format!("Unsupported conversation role: {role}"));
        }
        let content = message.get("content").and_then(Value::as_str)
            .ok_or_else(|| "A conversation message is missing text content.".to_owned())?;
        normalized.push((role.to_owned(), sanitize_content(content)));
    }

    let system = normalized.iter().find(|(role, _)| role == "system")
        .map(|(_, content)| content.clone()).unwrap_or_default();
    let conversation: Vec<_> = normalized.iter().filter(|(role, _)| role != "system").collect();
    let max_tokens = max_tokens.clamp(1, 131072);

    let payload = match provider {
        "CENAI_CLOUD" => {
            if normalized.iter().any(|(_, content)| !vision_parts(content).iter().all(|(kind, _)| kind == "text")) {
                return Err("CenAI Cloud currently supports text-only chat. Choose another provider for image attachments.".to_owned());
            }
            let mut list = Vec::new();
            let mut remaining = 78_000usize;
            for (role, content) in normalized.iter().rev().take(50).collect::<Vec<_>>().into_iter().rev() {
                if remaining == 0 { break; }
                let excerpt: String = content.chars().take(remaining.min(20_000)).collect();
                if !excerpt.trim().is_empty() {
                    list.push(json!({
                        "role": if role == "tool" { "assistant" } else { role },
                        "content": excerpt
                    }));
                    remaining = remaining.saturating_sub(excerpt.chars().count());
                }
            }
            json!({
                "model": model,
                "stream": false,
                "max_tokens": max_tokens.clamp(1, 1024),
                "messages": list
            })
        }
        "CLAUDE" => {
            let mut list = Vec::new();
            for (role, content) in &conversation {
                let parts = vision_parts(content);
                let blocks: Vec<Value> = parts.iter().map(|(kind, value)| {
                    if kind == "text" {
                        json!({"type":"text","text":value})
                    } else {
                        json!({"type":"image","source":{"type":"base64","media_type":kind,"data":value}})
                    }
                }).collect();
                let content_value = if blocks.len() == 1 && blocks[0]["type"] == "text" {
                    Value::String(content.clone())
                } else {
                    Value::Array(blocks)
                };
                list.push(json!({"role":role,"content":content_value}));
            }
            let mut payload = json!({
                "model": model,
                "max_tokens": max_tokens,
                "stream": true,
                "messages": list
            });
            if !system.is_blank() {
                payload["system"] = Value::String(system);
            }
            payload
        }
        "GEMINI" => {
            let mut contents = Vec::new();
            for (role, content) in &conversation {
                let parts: Vec<Value> = vision_parts(content).iter().map(|(kind, value)| {
                    if kind == "text" {
                        json!({"text":value})
                    } else {
                        json!({"inline_data":{"mime_type":kind,"data":value}})
                    }
                }).collect();
                contents.push(json!({
                    "role": if role == "assistant" { "model" } else { "user" },
                    "parts": parts
                }));
            }
            let mut payload = json!({
                "contents": contents,
                "generationConfig": {"maxOutputTokens": max_tokens}
            });
            if !system.is_blank() {
                payload["systemInstruction"] = json!({"parts":[{"text":system}]});
            }
            payload
        }
        "OPENAI" | "DEEPSEEK" | "CUSTOM" => {
            let mut list = Vec::new();
            for (role, content) in &normalized {
                let parts = vision_parts(content);
                let has_image = parts.iter().any(|(kind, _)| kind != "text");
                let content_value = if !has_image {
                    Value::String(content.clone())
                } else {
                    let blocks: Vec<Value> = parts.iter().map(|(kind, value)| {
                        if kind == "text" {
                            json!({"type":"text","text":value})
                        } else {
                            json!({"type":"image_url","image_url":{"url":format!("data:{kind};base64,{value}")}})
                        }
                    }).collect();
                    Value::Array(blocks)
                };
                list.push(json!({"role":role,"content":content_value}));
            }
            let mut payload = json!({
                "model": model,
                "stream": true,
                "max_tokens": max_tokens,
                "messages": list
            });
            if provider == "CUSTOM" {
                let custom: Value = serde_json::from_str(custom_params_json)
                    .map_err(|_| "Custom parameters must be valid JSON.".to_owned())?;
                let object = custom.as_object()
                    .ok_or_else(|| "Custom parameters must be a JSON object.".to_owned())?;
                for (key, value) in object {
                    if !matches!(key.as_str(), "model" | "messages" | "stream") {
                        payload[key] = value.clone();
                    }
                }
            }
            payload
        }
        _ => return Err("Unsupported AI provider.".to_owned()),
    };
    serde_json::to_string(&payload).map_err(|_| "Could not encode the provider request.".to_owned())
}

fn sanitize_content(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            if chars.peek() == Some(&'\n') { chars.next(); }
            output.push('\n');
        } else if !ch.is_control() || ch == '\n' || ch == '\t' {
            output.push(ch);
        }
    }
    output
}

fn vision_parts(content: &str) -> Vec<(String, String)> {
    const PREFIX: &str = "[[CENAI_IMAGE|";
    let mut result = Vec::new();
    let mut cursor = 0usize;
    while let Some(relative) = content[cursor..].find(PREFIX) {
        let start = cursor + relative;
        let payload_start = start + PREFIX.len();
        let Some(close_relative) = content[payload_start..].find("]]") else { break; };
        let close = payload_start + close_relative;
        let raw = &content[payload_start..close];
        let mut fields = raw.split('|');
        let mime = fields.next().unwrap_or("");
        let data = fields.next().unwrap_or("");
        let valid = fields.next().is_none()
            && !mime.is_empty()
            && mime.len() <= 64
            && data.len() > 16
            && data.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '/' | '='));
        if valid {
            let before = &content[cursor..start];
            if !before.trim().is_empty() { result.push(("text".to_owned(), before.to_owned())); }
            result.push((mime.to_owned(), data.to_owned()));
            cursor = close + 2;
        } else {
            cursor = payload_start;
        }
    }
    let tail = &content[cursor..];
    if !tail.trim().is_empty() || result.is_empty() {
        result.push(("text".to_owned(), tail.to_owned()));
    }
    result
}

trait Blank {
    fn is_blank(&self) -> bool;
}
impl Blank for String {
    fn is_blank(&self) -> bool { self.trim().is_empty() }
}

#[cfg(test)]
mod tests {
    use super::build_provider_payload;
    use serde_json::Value;

    fn messages() -> String {
        r#"[{"role":"system","content":"Be helpful"},{"role":"user","content":"Hello"},{"role":"assistant","content":"Hi"}]"#.to_owned()
    }

    #[test]
    fn builds_openai_and_custom_payloads() {
        let openai = build_provider_payload("OPENAI", "gpt-test", 100, &messages(), "{}").unwrap();
        let value: Value = serde_json::from_str(&openai).unwrap();
        assert_eq!(value["messages"].as_array().unwrap().len(), 3);
        let custom = build_provider_payload("CUSTOM", "local", 100, &messages(), r#"{"temperature":0.2,"stream":false}"#).unwrap();
        let value: Value = serde_json::from_str(&custom).unwrap();
        assert_eq!(value["temperature"], 0.2);
        assert_eq!(value["stream"], true);
    }

    #[test]
    fn builds_claude_and_gemini_system_fields() {
        let claude = build_provider_payload("CLAUDE", "claude-test", 200, &messages(), "{}").unwrap();
        let value: Value = serde_json::from_str(&claude).unwrap();
        assert_eq!(value["system"], "Be helpful");
        let gemini = build_provider_payload("GEMINI", "gemini-test", 200, &messages(), "{}").unwrap();
        let value: Value = serde_json::from_str(&gemini).unwrap();
        assert_eq!(value["systemInstruction"]["parts"][0]["text"], "Be helpful");
        assert_eq!(value["contents"][1]["role"], "model");
    }

    #[test]
    fn rejects_cloud_images_and_unknown_providers() {
        let image = r#"[{"role":"user","content":"[[CENAI_IMAGE|image/png|YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXo=]]"}]"#;
        assert!(build_provider_payload("CENAI_CLOUD", "cloud-model", 100, image, "{}").is_err());
        assert!(build_provider_payload("UNKNOWN", "model", 100, &messages(), "{}").is_err());
    }

    #[test]
    fn maps_image_markers_for_openai_claude_and_gemini() {
        let image = r#"[{"role":"user","content":"Describe this [[CENAI_IMAGE|image/png|YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXo=]]"}]"#;
        let openai = build_provider_payload("OPENAI", "gpt-vision", 100, image, "{}").unwrap();
        let value: Value = serde_json::from_str(&openai).unwrap();
        assert!(value["messages"][0]["content"][1]["image_url"]["url"]
            .as_str().unwrap().starts_with("data:image/png;base64,"));

        let claude = build_provider_payload("CLAUDE", "claude-vision", 100, image, "{}").unwrap();
        let value: Value = serde_json::from_str(&claude).unwrap();
        assert_eq!(value["messages"][0]["content"][1]["source"]["media_type"], "image/png");

        let gemini = build_provider_payload("GEMINI", "gemini-vision", 100, image, "{}").unwrap();
        let value: Value = serde_json::from_str(&gemini).unwrap();
        assert_eq!(value["contents"][0]["parts"][1]["inline_data"]["mime_type"], "image/png");
    }

}


/// Parses one server-sent event payload into a provider-neutral text delta.
pub fn parse_provider_delta(provider: &str, raw: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(raw) else {
        return String::new();
    };
    let delta = match provider {
        "CLAUDE" => {
            if value.get("type").and_then(Value::as_str) == Some("content_block_delta") {
                value.get("delta").and_then(|v| v.get("text")).and_then(Value::as_str).unwrap_or("").to_owned()
            } else {
                String::new()
            }
        }
        "GEMINI" => {
            let Some(parts) = value.get("candidates").and_then(|v| v.get(0))
                .and_then(|v| v.get("content")).and_then(|v| v.get("parts"))
                .and_then(Value::as_array) else { return String::new(); };
            parts.iter().filter_map(|part| part.get("text").and_then(Value::as_str)).collect::<String>()
        }
        _ => value.get("choices").and_then(|v| v.get(0))
            .and_then(|v| v.get("delta")).and_then(|v| v.get("content"))
            .and_then(Value::as_str).unwrap_or("").to_owned(),
    };
    if delta == "null" { String::new() } else { delta }
}

#[cfg(test)]
mod stream_tests {
    use super::parse_provider_delta;

    #[test]
    fn parses_provider_stream_deltas() {
        assert_eq!(
            parse_provider_delta("CLAUDE", r#"{"type":"content_block_delta","delta":{"text":"hello"}}"#),
            "hello"
        );
        assert_eq!(
            parse_provider_delta("GEMINI", r#"{"candidates":[{"content":{"parts":[{"text":"a"},{"text":"b"}]}}]}"#),
            "ab"
        );
        assert_eq!(
            parse_provider_delta("OPENAI", r#"{"choices":[{"delta":{"content":"world"}}]}"#),
            "world"
        );
        assert_eq!(parse_provider_delta("OPENAI", "not json"), "");
    }
}


/// Builds the HTTP transport configuration for a provider; network I/O stays on Android.
pub fn build_provider_request_config(
    provider: &str,
    base_url_or_model: &str,
    api_key: &str,
    oauth: bool,
    project_id: &str,
    custom_headers_json: &str,
) -> Result<String, String> {
    use serde_json::{json, Map, Value};

    let mut headers = Map::new();
    headers.insert("Accept".to_owned(), Value::String("text/event-stream".to_owned()));
    headers.insert("Content-Type".to_owned(), Value::String("application/json".to_owned()));

    let url = match provider {
        "CENAI_CLOUD" => {
            headers.insert("Authorization".to_owned(), Value::String(format!("Bearer {api_key}")));
            "https://cenai-website.aglibotcarldaniel.workers.dev/api/ai/chat/completions".to_owned()
        }
        "CLAUDE" => {
            headers.insert("x-api-key".to_owned(), Value::String(api_key.to_owned()));
            headers.insert("anthropic-version".to_owned(), Value::String("2023-06-01".to_owned()));
            "https://api.anthropic.com/v1/messages".to_owned()
        }
        "GEMINI" => {
            if oauth {
                headers.insert("Authorization".to_owned(), Value::String(format!("Bearer {api_key}")));
                if !project_id.trim().is_empty() {
                    headers.insert("x-goog-user-project".to_owned(), Value::String(project_id.to_owned()));
                }
            } else {
                headers.insert("x-goog-api-key".to_owned(), Value::String(api_key.to_owned()));
            }
            if base_url_or_model.is_empty() || !base_url_or_model.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.')) {
                return Err("Gemini model ID contains unsupported characters.".to_owned());
            }
            format!("https://generativelanguage.googleapis.com/v1beta/models/{base_url_or_model}:streamGenerateContent?alt=sse")
        }
        "OPENAI" | "DEEPSEEK" | "CUSTOM" => {
            if !base_url_or_model.starts_with("https://") {
                return Err("API URL must use HTTPS.".to_owned());
            }
            headers.insert("Authorization".to_owned(), Value::String(format!("Bearer {api_key}")));
            format!("{}/chat/completions", base_url_or_model.trim_end_matches('/'))
        }
        _ => return Err("Unsupported AI provider.".to_owned()),
    };

    if matches!(provider, "CUSTOM" | "CLAUDE") {
        let custom: Value = serde_json::from_str(custom_headers_json)
            .map_err(|_| "Custom headers must be valid JSON.".to_owned())?;
        let object = custom.as_object()
            .ok_or_else(|| "Custom headers must be a JSON object.".to_owned())?;
        for (name, value) in object {
            let Some(value) = value.as_str() else { continue; };
            let lower = name.to_ascii_lowercase();
            if matches!(lower.as_str(), "host" | "content-length" | "connection" | "authorization" | "x-api-key" | "x-goog-api-key") {
                continue;
            }
            if name.is_empty() || name.len() > 128
                || !name.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_|~".contains(&byte))
                || value.chars().any(|ch| ch.is_control())
            {
                continue;
            }
            headers.insert(name.clone(), Value::String(value.to_owned()));
        }
    }
    serde_json::to_string(&json!({"url":url,"headers":headers}))
        .map_err(|_| "Could not encode provider transport configuration.".to_owned())
}


#[cfg(test)]
mod transport_tests {
    use super::build_provider_request_config;
    use serde_json::Value;

    #[test]
    fn builds_secure_provider_transport_configurations() {
        let claude: Value = serde_json::from_str(
            &build_provider_request_config("CLAUDE", "", "secret", false, "", "{}").unwrap()
        ).unwrap();
        assert_eq!(claude["url"], "https://api.anthropic.com/v1/messages");
        assert_eq!(claude["headers"]["x-api-key"], "secret");

        let gemini: Value = serde_json::from_str(
            &build_provider_request_config("GEMINI", "gemini-test", "key", false, "", "{}").unwrap()
        ).unwrap();
        assert_eq!(gemini["headers"]["x-goog-api-key"], "key");
        assert!(gemini["url"].as_str().unwrap().contains("gemini-test"));

        assert!(build_provider_request_config("CUSTOM", "http://unsafe.local", "key", false, "", "{}").is_err());
    }

    #[test]
    fn custom_headers_cannot_override_auth_or_inject_control_chars() {
        let config: Value = serde_json::from_str(
            &build_provider_request_config(
                "CUSTOM", "https://example.com/v1", "key", false, "",
                r#"{"x-extra":"ok","Authorization":"Bearer other","x-bad":"line\nbreak"}"#
            ).unwrap()
        ).unwrap();
        assert_eq!(config["headers"]["x-extra"], "ok");
        assert_eq!(config["headers"]["Authorization"], "Bearer key");
        assert!(config["headers"].get("x-bad").is_none());
    }
}


/// Produces a bounded cross-provider synthesis prompt from independent responses.
pub fn build_synthesis_prompt(user_request: &str, answers_json: &str) -> Result<String, String> {
    let answers: Value = serde_json::from_str(answers_json)
        .map_err(|_| "Provider answers must be a JSON array.".to_owned())?;
    let answers = answers.as_array()
        .ok_or_else(|| "Provider answers must be a JSON array.".to_owned())?;
    let mut blocks = Vec::new();
    let mut remaining = 32_000usize;
    for answer in answers.iter().take(8) {
        let Some(answer) = answer.as_str() else { continue; };
        if remaining == 0 { break; }
        let excerpt: String = answer.chars().take(remaining.min(8_000)).collect();
        remaining = remaining.saturating_sub(excerpt.chars().count());
        if !excerpt.trim().is_empty() { blocks.push(excerpt); }
    }
    let request: String = user_request.chars().take(8_000).collect();
    Ok(format!(
        "USER REQUEST:\n{request}\n\nCompare these independent AI responses to that request. Produce one clear, accurate, balanced answer. Resolve contradictions where evidence permits, preserve useful disagreements and uncertainty, remove repetition, and do not invent facts.\n\n{}",
        blocks.join("\n\n---\n\n")
    ))
}

#[cfg(test)]
mod synthesis_tests {
    use super::build_synthesis_prompt;

    #[test]
    fn combines_bounded_provider_answers_for_synthesis() {
        let prompt = build_synthesis_prompt(
            "Compare options",
            r#"["Provider A: answer one","Provider B: answer two"]"#
        ).unwrap();
        assert!(prompt.contains("USER REQUEST:\nCompare options"));
        assert!(prompt.contains("Provider A: answer one"));
        assert!(prompt.contains("Provider B: answer two"));
    }
}


/// Selects the provider execution set from account-configured providers.
pub fn select_provider_targets(
    primary_provider: &str,
    multi_agent: bool,
    local_model_enabled: bool,
    configured_json: &str,
) -> Result<String, String> {
    let configured: Value = serde_json::from_str(configured_json)
        .map_err(|_| "Configured provider list must be valid JSON.".to_owned())?;
    let configured = configured.as_array()
        .ok_or_else(|| "Configured provider list must be a JSON array.".to_owned())?;
    let selected: Vec<String> = if local_model_enabled || !multi_agent {
        vec![primary_provider.to_owned()]
    } else {
        configured.iter().filter_map(Value::as_str)
            .filter(|name| matches!(*name, "OPENAI" | "DEEPSEEK" | "CLAUDE" | "GEMINI" | "CUSTOM" | "CENAI_CLOUD"))
            .map(str::to_owned).collect()
    };
    serde_json::to_string(&selected)
        .map_err(|_| "Could not encode selected provider targets.".to_owned())
}

#[cfg(test)]
mod target_selection_tests {
    use super::select_provider_targets;
    use serde_json::Value;

    #[test]
    fn selects_primary_or_configured_multi_agent_providers() {
        let primary: Value = serde_json::from_str(
            &select_provider_targets("OPENAI", false, false, r#"["GEMINI"]"#).unwrap()
        ).unwrap();
        assert_eq!(primary[0], "OPENAI");
        let multi: Value = serde_json::from_str(
            &select_provider_targets("OPENAI", true, false, r#"["OPENAI","GEMINI","BAD"]"#).unwrap()
        ).unwrap();
        assert_eq!(multi.as_array().unwrap().len(), 2);
        let local: Value = serde_json::from_str(
            &select_provider_targets("OPENAI", true, true, "[]").unwrap()
        ).unwrap();
        assert_eq!(local[0], "OPENAI");
    }
}


/// Extracts a concise provider error message without leaking an entire response body.
pub fn parse_provider_error(raw: &str) -> String {
    let fallback: String = raw.trim().chars().take(500).collect();
    let Ok(value) = serde_json::from_str::<Value>(raw) else { return fallback; };
    let error = value.get("error");
    let message = error.and_then(|item| item.as_str())
        .or_else(|| error.and_then(|item| item.get("message")).and_then(Value::as_str))
        .or_else(|| error.and_then(|item| item.get("detail")).and_then(Value::as_str))
        .or_else(|| value.get("error_description").and_then(Value::as_str))
        .or_else(|| value.get("message").and_then(Value::as_str))
        .or_else(|| value.get("detail").and_then(Value::as_str));
    message.map(|text| text.chars().take(500).collect()).unwrap_or(fallback)
}

#[cfg(test)]
mod error_tests {
    use super::parse_provider_error;

    #[test]
    fn extracts_provider_error_messages_with_bounded_fallbacks() {
        assert_eq!(parse_provider_error(r#"{"error":{"message":"Invalid API key"}}"#), "Invalid API key");
        assert_eq!(parse_provider_error(r#"{"message":"Rate limited"}"#), "Rate limited");
        assert_eq!(parse_provider_error("plain provider error"), "plain provider error");
        assert_eq!(parse_provider_error(&"x".repeat(700)).len(), 500);
    }
}


/// Replaces embedded image payloads with a safe textual marker for retrieval and synthesis.
pub fn strip_image_markers(content: &str) -> String {
    let parts = vision_parts(content);
    let mut output = String::new();
    for (kind, value) in parts {
        if kind == "text" {
            output.push_str(&value);
        } else {
            output.push_str("[Image attached]");
        }
    }
    output
}

#[cfg(test)]
mod image_marker_tests {
    use super::strip_image_markers;

    #[test]
    fn strips_encoded_image_payloads_without_losing_surrounding_text() {
        let value = "Describe [[CENAI_IMAGE|image/png|YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXo=]] please";
        assert_eq!(strip_image_markers(value), "Describe [Image attached] please");
    }
}


/// Extracts the assistant response from the CenAI Cloud OpenAI-compatible envelope.
pub fn parse_cloud_response(raw: &str) -> Result<String, String> {
    let value: Value = serde_json::from_str(raw)
        .map_err(|_| "CenAI Cloud returned invalid JSON.".to_owned())?;
    if let Some(error) = value.get("error") {
        let message = error.as_str()
            .or_else(|| error.get("message").and_then(Value::as_str))
            .unwrap_or("CenAI Cloud request failed.");
        return Err(message.chars().take(500).collect());
    }
    let answer = value.get("choices").and_then(|v| v.get(0))
        .and_then(|v| v.get("message")).and_then(|v| v.get("content"))
        .and_then(Value::as_str).unwrap_or("");
    if answer.trim().is_empty() {
        return Err("CenAI Cloud returned an empty response.".to_owned());
    }
    Ok(answer.to_owned())
}

#[cfg(test)]
mod cloud_response_tests {
    use super::parse_cloud_response;

    #[test]
    fn parses_cloud_chat_completions_and_errors() {
        assert_eq!(
            parse_cloud_response(r#"{"choices":[{"message":{"content":"Hello from cloud"}}]}"#).unwrap(),
            "Hello from cloud"
        );
        assert!(parse_cloud_response(r#"{"error":{"message":"quota exceeded"}}"#)
            .unwrap_err().contains("quota exceeded"));
        assert!(parse_cloud_response("{}").is_err());
    }
}


pub fn contains_image_marker(content: &str) -> bool {
    vision_parts(content).iter().any(|(kind, _)| kind != "text")
}

#[cfg(test)]
mod image_detection_tests {
    use super::contains_image_marker;

    #[test]
    fn detects_only_valid_embedded_image_payloads() {
        assert!(contains_image_marker("[[CENAI_IMAGE|image/png|YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXo=]]"));
        assert!(!contains_image_marker("[[CENAI_IMAGE|image/png|not-base64]]"));
    }
}

/// Canonical provider identity and default configuration. Kotlin keeps only enum IDs.
pub fn provider_metadata(name: &str) -> Result<String, String> {
    let (label, model, base_url) = match name {
        "OPENAI" => ("OpenAI", "gpt-4.1-mini", "https://api.openai.com/v1"),
        "DEEPSEEK" => ("DeepSeek", "deepseek-flash", "https://api.deepseek.com"),
        "CLAUDE" => ("Claude", "claude-3-5-haiku-latest", "https://api.anthropic.com"),
        "GEMINI" => ("Gemini", "gemini-2.0-flash", "https://generativelanguage.googleapis.com"),
        "CENAI_CLOUD" => ("CenAI Cloud · Gemma 4", "cloudflare-gemma-4-26b-a4b-it", "https://cenai-website.aglibotcarldaniel.workers.dev"),
        "CUSTOM" => ("Custom OpenAI-compatible", "", "https://"),
        _ => return Err("Unknown AI provider.".to_owned()),
    };
    serde_json::to_string(&json!({"name":name,"label":label,"model":model,"base_url":base_url}))
        .map_err(|_| "Could not encode provider metadata.".to_owned())
}

#[cfg(test)]
mod provider_metadata_tests {
    use super::provider_metadata;
    use serde_json::Value;

    #[test]
    fn rust_registry_is_the_single_source_for_provider_defaults() {
        let openai: Value = serde_json::from_str(&provider_metadata("OPENAI").unwrap()).unwrap();
        assert_eq!(openai["model"], "gpt-4.1-mini");
        assert_eq!(openai["base_url"], "https://api.openai.com/v1");
        let local: Value = serde_json::from_str(&provider_metadata("CUSTOM").unwrap()).unwrap();
        assert_eq!(local["model"], "");
        assert!(provider_metadata("UNKNOWN").is_err());
    }
}

/// Chooses the account credential channel without exposing credential values to the UI layer.
pub fn credential_source(provider: &str, oauth_available: bool) -> &'static str {
    if provider == "CENAI_CLOUD" {
        "cloud_access_token"
    } else if oauth_available {
        "google_oauth"
    } else {
        "provider_api_key"
    }
}

#[cfg(test)]
mod credential_source_tests {
    use super::credential_source;

    #[test]
    fn selects_cloud_oauth_and_provider_key_channels() {
        assert_eq!(credential_source("CENAI_CLOUD", true), "cloud_access_token");
        assert_eq!(credential_source("GEMINI", true), "google_oauth");
        assert_eq!(credential_source("OPENAI", false), "provider_api_key");
        assert_eq!(credential_source("GEMINI", false), "provider_api_key");
    }
}

/// Resolves per-provider model and endpoint settings while keeping defaults in the Rust registry.
pub fn resolve_provider_settings(
    provider: &str,
    selected_provider: &str,
    selected_model: &str,
    selected_base_url: &str,
    stored_model: &str,
    stored_base_url: &str,
) -> Result<String, String> {
    let defaults: Value = serde_json::from_str(&provider_metadata(provider)?)
        .map_err(|_| "Provider registry returned invalid metadata.".to_owned())?;
    let default_model = defaults.get("model").and_then(Value::as_str).unwrap_or("");
    let default_base = defaults.get("base_url").and_then(Value::as_str).unwrap_or("");
    let model = if provider == selected_provider {
        selected_model
    } else if provider == "DEEPSEEK" && matches!(stored_model, "deepseek-chat" | "deepseek-reasoner") {
        default_model
    } else if stored_model.is_empty() {
        default_model
    } else {
        stored_model
    };
    let base_url = if provider == selected_provider {
        selected_base_url
    } else if stored_base_url.is_empty() {
        default_base
    } else {
        stored_base_url
    };
    Ok(json!({"model":model,"base_url":base_url}).to_string())
}

#[cfg(test)]
mod provider_settings_tests {
    use super::resolve_provider_settings;
    use serde_json::Value;

    #[test]
    fn resolves_primary_custom_and_legacy_deepseek_settings() {
        let primary: Value = serde_json::from_str(&resolve_provider_settings("OPENAI", "OPENAI", "my-model", "https://custom", "", "").unwrap()).unwrap();
        assert_eq!(primary["model"], "my-model");
        assert_eq!(primary["base_url"], "https://custom");
        let legacy: Value = serde_json::from_str(&resolve_provider_settings("DEEPSEEK", "OPENAI", "", "", "deepseek-chat", "").unwrap()).unwrap();
        assert_eq!(legacy["model"], "deepseek-flash");
        assert_eq!(legacy["base_url"], "https://api.deepseek.com");
    }
}
