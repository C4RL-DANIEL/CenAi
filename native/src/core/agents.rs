//! Persistent task-plan state machine; execution is delegated to authorized tools.
use serde::{Deserialize, Serialize};
use crate::core::permissions::{approval_policy, ApprovalDecision, OperationClass};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole { Planner, Researcher, Developer, Tester, SecurityReviewer, QualityReviewer }

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus { Pending, Running, WaitingForApproval, Completed, Blocked, Failed, Cancelled }

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepExecutionDisposition { Completed, Blocked }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StepExecutionResult { pub disposition: StepExecutionDisposition, pub output: String }

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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskExecutionEventKind {
    Started, ApprovalRequested, ApprovalGranted, ApprovalDenied, ApprovalConsumed,
    Completed, Blocked, Failed, Retried, Interrupted, PlanStopped, PlanPaused, PlanResumed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TaskExecutionEvent {
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub step_id: Option<u64>,
    pub kind: TaskExecutionEventKind,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TaskPlan {
    pub id: String,
    pub goal: String,
    pub steps: Vec<TaskStep>,
    pub stopped: bool,
    /// Bounded audit history; absent in older saved plans.
    #[serde(default)]
    pub events: Vec<TaskExecutionEvent>,
}

impl TaskPlan {
    pub fn is_paused(&self) -> bool {
        self.events.iter().rev().find(|event| event.step_id.is_none()
            && matches!(event.kind, TaskExecutionEventKind::PlanPaused | TaskExecutionEventKind::PlanResumed | TaskExecutionEventKind::PlanStopped))
            .is_some_and(|event| event.kind == TaskExecutionEventKind::PlanPaused)
    }

    pub fn pause(&mut self) -> Result<(), &'static str> {
        if self.stopped { return Err("Task plan is stopped"); }
        if self.is_paused() { return Err("Task plan is already paused"); }
        self.record_event(None, TaskExecutionEventKind::PlanPaused, None);
        Ok(())
    }

    pub fn resume(&mut self) -> Result<(), &'static str> {
        if self.stopped { return Err("Task plan is stopped"); }
        if !self.is_paused() { return Err("Task plan is not paused"); }
        self.record_event(None, TaskExecutionEventKind::PlanResumed, None);
        Ok(())
    }

    fn record_event(&mut self, step_id: Option<u64>, kind: TaskExecutionEventKind, detail: Option<&str>) {
        let sequence = self.events.last().map_or(1, |event| event.sequence.saturating_add(1));
        let timestamp_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default().as_millis().min(u64::MAX as u128) as u64;
        self.events.push(TaskExecutionEvent {
            sequence, timestamp_ms, step_id, kind,
            detail: detail.map(|value| value.chars().take(512).collect()),
        });
        if self.events.len() > 128 {
            let excess = self.events.len() - 128;
            self.events.drain(0..excess);
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.id.trim().is_empty() || self.id.len() > 128 || self.goal.trim().is_empty() || self.goal.len() > 12_000 {
            return Err("Task identity and goal are required and must fit the supported limits");
        }
        if self.stopped || !self.events.is_empty() || self.steps.is_empty() || self.steps.len() > 12 {
            return Err("Task plan must contain 1-12 steps and cannot start in a stopped state");
        }
        let ids: std::collections::HashSet<u64> = self.steps.iter().map(|step| step.id).collect();
        if ids.len() != self.steps.len() { return Err("Task step identifiers must be unique"); }
        if self.steps.iter().any(|step| {
            step.id == 0 || step.title.trim().is_empty() || step.title.len() > 240
                || step.status != StepStatus::Pending || step.output.is_some() || step.depends_on.len() > 12
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
        structure.events.clear();
        for step in &mut structure.steps { step.status = StepStatus::Pending; step.output = None; }
        structure.validate()?;
        if self.stopped && self.steps.iter().any(|step| matches!(step.status, StepStatus::Pending | StepStatus::Running | StepStatus::WaitingForApproval)) {
            return Err("Stopped task plans cannot contain active steps");
        }
        if self.events.len() > 128 { return Err("Task execution history exceeds the 128-event limit"); }
        let step_ids: std::collections::HashSet<u64> = self.steps.iter().map(|step| step.id).collect();
        let mut previous_sequence = 0u64;
        for event in &self.events {
            if event.sequence <= previous_sequence
                || event.step_id.is_some_and(|step_id| !step_ids.contains(&step_id))
                || event.detail.as_ref().is_some_and(|detail| detail.len() > 2048) {
                return Err("Task execution history contains an invalid event");
            }
            previous_sequence = event.sequence;
        }
        for step in &self.steps {
            if step.output.as_ref().is_some_and(|output| output.is_empty() || output.len() > 24_000) {
                return Err("A task step result is empty or exceeds the 24 KB limit");
            }
            if step.status == StepStatus::Blocked && step.output.is_none() {
                return Err("A blocked task step must retain its blocker explanation");
            }
            if !matches!(step.status, StepStatus::Pending | StepStatus::Cancelled)
                && !step.depends_on.iter().all(|dependency| self.steps.iter().any(|candidate| candidate.id == *dependency && candidate.status == StepStatus::Completed)) {
                return Err("A task step cannot advance before all dependencies complete");
            }
        }
        Ok(())
    }

    pub fn begin_step(&mut self, step_id: u64) -> Result<(), &'static str> {
        if self.stopped { return Err("Task plan is stopped"); }
        if self.is_paused() { return Err("Task plan is paused"); }
        let ready = self.steps.iter().find(|step| step.id == step_id)
            .is_some_and(|step| step.status == StepStatus::Pending
                && step.depends_on.iter().all(|dependency| self.steps.iter().any(|candidate| candidate.id == *dependency && candidate.status == StepStatus::Completed)));
        if !ready { return Err("Task step is not ready to run"); }
        let step = self.steps.iter_mut().find(|step| step.id == step_id).ok_or("Task step does not exist")?;
        step.status = StepStatus::Running;
        self.record_event(Some(step_id), TaskExecutionEventKind::Started, None);
        Ok(())
    }

    pub fn request_step_approval(&mut self, step_id: u64) -> Result<(), &'static str> {
        self.request_step_approval_with_binding(step_id, None)
    }

    fn request_step_approval_with_binding(&mut self, step_id: u64, binding: Option<&str>) -> Result<(), &'static str> {
        if self.stopped { return Err("Task plan is stopped"); }
        if self.is_paused() { return Err("Task plan is paused"); }
        let ready = self.steps.iter().find(|step| step.id == step_id)
            .is_some_and(|step| step.status == StepStatus::Pending
                && step.depends_on.iter().all(|dependency| self.steps.iter().any(|candidate| candidate.id == *dependency && candidate.status == StepStatus::Completed)));
        if !ready { return Err("Task step is not ready for approval"); }
        self.steps.iter_mut().find(|step| step.id == step_id).ok_or("Task step does not exist")?.status = StepStatus::WaitingForApproval;
        self.record_event(Some(step_id), TaskExecutionEventKind::ApprovalRequested, binding);
        Ok(())
    }

    /// Applies the central permission policy before a step can enter execution.
    /// Sensitive, external-account and destructive operations require a fresh
    /// per-step approval event even when a broad capability was previously granted.
    pub fn authorize_and_begin_step(
        &mut self,
        step_id: u64,
        class: OperationClass,
        explicitly_granted: bool,
    ) -> Result<ApprovalDecision, &'static str> {
        if self.stopped { return Err("Task plan is stopped"); }
        if self.is_paused() { return Err("Task plan is paused"); }
        let step = self.steps.iter().find(|step| step.id == step_id).ok_or("Task step does not exist")?;
        if step.status != StepStatus::Pending
            || !step.depends_on.iter().all(|dependency| self.steps.iter().any(|candidate|
                candidate.id == *dependency && candidate.status == StepStatus::Completed)) {
            return Err("Task step is not ready to be authorized");
        }

        let decision = approval_policy(class, explicitly_granted);
        if decision == ApprovalDecision::Deny {
            return Err("Operation is denied by the permission policy");
        }

        let requires_fresh_approval = matches!(
            class,
            OperationClass::ExternalAccount | OperationClass::Sensitive | OperationClass::Destructive
        );
        let operation_class = match class {
            OperationClass::ReadOnly => "read_only",
            OperationClass::Reversible => "reversible",
            OperationClass::ExternalAccount => "external_account",
            OperationClass::Sensitive => "sensitive",
            OperationClass::Destructive => "destructive",
        };
        // Bind approval to both the operation class and the exact step description.
        // A grant for a less risky action cannot be replayed for a changed action.
        let binding = format!("{operation_class}:{}", step.title);
        let approval_event = self.events.iter().rev().find(|event| event.step_id == Some(step_id)
            && matches!(event.kind, TaskExecutionEventKind::ApprovalGranted
                | TaskExecutionEventKind::ApprovalDenied | TaskExecutionEventKind::ApprovalConsumed));
        let has_fresh_approval = approval_event.is_some_and(|event|
            event.kind == TaskExecutionEventKind::ApprovalGranted
                && event.detail.as_deref() == Some(binding.as_str()));
        let approval_required = decision == ApprovalDecision::RequireUserApproval || requires_fresh_approval;

        if approval_required && !has_fresh_approval {
            self.request_step_approval_with_binding(step_id, Some(&binding))?;
            return Ok(ApprovalDecision::RequireUserApproval);
        }

        self.begin_step(step_id)?;
        if approval_required {
            // A grant authorizes exactly one execution attempt. Retrying a failed
            // action requires a fresh review, even if its description is unchanged.
            self.record_event(Some(step_id), TaskExecutionEventKind::ApprovalConsumed, Some(&binding));
        }
        Ok(ApprovalDecision::Allow)
    }

    pub fn resolve_step_approval(&mut self, step_id: u64, approved: bool) -> Result<(), &'static str> {
        if self.stopped { return Err("Task plan is stopped"); }
        let step = self.steps.iter_mut().find(|step| step.id == step_id).ok_or("Task step does not exist")?;
        if step.status != StepStatus::WaitingForApproval {
            return Err("Task step is not waiting for approval");
        }
        step.status = if approved { StepStatus::Pending } else { StepStatus::Cancelled };
        let binding = self.events.iter().rev().find(|event|
            event.step_id == Some(step_id) && event.kind == TaskExecutionEventKind::ApprovalRequested
        ).and_then(|event| event.detail.clone());
        self.record_event(Some(step_id), if approved { TaskExecutionEventKind::ApprovalGranted } else { TaskExecutionEventKind::ApprovalDenied }, binding.as_deref());
        Ok(())
    }

    pub fn complete_step(&mut self, step_id: u64) -> Result<(), &'static str> {
        let step = self.steps.iter_mut().find(|step| step.id == step_id).ok_or("Task step does not exist")?;
        if step.status != StepStatus::Running { return Err("Only a running task step can complete"); }
        step.status = StepStatus::Completed;
        self.record_event(Some(step_id), TaskExecutionEventKind::Completed, None);
        Ok(())
    }

    pub fn block_step(&mut self, step_id: u64) -> Result<(), &'static str> {
        let step = self.steps.iter_mut().find(|step| step.id == step_id).ok_or("Task step does not exist")?;
        if step.status != StepStatus::Running || step.output.as_ref().is_none_or(|output| output.trim().is_empty()) {
            return Err("Only a running task step with a saved blocker explanation can be blocked");
        }
        step.status = StepStatus::Blocked;
        let detail = step.output.clone();
        self.record_event(Some(step_id), TaskExecutionEventKind::Blocked, detail.as_deref());
        Ok(())
    }

    /// Re-queues a blocked step after the user has resolved its external blocker.
    /// The old blocker explanation is cleared so a new execution cannot inherit stale output.
    pub fn retry_blocked_step(&mut self, step_id: u64) -> Result<(), &'static str> {
        if self.stopped { return Err("Task plan is stopped"); }
        if self.is_paused() { return Err("Task plan is paused"); }
        let ready = self.steps.iter().find(|step| step.id == step_id)
            .is_some_and(|step| step.status == StepStatus::Blocked
                && step.depends_on.iter().all(|dependency| self.steps.iter().any(|candidate| candidate.id == *dependency && candidate.status == StepStatus::Completed)));
        if !ready { return Err("Only a blocked step with completed prerequisites can be retried"); }
        let step = self.steps.iter_mut().find(|step| step.id == step_id).ok_or("Task step does not exist")?;
        step.status = StepStatus::Pending;
        step.output = None;
        self.record_event(Some(step_id), TaskExecutionEventKind::Retried, Some("Blocked step re-queued after user review"));
        Ok(())
    }

    /// Re-queues a failed AI step for a deliberate user-initiated retry.
    /// Any prior output is cleared so a later attempt cannot display stale data.
    pub fn retry_failed_step(&mut self, step_id: u64) -> Result<(), &'static str> {
        if self.stopped { return Err("Task plan is stopped"); }
        if self.is_paused() { return Err("Task plan is paused"); }
        let ready = self.steps.iter().find(|step| step.id == step_id)
            .is_some_and(|step| step.status == StepStatus::Failed
                && step.depends_on.iter().all(|dependency| self.steps.iter().any(|candidate| candidate.id == *dependency && candidate.status == StepStatus::Completed)));
        if !ready { return Err("Only a failed step with completed prerequisites can be retried"); }
        let step = self.steps.iter_mut().find(|step| step.id == step_id).ok_or("Task step does not exist")?;
        step.status = StepStatus::Pending;
        step.output = None;
        self.record_event(Some(step_id), TaskExecutionEventKind::Retried, Some("Failed step re-queued by user"));
        Ok(())
    }

    /// Returns interrupted steps that can safely be retried now. Only steps
    /// with a recorded interruption and completed prerequisites are eligible.
    pub fn interrupted_steps_ready_to_retry(&self) -> Vec<u64> {
        if self.stopped || self.is_paused() { return Vec::new(); }
        self.steps.iter().filter(|step| {
            step.status == StepStatus::Failed
                && self.events.iter().any(|event| event.step_id == Some(step.id)
                    && event.kind == TaskExecutionEventKind::Interrupted)
                && step.depends_on.iter().all(|dependency| self.steps.iter().any(|candidate|
                    candidate.id == *dependency && candidate.status == StepStatus::Completed))
        }).map(|step| step.id).collect()
    }

    /// Re-queues all currently retryable interrupted steps. Execution remains
    /// a separate explicit action and no request is replayed by recovery itself.
    pub fn retry_interrupted_steps(&mut self) -> Result<Vec<u64>, &'static str> {
        let ids = self.interrupted_steps_ready_to_retry();
        if ids.is_empty() { return Err("There are no interrupted steps ready to retry"); }
        for id in &ids {
            let step = self.steps.iter_mut().find(|step| step.id == *id).ok_or("Task step does not exist")?;
            step.status = StepStatus::Pending;
            step.output = None;
            self.record_event(Some(*id), TaskExecutionEventKind::Retried, Some("Interrupted step re-queued by user"));
        }
        Ok(ids)
    }

    pub fn fail_step(&mut self, step_id: u64) -> Result<(), &'static str> {
        let step = self.steps.iter_mut().find(|step| step.id == step_id).ok_or("Task step does not exist")?;
        if step.status != StepStatus::Running { return Err("Only a running task step can fail"); }
        step.status = StepStatus::Failed;
        self.record_event(Some(step_id), TaskExecutionEventKind::Failed, None);
        Ok(())
    }

    pub fn stop(&mut self) {
        self.stopped = true;
        for step in &mut self.steps {
            if matches!(step.status, StepStatus::Pending | StepStatus::Running | StepStatus::WaitingForApproval) {
                step.status = StepStatus::Cancelled;
            }
        }
        self.record_event(None, TaskExecutionEventKind::PlanStopped, None);
    }

    /// Converts in-flight work from a previous process into a retryable failure.
    /// It deliberately does not auto-run again: the model request may have completed
    /// remotely before the process died, so replaying it could duplicate work.
    pub fn recover_interrupted_steps(&mut self) -> Vec<u64> {
        if self.stopped { return Vec::new(); }
        let mut recovered = Vec::new();
        for step in &mut self.steps {
            if step.status == StepStatus::Running {
                step.status = StepStatus::Failed;
                step.output = Some("Execution was interrupted when CenAI closed or restarted. Review the step, then retry it manually.".into());
                recovered.push(step.id);
            }
        }
        for step_id in &recovered {
            self.record_event(Some(*step_id), TaskExecutionEventKind::Interrupted, Some("Recovered after process restart; manual retry required"));
        }
        recovered
    }

    pub fn ready_steps(&self) -> Vec<u64> {
        if self.stopped || self.is_paused() { return Vec::new(); }
        self.steps.iter().filter(|step| step.status == StepStatus::Pending
            && step.depends_on.iter().all(|dependency| self.steps.iter().any(|candidate| candidate.id == *dependency && candidate.status == StepStatus::Completed)))
            .map(|step| step.id).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_and_resume_are_durable_and_gate_new_execution() {
        let mut plan = TaskPlan {
            id: "pause-plan".into(), goal: "Pause safely".into(), stopped: false, events: Vec::new(),
            steps: vec![
                TaskStep { id: 1, role: AgentRole::Developer, title: "Implement".into(), status: StepStatus::Pending, depends_on: Vec::new(), output: None },
                TaskStep { id: 2, role: AgentRole::Tester, title: "Verify".into(), status: StepStatus::Pending, depends_on: vec![1], output: None },
            ],
        };
        assert!(!plan.is_paused());
        assert_eq!(plan.ready_steps(), vec![1]);
        plan.pause().unwrap();
        assert!(plan.is_paused());
        assert!(plan.ready_steps().is_empty());
        assert!(plan.begin_step(1).is_err());
        assert!(plan.request_step_approval(1).is_err());
        plan.validate_persisted().unwrap();

        let restored: TaskPlan = serde_json::from_slice(&serde_json::to_vec(&plan).unwrap()).unwrap();
        assert!(restored.is_paused());
        plan.resume().unwrap();
        assert!(!plan.is_paused());
        assert_eq!(plan.ready_steps(), vec![1]);
        plan.begin_step(1).unwrap();
        plan.pause().unwrap();
        assert_eq!(plan.steps[0].status, StepStatus::Running);
        assert!(plan.ready_steps().is_empty());
        plan.complete_step(1).unwrap();
        plan.resume().unwrap();
        assert_eq!(plan.ready_steps(), vec![2]);
        plan.stop();
        assert!(plan.resume().is_err());
    }

    #[test]
    fn dependencies_gate_execution_and_stop_is_terminal() {
        let mut plan = TaskPlan {
            id: "task-1".into(), goal: "Build and verify".into(), stopped: false,
            events: Vec::new(),
            steps: vec![
                TaskStep { id: 1, role: AgentRole::Developer, title: "Implement".into(), status: StepStatus::Pending, depends_on: vec![],
                        output: None, },
                TaskStep { id: 2, role: AgentRole::Tester, title: "Test".into(), status: StepStatus::Pending, depends_on: vec![1],
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
    fn ready_steps_selects_independent_work_and_excludes_approval_gated_steps() {
        let mut plan = TaskPlan {
            id: "parallel-plan".into(), goal: "Research and summarize".into(), stopped: false,
            events: Vec::new(),
            steps: vec![
                TaskStep { id: 1, role: AgentRole::Researcher, title: "Research A".into(), status: StepStatus::Pending, depends_on: vec![], output: None },
                TaskStep { id: 2, role: AgentRole::Researcher, title: "Research B".into(), status: StepStatus::Pending, depends_on: vec![], output: None },
                TaskStep { id: 3, role: AgentRole::Developer, title: "Combine research".into(), status: StepStatus::Pending, depends_on: vec![1, 2], output: None },
            ],
        };
        assert_eq!(plan.ready_steps(), vec![1, 2]);
        plan.request_step_approval(1).unwrap();
        assert_eq!(plan.ready_steps(), vec![2]);
        plan.steps.iter_mut().find(|step| step.id == 2).unwrap().status = StepStatus::Running;
        assert!(plan.ready_steps().is_empty());
        plan.steps.iter_mut().find(|step| step.id == 2).unwrap().status = StepStatus::Completed;
        plan.steps.iter_mut().find(|step| step.id == 1).unwrap().status = StepStatus::Completed;
        assert_eq!(plan.ready_steps(), vec![3]);
    }

    #[test]
    fn planner_cannot_claim_completed_steps_or_create_forward_dependencies() {
        let mut plan = TaskPlan {
            id: "task-1".into(), goal: "Build and verify".into(), stopped: false,
            events: Vec::new(),
            steps: vec![
                TaskStep { id: 1, role: AgentRole::Developer, title: "Implement".into(), status: StepStatus::Pending, depends_on: vec![],
                        output: None, },
                TaskStep { id: 2, role: AgentRole::Tester, title: "Test".into(), status: StepStatus::Pending, depends_on: vec![1],
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
            events: Vec::new(),
            steps: vec![
                TaskStep { id: 1, role: AgentRole::Developer, title: "Prepare".into(), status: StepStatus::Pending, depends_on: vec![],
                        output: None, },
                TaskStep { id: 2, role: AgentRole::Tester, title: "Run external action".into(), status: StepStatus::Pending, depends_on: vec![1],
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
    fn coordinated_step_authorization_requires_fresh_approval_for_sensitive_actions() {
        let mut plan = TaskPlan {
            id: "task-permission-coordination".into(),
            goal: "Coordinate protected work".into(),
            stopped: false,
            events: Vec::new(),
            steps: vec![
                TaskStep { id: 1, role: AgentRole::Developer, title: "Read local context".into(), status: StepStatus::Pending, depends_on: vec![], output: None },
                TaskStep { id: 2, role: AgentRole::Developer, title: "Modify external account".into(), status: StepStatus::Pending, depends_on: vec![1], output: None },
            ],
        };

        assert_eq!(
            plan.authorize_and_begin_step(1, OperationClass::ReadOnly, true).unwrap(),
            ApprovalDecision::Allow
        );
        plan.complete_step(1).unwrap();

        assert_eq!(
            plan.authorize_and_begin_step(2, OperationClass::ExternalAccount, true).unwrap(),
            ApprovalDecision::RequireUserApproval
        );
        assert_eq!(plan.steps[1].status, StepStatus::WaitingForApproval);
        plan.resolve_step_approval(2, true).unwrap();

        assert_eq!(
            plan.authorize_and_begin_step(2, OperationClass::ExternalAccount, true).unwrap(),
            ApprovalDecision::Allow
        );
        assert_eq!(plan.steps[1].status, StepStatus::Running);
        assert!(plan.validate_persisted().is_ok());
    }

    #[test]
    fn denied_per_action_approval_cannot_be_bypassed_by_a_broad_grant() {
        let mut plan = TaskPlan {
            id: "task-denied-permission".into(),
            goal: "Protect account actions".into(),
            stopped: false,
            events: Vec::new(),
            steps: vec![TaskStep {
                id: 1, role: AgentRole::Developer, title: "Delete remote data".into(),
                status: StepStatus::Pending, depends_on: vec![], output: None,
            }],
        };
        assert_eq!(
            plan.authorize_and_begin_step(1, OperationClass::Destructive, true).unwrap(),
            ApprovalDecision::RequireUserApproval
        );
        plan.resolve_step_approval(1, false).unwrap();
        assert!(plan.authorize_and_begin_step(1, OperationClass::Destructive, true).is_err());
        assert_eq!(plan.steps[0].status, StepStatus::Cancelled);
    }

    #[test]
    fn approval_is_bound_to_the_exact_operation_class_and_description() {
        let mut plan = TaskPlan {
            id: "task-bound-approval".into(),
            goal: "Prevent approval replay across operations".into(),
            stopped: false,
            events: Vec::new(),
            steps: vec![TaskStep {
                id: 1, role: AgentRole::Developer, title: "Update profile".into(),
                status: StepStatus::Pending, depends_on: vec![], output: None,
            }],
        };
        assert_eq!(
            plan.authorize_and_begin_step(1, OperationClass::Sensitive, true).unwrap(),
            ApprovalDecision::RequireUserApproval
        );
        plan.resolve_step_approval(1, true).unwrap();
        assert_eq!(
            plan.authorize_and_begin_step(1, OperationClass::Sensitive, true).unwrap(),
            ApprovalDecision::Allow
        );
        plan.fail_step(1).unwrap();
        plan.retry_failed_step(1).unwrap();

        // The previous approval was for a sensitive read, not a destructive action.
        assert_eq!(
            plan.authorize_and_begin_step(1, OperationClass::Destructive, true).unwrap(),
            ApprovalDecision::RequireUserApproval
        );
        assert_eq!(plan.steps[0].status, StepStatus::WaitingForApproval);
        assert!(plan.validate_persisted().is_ok());
    }

    #[test]
    fn approval_is_single_use_and_must_be_renewed_after_a_failed_attempt() {
        let mut plan = TaskPlan {
            id: "task-single-use-approval".into(),
            goal: "Prevent approval replay after failure".into(),
            stopped: false,
            events: Vec::new(),
            steps: vec![TaskStep {
                id: 1, role: AgentRole::Developer, title: "Change account setting".into(),
                status: StepStatus::Pending, depends_on: vec![], output: None,
            }],
        };
        assert_eq!(
            plan.authorize_and_begin_step(1, OperationClass::ExternalAccount, true).unwrap(),
            ApprovalDecision::RequireUserApproval
        );
        plan.resolve_step_approval(1, true).unwrap();
        assert_eq!(
            plan.authorize_and_begin_step(1, OperationClass::ExternalAccount, true).unwrap(),
            ApprovalDecision::Allow
        );
        assert_eq!(plan.events.last().unwrap().kind, TaskExecutionEventKind::ApprovalConsumed);
        plan.fail_step(1).unwrap();
        plan.retry_failed_step(1).unwrap();
        assert_eq!(
            plan.authorize_and_begin_step(1, OperationClass::ExternalAccount, true).unwrap(),
            ApprovalDecision::RequireUserApproval
        );
        assert_eq!(plan.steps[0].status, StepStatus::WaitingForApproval);
    }

    #[test]
    fn ordinary_operations_can_proceed_after_their_requested_approval() {
        let mut plan = TaskPlan {
            id: "task-ordinary-approval".into(),
            goal: "Allow explicitly reviewed ordinary operations".into(),
            stopped: false,
            events: Vec::new(),
            steps: vec![TaskStep {
                id: 1, role: AgentRole::Researcher, title: "Read local data".into(),
                status: StepStatus::Pending, depends_on: vec![], output: None,
            }],
        };
        assert_eq!(
            plan.authorize_and_begin_step(1, OperationClass::ReadOnly, false).unwrap(),
            ApprovalDecision::RequireUserApproval
        );
        plan.resolve_step_approval(1, true).unwrap();
        assert_eq!(
            plan.authorize_and_begin_step(1, OperationClass::ReadOnly, false).unwrap(),
            ApprovalDecision::Allow
        );
        assert_eq!(plan.steps[0].status, StepStatus::Running);
    }

    #[test]
    fn approval_is_invalidated_when_the_step_description_changes() {
        let mut plan = TaskPlan {
            id: "task-description-binding".into(),
            goal: "Prevent stale approval reuse".into(),
            stopped: false,
            events: Vec::new(),
            steps: vec![TaskStep {
                id: 1, role: AgentRole::Developer, title: "Send a message".into(),
                status: StepStatus::Pending, depends_on: vec![], output: None,
            }],
        };
        assert_eq!(
            plan.authorize_and_begin_step(1, OperationClass::ExternalAccount, true).unwrap(),
            ApprovalDecision::RequireUserApproval
        );
        plan.resolve_step_approval(1, true).unwrap();
        plan.steps[0].title = "Delete the account".into();
        assert_eq!(
            plan.authorize_and_begin_step(1, OperationClass::ExternalAccount, true).unwrap(),
            ApprovalDecision::RequireUserApproval
        );
        assert_eq!(plan.steps[0].status, StepStatus::WaitingForApproval);
    }

    #[test]
    fn denied_approval_and_failed_dependencies_block_downstream_steps() {
        let mut plan = TaskPlan {
            id: "task-3".into(), goal: "Test denied work".into(), stopped: false,
            events: Vec::new(),
            steps: vec![
                TaskStep { id: 1, role: AgentRole::Developer, title: "Prepare".into(), status: StepStatus::Pending, depends_on: vec![],
                        output: None, },
                TaskStep { id: 2, role: AgentRole::Tester, title: "External action".into(), status: StepStatus::Pending, depends_on: vec![1],
                        output: None, },
                TaskStep { id: 3, role: AgentRole::QualityReviewer, title: "Review".into(), status: StepStatus::Pending, depends_on: vec![2],
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
            events: Vec::new(),
            steps: vec![
                TaskStep { id: 1, role: AgentRole::Developer, title: "Implement".into(), status: StepStatus::Pending, depends_on: vec![],
                        output: None, },
                TaskStep { id: 2, role: AgentRole::Tester, title: "Verify".into(), status: StepStatus::Pending, depends_on: vec![1],
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
    fn persisted_step_results_are_bounded_and_new_plans_cannot_smuggle_outputs() {
        let mut plan = TaskPlan {
            id: "task-output".into(), goal: "Produce a saved result".into(), stopped: false,
            events: Vec::new(),
            steps: vec![TaskStep {
                id: 1, role: AgentRole::Researcher, title: "Research".into(),
                status: StepStatus::Pending, depends_on: Vec::new(), output: None,
            }],
        };
        plan.steps[0].output = Some("untrusted pre-filled result".into());
        assert!(plan.validate().is_err());
        plan.steps[0].output = None;
        plan.begin_step(1).unwrap();
        plan.steps[0].output = Some("A verified bounded result".into());
        plan.complete_step(1).unwrap();
        assert!(plan.validate_persisted().is_ok());
        plan.steps[0].output = Some("x".repeat(24_001));
        assert!(plan.validate_persisted().is_err());
        plan.steps[0].output = Some(String::new());
        assert!(plan.validate_persisted().is_err());
    }

    #[test]
    fn blocked_steps_keep_their_handoff_and_never_unlock_dependents() {
        let mut plan = TaskPlan {
            id: "task-blocked".into(), goal: "Perform a protected action".into(), stopped: false,
            events: Vec::new(),
            steps: vec![
                TaskStep { id: 1, role: AgentRole::Developer, title: "Perform external action".into(), status: StepStatus::Pending, depends_on: vec![], output: None },
                TaskStep { id: 2, role: AgentRole::Tester, title: "Verify action".into(), status: StepStatus::Pending, depends_on: vec![1], output: None },
            ],
        };
        plan.begin_step(1).unwrap();
        plan.steps[0].output = Some("Needs explicit device permission".into());
        plan.block_step(1).unwrap();
        assert_eq!(plan.steps[0].status, StepStatus::Blocked);
        assert!(plan.ready_steps().is_empty());
        assert!(plan.validate_persisted().is_ok());
        plan.steps[0].output = None;
        assert!(plan.validate_persisted().is_err());
    }

    #[test]
    fn blocked_step_can_be_retried_only_after_blocker_resolution_and_clears_stale_output() {
        let mut plan = TaskPlan {
            id: "task-retry".into(), goal: "Perform a protected action".into(), stopped: false,
            events: Vec::new(),
            steps: vec![
                TaskStep { id: 1, role: AgentRole::Developer, title: "Perform external action".into(), status: StepStatus::Pending, depends_on: vec![], output: None },
                TaskStep { id: 2, role: AgentRole::Tester, title: "Verify action".into(), status: StepStatus::Pending, depends_on: vec![1], output: None },
            ],
        };
        plan.begin_step(1).unwrap();
        plan.steps[0].output = Some("Waiting for user permission".into());
        plan.block_step(1).unwrap();
        plan.retry_blocked_step(1).unwrap();
        assert_eq!(plan.steps[0].status, StepStatus::Pending);
        assert_eq!(plan.steps[0].output, None);
        assert_eq!(plan.ready_steps(), vec![1]);
        assert!(plan.retry_blocked_step(1).is_err());
        plan.begin_step(1).unwrap();
        plan.complete_step(1).unwrap();
        assert_eq!(plan.ready_steps(), vec![2]);
    }

    #[test]
    fn stopping_plan_cancels_running_step_and_prevents_late_result_from_being_applied() {
        let mut plan = TaskPlan {
            id: "task-late-result".into(), goal: "Cancel safely".into(), stopped: false,
            events: Vec::new(),
            steps: vec![TaskStep { id: 1, role: AgentRole::Developer, title: "Work".into(), status: StepStatus::Pending, depends_on: vec![], output: None }],
        };
        plan.begin_step(1).unwrap();
        plan.stop();
        assert!(plan.stopped);
        assert_eq!(plan.steps[0].status, StepStatus::Cancelled);
        assert!(plan.complete_step(1).is_err());
        assert_eq!(plan.steps[0].status, StepStatus::Cancelled);
    }

    #[test]
    fn failed_step_can_be_retried_without_unlocking_dependents_or_reusing_output() {
        let mut plan = TaskPlan {
            id: "task-failed-retry".into(), goal: "Recover failed work".into(), stopped: false,
            events: Vec::new(),
            steps: vec![
                TaskStep { id: 1, role: AgentRole::Developer, title: "Implement".into(), status: StepStatus::Pending, depends_on: vec![], output: None },
                TaskStep { id: 2, role: AgentRole::Tester, title: "Verify".into(), status: StepStatus::Pending, depends_on: vec![1], output: None },
            ],
        };
        plan.begin_step(1).unwrap();
        plan.fail_step(1).unwrap();
        plan.steps[0].output = Some("stale diagnostic".into());
        assert!(plan.ready_steps().is_empty());
        plan.retry_failed_step(1).unwrap();
        assert_eq!(plan.steps[0].status, StepStatus::Pending);
        assert_eq!(plan.steps[0].output, None);
        assert_eq!(plan.ready_steps(), vec![1]);
        assert!(plan.retry_failed_step(1).is_err());
        plan.begin_step(1).unwrap();
        plan.complete_step(1).unwrap();
        assert_eq!(plan.ready_steps(), vec![2]);
    }

    #[test]
    fn stopped_plans_cannot_retry_failed_steps() {
        let mut plan = TaskPlan {
            id: "task-stopped-failed-retry".into(), goal: "Recover".into(), stopped: true,
            events: Vec::new(),
            steps: vec![TaskStep { id: 1, role: AgentRole::Developer, title: "Recover".into(), status: StepStatus::Failed, depends_on: vec![], output: None }],
        };
        assert!(plan.retry_failed_step(1).is_err());
    }

    #[test]
    fn stopped_plans_cannot_retry_blocked_steps() {
        let mut plan = TaskPlan {
            id: "task-stopped-retry".into(), goal: "Recover".into(), stopped: true,
            events: Vec::new(),
            steps: vec![TaskStep { id: 1, role: AgentRole::Developer, title: "Recover".into(), status: StepStatus::Blocked, depends_on: vec![], output: Some("Blocked".into()) }],
        };
        assert!(plan.retry_blocked_step(1).is_err());
    }

    #[test]
    fn interrupted_running_steps_recover_as_retryable_failures_without_unlocking_dependents() {
        let mut plan = TaskPlan {
            id: "task-recovery".into(), goal: "Recover safely after restart".into(), stopped: false,
            events: Vec::new(),
            steps: vec![
                TaskStep { id: 1, role: AgentRole::Developer, title: "Implement".into(), status: StepStatus::Running, depends_on: vec![], output: None },
                TaskStep { id: 2, role: AgentRole::Tester, title: "Verify".into(), status: StepStatus::Pending, depends_on: vec![1], output: None },
            ],
        };
        assert_eq!(plan.recover_interrupted_steps(), vec![1]);
        assert_eq!(plan.steps[0].status, StepStatus::Failed);
        assert!(plan.steps[0].output.as_deref().unwrap().contains("interrupted"));
        assert!(plan.ready_steps().is_empty());
        assert!(plan.validate_persisted().is_ok());
        assert_eq!(plan.recover_interrupted_steps(), Vec::<u64>::new());
        plan.retry_failed_step(1).unwrap();
        assert_eq!(plan.ready_steps(), vec![1]);
        plan.begin_step(1).unwrap();
        plan.complete_step(1).unwrap();
        assert_eq!(plan.ready_steps(), vec![2]);
    }

    #[test]
    fn interrupted_steps_can_be_retried_in_one_explicit_batch_without_replaying_dependents() {
        let mut plan = TaskPlan {
            id: "task-batch-recovery".into(), goal: "Retry recovered work".into(), stopped: false,
            events: Vec::new(),
            steps: vec![
                TaskStep { id: 1, role: AgentRole::Developer, title: "Implement".into(), status: StepStatus::Running, depends_on: vec![], output: None },
                TaskStep { id: 2, role: AgentRole::Tester, title: "Verify".into(), status: StepStatus::Pending, depends_on: vec![1], output: None },
                TaskStep { id: 3, role: AgentRole::Researcher, title: "Independent research".into(), status: StepStatus::Running, depends_on: vec![], output: None },
            ],
        };
        assert_eq!(plan.recover_interrupted_steps(), vec![1, 3]);
        assert_eq!(plan.interrupted_steps_ready_to_retry(), vec![1, 3]);
        let retried = plan.retry_interrupted_steps().unwrap();
        assert_eq!(retried, vec![1, 3]);
        assert_eq!(plan.steps[0].status, StepStatus::Pending);
        assert_eq!(plan.steps[0].output, None);
        assert_eq!(plan.steps[1].status, StepStatus::Pending);
        assert_eq!(plan.steps[2].status, StepStatus::Pending);
        assert!(plan.validate_persisted().is_ok());
        plan.begin_step(1).unwrap();
        plan.complete_step(1).unwrap();
        assert_eq!(plan.ready_steps(), vec![2, 3]);
    }

    #[test]
    fn stopped_plans_do_not_recover_cancelled_or_running_work() {
        let mut plan = TaskPlan {
            id: "task-stopped-recovery".into(), goal: "Keep stopped plans terminal".into(), stopped: true,
            events: Vec::new(),
            steps: vec![TaskStep { id: 1, role: AgentRole::Developer, title: "Work".into(), status: StepStatus::Cancelled, depends_on: vec![], output: None }],
        };
        assert!(plan.recover_interrupted_steps().is_empty());
        assert_eq!(plan.steps[0].status, StepStatus::Cancelled);
    }

    #[test]
    fn execution_history_records_approval_start_and_completion_in_order() {
        let mut plan = TaskPlan {
            id: "task-history".into(), goal: "Track every execution transition".into(),
            stopped: false, events: Vec::new(),
            steps: vec![TaskStep {
                id: 1, role: AgentRole::Developer, title: "Implement".into(),
                status: StepStatus::Pending, depends_on: Vec::new(), output: None,
            }],
        };
        plan.request_step_approval(1).unwrap();
        plan.resolve_step_approval(1, true).unwrap();
        plan.begin_step(1).unwrap();
        plan.complete_step(1).unwrap();
        let kinds = plan.events.iter().map(|event| event.kind).collect::<Vec<_>>();
        assert_eq!(kinds, vec![
            TaskExecutionEventKind::ApprovalRequested,
            TaskExecutionEventKind::ApprovalGranted,
            TaskExecutionEventKind::Started,
            TaskExecutionEventKind::Completed,
        ]);
        assert!(plan.events.windows(2).all(|pair| pair[0].sequence < pair[1].sequence));
        assert!(plan.validate_persisted().is_ok());
        assert!(plan.validate().is_err());
        let encoded = serde_json::to_string(&plan).unwrap();
        let decoded: TaskPlan = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, plan);
    }

    #[test]
    fn execution_history_is_bounded_and_rejects_invalid_event_references() {
        let mut plan = TaskPlan {
            id: "task-history-bound".into(), goal: "Bound execution history".into(),
            stopped: false, events: Vec::new(),
            steps: vec![TaskStep {
                id: 1, role: AgentRole::Developer, title: "Work".into(),
                status: StepStatus::Pending, depends_on: Vec::new(), output: None,
            }],
        };
        for _ in 0..140 {
            plan.record_event(None, TaskExecutionEventKind::Started, Some("Recorded"));
        }
        assert_eq!(plan.events.len(), 128);
        assert_eq!(plan.events.first().unwrap().sequence, 13);
        assert!(plan.validate_persisted().is_ok());
        plan.events[0].step_id = Some(999);
        assert!(plan.validate_persisted().is_err());
    }

    #[test]
    fn older_task_plan_json_defaults_to_empty_execution_history() {
        let json = r#"{"id":"legacy","goal":"Old saved plan","steps":[{"id":1,"role":"developer","title":"Work","status":"pending","depends_on":[],"output":null}],"stopped":false}"#;
        let plan: TaskPlan = serde_json::from_str(json).unwrap();
        assert!(plan.events.is_empty());
        assert!(plan.validate_persisted().is_ok());
    }

    #[test]
    fn dependency_cycles_are_rejected() {
        let plan = TaskPlan {
            id: "cyclic".into(), goal: "Invalid plan".into(), stopped: false,
            events: Vec::new(),
            steps: vec![
                TaskStep { id: 1, role: AgentRole::Developer, title: "A".into(), status: StepStatus::Pending, depends_on: vec![2],
                        output: None, },
                TaskStep { id: 2, role: AgentRole::Tester, title: "B".into(), status: StepStatus::Pending, depends_on: vec![1],
                        output: None, },
            ],
        };
        assert!(plan.validate().is_err());
    }
}
