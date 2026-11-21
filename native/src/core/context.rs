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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ContextGrants {
    allow_cross_feature: bool,
    include_personal_memory: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextRequest {
    pub user_id: String,
    pub feature_id: String,
    pub session_id: String,
    /// Private grant bits cannot be populated by model output or deserialized context.
    grants: ContextGrants,
}

fn valid_user_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 36
        && [8, 13, 18, 23].iter().all(|index| bytes[*index] == b'-')
        && bytes.iter().enumerate().all(|(index, byte)| {
            [8, 13, 18, 23].contains(&index) || byte.is_ascii_hexdigit()
        })
}

impl ContextRequest {
    pub fn new(user_id: impl Into<String>, feature_id: impl Into<String>, session_id: impl Into<String>) -> Self {
        Self { user_id: user_id.into(), feature_id: feature_id.into(), session_id: session_id.into(), grants: ContextGrants::default() }
    }

    /// Called only after the application has recorded the user's explicit approval/settings.
    pub fn apply_user_approved_grants(&mut self, cross_feature: bool, personal_memory: bool) {
        self.grants.allow_cross_feature = cross_feature;
        self.grants.include_personal_memory = personal_memory;
    }
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
        if !valid_user_uuid(&self.user_id) || self.feature_id.trim().is_empty() || self.session_id.trim().is_empty() {
            return Err("A verified Supabase user UUID, feature and session scope are required");
        }
        if self.feature_id.len() > 128 || self.session_id.len() > 128
            || !self.feature_id.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
            || !self.session_id.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
        {
            return Err("Context feature or session scope is invalid");
        }
        Ok(())
    }
}

impl ContextRecord {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !valid_user_uuid(&self.owner_user_id) || self.feature_id.trim().is_empty() || self.content.trim().is_empty() {
            return Err("Context owner UUID, feature and content are required");
        }
        if self.feature_id.len() > 128 || self.content.len() > 100_000
            || !self.feature_id.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        {
            return Err("Context record exceeds supported scope or content limits");
        }
        if let Some(session_id) = &self.session_id {
            if session_id.is_empty() || session_id.len() > 128
                || !session_id.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
            {
                return Err("Context session identifier is invalid");
            }
        }
        if self.kind == ContextKind::SessionState && self.session_id.is_none() {
            return Err("Session state must carry its exact session identifier");
        }
        Ok(())
    }

    pub fn visible_to(&self, request: &ContextRequest) -> bool {
        if self.owner_user_id != request.user_id {
            return false;
        }
        match self.kind {
            ContextKind::PersonalMemory => request.grants.include_personal_memory,
            ContextKind::FeatureData => self.feature_id == request.feature_id || request.grants.allow_cross_feature,
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
        // Ignore foreign-user records before inspecting their contents; they must not influence this request.
        if record.owner_user_id != request.user_id {
            continue;
        }
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
        ContextRequest::new("550e8400-e29b-41d4-a716-446655440000", "research", "session-1")
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
            record("550e8400-e29b-41d4-a716-446655440000", "research", None, ContextKind::FeatureData, "A private note"),
            record("650e8400-e29b-41d4-a716-446655440000", "research", None, ContextKind::FeatureData, "B private note"),
        ];
        assert_eq!(collect_authorized_context(&request(), &records).unwrap(), vec!["A private note"]);
    }

    #[test]
    fn feature_context_requires_explicit_cross_feature_grant() {
        let records = vec![record("550e8400-e29b-41d4-a716-446655440000", "coding", None, ContextKind::FeatureData, "Code workspace")];
        assert!(collect_authorized_context(&request(), &records).unwrap().is_empty());
        let mut granted = request();
        granted.apply_user_approved_grants(true, false);
        assert_eq!(collect_authorized_context(&granted, &records).unwrap(), vec!["Code workspace"]);
    }

    #[test]
    fn session_state_never_crosses_sessions_or_features() {
        let records = vec![
            record("550e8400-e29b-41d4-a716-446655440000", "research", Some("session-1"), ContextKind::SessionState, "Current task"),
            record("550e8400-e29b-41d4-a716-446655440000", "research", Some("session-2"), ContextKind::SessionState, "Other session"),
            record("550e8400-e29b-41d4-a716-446655440000", "coding", Some("session-1"), ContextKind::SessionState, "Other feature"),
        ];
        assert_eq!(collect_authorized_context(&request(), &records).unwrap(), vec!["Current task"]);
    }

    #[test]
    fn personal_memory_requires_explicit_request_permission() {
        let records = vec![record("550e8400-e29b-41d4-a716-446655440000", "memory", None, ContextKind::PersonalMemory, "Saved preference")];
        assert!(collect_authorized_context(&request(), &records).unwrap().is_empty());
        let mut enabled = request();
        enabled.apply_user_approved_grants(false, true);
        assert_eq!(collect_authorized_context(&enabled, &records).unwrap(), vec!["Saved preference"]);
    }

    #[test]
    fn malformed_foreign_records_cannot_block_or_influence_another_user() {
        let records = vec![
            record("650e8400-e29b-41d4-a716-446655440000", "", None, ContextKind::SessionState, "Foreign malformed content"),
            record("550e8400-e29b-41d4-a716-446655440000", "research", None, ContextKind::FeatureData, "Visible note"),
        ];
        assert_eq!(collect_authorized_context(&request(), &records).unwrap(), vec!["Visible note"]);
    }

    #[test]
    fn malformed_session_records_are_rejected() {
        let records = vec![record("550e8400-e29b-41d4-a716-446655440000", "research", None, ContextKind::SessionState, "No session id")];
        assert!(collect_authorized_context(&request(), &records).is_err());
    }
}
