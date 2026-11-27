//! Local diagnostics deliberately contain no prompts, file contents or credentials.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticKind {
    Crash,
    Anr,
    Network,
    ApiFailure,
    RenderFailure,
    Storage,
    BackgroundTask,
    McpFailure,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiagnosticEvent {
    pub kind: DiagnosticKind,
    pub component: String,
    pub code: String,
    pub occurred_at_unix_ms: u64,
}

fn sanitize_label(value: &str, limit: usize) -> String {
    const SENSITIVE: &[&str] = &[
        "secret",
        "token",
        "password",
        "private",
        "credential",
        "authorization",
        "apikey",
        "api_key",
        "bearer",
        "cookie",
        "session",
        "refresh",
    ];
    // Check the complete original label before tokenizing. Checking only split
    // words misses compound forms such as "api_key" and "access-token".
    let normalized = value.to_ascii_lowercase();
    if SENSITIVE
        .iter()
        .any(|sensitive| normalized.contains(sensitive))
    {
        return String::new();
    }
    value.split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| {
            !word.is_empty()
                && word.len() <= 32
                // Long mixed alphanumeric fragments are commonly keys, hashes or opaque identifiers.
                && !(word.len() >= 20 && word.chars().any(|c| c.is_ascii_alphabetic()) && word.chars().any(|c| c.is_ascii_digit()))
        })
        .take(8)
        .collect::<Vec<_>>()
        .join("_")
        .chars().take(limit).collect()
}

impl DiagnosticEvent {
    pub fn sanitized(
        kind: DiagnosticKind,
        component: &str,
        code: &str,
        occurred_at_unix_ms: u64,
    ) -> Self {
        Self {
            kind,
            component: sanitize_label(component, 48),
            code: sanitize_label(code, 64),
            occurred_at_unix_ms,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GuardianSnapshot {
    pub cpu_usage_percent: Option<u8>,
    pub ram_total_kb: u64,
    pub ram_available_kb: u64,
    pub network_rx_bytes: u64,
    pub network_tx_bytes: u64,
    pub storage_available_bytes: u64,
    pub battery_percent: Option<u8>,
    pub battery_status: Option<String>,
}

fn parse_memory_info(content: &str) -> (u64, u64) {
    let mut total = 0;
    let mut available = 0;
    for line in content.lines() {
        let mut parts = line.split_whitespace();
        match parts.next() {
            Some("MemTotal:") => {
                total = parts
                    .next()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0)
            }
            Some("MemAvailable:") => {
                available = parts
                    .next()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0)
            }
            _ => {}
        }
    }
    (total, available.min(total))
}

fn parse_cpu_counters(content: &str) -> Option<(u64, u64)> {
    let line = content.lines().next()?;
    let mut fields = line.split_whitespace();
    if fields.next()? != "cpu" {
        return None;
    }
    let values = fields
        .take(8)
        .map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if values.len() < 4 {
        return None;
    }
    let total = values.iter().copied().sum();
    let idle = values
        .get(3)
        .copied()
        .unwrap_or(0)
        .saturating_add(values.get(4).copied().unwrap_or(0));
    Some((total, idle))
}

fn parse_network_counters(content: &str) -> (u64, u64) {
    let mut received = 0u64;
    let mut transmitted = 0u64;
    for line in content.lines().skip(2) {
        let Some((_, counters)) = line.split_once(':') else {
            continue;
        };
        let values = counters
            .split_whitespace()
            .filter_map(|value| value.parse::<u64>().ok())
            .collect::<Vec<_>>();
        if values.len() >= 16 {
            received = received.saturating_add(values[0]);
            transmitted = transmitted.saturating_add(values[8]);
        }
    }
    (received, transmitted)
}

#[cfg(unix)]
fn available_storage_bytes(path: &std::path::Path) -> u64 {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let Ok(path) = CString::new(path.as_os_str().as_bytes()) else {
        return 0;
    };
    let mut stats: libc::statvfs = unsafe { std::mem::zeroed() };
    let result = unsafe { libc::statvfs(path.as_ptr(), &mut stats) };
    if result == 0 {
        (stats.f_bavail as u64).saturating_mul(stats.f_frsize as u64)
    } else {
        0
    }
}

#[cfg(not(unix))]
fn available_storage_bytes(_path: &std::path::Path) -> u64 {
    0
}

#[derive(Default)]
pub struct GuardianMonitor {
    telemetry_consent: bool,
    local_events: std::collections::VecDeque<DiagnosticEvent>,
    pending: std::collections::VecDeque<DiagnosticEvent>,
    last_seen: std::collections::HashMap<String, u64>,
    last_cpu_sample: Option<(u64, u64)>,
}

impl GuardianMonitor {
    /// Samples local device health only; these values are never included in telemetry uploads.
    pub fn sample_system(&mut self, data_directory: &std::path::Path) -> GuardianSnapshot {
        let memory = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
        let (ram_total_kb, ram_available_kb) = parse_memory_info(&memory);
        let cpu = std::fs::read_to_string("/proc/stat")
            .ok()
            .and_then(|content| parse_cpu_counters(&content));
        let cpu_usage_percent = cpu.and_then(|(total, idle)| {
            let previous = self.last_cpu_sample.replace((total, idle))?;
            let total_delta = total.saturating_sub(previous.0);
            let idle_delta = idle.saturating_sub(previous.1);
            (total_delta > 0).then(|| {
                ((total_delta.saturating_sub(idle_delta).saturating_mul(100) / total_delta)
                    .min(100)) as u8
            })
        });
        let network = std::fs::read_to_string("/proc/net/dev").unwrap_or_default();
        let (network_rx_bytes, network_tx_bytes) = parse_network_counters(&network);
        let battery_percent = std::fs::read_to_string("/sys/class/power_supply/battery/capacity")
            .ok()
            .and_then(|value| value.trim().parse::<u8>().ok())
            .filter(|value| *value <= 100);
        let battery_status = std::fs::read_to_string("/sys/class/power_supply/battery/status")
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        GuardianSnapshot {
            cpu_usage_percent,
            ram_total_kb,
            ram_available_kb,
            network_rx_bytes,
            network_tx_bytes,
            storage_available_bytes: available_storage_bytes(data_directory),
            battery_percent,
            battery_status,
        }
    }

