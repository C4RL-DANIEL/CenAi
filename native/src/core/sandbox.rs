//! Safe remote execution request model. The executor must run in an isolated per-user container.
use std::{collections::HashSet, path::{Component, Path}};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SandboxQuota {
    pub cpu_count: u8,
    pub memory_mb: u32,
    pub timeout_seconds: u32,
    pub disk_mb: u32,
}

impl Default for SandboxQuota {
    fn default() -> Self {
        Self { cpu_count: 2, memory_mb: 2048, timeout_seconds: 300, disk_mb: 4096 }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SandboxRequest {
    pub user_scope: String,
    pub project_id: String,
    pub command: String,
    pub arguments: Vec<String>,
    pub working_directory: String,
    pub environment: Vec<(String, String)>,
    pub network_hosts: Vec<String>,
    pub quota: SandboxQuota,
}

fn valid_user_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 36
        && [8, 13, 18, 23].iter().all(|index| bytes[*index] == b'-')
        && bytes.iter().enumerate().all(|(index, byte)| {
            [8, 13, 18, 23].contains(&index) || byte.is_ascii_hexdigit()
        })
}

fn safe_project_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 80
        && value.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn contains_credential_material(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("bearer ")
        || lower.contains("-----begin ")
        || lower.contains("sk-")
        || lower.contains("sb_secret_")
        || lower.contains("api_key=") || lower.contains("api-key=")
        || lower.contains("access_token=") || lower.contains("refresh_token=")
        || lower.contains("password=") || lower.contains("secret=")
        || (lower.contains("eyj") && value.matches('.').count() >= 2)
}

fn safe_public_host(host: &str) -> bool {
    if host.is_empty() || host.len() > 253 || host != host.to_ascii_lowercase()
        || host == "localhost" || host.ends_with(".localhost")
        || host.ends_with(".local") || host.ends_with(".internal")
    {
        return false;
    }
    if let Ok(address) = host.parse::<std::net::IpAddr>() {
        return match address {
            std::net::IpAddr::V4(ip) => !(ip.is_private() || ip.is_loopback() || ip.is_link_local()
                || ip.is_unspecified() || ip.is_multicast() || ip.is_broadcast()),
            std::net::IpAddr::V6(ip) => !(ip.is_loopback() || ip.is_unique_local()
                || ip.is_unicast_link_local() || ip.is_unspecified() || ip.is_multicast()),
        };
    }
    let labels = host.split('.').collect::<Vec<_>>();
    labels.len() >= 2 && labels.iter().all(|label| {
        !label.is_empty() && label.len() <= 63
            && label.as_bytes().first().is_some_and(|byte| byte.is_ascii_alphanumeric())
            && label.as_bytes().last().is_some_and(|byte| byte.is_ascii_alphanumeric())
            && label.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    })
}

impl SandboxRequest {
    pub fn validate(&self, allowed_commands: &HashSet<String>, allowed_hosts: &HashSet<String>) -> Result<(), &'static str> {
        if !valid_user_uuid(&self.user_scope) || !safe_project_id(&self.project_id) {
            return Err("Sandbox jobs require a verified account UUID and safe project identifier");
        }
        if !allowed_commands.contains(&self.command) {
            return Err("Command is not in the sandbox allowlist");
        }
        if self.arguments.len() > 128 || self.arguments.iter().any(|argument| argument.len() > 4096
            || argument.contains('\0') || contains_credential_material(argument)) {
            return Err("Sandbox arguments exceed the allowed limits");
        }
        let path = Path::new(&self.working_directory);
        if path.is_absolute() || path.components().any(|component| !matches!(component, Component::Normal(_) | Component::CurDir)) {
            return Err("Sandbox working directory must remain inside the project");
        }
        if self.quota.cpu_count == 0 || self.quota.cpu_count > 4
            || self.quota.memory_mb < 256 || self.quota.memory_mb > 8192
            || self.quota.timeout_seconds == 0 || self.quota.timeout_seconds > 900
            || self.quota.disk_mb < 128 || self.quota.disk_mb > 16_384
        {
            return Err("Sandbox resource quota is outside the allowed range");
        }
        if self.environment.len() > 64 { return Err("Sandbox environment exceeds the allowed limits"); }
        let mut environment_names = HashSet::new();
        for (name, value) in &self.environment {
            let upper = name.to_ascii_uppercase();
            if upper.is_empty() || !upper.bytes().enumerate().all(|(index, byte)| {
                byte.is_ascii_uppercase() || (byte.is_ascii_digit() && index > 0) || byte == b'_'
            }) || upper.len() > 64 || !environment_names.insert(upper.clone()) || value.len() > 4096 || contains_credential_material(value) {
                return Err("Sandbox environment contains an invalid, duplicate, or sensitive value");
            }
            if upper.contains("TOKEN") || upper.contains("SECRET") || upper.contains("PASSWORD")
                || upper.contains("API_KEY") || upper.contains("SERVICE_ROLE") {
                return Err("Secrets and privileged credentials must never enter the sandbox");
            }
        }
        if self.network_hosts.len() > 32 || self.network_hosts.iter().any(|host| {
            !safe_public_host(host) || !allowed_hosts.contains(host)
        }) {
            return Err("Sandbox network host is not allowlisted");
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SandboxJobState { Queued, Starting, Running, Succeeded, Failed, Cancelled, TimedOut }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SandboxJobSummary {
    pub job_id: String,
    pub user_scope: String,
    pub state: SandboxJobState,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
}

impl SandboxJobSummary {
    fn redact_output(value: &str, max_bytes: usize) -> String {
        let mut output = String::new();
        for line in value.lines() {
            let lower = line.to_ascii_lowercase();
            let sensitive_assignment = [
                "authorization:", "bearer ", "api_key=", "api-key=", "access_token=",
                "refresh_token=", "password=", "password:", "passphrase=", "client_secret=",
                "service_role=", "private_key=", "api_key:", "api-key:", "access_token:",
                "refresh_token:", "secret:", "sk-", "sb_secret_", "-----begin ",
            ].iter().any(|needle| lower.contains(needle))
                || (lower.contains("eyj") && line.matches('.').count() >= 2);
            let safe_line = if sensitive_assignment { "[redacted sensitive output]" } else { line };
            if !output.is_empty() { output.push('\n'); }
            let remaining = max_bytes.saturating_sub(output.len());
            if remaining == 0 { break; }
            let mut bounded = safe_line.to_owned();
            while bounded.len() > remaining {
                bounded.pop();
            }
            output.push_str(&bounded);
            if output.len() >= max_bytes { break; }
        }
        output
    }

    pub fn sanitized_output(&self, max_bytes: usize) -> (String, String) {
        (
            Self::redact_output(&self.stdout, max_bytes),
            Self::redact_output(&self.stderr, max_bytes),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> (HashSet<String>, HashSet<String>) {
        (HashSet::from(["cargo".into(), "rustc".into(), "python".into()]),
         HashSet::from(["crates.io".into(), "static.crates.io".into()]))
    }

    fn request() -> SandboxRequest {
        SandboxRequest {
            user_scope: "550e8400-e29b-41d4-a716-446655440001".into(),
            project_id: "project-1".into(),
            command: "cargo".into(),
            arguments: vec!["test".into()],
            working_directory: ".".into(),
            environment: vec![("RUST_LOG".into(), "info".into())],
            network_hosts: vec!["crates.io".into()],
            quota: SandboxQuota::default(),
        }
    }

    #[test]
    fn valid_project_scoped_job_passes_policy() {
        let (commands, hosts) = policy();
        assert!(request().validate(&commands, &hosts).is_ok());
    }

    #[test]
    fn sandbox_output_redacts_credential_lines_and_obeys_byte_limits() {
        let summary = SandboxJobSummary {
            job_id: "job-1".into(),
            user_scope: "550e8400-e29b-41d4-a716-446655440001".into(),
            state: SandboxJobState::Failed,
            exit_code: Some(1),
            stdout: "build started\nAPI_KEY=secret-value\nbuild failed".into(),
            stderr: "Authorization: Bearer secret-token".into(),
            duration_ms: 10,
        };
        let (stdout, stderr) = summary.sanitized_output(128);
        assert!(stdout.contains("build started"));
        assert!(!stdout.contains("secret-value"));
        assert!(stdout.contains("[redacted sensitive output]"));
        assert!(!stderr.contains("secret-token"));
        let (short, _) = summary.sanitized_output(8);
        assert!(short.len() <= 8);
    }

    #[test]
    fn secrets_paths_and_unapproved_hosts_are_rejected() {
        let (commands, hosts) = policy();
        let mut job = request();
        job.environment.push(("OPENAI_API_KEY".into(), "never-copy-this".into()));
        assert_eq!(job.validate(&commands, &hosts), Err("Secrets and privileged credentials must never enter the sandbox"));
        let mut job = request();
        job.working_directory = "../".into();
        assert_eq!(job.validate(&commands, &hosts), Err("Sandbox working directory must remain inside the project"));
        let mut job = request();
        job.network_hosts.push("example.org".into());
        assert_eq!(job.validate(&commands, &hosts), Err("Sandbox network host is not allowlisted"));
        let mut job = request();
        job.network_hosts = vec!["127.0.0.1".into()];
        let mut unsafe_hosts = hosts.clone();
        unsafe_hosts.insert("127.0.0.1".into());
        assert!(job.validate(&commands, &unsafe_hosts).is_err());
        let mut job = request();
        job.network_hosts = vec!["localhost".into()];
        let mut unsafe_hosts = hosts.clone();
        unsafe_hosts.insert("localhost".into());
        assert!(job.validate(&commands, &unsafe_hosts).is_err());
    }

    #[test]
    fn sandbox_requires_verified_user_uuid_and_rejects_duplicate_or_secret_environment() {
        let (commands, hosts) = policy();
        let mut job = request();
        job.user_scope = "alice@example.com".into();
        assert!(job.validate(&commands, &hosts).is_err());
        let mut job = request();
        job.environment.push(("RUST_LOG".into(), "debug".into()));
        assert!(job.validate(&commands, &hosts).is_err());
        let mut job = request();
        job.environment.push((String::new(), "empty-name".into()));
        assert!(job.validate(&commands, &hosts).is_err());
        let mut job = request();
        job.environment.push(("SERVICE".into(), "Bearer eyJhbGciOiJIUzI1NiJ9.payload.signature".into()));
        assert!(job.validate(&commands, &hosts).is_err());
        let mut job = request();
        job.arguments.push("--api-key=private-value".into());
        assert!(job.validate(&commands, &hosts).is_err());
    }

    #[test]
    fn privileged_commands_and_excessive_quotas_are_rejected() {
        let (commands, hosts) = policy();
        let mut job = request();
        job.command = "su".into();
        assert_eq!(job.validate(&commands, &hosts), Err("Command is not in the sandbox allowlist"));
        let mut job = request();
        job.quota.memory_mb = 32_768;
        assert_eq!(job.validate(&commands, &hosts), Err("Sandbox resource quota is outside the allowed range"));
    }
}
