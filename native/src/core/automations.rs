//! User-owned automation definitions with explicit pause and consent state.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Automation {
    pub id: String,
    pub owner_scope: String,
    pub title: String,
    pub schedule: String,
    pub enabled: bool,
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

fn valid_cron_field(field: &str, minimum: u32, maximum: u32) -> bool {
    if field.is_empty() { return false; }
    field.split(',').all(|part| {
        if part.is_empty() { return false; }
        let mut step_parts = part.split('/');
        let base = step_parts.next().unwrap_or_default();
        let step = step_parts.next();
        if step_parts.next().is_some() { return false; }
        if let Some(step) = step {
            if step.parse::<u32>().map_or(true, |value| value == 0 || value > maximum - minimum + 1) {
                return false;
            }
        }
        if base == "*" { return true; }
        let mut range = base.split('-');
        let first = range.next().and_then(|value| value.parse::<u32>().ok());
        let second = range.next();
        if range.next().is_some() { return false; }
        let Some(first) = first else { return false; };
        if first < minimum || first > maximum { return false; }
        if let Some(second) = second {
            let Some(last) = second.parse::<u32>().ok() else { return false; };
            last >= first && last <= maximum
        } else {
            true
        }
    })
}

fn valid_cron_schedule(schedule: &str) -> bool {
    let fields = schedule.split_whitespace().collect::<Vec<_>>();
    fields.len() == 5
        && valid_cron_field(fields[0], 0, 59)
        && valid_cron_field(fields[1], 0, 23)
        && valid_cron_field(fields[2], 1, 31)
        && valid_cron_field(fields[3], 1, 12)
        && valid_cron_field(fields[4], 0, 7)
}

impl Automation {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.id.trim().is_empty() || self.id.len() > 128
            || !self.id.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
            || !valid_user_uuid(&self.owner_scope) || self.title.trim().is_empty() || self.title.len() > 160 {
            return Err("Automation requires a safe identity, authenticated owner UUID and title");
        }
        if self.schedule.len() > 120 || !valid_cron_schedule(&self.schedule) {
            return Err("Automation schedule must be a valid five-field cron expression");
        }
        Ok(())
    }

    pub fn pause(&mut self) { self.enabled = false; }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automation_rejects_unscoped_owners_and_invalid_schedules() {
        let mut automation = Automation {
            id: "daily-digest".into(),
            owner_scope: "not-a-user".into(),
            title: "Daily digest".into(),
            schedule: "0 9 * * *".into(),
            enabled: true,
            requires_approval: true,
        };
        assert!(automation.validate().is_err());
        automation.owner_scope = "550e8400-e29b-41d4-a716-446655440010".into();
        assert!(automation.validate().is_ok());
        automation.schedule = "90 25 * * *".into();
        assert!(automation.validate().is_err());
        automation.schedule = "*/15 9-17 * * 1-5".into();
        assert!(automation.validate().is_ok());
    }

    #[test]
    fn automation_is_scoped_and_can_be_paused() {
        let mut automation = Automation {
            id: "weekly-report".into(), owner_scope: "550e8400-e29b-41d4-a716-446655440010".into(), title: "Weekly report".into(),
            schedule: "0 9 * * 1".into(), enabled: true, requires_approval: true,
        };
        assert!(automation.validate().is_ok());
        automation.pause();
        assert!(!automation.enabled);
    }
}
