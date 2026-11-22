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
    /// Bounded AI-generated result; it is data, never an executable command.
    #[serde(default)]
    pub output: Option<String>,
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

    /// Validates a persisted execution snapshot while allowing runtime statuses.
    pub fn validate_persisted(&self) -> Result<(), &'static str> {
        let mut structure = self.clone();
        structure.stopped = false;
        for step in &mut structure.steps { step.status = StepStatus::Pending; }
        structure.validate()?;
        if self.stopped && self.steps.iter().any(|step| matches!(step.status, StepStatus::Pending | StepStatus::Running | StepStatus::WaitingForApproval)) {
            return Err("Stopped task plans cannot contain active steps");
        }
        for step in &self.steps {
            if !matches!(step.status, StepStatus::Pending | StepStatus::Cancelled)
                && !step.depends_on.iter().all(|dependency| self.steps.iter().any(|candidate| candidate.id == *dependency && candidate.status == StepStatus::Completed)) {
                return Err("A task step cannot advance before all dependencies complete");
            }
        }
        Ok(())
    }

    pub fn begin_step(&mut self, step_id: u64) -> Result<(), &'static str> {
        if self.stopped { return Err("Task plan is stopped"); }
        let ready = self.steps.iter().find(|step| step.id == step_id)
            .is_some_and(|step| step.status == StepStatus::Pending
                && step.depends_on.iter().all(|dependency| self.steps.iter().any(|candidate| candidate.id == *dependency && candidate.status == StepStatus::Completed)));
        if !ready { return Err("Task step is not ready to run"); }
        let step = self.steps.iter_mut().find(|step| step.id == step_id).ok_or("Task step does not exist")?;
        step.status = StepStatus::Running;
        Ok(())
    }

    pub fn request_step_approval(&mut self, step_id: u64) -> Result<(), &'static str> {
        if self.stopped { return Err("Task plan is stopped"); }
        let ready = self.steps.iter().find(|step| step.id == step_id)
            .is_some_and(|step| step.status == StepStatus::Pending
                && step.depends_on.iter().all(|dependency| self.steps.iter().any(|candidate| candidate.id == *dependency && candidate.status == StepStatus::Completed)));
        if !ready { return Err("Task step is not ready for approval"); }
        self.steps.iter_mut().find(|step| step.id == step_id).ok_or("Task step does not exist")?.status = StepStatus::WaitingForApproval;
        Ok(())
    }

    pub fn resolve_step_approval(&mut self, step_id: u64, approved: bool) -> Result<(), &'static str> {
        if self.stopped { return Err("Task plan is stopped"); }
        let step = self.steps.iter_mut().find(|step| step.id == step_id).ok_or("Task step does not exist")?;
        if step.status != StepStatus::WaitingForApproval {
            return Err("Task step is not waiting for approval");
        }
        step.status = if approved { StepStatus::Pending } else { StepStatus::Cancelled };
        Ok(())
    }

    pub fn complete_step(&mut self, step_id: u64) -> Result<(), &'static str> {
        let step = self.steps.iter_mut().find(|step| step.id == step_id).ok_or("Task step does not exist")?;
        if step.status != StepStatus::Running { return Err("Only a running task step can complete"); }
        step.status = StepStatus::Completed;
        Ok(())
    }

    pub fn fail_step(&mut self, step_id: u64) -> Result<(), &'static str> {
        let step = self.steps.iter_mut().find(|step| step.id == step_id).ok_or("Task step does not exist")?;
        if step.status != StepStatus::Running { return Err("Only a running task step can fail"); }
        step.status = StepStatus::Failed;
        Ok(())
    }

    pub fn stop(&mut self) {
        self.stopped = true;
        for step in &mut self.steps {
            if matches!(step.status, StepStatus::Pending | StepStatus::Running | StepStatus::WaitingForApproval) {
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
                TaskStep { id: 1, role: AgentRole::Developer, title: "Implement".into(), status: StepStatus::Pending, depends_on: vec![]
                        output: None, },
                TaskStep { id: 2, role: AgentRole::Tester, title: "Test".into(), status: StepStatus::Pending, depends_on: vec![1]
                        output: None, },
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
                TaskStep { id: 1, role: AgentRole::Developer, title: "Implement".into(), status: StepStatus::Pending, depends_on: vec![]
                        output: None, },
                TaskStep { id: 2, role: AgentRole::Tester, title: "Test".into(), status: StepStatus::Pending, depends_on: vec![1]
                        output: None, },
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
    fn execution_state_machine_enforces_dependencies_and_approval() {
        let mut plan = TaskPlan {
            id: "task-2".into(), goal: "Run a protected workflow".into(), stopped: false,
            steps: vec![
                TaskStep { id: 1, role: AgentRole::Developer, title: "Prepare".into(), status: StepStatus::Pending, depends_on: vec![]
                        output: None, },
                TaskStep { id: 2, role: AgentRole::Tester, title: "Run external action".into(), status: StepStatus::Pending, depends_on: vec![1]
                        output: None, },
            ],
        };
        assert!(plan.begin_step(2).is_err());
        plan.begin_step(1).unwrap();
        assert!(plan.complete_step(2).is_err());
        plan.complete_step(1).unwrap();
        plan.request_step_approval(2).unwrap();
        assert_eq!(plan.ready_steps(), Vec::<u64>::new());
        plan.resolve_step_approval(2, true).unwrap();
        assert_eq!(plan.ready_steps(), vec![2]);
        plan.begin_step(2).unwrap();
        plan.complete_step(2).unwrap();
        assert_eq!(plan.steps[1].status, StepStatus::Completed);
    }

    #[test]
    fn denied_approval_and_failed_dependencies_block_downstream_steps() {
        let mut plan = TaskPlan {
            id: "task-3".into(), goal: "Test denied work".into(), stopped: false,
            steps: vec![
                TaskStep { id: 1, role: AgentRole::Developer, title: "Prepare".into(), status: StepStatus::Pending, depends_on: vec![]
                        output: None, },
                TaskStep { id: 2, role: AgentRole::Tester, title: "External action".into(), status: StepStatus::Pending, depends_on: vec![1]
                        output: None, },
                TaskStep { id: 3, role: AgentRole::QualityReviewer, title: "Review".into(), status: StepStatus::Pending, depends_on: vec![2]
                        output: None, },
            ],
        };
        plan.request_step_approval(1).unwrap();
        plan.resolve_step_approval(1, false).unwrap();
        assert!(plan.ready_steps().is_empty());
        assert!(plan.begin_step(2).is_err());
        plan.steps[0].status = StepStatus::Pending;
        plan.begin_step(1).unwrap();
        plan.fail_step(1).unwrap();
        assert!(plan.ready_steps().is_empty());
    }

    #[test]
    fn runtime_statuses_and_stopped_plans_can_be_persisted_safely() {
        let mut plan = TaskPlan {
            id: "task-persist".into(), goal: "Persist execution".into(), stopped: false,
            steps: vec![
                TaskStep { id: 1, role: AgentRole::Developer, title: "Implement".into(), status: StepStatus::Pending, depends_on: vec![]
                        output: None, },
                TaskStep { id: 2, role: AgentRole::Tester, title: "Verify".into(), status: StepStatus::Pending, depends_on: vec![1]
                        output: None, },
            ],
        };
        plan.begin_step(1).unwrap();
        assert!(plan.validate_persisted().is_ok());
        plan.complete_step(1).unwrap();
        plan.request_step_approval(2).unwrap();
        assert!(plan.validate_persisted().is_ok());
        plan.stop();
        assert!(plan.validate_persisted().is_ok());
    }

    #[test]
    fn dependency_cycles_are_rejected() {
        let plan = TaskPlan {
            id: "cyclic".into(), goal: "Invalid plan".into(), stopped: false,
            steps: vec![
                TaskStep { id: 1, role: AgentRole::Developer, title: "A".into(), status: StepStatus::Pending, depends_on: vec![2]
                        output: None, },
                TaskStep { id: 2, role: AgentRole::Tester, title: "B".into(), status: StepStatus::Pending, depends_on: vec![1]
                        output: None, },
            ],
        };
        assert!(plan.validate().is_err());
    }
}
