//! Enforced per-user, per-feature and per-session context boundaries for the shared Central Brain.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextKind {
    /// Persistent personal memory; available only when the user has enabled memory for this request.
    PersonalMemory,
    /// Data belonging to one feature/workspace; cross-feature access requires an explicit user grant.
    FeatureData,
    /// Ephemeral state belonging to one exact feature session; never reusable by another session.
    SessionState,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContextRequest {
    pub user_id: String,
    pub feature_id: String,
    pub session_id: String,
    /// Must be derived from a user-approved grant, never inferred or self-issued by the model.
    pub allow_cross_feature: bool,
    /// Must reflect the user's personal-memory setting and request-level permission.
    pub include_personal_memory: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContextRecord {
    pub owner_user_id: String,
    pub feature_id: String,
    pub session_id: Option<String>,
    pub kind: ContextKind,
    pub content: String,
}

impl ContextRequest {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.user_id.trim().is_empty() || self.feature_id.trim().is_empty() || self.session_id.trim().is_empty() {
            return Err("Verified user, feature and session scopes are required");
        }
        if self.user_id.len() > 256 || self.feature_id.len() > 128 || self.session_id.len() > 128 {
            return Err("Context scope exceeds the supported size");
        }
        Ok(())
    }
}

impl ContextRecord {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.owner_user_id.trim().is_empty() || self.feature_id.trim().is_empty() || self.content.trim().is_empty() {
            return Err("Context owner, feature and content are required");
        }
        if self.content.len() > 100_000 {
            return Err("Context record exceeds the supported size");
        }
        if self.kind == ContextKind::SessionState && self.session_id.as_deref().unwrap_or_default().is_empty() {
            return Err("Session state must carry its exact session identifier");
        }
        Ok(())
    }

    pub fn visible_to(&self, request: &ContextRequest) -> bool {
        if self.owner_user_id != request.user_id {
            return false;
        }
        match self.kind {
            ContextKind::PersonalMemory => request.include_personal_memory,
            ContextKind::FeatureData => self.feature_id == request.feature_id || request.allow_cross_feature,
            ContextKind::SessionState => {
                self.feature_id == request.feature_id
                    && self.session_id.as_deref() == Some(request.session_id.as_str())
            }
        }
    }
}

/// Returns only records authorized for this exact user/feature/session request.
pub fn collect_authorized_context<'a>(
    request: &ContextRequest,
    records: &'a [ContextRecord],
) -> Result<Vec<&'a str>, &'static str> {
    request.validate()?;
    let mut visible = Vec::new();
    for record in records {
        record.validate()?;
        if record.visible_to(request) {
            visible.push(record.content.as_str());
        }
    }
    Ok(visible)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> ContextRequest {
        ContextRequest {
            user_id: "user-a".into(),
            feature_id: "research".into(),
            session_id: "session-1".into(),
            allow_cross_feature: false,
            include_personal_memory: false,
        }
    }

    fn record(owner: &str, feature: &str, session: Option<&str>, kind: ContextKind, content: &str) -> ContextRecord {
        ContextRecord {
            owner_user_id: owner.into(),
            feature_id: feature.into(),
            session_id: session.map(str::to_owned),
            kind,
            content: content.into(),
        }
    }

    #[test]
    fn shared_brain_never_reads_another_users_context() {
        let records = vec![
            record("user-a", "research", None, ContextKind::FeatureData, "A private note"),
            record("user-b", "research", None, ContextKind::FeatureData, "B private note"),
        ];
        assert_eq!(collect_authorized_context(&request(), &records).unwrap(), vec!["A private note"]);
    }

    #[test]
    fn feature_context_requires_explicit_cross_feature_grant() {
        let records = vec![record("user-a", "coding", None, ContextKind::FeatureData, "Code workspace")];
        assert!(collect_authorized_context(&request(), &records).unwrap().is_empty());
        let mut granted = request();
        granted.allow_cross_feature = true;
        assert_eq!(collect_authorized_context(&granted, &records).unwrap(), vec!["Code workspace"]);
    }

    #[test]
    fn session_state_never_crosses_sessions_or_features() {
        let records = vec![
            record("user-a", "research", Some("session-1"), ContextKind::SessionState, "Current task"),
            record("user-a", "research", Some("session-2"), ContextKind::SessionState, "Other session"),
            record("user-a", "coding", Some("session-1"), ContextKind::SessionState, "Other feature"),
        ];
        assert_eq!(collect_authorized_context(&request(), &records).unwrap(), vec!["Current task"]);
    }

    #[test]
    fn personal_memory_requires_explicit_request_permission() {
        let records = vec![record("user-a", "memory", None, ContextKind::PersonalMemory, "Saved preference")];
        assert!(collect_authorized_context(&request(), &records).unwrap().is_empty());
        let mut enabled = request();
        enabled.include_personal_memory = true;
        assert_eq!(collect_authorized_context(&enabled, &records).unwrap(), vec!["Saved preference"]);
    }

    #[test]
    fn malformed_session_records_are_rejected() {
        let records = vec![record("user-a", "research", None, ContextKind::SessionState, "No session id")];
        assert!(collect_authorized_context(&request(), &records).is_err());
    }
}
