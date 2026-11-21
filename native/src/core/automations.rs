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

impl Automation {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.id.trim().is_empty() || self.owner_scope.trim().is_empty() || self.title.trim().is_empty() {
            return Err("Automation identity, owner and title are required");
        }
        if self.schedule.trim().is_empty() || self.schedule.len() > 120 {
            return Err("Automation schedule is invalid");
        }
        Ok(())
    }

    pub fn pause(&mut self) { self.enabled = false; }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automation_is_scoped_and_can_be_paused() {
        let mut automation = Automation {
            id: "weekly-report".into(), owner_scope: "user-hash".into(), title: "Weekly report".into(),
            schedule: "0 9 * * 1".into(), enabled: true, requires_approval: true,
        };
        assert!(automation.validate().is_ok());
        automation.pause();
        assert!(!automation.enabled);
    }
}
