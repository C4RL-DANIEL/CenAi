//! Persistent task-plan state machine; execution is delegated to authorized tools.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole { Planner, Researcher, Developer, Tester, SecurityReviewer, QualityReviewer }

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus { Pending, Running, WaitingForApproval, Completed, Failed, Cancelled }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TaskStep {
    pub id: u64,
    pub role: AgentRole,
    pub title: String,
    pub status: StepStatus,
    pub depends_on: Vec<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TaskPlan {
    pub id: String,
    pub goal: String,
    pub steps: Vec<TaskStep>,
    pub stopped: bool,
}

impl TaskPlan {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.id.trim().is_empty() || self.id.len() > 128 || self.goal.trim().is_empty() || self.goal.len() > 12_000 {
            return Err("Task identity and goal are required and must fit the supported limits");
        }
        if self.stopped || self.steps.is_empty() || self.steps.len() > 12 {
            return Err("Task plan must contain 1-12 steps and cannot start in a stopped state");
        }
        let ids: std::collections::HashSet<u64> = self.steps.iter().map(|step| step.id).collect();
        if ids.len() != self.steps.len() { return Err("Task step identifiers must be unique"); }
        if self.steps.iter().any(|step| {
            step.id == 0 || step.title.trim().is_empty() || step.title.len() > 240
                || step.status != StepStatus::Pending || step.depends_on.len() > 12
                || {
                    let unique: std::collections::HashSet<u64> = step.depends_on.iter().copied().collect();
                    unique.len() != step.depends_on.len()
                }
                || step.depends_on.iter().any(|id| !ids.contains(id) || *id >= step.id)
        }) {
            return Err("Task plan contains an invalid step, status or dependency");
        }
        let mut completed = std::collections::HashSet::new();
        loop {
            let before = completed.len();
            for step in &self.steps {
                if step.depends_on.iter().all(|dependency| completed.contains(dependency)) {
                    completed.insert(step.id);
                }
            }
            if completed.len() == self.steps.len() { break; }
            if completed.len() == before { return Err("Task plan contains a dependency cycle"); }
        }
        Ok(())
    }

    pub fn stop(&mut self) {
        self.stopped = true;
        for step in &mut self.steps {
            if matches!(step.status, StepStatus::Pending | StepStatus::Running) {
                step.status = StepStatus::Cancelled;
            }
        }
    }

    pub fn ready_steps(&self) -> Vec<u64> {
        if self.stopped { return Vec::new(); }
        self.steps.iter().filter(|step| step.status == StepStatus::Pending
            && step.depends_on.iter().all(|dependency| self.steps.iter().any(|candidate| candidate.id == *dependency && candidate.status == StepStatus::Completed)))
            .map(|step| step.id).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dependencies_gate_execution_and_stop_is_terminal() {
        let mut plan = TaskPlan {
            id: "task-1".into(), goal: "Build and verify".into(), stopped: false,
            steps: vec![
                TaskStep { id: 1, role: AgentRole::Developer, title: "Implement".into(), status: StepStatus::Pending, depends_on: vec![] },
                TaskStep { id: 2, role: AgentRole::Tester, title: "Test".into(), status: StepStatus::Pending, depends_on: vec![1] },
            ],
        };
        assert_eq!(plan.ready_steps(), vec![1]);
        plan.steps[0].status = StepStatus::Completed;
        assert_eq!(plan.ready_steps(), vec![2]);
        plan.stop();
        assert!(plan.ready_steps().is_empty());
        assert_eq!(plan.steps[1].status, StepStatus::Cancelled);
    }

    #[test]
    fn planner_cannot_claim_completed_steps_or_create_forward_dependencies() {
        let mut plan = TaskPlan {
            id: "task-1".into(), goal: "Build and verify".into(), stopped: false,
            steps: vec![
                TaskStep { id: 1, role: AgentRole::Developer, title: "Implement".into(), status: StepStatus::Pending, depends_on: vec![] },
                TaskStep { id: 2, role: AgentRole::Tester, title: "Test".into(), status: StepStatus::Pending, depends_on: vec![1] },
            ],
        };
        plan.steps[0].status = StepStatus::Completed;
        assert!(plan.validate().is_err());
        plan.steps[0].status = StepStatus::Pending;
        plan.steps[0].depends_on = vec![2];
        assert!(plan.validate().is_err());
        plan.steps[0].depends_on.clear();
        plan.steps[1].depends_on = vec![1, 1];
        assert!(plan.validate().is_err());
    }

    #[test]
    fn dependency_cycles_are_rejected() {
        let plan = TaskPlan {
            id: "cyclic".into(), goal: "Invalid plan".into(), stopped: false,
            steps: vec![
                TaskStep { id: 1, role: AgentRole::Developer, title: "A".into(), status: StepStatus::Pending, depends_on: vec![2] },
                TaskStep { id: 2, role: AgentRole::Tester, title: "B".into(), status: StepStatus::Pending, depends_on: vec![1] },
            ],
        };
        assert_eq!(plan.validate(), Err("Task plan contains a dependency cycle"));
    }
}
