//! Safe one-time import of the legacy Kotlin conversation store.
use serde::Deserialize;
use crate::ui::model::{Message, MessageRole};

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyConversation {
    pub id: String,
    #[serde(default = "default_title")]
    pub title: String,
    #[serde(default)]
    pub updated_at: u64,
    #[serde(default)]
    pub owner_email: String,
    #[serde(default)]
    pub messages: Vec<LegacyMessage>,
}
#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
pub struct LegacyMessage {
    pub role: String,
    pub content: String,
}
fn default_title() -> String { "New chat".into() }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportedConversation {
    pub id: String,
    pub title: String,
    pub updated_at_unix_ms: u64,
    pub messages: Vec<Message>,
}

pub fn parse_legacy_history(json: &str, expected_owner_email: &str) -> Result<Vec<ImportedConversation>, String> {
    let expected_owner_email = expected_owner_email.trim();
    if expected_owner_email.is_empty() || !expected_owner_email.contains('@') {
        return Err("A verified signed-in account is required to import legacy history".into());
    }
    if json.len() > 70 * 1024 * 1024 {
        return Err("Legacy conversation export exceeds the 70 MB import limit".into());
    }
    let legacy: Vec<LegacyConversation> = serde_json::from_str(json)
        .map_err(|error| format!("Legacy conversation export is invalid: {error}"))?;
    if legacy.len() > 5000 { return Err("Legacy conversation count exceeds the import limit".into()); }
    let mut seen = std::collections::HashSet::new();
    let mut imported = Vec::with_capacity(legacy.len());
    let mut total_messages = 0usize;
    for conversation in legacy {
        if conversation.id.is_empty() || conversation.id.len() > 80
            || !conversation.id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')) {
            return Err("Legacy conversation contains an invalid identifier".into());
        }
        if !seen.insert(conversation.id.clone()) {
            return Err("Legacy conversation export contains duplicate identifiers".into());
        }
        if conversation.title.len() > 512 { return Err("Legacy conversation title is too large".into()); }
        if conversation.owner_email.trim().is_empty()
            || !conversation.owner_email.trim().eq_ignore_ascii_case(expected_owner_email) {
            return Err("Legacy conversation ownership is missing or belongs to a different account; original history was retained".into());
        }
        if conversation.messages.len() > 5000 { return Err("Legacy conversation exceeds the message limit".into()); }
        total_messages = total_messages.saturating_add(conversation.messages.len());
        if total_messages > 100_000 { return Err("Legacy export contains too many messages".into()); }
        let mut messages = Vec::with_capacity(conversation.messages.len());
        for (index, message) in conversation.messages.into_iter().enumerate() {
            if message.content.len() > 200_000 { return Err("Legacy message exceeds the 200 KB limit".into()); }
            let role = match message.role.trim().to_ascii_lowercase().as_str() {
                "user" => MessageRole::User,
                "assistant" | "ai" | "model" => MessageRole::Assistant,
                "system" => MessageRole::System,
                _ => return Err("Legacy message has an unsupported role".into()),
            };
            messages.push(Message { id: index as u64 + 1, role, body: message.content, streaming: false });
        }
        let encoded_size = serde_json::to_vec(&messages).map_err(|error| error.to_string())?.len();
        if encoded_size > 8 * 1024 * 1024 {
            return Err("Legacy conversation exceeds the 8 MB local history limit".into());
        }
        imported.push(ImportedConversation {
            id: conversation.id, title: conversation.title, updated_at_unix_ms: conversation.updated_at, messages,
        });
    }
    imported.sort_by_key(|a| std::cmp::Reverse(a.updated_at_unix_ms));
    Ok(imported)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_roles_order_and_timestamps() {
        let parsed = parse_legacy_history(r#"[{"id":"c1","title":"First","updatedAt":123,"ownerEmail":"owner@example.com","messages":[{"role":"user","content":"hello"},{"role":"assistant","content":"world"}]}]"#, "owner@example.com").unwrap();
        assert_eq!(parsed[0].updated_at_unix_ms, 123);
        assert_eq!(parsed[0].title, "First");
        assert_eq!(parsed[0].messages[0].role, MessageRole::User);
        assert_eq!(parsed[0].messages[1].body, "world");
    }
    #[test]
    fn rejects_duplicate_ids_malformed_json_and_unknown_roles() {
        assert!(parse_legacy_history("{", "owner@example.com").is_err());
        assert!(parse_legacy_history(r#"[{"id":"x","ownerEmail":"owner@example.com","messages":[]},{"id":"x","ownerEmail":"owner@example.com","messages":[]}]"#, "owner@example.com").is_err());
        assert!(parse_legacy_history(r#"[{"id":"x","ownerEmail":"owner@example.com","messages":[{"role":"tool","content":"x"}]}]"#, "owner@example.com").is_err());
    }
    #[test]
    fn refuses_unowned_or_cross_account_history() {
        let legacy = r#"[{"id":"chat","ownerEmail":"alice@example.com","messages":[]}]"#;
        assert!(parse_legacy_history(legacy, "").is_err());
        assert!(parse_legacy_history(legacy, "bob@example.com").is_err());
        assert!(parse_legacy_history(r#"[{"id":"chat","messages":[]}]"#, "alice@example.com").is_err());
        assert_eq!(parse_legacy_history(legacy, "ALICE@example.com").unwrap().len(), 1);
    }
    #[test]
    fn rejects_path_traversal_and_oversized_messages() {
        assert!(parse_legacy_history(r#"[{"id":"../escape","ownerEmail":"owner@example.com","messages":[]}]"#, "owner@example.com").is_err());
        let json = format!(r#"[{{"id":"x","ownerEmail":"owner@example.com","messages":[{{"role":"user","content":"{}"}}]}}]"#, "x".repeat(200_001), "owner@example.com");
        assert!(parse_legacy_history(&json).is_err());
    }
}
