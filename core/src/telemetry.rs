//! Rust-owned CenAI telemetry payload construction and bounds.

use serde_json::json;

pub fn build_consent_payload(usage_analytics: bool, guardian: bool) -> String {
    json!({"action":"consent","usageAnalytics":usage_analytics,"guardian":guardian}).to_string()
}

pub fn build_usage_payload(provider: &str, model: &str, input_tokens: i32, output_tokens: i32, latency_ms: i64, status: &str, error_code: &str) -> String {
    json!({
        "action":"usage",
        "provider":provider,
        "model":model,
        "inputTokens":input_tokens.max(0),
        "outputTokens":output_tokens.max(0),
        "latencyMs":latency_ms.max(0),
        "status":status,
        "errorCode":error_code.chars().take(80).collect::<String>()
    }).to_string()
}

pub fn build_guardian_payload(app_version: &str, android_version: &str, battery_percent: i32, available_ram_mb: i64, free_storage_mb: i64, network: &str, crash_category: &str) -> String {
    json!({
        "action":"guardian",
        "appVersion":app_version,
        "androidVersion":android_version,
        "batteryPercent":battery_percent.clamp(0, 100),
        "availableRamMb":available_ram_mb.max(0),
        "freeStorageMb":free_storage_mb.max(0),
        "network":network,
        "crashCategory":crash_category.chars().take(64).collect::<String>()
    }).to_string()
}

#[cfg(test)]
mod tests {
    use super::{build_consent_payload, build_guardian_payload, build_usage_payload};
    use serde_json::Value;

    #[test]
    fn builds_consent_and_usage_payloads_with_bounded_values() {
        let consent: Value = serde_json::from_str(&build_consent_payload(true, false)).unwrap();
        assert_eq!(consent["usageAnalytics"], true);
        let usage: Value = serde_json::from_str(&build_usage_payload("local", "offline", -1, 8, -10, "error", &"x".repeat(100))).unwrap();
        assert_eq!(usage["inputTokens"], 0);
        assert_eq!(usage["latencyMs"], 0);
        assert_eq!(usage["errorCode"].as_str().unwrap().len(), 80);
    }

    #[test]
    fn bounds_guardian_device_metrics_and_crash_category() {
        let guardian: Value = serde_json::from_str(&build_guardian_payload("1.0", "16", 150, -1, 200, "wifi", &"x".repeat(80))).unwrap();
        assert_eq!(guardian["batteryPercent"], 100);
        assert_eq!(guardian["availableRamMb"], 0);
        assert_eq!(guardian["crashCategory"].as_str().unwrap().len(), 64);
    }
}
