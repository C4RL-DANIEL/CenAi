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


#[derive(Default)]
pub struct GuardianMonitor {
    telemetry_consent: bool,
    pending: std::collections::VecDeque<DiagnosticEvent>,
    last_seen: std::collections::HashMap<String, u64>,
}

impl GuardianMonitor {
    pub fn set_telemetry_consent(&mut self, enabled: bool) {
        self.telemetry_consent = enabled;
    }

    pub fn record(&mut self, event: DiagnosticEvent) -> bool {
        let key = format!("{:?}:{}:{}", event.kind, event.component, event.code);
        if self.last_seen.get(&key).is_some_and(|last| event.occurred_at_unix_ms.saturating_sub(*last) < 60_000) {
            return false;
        }
        self.last_seen.insert(key, event.occurred_at_unix_ms);
        if self.pending.len() == 64 {
            self.pending.pop_front();
        }
        self.pending.push_back(event);
        true
    }

    pub fn pending_batch(&self, limit: usize) -> Vec<DiagnosticEvent> {
        if !self.telemetry_consent { return Vec::new(); }
        self.pending.iter().take(limit.min(16)).cloned().collect()
    }

    pub fn acknowledge_uploaded(&mut self, count: usize) {
        for _ in 0..count.min(self.pending.len()) {
            self.pending.pop_front();
        }
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guardian_requires_consent_and_deduplicates_repeated_events() {
        let mut monitor = GuardianMonitor::default();
        let event = DiagnosticEvent::sanitized(DiagnosticKind::Network, "chat", "timeout", 100_000);
        assert!(monitor.record(event.clone()));
        assert!(!monitor.record(event.clone()));
        assert!(monitor.pending_batch(8).is_empty());
        monitor.set_telemetry_consent(true);
        assert_eq!(monitor.pending_batch(8).len(), 1);
        monitor.acknowledge_uploaded(1);
        assert_eq!(monitor.pending_count(), 0);
    }

    #[test]
    fn guardian_sanitizes_diagnostic_labels() {
        let event = DiagnosticEvent::sanitized(DiagnosticKind::ApiFailure, "chat/api?token=secret", "HTTP 401: private", 42);
        assert_eq!(event.component, "chatapi");
        assert_eq!(event.code, "HTTP401");
        let serialized = serde_json::to_string(&event).unwrap();
        assert!(!serialized.contains("secret"));
    }
}
