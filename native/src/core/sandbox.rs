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

impl SandboxRequest {
    pub fn validate(&self, allowed_commands: &HashSet<String>, allowed_hosts: &HashSet<String>) -> Result<(), &'static str> {
        if self.user_scope.trim().is_empty() || self.project_id.trim().is_empty() {
            return Err("Sandbox jobs must be scoped to an authenticated user and project");
        }
        if !allowed_commands.contains(&self.command) {
            return Err("Command is not in the sandbox allowlist");
        }
        if self.arguments.len() > 128 || self.arguments.iter().any(|argument| argument.len() > 4096) {
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
        for (name, _) in &self.environment {
            let upper = name.to_ascii_uppercase();
            if upper.contains("TOKEN") || upper.contains("SECRET") || upper.contains("PASSWORD")
                || upper.contains("API_KEY") || upper.contains("SERVICE_ROLE") {
                return Err("Secrets and privileged credentials must never enter the sandbox");
            }
        }
        if self.network_hosts.iter().any(|host| !allowed_hosts.contains(host)) {
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
    pub fn sanitized_output(&self, max_bytes: usize) -> (String, String) {
        (
            self.stdout.chars().take(max_bytes).collect(),
            self.stderr.chars().take(max_bytes).collect(),
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
            user_scope: "user-hash".into(),
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
