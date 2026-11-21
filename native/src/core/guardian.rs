//! Local diagnostics deliberately contain no prompts, file contents or credentials.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticKind { Crash, Anr, Network, ApiFailure, RenderFailure, Storage, BackgroundTask, McpFailure }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiagnosticEvent {
    pub kind: DiagnosticKind,
    pub component: String,
    pub code: String,
    pub occurred_at_unix_ms: u64,
}

fn sanitize_label(value: &str, limit: usize) -> String {
    value.split(|character: char| !character.is_ascii_alphanumeric() && character != '_' && character != '-')
        .filter(|word| !matches!(word.to_ascii_lowercase().as_str(), "secret" | "token" | "password" | "private" | "credential" | "authorization" | "key"))
        .collect::<String>()
        .chars().take(limit).collect()
}

impl DiagnosticEvent {
    pub fn sanitized(kind: DiagnosticKind, component: &str, code: &str, occurred_at_unix_ms: u64) -> Self {
        Self {
            kind,
            component: sanitize_label(component, 48),
            code: sanitize_label(code, 64),
            occurred_at_unix_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guardian_sanitizes_diagnostic_labels() {
        let event = DiagnosticEvent::sanitized(DiagnosticKind::ApiFailure, "chat/api?token=secret", "HTTP 401: private", 42);
        assert_eq!(event.component, "chatapi");
        assert_eq!(event.code, "HTTP401");
        let serialized = serde_json::to_string(&event).unwrap();
        assert!(!serialized.contains("secret"));
    }
}
