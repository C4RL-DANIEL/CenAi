//! User-owned automation definitions with explicit pause and consent state.
use chrono::{DateTime, Duration, Utc};
use cron_parser::Schedule;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use crate::core::workspace::WorkspaceStore;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Automation {
    pub id: String,
    pub owner_scope: String,
    pub title: String,
    /// Standard five-field cron expression evaluated in UTC.
    pub schedule: String,
    pub enabled: bool,
    pub requires_approval: bool,
    #[serde(default)]
    pub last_scheduled_at: Option<i64>,
    /// A due occurrence remains pending until its review reminder is durably saved.
    #[serde(default)]
    pub pending_scheduled_at: Option<i64>,
}

impl Automation {
    /// Builds a user-reviewed handoff prompt for a due occurrence. This never
    /// authorizes execution; the agent must still obtain per-action approval.
    pub fn review_prompt(&self) -> Option<String> {
        if !self.enabled || !self.requires_approval { return None; }
        let scheduled_at = self.pending_scheduled_at?;
        let scheduled = DateTime::<Utc>::from_timestamp(scheduled_at, 0)?;
        let title = self.title.trim();
        if title.is_empty() || title.len() > 2_000 { return None; }
        Some(format!(
            "A scheduled CenAI task is ready for review.\nTask: {title}\nScheduled time: {}\n\nPlease prepare a plan and explain what¶»§q«^