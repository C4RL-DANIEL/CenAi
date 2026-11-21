//! User-owned automation definitions with explicit pause and consent state.
use chrono::{DateTime, Duration, Utc};
use cron_parser::Schedule;
use serde::{Deserialize, Serialize};
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AutomationTrigger {
    pub automation_id: String,
    pub owner_scope: String,
    pub scheduled_at: i64,
    pub requires_approval: bool,
}

fn valid_user_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 36
        && [8, 13, 18, 23].iter().all(|index| bytes[*index] == b'-')
        && bytes.iter().enumerate().all(|(index, byte)| {
            [8, 13, 18, 23].contains(&index) || byte.is_ascii_hexdigit()
        })
}

impl Automation {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.id.trim().is_empty() || self.id.len() > 128
            || !self.id.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
            || !valid_user_uuid(&self.owner_scope) || self.title.trim().is_empty()
            || self.title.len() > 160 || self.title.chars().any(char::is_control) {
            return Err("Automation requires a safe identity, authenticated owner UUID and title");
        }
        if self.schedule.len() > 120 || self.schedule.parse::<Schedule>().is_err() {
            return Err("Automation schedule must be a valid five-field cron expression");
        }
        Ok(())
    }

    pub fn next_run_after(&self, after: DateTime<Utc>) -> Result<Option<DateTime<Utc>>, &'static str> {
        self.validate()?;
        let schedule = self.schedule.parse::<Schedule>().map_err(|_| "Automation cron schedule is invalid")?;
        Ok(schedule.next_after(&after))
    }

    pub fn pause(&mut self) { self.enabled = false; }
    pub fn resume(&mut self) { self.enabled = true; }
}

#[derive(Default)]
pub struct AutomationScheduler {
    automations: HashMap<String, Automation>,
}

impl AutomationScheduler {
    pub fn from_workspace(store: &WorkspaceStore) -> Result<Self, String> {
        let mut scheduler = Self::default();
        for automation in store.automations()? {
            scheduler.upsert(automation).map_err(|error| error.to_owned())?;
        }
        Ok(scheduler)
    }

    pub fn persist_to_workspace(&self, store: &WorkspaceStore) -> Result<(), String> {
        let automations = self.automations.values().cloned().collect::<Vec<_>>();
        store.save_automations(&automations)
    }

    pub fn poll_due_and_persist(&mut self, now: DateTime<Utc>, store: &WorkspaceStore) -> Result<Vec<AutomationTrigger>, String> {
        let previous = self.automations.clone();
        let due = self.poll_due(now);
        if let Err(error) = self.persist_to_workspace(store) {
            self.automations = previous;
            return Err(error);
        }
        Ok(due)
    }

    fn key(owner_scope: &str, automation_id: &str) -> String {
        format!("{owner_scope}:{automation_id}")
    }