    pub fn set_telemetry_consent(&mut self, enabled: bool) {
        self.telemetry_consent = enabled;
        if !enabled {
            // Revoking consent immediately discards anything waiting for cloud upload.
            self.pending.clear();
        }
    }

    pub fn record(&mut self, event: DiagnosticEvent) -> bool {
        let key = format!("{:?}:{}:{}", event.kind, event.component, event.code);
        if self
            .last_seen
            .get(&key)
            .is_some_and(|last| event.occurred_at_unix_ms.saturating_sub(*last) < 60_000)
        {
            return false;
        }
        if self.last_seen.len() >= 256 {
            if let Some(oldest) = self
                .last_seen
                .iter()
                .min_by_key(|(_, timestamp)| **timestamp)
                .map(|(key, _)| key.clone())
            {
                self.last_seen.remove(&oldest);
            }
        }
        self.last_seen.insert(key, event.occurred_at_unix_ms);
        if self.local_events.len() == 64 {
            self.local_events.pop_front();
        }
        self.local_events.push_back(event.clone());
        // Events created before consent are never uploaded retroactively.
        if self.telemetry_consent {
            if self.pending.len() == 64 {
                self.pending.pop_front();
            }
            self.pending.push_back(event);
        }
        true
    }

    pub fn local_batch(&self, limit: usize) -> Vec<DiagnosticEvent> {
        self.local_events
            .iter()
            .rev()
            .take(limit.min(64))
            .cloned()
            .collect()
    }

    pub fn pending_batch(&self, limit: usize) -> Vec<DiagnosticEvent> {
        if !self.telemetry_consent {
            return Vec::new();
        }
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
    fn system_metrics_parsers_read_procfs_counters() {
        let (total, available) = parse_memory_info("MemTotal: 8000 kB\nMemAvailable: 3000 kB\n");
        assert_eq!((total, available), (8000, 3000));
        assert_eq!(
            parse_cpu_counters("cpu 100 0 50 850 10 0 0 0\ncpu0 20 0 10 170 2 0 0 0"),
            Some((1010, 860))
        );
        let (rx, tx) = parse_network_counters("Inter-| Receive | Transmit\n face |bytes packets errs drop fifo frame compressed multicast|bytes packets errs drop fifo colls carrier compressed\n wlan0: 100 1 0 0 0 0 0 0 250 2 0 0 0 0 0 0\n");
        assert_eq!((rx, tx), (100, 250));
    }

    #[test]
    fn guardian_requires_consent_and_deduplicates_repeated_events() {
        let mut monitor = GuardianMonitor::default();
        let event = DiagnosticEvent::sanitized(DiagnosticKind::Network, "chat", "timeout", 100_000);
        assert!(monitor.record(event.clone()));
        assert!(!monitor.record(event.clone()));
        assert_eq!(monitor.local_batch(8).len(), 1);
        assert!(monitor.pending_batch(8).is_empty());
        monitor.set_telemetry_consent(true);
        // Opting in later must not retroactively upload pre-consent diagnostics.
        assert!(monitor.pending_batch(8).is_empty());
        let next = DiagnosticEvent::sanitized(DiagnosticKind::Network, "chat", "offline", 200_000);
        assert!(monitor.record(next));
        assert_eq!(monitor.pending_batch(8).len(), 1);
        monitor.set_telemetry_consent(false);
        assert!(monitor.pending_batch(8).is_empty());
        assert_eq!(monitor.pending_count(), 0);
    }

    #[test]
    fn guardian_sanitizes_diagnostic_labels() {
        let event = DiagnosticEvent::sanitized(
            DiagnosticKind::ApiFailure,
            "chat/api?token=secret",
            "HTTP 401",
            42,
        );
        assert_eq!(event.component, "");
        assert_eq!(event.code, "HTTP_401");
        let serialized = serde_json::to_string(&event).unwrap();
        assert!(!serialized.contains("secret"));
        let compound = DiagnosticEvent::sanitized(
            DiagnosticKind::ApiFailure,
            "provider",
            "api_key=sk_test_123",
            43,
        );
        assert_eq!(compound.code, "");
        let separated = DiagnosticEvent::sanitized(
            DiagnosticKind::Network,
            "network interface",
            "connection timeout",
            44,
        );
        assert_eq!(separated.component, "network_interface");
    }
}
