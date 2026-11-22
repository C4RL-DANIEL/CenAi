//! Privacy-preserving application telemetry aggregation and classification.

pub fn event_key(event: &str) -> Option<&'static str> {
    match event {
        "app_start" => Some("app_starts"),
        "voice_input" => Some("voice_inputs"),
        "voice_output" => Some("voice_outputs"),
        "document_attached" => Some("documents_attached"),
        "update_check" => Some("update_checks"),
        "provider_test" => Some("provider_tests"),
        _ => None,
    }
}

pub fn success_rate(succeeded: i32, failed: i32) -> i32 {
    let successes = i64::from(succeeded.max(0));
    let failures = i64::from(failed.max(0));
    let completed = successes + failures;
    if completed == 0 { 0 } else { ((successes * 100) / completed).clamp(0, 100) as i32 }
}

pub fn average_latency(total_ms: i64, completed: i32) -> i64 {
    if completed <= 0 { 0 } else { total_ms.max(0) / i64::from(completed) }
}

pub fn failure_category(message: &str, class_name: &str) -> String {
    for part in message.split("HTTP ").skip(1) {
        let code: String = part.chars().take(3).collect();
        if code.len() == 3 && code.bytes().all(|byte| byte.is_ascii_digit()) {
            return format!("HTTP {code}");
        }
    }
    class_name.chars().take(48).collect()
}

#[cfg(test)]
mod tests {
    use super::{average_latency, event_key, failure_category, success_rate};

    #[test]
    fn maps_only_known_non_sensitive_feature_events() {
        assert_eq!(event_key("voice_input"), Some("voice_inputs"));
        assert_eq!(event_key("provider_test"), Some("provider_tests"));
        assert_eq!(event_key("unknown_sensitive_event"), None);
    }

    #[test]
    fn computes_bounded_aggregate_metrics() {
        assert_eq!(success_rate(1, 1), 50);
        assert_eq!(success_rate(0, 0), 0);
        assert_eq!(success_rate(-4, 1), 0);
        assert_eq!(average_latency(200, 2), 100);
        assert_eq!(average_latency(200, 0), 0);
        assert_eq!(average_latency(-1, 2), 0);
    }

    #[test]
    fn stores_only_failure_category_not_error_text() {
        assert_eq!(failure_category("request HTTP 429 private-token", "IOException"), "HTTP 429");
        assert_eq!(failure_category("private prompt", "SomeVeryLongExceptionNameThatMustBeTruncatedAtFortyEightCharsAndMore"), "SomeVeryLongExceptionNameThatMustBeTruncatedAtFortyEightCharsAnd");
    }
}