    pub fn upsert(&mut self, automation: Automation) -> Result<(), &'static str> {
        automation.validate()?;
        let key = Self::key(&automation.owner_scope, &automation.id);
        self.automations.insert(key, automation);
        Ok(())
    }

    pub fn remove(&mut self, owner_scope: &str, automation_id: &str) -> bool {
        if !valid_user_uuid(owner_scope) { return false; }
        self.automations.remove(&Self::key(owner_scope, automation_id)).is_some()
    }

    pub fn pause(&mut self, owner_scope: &str, automation_id: &str) -> bool {
        self.update_enabled(owner_scope, automation_id, false)
    }

    pub fn resume(&mut self, owner_scope: &str, automation_id: &str) -> bool {
        self.update_enabled(owner_scope, automation_id, true)
    }

    fn update_enabled(&mut self, owner_scope: &str, automation_id: &str, enabled: bool) -> bool {
        if !valid_user_uuid(owner_scope) { return false; }
        let Some(automation) = self.automations.get_mut(&Self::key(owner_scope, automation_id)) else { return false; };
        automation.enabled = enabled;
        true
    }

    /// Returns each due occurrence once. The caller must persist the updated
    /// automation snapshot before dispatching its trigger to prevent replay.
    pub fn poll_due(&mut self, now: DateTime<Utc>) -> Vec<AutomationTrigger> {
        let mut due = Vec::new();
        for automation in self.automations.values_mut() {
            if !automation.enabled { continue; }
            let cursor = automation.last_scheduled_at
                .and_then(|timestamp| DateTime::<Utc>::from_timestamp(timestamp, 0))
                .unwrap_or_else(|| now - Duration::minutes(1));
            let Ok(Some(next)) = automation.next_run_after(cursor) else { continue; };
            if next <= now {
                automation.last_scheduled_at = Some(next.timestamp());
                due.push(AutomationTrigger {
                    automation_id: automation.id.clone(),
                    owner_scope: automation.owner_scope.clone(),
                    scheduled_at: next.timestamp(),
                    requires_approval: automation.requires_approval,
                });
            }
        }
        due.sort_by_key(|trigger| (trigger.scheduled_at, trigger.owner_scope.clone(), trigger.automation_id.clone()));
        due
    }

    pub fn snapshot(&self, owner_scope: &str) -> Vec<Automation> {
        if !valid_user_uuid(owner_scope) { return Vec::new(); }
        self.automations.values().filter(|automation| automation.owner_scope == owner_scope).cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNER: &str = "550e8400-e29b-41d4-a716-446655440010";
    const OTHER: &str = "550e8400-e29b-41d4-a716-446655440011";

    fn automation(id: &str, owner: &str, schedule: &str) -> Automation {
        Automation {
            id: id.into(),
            owner_scope: owner.into(),
            title: "Scheduled report".into(),
            schedule: schedule.into(),
            enabled: true,
            requires_approval: true,
            last_scheduled_at: None,
        }
    }

    #[test]
    fn automation_rejects_unscoped_owners_and_invalid_schedules() {
        let mut automation = automation("daily-digest", "not-a-user", "0 9 * * *");
        assert!(automation.validate().is_err());
        automation.owner_scope = OWNER.into();
        assert!(automation.validate().is_ok());
        automation.schedule = "90 25 * * *".into();
        assert!(automation.validate().is_err());
        automation.schedule = "*/15 9-17 * * 1-5".into();
        assert!(automation.validate().is_ok());
    }

    #[test]
    fn automation_scheduler_emits_each_due_occurrence_once_and_respects_pause() {
        let mut scheduler = AutomationScheduler::default();
        let mut item = automation("quarter-hour", OWNER, "*/15 * * * *");
        item.last_scheduled_at = Some(DateTime::parse_from_rfc3339("2026-11-21T09:00:00Z").unwrap().timestamp());
        scheduler.upsert(item).unwrap();
        let now = DateTime::parse_from_rfc3339("2026-11-21T09:15:30Z").unwrap().with_timezone(&Utc);
        let due = scheduler.poll_due(now);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].scheduled_at, DateTime::parse_from_rfc3339("2026-11-21T09:15:00Z").unwrap().timestamp());
        assert!(due[0].requires_approval);
        assert!(scheduler.poll_due(now).is_empty());
        assert!(scheduler.pause(OWNER, "quarter-hour"));
        assert!(scheduler.poll_due(now + Duration::minutes(15)).is_empty());
        assert!(scheduler.resume(OWNER, "quarter-hour"));
    }

    #[test]
    fn due_automation_occurrence_is_persisted_before_it_is_dispatched() {
        let root = std::env::temp_dir().join(format!("cenai-automation-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let store = WorkspaceStore::new(&root, OWNER).unwrap();
        let mut item = automation("quarter-hour", OWNER, "*/15 * * * *");
        item.last_scheduled_at = Some(DateTime::parse_from_rfc3339("2026-11-21T09:00:00Z").unwrap().timestamp());
        store.save_automation(item).unwrap();
        let mut scheduler = AutomationScheduler::from_workspace(&store).unwrap();
        let now = DateTime::parse_from_rfc3339("2026-11-21T09:15:30Z").unwrap().with_timezone(&Utc);
        assert_eq!(scheduler.poll_due_and_persist(now, &store).unwrap().len(), 1);
        let reloaded = AutomationScheduler::from_workspace(&store).unwrap();
        assert!(reloaded.automations.values().all(|automation| automation.last_scheduled_at
            == Some(DateTime::parse_from_rfc3339("2026-11-21T09:15:00Z").unwrap().timestamp())));
        let mut reloaded = reloaded;
        assert!(reloaded.poll_due_and_persist(now, &store).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn automation_scheduler_isolates_accounts_and_supports_next_run_queries() {
        let mut scheduler = AutomationScheduler::default();
        scheduler.upsert(automation("daily", OWNER, "0 9 * * *")).unwrap();
        scheduler.upsert(automation("daily", OTHER, "0 9 * * *")).unwrap();
        let now = DateTime::parse_from_rfc3339("2026-11-21T08:30:00Z").unwrap().with_timezone(&Utc);
        let next = scheduler.snapshot(OWNER).pop().unwrap().next_run_after(now).unwrap().unwrap();
        assert_eq!(next, DateTime::parse_from_rfc3339("2026-11-21T09:00:00Z").unwrap());
        assert_eq!(scheduler.snapshot(OWNER).len(), 1);
        assert_eq!(scheduler.snapshot(OTHER).len(), 1);
        assert!(scheduler.remove(OWNER, "daily"));
        assert_eq!(scheduler.snapshot(OWNER).len(), 0);
        assert_eq!(scheduler.snapshot(OTHER).len(), 1);
    }
}
