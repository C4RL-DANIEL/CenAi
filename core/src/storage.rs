//! Canonical conversation-store validation and normalization.

use serde_json::{json, Value};

pub fn normalize_conversations(raw_json: &str) -> Result<String, String> {
    let parsed: Value = serde_json::from_str(raw_json)
        .map_err(|_| "Conversation store is not valid JSON.".to_owned())?;
    let items = parsed.as_array()
        .ok_or_else(|| "Conversation store must be a JSON array.".to_owned())?;

    let mut normalized = Vec::new();
    for item in items.iter().take(200) {
        let Some(object) = item.as_object() else { continue; };
        let id = object.get("id").and_then(Value::as_str).unwrap_or("");
        if id.is_empty() || id.len() > 128 { continue; }
        let title: String = object.get("title").and_then(Value::as_str).unwrap_or("New chat")
            .chars().take(200).collect();
        let updated_at = object.get("updatedAt").and_then(Value::as_i64).unwrap_or(0);
        let Some(messages) = object.get("messages").and_then(Value::as_array) else { continue; };
        let mut clean_messages = Vec::new();
        for message in messages.iter().take(1000) {
            let Some(message) = message.as_object() else { continue; };
            let role = message.get("role").and_then(Value::as_str).unwrap_or("");
            if !matches!(role, "user" | "assistant" | "system" | "tool") { continue; }
            let Some(content) = message.get("content").and_then(Value::as_str) else { continue; };
            if content.len() > 250_000 { continue; }
            clean_messages.push(json!({"role":role,"content":sanitize(content)}));
        }
        normalized.push(json!({
            "id": id,
            "title": title,
            "updatedAt": updated_at,
            "messages": clean_messages
        }));
    }
    normalized.sort_by(|a, b| {
        b.get("updatedAt").and_then(Value::as_i64).unwrap_or(0)
            .cmp(&a.get("updatedAt").and_then(Value::as_i64).unwrap_or(0))
    });
    serde_json::to_string(&normalized)
        .map_err(|_| "Could not normalize conversation store.".to_owned())
}

fn sanitize(value: &str) -> String {
    value.chars().filter(|ch| !ch.is_control() || *ch == '\n' || *ch == '\t').collect()
}

#[cfg(test)]
mod tests {
    use super::normalize_conversations;
    use serde_json::Value;

    #[test]
    fn normalizes_and_sorts_conversations() {
        let raw = r#"[{"id":"old","title":"Old","updatedAt":1,"messages":[{"role":"user","content":"hello"},{"role":"system-admin","content":"bad"}]},{"id":"new","title":"New","updatedAt":2,"messages":[{"role":"assistant","content":"world"}]}]"#;
        let output: Value = serde_json::from_str(&normalize_conversations(raw).unwrap()).unwrap();
        assert_eq!(output[0]["id"], "new");
        assert_eq!(output[1]["messages"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn rejects_invalid_roots_and_drops_malformed_entries() {
        assert!(normalize_conversations("{}").is_err());
        let output: Value = serde_json::from_str(
            &normalize_conversations(r#"[{"id":"","messages":[]},{"id":"valid","messages":[{"role":"user","content":"ok"}]}]"#).unwrap()
        ).unwrap();
        assert_eq!(output.as_array().unwrap().len(), 1);
    }
}

/// Builds a bounded conversation title from the first user message.
pub fn conversation_title(messages_json: &str) -> Result<String, String> {
    let messages: Value = serde_json::from_str(messages_json)
        .map_err(|_| "Conversation messages are not valid JSON.".to_owned())?;
    let items = messages.as_array()
        .ok_or_else(|| "Conversation messages must be a JSON array.".to_owned())?;
    let first_prompt = items.iter()
        .find(|message| message.get("role").and_then(Value::as_str) == Some("user"))
        .and_then(|message| message.get("content").and_then(Value::as_str))
        .unwrap_or("");
    let title: String = first_prompt.chars().take(42).collect();
    Ok(if title.trim().is_empty() { "New chat".to_owned() } else { title })
}

#[cfg(test)]
mod title_tests {
    use super::conversation_title;

    #[test]
    fn derives_title_from_first_user_message_with_safe_fallback() {
        assert_eq!(conversation_title(r#"[{"role":"assistant","content":"hello"},{"role":"user","content":"A first prompt"}]"#).unwrap(), "A first prompt");
        assert_eq!(conversation_title(r#"[{"role":"user","content":""}]"#).unwrap(), "New chat");
        assert_eq!(conversation_title(&format!(r#"[{{"role":"user","content":"{}"}}]"#, "x".repeat(80)).unwrap()).unwrap().len(), 42);
    }
}
