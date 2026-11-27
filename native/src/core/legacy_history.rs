//! Safe one-time import of the legacy Kotlin conversation store.
use crate::ui::model::{Message, MessageRole};
use serde::Deserialize;

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
fn default_title() -> String {
    "New chat".into()
}

/// Returns the old account email only when the legacy encrypted store explicitly
/// marked that email as verified. This value may recover ownerless history only
/// after it is compared with the currently authenticated account email.
pub fn verified_legacy_owner_email(export: &str) -> Option<String> {
    if export.len() > 64 * 1024 {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(export).ok()?;
    if value
        .get("email_verified")
        .and_then(serde_json::Value::as_bool)
        != Some(true)
    {
        return None;
    }
    let email = value
        .get("account_email")
        .and_then(serde_json::Value::as_str)?
        .trim();
    (email.contains('@') && email.len() <= 320).then(|| email.to_owned())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportedConversation {
    pub id: String,
    pub title: String,
    pub updated_at_unix_ms: u64,
    pub messages: Vec<Message>,
}

pub fn parse_legacy_history(
    json: &str,
    expected_owner_email: &str,
    legacy_verified_owner_email: Option<&str>,
) -> Result<Vec<ImportedConversation>, String> {
    let expected_owner_email = expected_owner_email.trim();
    if expected_owner_email.is_empty() || !expected_owner_email.contains('@') {
        return Err("A verified signed-in account is required to import legacy history".into());
    }
    if json.len() > 70 * 1024 * 1024 {
        return Err("Legacy conversation export exceeds the 70 MB import limit".into());
    }
    let legacy: Vec<LegacyConversation> = serde_json::from_str(json)
        .map_err(|error| format!("Legacy conversation export is invalid: {error}"))?;
    if legacy.len() > 5000 {
        return Err("Legacy conversation count exceeds the import limit".into());
    }
    let mut seen = std::collections::HashSet::new();
    let mut imported = Vec::with_capacity(legacy.len());
    let mut total_messages = 0usize;
    for conversation in legacy {
        if conversation.id.is_empty()
            || conversation.id.len() > 80
            || !conversation
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        {
            return Err("Legacy conversation contains an invalid identifier".into());
        }
        if !seen.insert(conversation.id.clone()) {
            return Err("Legacy conversation export contains duplicate identifiers".into());
        }
        if conversation.title.len() > 512 {
            return Err("Legacy conversation title is too large".into());
        }
        let recorded_owner = conversation.owner_email.trim();
        let ownership_matches = if recorded_owner.is_empty() {
            legacy_verified_owner_email.is_some_and(|legacy_email| {
                legacy_email
                    .trim()
                    .eq_ignore_ascii_case(expected_owner_email)
            })
        } else {
            recorded_owner.eq_ignore_ascii_case(expected_owner_email)
        };
        if !ownership_matches {
            return Err("Legacy conversation ownership is missing or belongs to a different account; original history was retained".into());
        }
        if conversation.messages.len() > 5000 {
            return Err("Legacy conversation exceeds the message limit".into());
        }
        total_messages = total_messages.saturating_add(conversation.messages.len());
        if total_messages > 100_000 {
            return Err("Legacy export contains too many messages".into());
        }
        let mut messages = Vec::with_capacity(conversation.messages.len());
        for (index, message) in conversation.messages.into_iter().enumerate() {
            if message.content.len() > 200_000 {
                return Err("Legacy message exceeds the 200 KB limit".into());
            }
            let role = match message.role.trim().to_ascii_lowercase().as_str() {
                "user" => MessageRole::User,
                "assistant" | "ai" | "model" => MessageRole::Assistant,
                "system" => MessageRole::System,
                _ => return Err("Legacy message has an unsupported role".into()),
            };
            messages.push(Message {
                id: index as u64 + 1,
                role,
                body: message.content,
                streaming: false,
            });
        }
        let encoded_size = serde_json::to_vec(&messages)
            .map_err(|error| error.to_string())?
            .len();
        if encoded_size > 8 * 1024 * 1024 {
            return Err("Legacy conversation exceeds the 8 MB local history limit".into());
        }
        imported.push(ImportedConversation {
            id: conversation.id,
            title: conversation.title,
            updated_at_unix_ms: conversation.updated_at,
            messages,
        });
    }
    imported.sort_by_key(|a| std::cmp::Reverse(a.updated_at_unix_ms));
    Ok(imported)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trusts_only_explicitly_verified_legacy_account_email() {
        assert_eq!(
            verified_legacy_owner_email(
                r#"{"account_email":" Alice@example.com ","email_verified":true}"#
            )
            .as_deref(),
            Some("Alice@example.com")
        );
        assert!(verified_legacy_owner_email(
            r#"{"account_email":"alice@example.com","email_verified":false}"#
        )
        .is_none());
        assert!(verified_legacy_owner_email(r#"{"account_email":"alice@example.com"}"#).is_none());
        assert!(verified_legacy_owner_email(
            r#"{"account_email":"not-an-email","email_verified":true}"#
        )
        .is_none());
    }

    #[test]
    fn parses_roles_order_and_timestamps() {
        let parsed = parse_legacy_history(r#"[{"id":"c1","title":"First","updatedAt":123,"ownerEmail":"owner@example.com","messages":[{"role":"user","content":"hello"},{"role":"assistant","content":"world"}]}]"#, "owner@example.com", None).unwrap();
        assert_eq!(parsed[0].updated_at_unix_ms, 123);
        assert_eq!(parsed[0].title, "First");
        assert_eq!(parsed[0].messages[0].role, MessageRole::User);
        assert_eq!(parsed[0].messages[1].body, "world");
    }
    #[test]
    fn rejects_duplicate_ids_malformed_json_and_unknown_roles() {
        assert!(parse_legacy_history("{", "owner@example.com", None).is_err());
        assert!(parse_legacy_history(r#"[{"id":"x","ownerEmail":"owner@example.com","messages":[]},{"id":"x","ownerEmail":"owner@example.com","messages":[]}]"#, "owner@example.com", None).is_err());
        assert!(parse_legacy_history(r#"[{"id":"x","ownerEmail":"owner@example.com","messages":[{"role":"tool","content":"x"}]}]"#, "owner@example.com", None).is_err());
    }
    #[test]
    fn refuses_unowned_or_cross_account_history() {
        let legacy = r#"[{"id":"chat","ownerEmail":"alice@example.com","messages":[]}]"#;
        assert!(parse_legacy_history(legacy, "", None).is_err());
        assert!(parse_legacy_history(legacy, "bob@example.com", None).is_err());
        assert!(parse_legacy_history(
            r#"[{"id":"chat","messages":[]}]"#,
            "alice@example.com",
            None
        )
        .is_err());
        assert_eq!(
            parse_legacy_history(legacy, "ALICE@example.com", None)
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn recovers_ownerless_history_only_with_matching_verified_legacy_account() {
        let unowned = r#"[{"id":"old-chat","messages":[{"role":"user","content":"saved"}]}]"#;
        assert!(parse_legacy_history(unowned, "alice@example.com", None).is_err());
        assert!(
            parse_legacy_history(unowned, "bob@example.com", Some("alice@example.com")).is_err()
        );
        let recovered =
            parse_legacy_history(unowned, "alice@example.com", Some("ALICE@example.com")).unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].messages[0].body, "saved");
    }

    #[test]
    fn rejects_path_traversal_and_oversized_messages() {
        assert!(parse_legacy_history(
            r#"[{"id":"../escape","ownerEmail":"owner@example.com","messages":[]}]"#,
            "owner@example.com",
            None
        )
        .is_err());
        let json = format!(
            r#"[{{"id":"x","ownerEmail":"owner@example.com","messages":[{{"role":"user","content":"{}"}}]}}]"#,
            "x".repeat(200_001)
        );
        assert!(parse_legacy_history(&json, "owner@example.com", None).is_err());
    }
}
