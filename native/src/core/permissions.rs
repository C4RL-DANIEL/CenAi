//! Explicit operation classes used by agents and cross-app integrations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationClass {
    ReadOnly,
    Reversible,
    ExternalAccount,
    Sensitive,
    Destructive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApprovalDecision {
    Allow,
    RequireUserApproval,
    Deny,
}

pub fn approval_policy(class: OperationClass, explicitly_granted: bool) -> ApprovalDecision {
    match class {
        OperationClass::ReadOnly if explicitly_granted => ApprovalDecision::Allow,
        OperationClass::ReadOnly => ApprovalDecision::RequireUserApproval,
        OperationClass::Reversible if explicitly_granted => ApprovalDecision::Allow,
        OperationClass::Reversible => ApprovalDecision::RequireUserApproval,
        OperationClass::ExternalAccount | OperationClass::Sensitive | OperationClass::Destructive => {
            ApprovalDecision::RequireUserApproval
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dangerous_operations_always_require_per_action_approval() {
        for class in [OperationClass::ExternalAccount, OperationClass::Sensitive, OperationClass::Destructive] {
            assert_eq!(approval_policy(class, true), ApprovalDecision::RequireUserApproval);
        }
    }

    #[test]
    fn granted_read_and_reversible_operations_can_proceed() {
        assert_eq!(approval_policy(OperationClass::ReadOnly, true), ApprovalDecision::Allow);
        assert_eq!(approval_policy(OperationClass::Reversible, true), ApprovalDecision::Allow);
    }
}
