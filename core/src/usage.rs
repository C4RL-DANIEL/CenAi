//! Account-scoped daily usage policy.

use serde_json::json;

pub fn evaluate_daily_usage(today: &str, stored_day: &str, stored_count: &str, daily_limit: &str) -> String {
    let reset = today != stored_day;
    let used = if reset { 0 } else { stored_count.parse::<i32>().unwrap_or(0).max(0) };
    let limit = daily_limit.parse::<i32>().unwrap_or(0).max(0);
    let allowed = limit == 0 || used < limit;
    let next_count = if allowed { used.saturating_add(1) } else { used };
    json!({
        "allowed": allowed,
        "reset": reset,
        "used": used,
        "limit": limit,
        "next_count": next_count,
        "error": if allowed { "" } else { "Daily message limit reached. Change it in Settings or try again tomorrow." }
    }).to_string()
}

#[cfg(test)]
mod tests {
    use super::evaluate_daily_usage;
    use serde_json::Value;

    #[test]
    fn resets_usage_on_a_new_day_and_increments_once() {
        let result: Value = serde_json::from_str(&evaluate_daily_usage("2026-11-22", "2026-11-21", "9", "2")).unwrap();
        assert_eq!(result["reset"], true);
        assert_eq!(result["allowed"], true);
        assert_eq!(result["used"], 0);
        assert_eq!(result["next_count"], 1);
    }

    #[test]
    fn enforces_configured_limit_and_unlimited_mode() {
        let blocked: Value = serde_json::from_str(&evaluate_daily_usage("today", "today", "2", "2")).unwrap();
        assert_eq!(blocked["allowed"], false);
        assert_eq!(blocked["next_count"], 2);
        let unlimited: Value = serde_json::from_str(&evaluate_daily_usage("today", "today", "2", "0")).unwrap();
        assert_eq!(unlimited["allowed"], true);
        assert_eq!(unlimited["next_count"], 3);
    }

    #[test]
    fn malformed_negative_counters_are_safely_normalized() {
        let result: Value = serde_json::from_str(&evaluate_daily_usage("today", "today", "bad", "-5")).unwrap();
        assert_eq!(result["used"], 0);
        assert_eq!(result["limit"], 0);
        assert_eq!(result["next_count"], 1);
    }
}
