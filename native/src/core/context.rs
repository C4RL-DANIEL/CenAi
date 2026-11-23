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
    pub fn includes_personal_memory(&self) -> bool { self.grants.include_personal_memory }

    pub(crate) fn apply_user_approved_grants(&mut self, cross_feature: bool, personal_memory: bool) {
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

/// A conversation excerpt eligible for opt-in, on-device history retrieval.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalKnowledgeRecord {
    pub conversation_id: String,
    pub title: String,
    pub content: String,
}

/// Rank saved conversation excerpts without sending them anywhere. The active
/// conversation is excluded because its messages are already in the request.
pub fn rank_local_knowledge<'a>(
    query: &str,
    current_conversation_id: &str,
    records: &'a [LocalKnowledgeRecord],
    max_items: usize,
    max_bytes: usize,
) -> Vec<&'a LocalKnowledgeRecord> {
    fn terms(value: &str) -> std::collections::HashSet<String> {
        value.split(|character: char| !character.is_ascii_alphanumeric())
            .filter(|term| term.len() >= 3)
            .map(str::to_ascii_lowercase)
            .collect()
    }
    let query_terms = terms(query);
    if query_terms.is_empty() || max_items == 0 || max_bytes == 0 { return Vec::new(); }
    let mut ranked = records.iter().enumerate()
        .filter(|(_, record)| record.conversation_id != current_conversation_id && !record.content.trim().is_empty())
        .map(|(index, record)| {
            let haystack = format!("{} {}", record.title, record.content);
            let overlap = query_terms.intersection(&terms(&haystack)).count();
            (index, record, overlap)
        })
        .filter(|(_, _, score)| *score > 0)
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| right.2.cmp(&left.2).then_with(|| left.0.cmp(&right.0)));
    let mut selected = Vec::new();
    let mut used_bytes = 0usize;
    for (_, record, _) in ranked {
        if selected.len() >= max_items { break; }
        let next = record.title.len().saturating_add(record.content.len());
        if used_bytes.saturating_add(next) > max_bytes { continue; }
        used_bytes += next;
        selected.push(record);
    }
    selected
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


/// Selects a bounded set of private memories most relevant to the current goal.
/// Memory text is treated as untrusted data; this function only ranks content and
/// never grants access to records owned by another account.
pub fn rank_personal_memories<'a>(
    user_id: &str,
    goal: &str,
    records: &'a [ContextRecord],
    max_items: usize,
    max_bytes: usize,
) -> Vec<&'a ContextRecord> {
    fn terms(value: &str) -> std::collections::HashSet<String> {
        value.split(|character: char| !character.is_ascii_alphanumeric())
            .filter(|term| term.len() >= 3)
            .map(str::to_ascii_lowercase)
            .collect()
    }

    let goal_terms = terms(goal);
    let mut ranked = records.iter().enumerate()
        .filter(|(_, record)| record.owner_user_id == user_id && record.kind == ContextKind::PersonalMemory)
        .map(|(index, record)| {
            let memory_terms = terms(&record.content);
            let overlap = goal_terms.intersection(&memory_terms).count();
            let phrase_bonus = if !goal.trim().is_empty()
                && record.content.to_ascii_lowercase().contains(&goal.trim().to_ascii_lowercase()) { 8 } else { 0 };
            (index, record, overlap.saturating_mul(3) + phrase_bonus)
        })
        // Do not inject unrelated memories merely because fewer than max_items matched.
        .filter(|(_, _, score)| *score > 0)
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| right.2.cmp(&left.2).then_with(|| left.0.cmp(&right.0)));

    let mut selected = Vec::new();
    let mut used_bytes = 0usize;
    for (_, record, _) in ranked {
        if selected.len() >= max_items { break; }
        let next = record.content.len();
        if next == 0 || used_bytes.saturating_add(next) > max_bytes { continue; }
        used_bytes += next;
        selected.push(record);
    }
    selected
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
    fn personal_memory_ranking_prioritizes_goal_relevance_and_enforces_limits() {
        let owner = "550e8400-e29b-41d4-a716-446655440000";
        let records = vec![
            record(owner, "memory", None, ContextKind::PersonalMemory, "Prefers concise responses"),
            record(owner, "memory", None, ContextKind::PersonalMemory, "CenAI Android Rust renderer project"),
            record(owner, "memory", None, ContextKind::PersonalMemory, "Enjoys baking sourdough bread"),
            record(owner, "research", None, ContextKind::FeatureData, "Feature data is not personal memory"),
        ];
        let ranked = rank_personal_memories(owner, "Continue the CenAI Android Rust project", &records, 8, 100);
        assert_eq!(ranked.len(), 1, "unrelated memories must not be injected just to fill the limit");
        assert_eq!(ranked[0].content, "CenAI Android Rust renderer project");
        assert!(ranked.iter().all(|record| record.kind == ContextKind::PersonalMemory));
    }

    #[test]
    fn personal_memory_ranking_never_returns_another_users_records() {
        let records = vec![
            record("550e8400-e29b-41d4-a716-446655440000", "memory", None, ContextKind::PersonalMemory, "Owner's private note"),
            record("650e8400-e29b-41d4-a716-446655440000", "memory", None, ContextKind::PersonalMemory, "Other user's private note"),
        ];
        let selected = rank_personal_memories(
            "550e8400-e29b-41d4-a716-446655440000", "private note", &records, 10, 1000,
        );
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].content, "Owner's private note");
    }

    #[test]
    fn personal_memory_ranking_respects_zero_limits() {
        let records = vec![record(
            "550e8400-e29b-41d4-a716-446655440000",
            "memory", None, ContextKind::PersonalMemory, "Private preference",
        )];
        assert!(rank_personal_memories("550e8400-e29b-41d4-a716-446655440000", "preference", &records, 0, 100).is_empty());
        assert!(rank_personal_memories("550e8400-e29b-41d4-a716-446655440000", "preference", &records, 5, 0).is_empty());
    }

    #[test]
    fn local_knowledge_ranks_relevant_saved_chats_and_excludes_current_chat() {
        let records = vec![
            LocalKnowledgeRecord { conversation_id: "current".into(), title: "Rust project".into(), content: "CenAI Rust migration".into() },
            LocalKnowledgeRecord { conversation_id: "older-1".into(), title: "Cooking".into(), content: "Pasta recipe and tomato sauce".into() },
            LocalKnowledgeRecord { conversation_id: "older-2".into(), title: "CenAI Rust migration".into(), content: "Android Rust renderer and offline inference".into() },
        ];
        let ranked = rank_local_knowledge("Continue the CenAI Rust Android renderer", "current", &records, 3, 1000);
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].conversation_id, "older-2");
    }

    #[test]
    fn local_knowledge_obeys_item_and_byte_limits_and_ignores_empty_queries() {
        let records = vec![
            LocalKnowledgeRecord { conversation_id: "one".into(), title: "Rust".into(), content: "Rust Android build".into() },
            LocalKnowledgeRecord { conversation_id: "two".into(), title: "Rust".into(), content: "Rust offline model".into() },
        ];
        assert_eq!(rank_local_knowledge("Rust", "current", &records, 1, 1000).len(), 1);
        assert!(rank_local_knowledge("Rust", "current", &records, 5, 1).is_empty());
        assert!(rank_local_knowledge("!!", "current", &records, 5, 1000).is_empty());
    }

    #[test]
    fn malformed_session_records_are_rejected() {
        let records = vec![record("550e8400-e29b-41d4-a716-446655440000", "research", None, ContextKind::SessionState, "No session id")];
        assert!(collect_authorized_context(&request(), &records).is_err());
    }
}
