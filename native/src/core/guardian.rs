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

impl DiagnosticEvent {
    pub fn sanitized(kind: DiagnosticKind, component: &str, code: &str, occurred_at_unix_ms: u64) -> Self {
        Self {
            kind,
            component: component.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').take(48).collect(),
            code: code.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').take(64).collect(),
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
        assert_eq!(event.component, "chatapitokensecret");
        assert_eq!(event.code, "HTTP401private");
        let serialized = serde_json::to_string(&event).unwrap();
        assert!(!serialized.contains("secret"));
    }
}
