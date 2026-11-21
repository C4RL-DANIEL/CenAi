//! UI state model shared by the GPU scene graph and input system.
use std::collections::VecDeque;
use serde::{Deserialize, Serialize};
use crate::core::workspace::Project;
use crate::core::agents::TaskPlan;
use crate::core::guardian::GuardianSnapshot;
use crate::core::multimodal::AttachmentMetadata;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Destination {
    Conversation,
    YourWork,
    Connections,
    Settings,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub id: u64,
    pub role: MessageRole,
    pub body: String,
    pub streaming: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConversationState {
    messages: VecDeque<Message>,
    next_id: u64,
    pub draft: String,
    pub sending: bool,
}

impl Default for ConversationState {
    fn default() -> Self {
        Self { messages: VecDeque::new(), next_id: 1, draft: String::new(), sending: false }
    }
}

const MAX_CONVERSATION_MESSAGES: usize = 500;
const MAX_MESSAGE_BYTES: usize = 200_000;

fn truncate_utf8(value: &mut String, max_bytes: usize) {
    if value.len() <= max_bytes { return; }
    let mut boundary = max_bytes;
    while !value.is_char_boundary(boundary) { boundary -= 1; }
    value.truncate(boundary);
}

impl ConversationState {
    fn enforce_message_limit(&mut self) {
        while self.messages.len() > MAX_CONVERSATION_MESSAGES {
            self.messages.pop_front();
        }
    }
    pub fn restore_messages(&mut self, messages: Vec<Message>) {
        self.next_id = messages.iter().map(|message| message.id).max().unwrap_or(0).saturating_add(1).max(1);
        self.messages = messages.into_iter().map(|mut message| {
            message.streaming = false;
            truncate_utf8(&mut message.body, MAX_MESSAGE_BYTES);
            message
        }).collect();
        while self.messages.len() > MAX_CONVERSATION_MESSAGES {
            self.messages.pop_front();
        }
        self.sending = false;
    }

    pub fn messages(&self) -> impl Iterator<Item = &Message> {
        self.messages.iter()
    }

    pub fn push(&mut self, role: MessageRole, body: impl Into<String>) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        let mut body = body.into();
        truncate_utf8(&mut body, MAX_MESSAGE_BYTES);
        self.messages.push_back(Message { id, role, body, streaming: false });
        self.enforce_message_limit();
        id
    }

    pub fn begin_stream(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.messages.push_back(Message { id, role: MessageRole::Assistant, body: String::new(), streaming: true });
        self.enforce_message_limit();
        id
    }

    pub fn append_stream(&mut self, id: u64, chunk: &str) -> bool {
        match self.messages.iter_mut().find(|message| message.id == id && message.streaming) {
            Some(message) => {
                let remaining = MAX_MESSAGE_BYTES.saturating_sub(message.body.len());
                let mut bounded = chunk.to_owned();
                truncate_utf8(&mut bounded, remaining);
                message.body.push_str(&bounded);
                true
            }
            None => false,
        }
    }

    pub fn finish_stream(&mut self, id: u64) -> bool {
        match self.messages.iter_mut().find(|message| message.id == id && message.streaming) {
            Some(message) => { message.streaming = false; true }
            None => false,
        }
    }

    pub fn submit_draft(&mut self) -> Option<u64> {
        let body = self.draft.trim();
        if body.is_empty() || self.sending { return None; }
        let owned = body.to_owned();
        self.draft.clear();
        Some(self.push(MessageRole::User, owned))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersonalMemoryItem {
    pub id: String,
    pub category: String,
    pub content: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EcosystemAppItem {
    pub app_id: String,
    pub publisher_id: String,
    pub version: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub requested_scopes: Vec<String>,
    pub granted_scopes: Vec<String>,
    pub revoked: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MfaFactor {
    pub id: String,
    pub friendly_name: String,
    pub status: String,
}

pub struct AppState {
    pub destination: Destination,
    pub conversation: ConversationState,
    pub attachments: Vec<AttachmentMetadata>,
    pub attachment_status: String,
    pub attachment_analysis_result: String,
    pub attachment_prompt_draft: String,
    pub pending_attachment_picker: bool,
    pub pending_attachment_analysis: Option<(String, String)>,
    pub pending_attachment_delete: Option<String>,
    pub pending_backup_file_picker: bool,
    pub reduced_motion: bool,
    pub light_mode: bool,
    pub guardian_snapshot: Option<GuardianSnapshot>,
    pub guardian_consent: bool,
    pub pending_guardian_consent: Option<bool>,
    pub personal_memory_enabled: bool,
    pub personal_memory_status: String,
    pub pending_memory_enabled: Option<bool>,
    pub memory_category_draft: String,
    pub memory_content_draft: String,
    pub personal_memories: Vec<PersonalMemoryItem>,
    pub pending_memory_save: Option<(String, String)>,
    pub pending_memory_delete: Option<String>,
    pub confirm_memory_clear: bool,
    pub pending_memory_clear: bool,
    pub planning_mode: bool,
    pub include_memory_for_request: bool,
    pub provider_key_provider: String,
    pub provider_key_draft: String,
    pub provider_key_status: String,
    pub pending_provider_key: Option<(String, String)>,
    pub projects: Vec<Project>,
    pub task_plans: Vec<TaskPlan>,
    pub project_name_draft: String,
    pub project_description_draft: String,
    pub project_status: String,
    pub pending_project_create: Option<(String, String)>,
    pub artifact_project_id: String,
    pub artifact_name_draft: String,
    pub artifact_content_draft: String,
    pub artifact_status: String,
    pub pending_artifact: Option<(String, String, String, String)>,
    pub pending_artifact_open: Option<String>,
    pub pending_artifact_delete: Option<String>,
    pub confirm_artifact_delete: Option<String>,
    pub artifact_preview_id: String,
    pub artifact_preview_project_id: String,
    pub artifact_preview_name: String,
    pub artifact_preview_content: String,
    pub artifact_preview_status: String,
    pub account_email: String,
    pub email_verified: bool,
    pub mfa_factors: Vec<MfaFactor>,
    pub mfa_login_required: bool,
    pub mfa_friendly_name_draft: String,
    pub mfa_status: String,
    pub mfa_enrollment_factor_id: String,
    pub mfa_enrollment_secret: String,
    pub mfa_enrollment_qr: String,
    pub mfa_code_draft: String,
    pub pending_mfa_refresh: bool,
    pub pending_mfa_enroll: bool,
    pub pending_mfa_verify: bool,
    pub pending_mfa_unenroll: Option<String>,
    pub otp_draft: String,
    pub otp_status: String,
    pub pending_send_otp: bool,
    pub pending_verify_otp: Option<String>,
    pub auth_email_draft: String,
    pub auth_password_draft: String,
    pub auth_status: String,
    pub pending_sign_in: Option<(String, String)>,
    pub pending_sign_out: bool,
    pub pending_checkout_plan: Option<String>,
    pub checkout_status: String,
    pub pending_open_url: Option<String>,
    pub mcp_server_id_draft: String,
    pub mcp_endpoint_draft: String,
    pub mcp_publisher_draft: String,
    pub mcp_version_draft: String,
    pub mcp_license_draft: String,
    pub mcp_bearer_token_draft: String,
    pub pending_mcp_token_save: Option<(String, String)>,
    pub mcp_capabilities_draft: String,
    pub mcp_reviewed: bool,
    pub mcp_status: String,
    pub mcp_tools: Vec<crate::core::mcp::McpTool>,
    pub pending_mcp_connect: Option<crate::core::mcp::McpServerManifest>,
    pub mcp_pending_approval: Option<String>,
    pub mcp_call_arguments_draft: String,
    pub pending_mcp_call: Option<(String, String, String)>,
    pub mcp_call_result: String,
    pub ecosystem_apps: Vec<EcosystemAppItem>,
    pub ecosystem_status: String,
    pub ecosystem_app_id_draft: String,
    pub ecosystem_publisher_id_draft: String,
    pub ecosystem_version_draft: String,
    pub ecosystem_description_draft: String,
    pub ecosystem_capabilities_draft: String,
    pub ecosystem_scopes_draft: String,
    pub ecosystem_selected_app: String,
    pub ecosystem_grant_scopes_draft: String,
    pub pending_ecosystem_refresh: bool,
    pub pending_ecosystem_register: Option<serde_json::Value>,
    pub pending_ecosystem_grant: Option<(String, Vec<String>)>,
    pub pending_ecosystem_revoke: Option<String>,
    pub voice_active: bool,
    pub pending_voice_start: bool,
    pub pending_voice_stop: bool,
    pub voice_status: String,
    pub voice_service_status: String,
    pub pending_speech_text: Option<String>,
    pub pending_speech_stop: bool,
    pub speech_cancelled: bool,
    pub speech_status: String,
    pub backup_passphrase_draft: String,
    pub backup_restore_confirmed: bool,
    pub pending_backup_upload: bool,
    pub pending_backup_download: bool,
    pub backup_status: String,
    pub automations: Vec<crate::core::automations::Automation>,
    pub automation_id_draft: String,
    pub automation_title_draft: String,
    pub automation_schedule_draft: String,
    pub automation_status: String,
    pub pending_automation_save: bool,
    pub pending_automation_toggle: Option<(String, bool)>,
    pub pending_automation_delete: Option<String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            destination: Destination::Conversation,
            conversation: ConversationState::default(),
            attachments: Vec::new(),
            attachment_status: String::new(),
            attachment_analysis_result: String::new(),
            attachment_prompt_draft: "Describe this file and extract its key information.".into(),
            pending_attachment_picker: false,
            pending_attachment_analysis: None,
            pending_attachment_delete: None,
            pending_backup_file_picker: false,
            reduced_motion: false,
            light_mode: false,
            guardian_snapshot: None,
            guardian_consent: false,
            pending_guardian_consent: None,
            personal_memory_enabled: false,
            personal_memory_status: String::new(),
            pending_memory_enabled: None,
            memory_category_draft: "preference".into(),
            memory_content_draft: String::new(),
            personal_memories: Vec::new(),
            pending_memory_save: None,
            pending_memory_delete: None,
            confirm_memory_clear: false,
            pending_memory_clear: false,
            planning_mode: false,
            include_memory_for_request: false,
            provider_key_provider: "openai".into(),
            provider_key_draft: String::new(),
            provider_key_status: String::new(),
            pending_provider_key: None,
            projects: Vec::new(),
            task_plans: Vec::new(),
            project_name_draft: String::new(),
            project_description_draft: String::new(),
            project_status: String::new(),
            pending_project_create: None,
            artifact_project_id: String::new(),
            artifact_name_draft: String::new(),
            artifact_content_draft: String::new(),
            artifact_status: String::new(),
            pending_artifact: None,
            pending_artifact_open: None,
            pending_artifact_delete: None,
            confirm_artifact_delete: None,
            artifact_preview_id: String::new(),
            artifact_preview_project_id: String::new(),
            artifact_preview_name: String::new(),
            artifact_preview_content: String::new(),
            artifact_preview_status: String::new(),
            backup_passphrase_draft: String::new(),
            backup_restore_confirmed: false,
            pending_backup_upload: false,
            pending_backup_download: false,
            backup_status: String::new(),
            automations: Vec::new(),
            automation_id_draft: String::new(),
            automation_title_draft: String::new(),
            automation_schedule_draft: "0 9 * * 1-5".into(),
            automation_status: String::new(),
            pending_automation_save: false,
            pending_automation_toggle: None,
            pending_automation_delete: None,
            account_email: String::new(),
            email_verified: false,
            mfa_factors: Vec::new(),
            mfa_login_required: false,
            mfa_friendly_name_draft: "CenAI Android".into(),
            mfa_status: String::new(),
            mfa_enrollment_factor_id: String::new(),
            mfa_enrollment_secret: String::new(),
            mfa_enrollment_qr: String::new(),
            mfa_code_draft: String::new(),
            pending_mfa_refresh: false,
            pending_mfa_enroll: false,
            pending_mfa_verify: false,
            pending_mfa_unenroll: None,
            otp_draft: String::new(),
            otp_status: String::new(),
            pending_send_otp: false,
            pending_verify_otp: None,
            auth_email_draft: String::new(),
            auth_password_draft: String::new(),
            auth_status: String::new(),
            pending_sign_in: None,
            pending_sign_out: false,
            pending_checkout_plan: None,
            checkout_status: String::new(),
            pending_open_url: None,
            mcp_server_id_draft: String::new(),
            mcp_endpoint_draft: String::new(),
            mcp_publisher_draft: String::new(),
            mcp_version_draft: "1.0.0".into(),
            mcp_license_draft: "MIT".into(),
            mcp_bearer_token_draft: String::new(),
            pending_mcp_token_save: None,
            mcp_capabilities_draft: String::new(),
            mcp_reviewed: false,
            mcp_status: String::new(),
            mcp_tools: Vec::new(),
            pending_mcp_connect: None,
            mcp_pending_approval: None,
            mcp_call_arguments_draft: "{}".into(),
            pending_mcp_call: None,
            mcp_call_result: String::new(),
            ecosystem_apps: Vec::new(),
            ecosystem_status: String::new(),
            ecosystem_app_id_draft: String::new(),
            ecosystem_publisher_id_draft: String::new(),
            ecosystem_version_draft: "1.0.0".into(),
            ecosystem_description_draft: String::new(),
            ecosystem_capabilities_draft: String::new(),
            ecosystem_scopes_draft: String::new(),
            ecosystem_selected_app: String::new(),
            ecosystem_grant_scopes_draft: String::new(),
            pending_ecosystem_refresh: false,
            pending_ecosystem_register: None,
            pending_ecosystem_grant: None,
            pending_ecosystem_revoke: None,
            voice_active: false,
            pending_voice_start: false,
            pending_voice_stop: false,
            voice_status: String::new(),
            voice_service_status: "Cloud voice service is awaiting deployment.".into(),
            pending_speech_text: None,
            pending_speech_stop: false,
            speech_cancelled: false,
            speech_status: String::new(),
                    }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_submission_trims_and_clears() {
        let mut state = ConversationState {
            draft: "  Hello CenAI  ".into(),
            ..ConversationState::default()
        };
        let id = state.submit_draft().expect("non-empty draft submits");
        let message = state.messages().next().unwrap();
        assert_eq!(message.id, id);
        assert_eq!(message.body, "Hello CenAI");
        assert!(state.draft.is_empty());
    }

    #[test]
    fn empty_or_inflight_submission_is_rejected() {
        let mut state = ConversationState::default();
        assert_eq!(state.submit_draft(), None);
        state.draft = "work".into();
        state.sending = true;
        assert_eq!(state.submit_draft(), None);
        assert_eq!(state.draft, "work");
    }

    #[test]
    fn conversation_history_and_streamed_output_are_bounded() {
        let mut state = ConversationState::default();
        for index in 0..(MAX_CONVERSATION_MESSAGES + 25) {
            state.push(MessageRole::User, format!("message {index}"));
        }
        assert_eq!(state.messages().count(), MAX_CONVERSATION_MESSAGES);
        let stream = state.begin_stream();
        assert!(state.append_stream(stream, &"x".repeat(MAX_MESSAGE_BYTES + 10)));
        let body = &state.messages().last().unwrap().body;
        assert_eq!(body.len(), MAX_MESSAGE_BYTES);
    }

    #[test]
    fn restored_history_truncates_oversized_messages_and_limits_count() {
        let mut state = ConversationState::default();
        let messages = (0..(MAX_CONVERSATION_MESSAGES + 1)).map(|id| Message {
            id: id as u64 + 1,
            role: MessageRole::Assistant,
            body: "🙂".repeat(MAX_MESSAGE_BYTES),
            streaming: true,
        }).collect();
        state.restore_messages(messages);
        assert_eq!(state.messages().count(), MAX_CONVERSATION_MESSAGES);
        assert!(state.messages().all(|message| !message.streaming && message.body.len() <= MAX_MESSAGE_BYTES));
    }

    #[test]
    fn stream_chunks_are_appended_only_to_active_stream() {
        let mut state = ConversationState::default();
        let id = state.begin_stream();
        assert!(state.append_stream(id, "Hello "));
        assert!(state.append_stream(id, "world"));
        assert!(state.finish_stream(id));
        assert!(!state.append_stream(id, "!"));
        assert_eq!(state.messages().next().unwrap().body, "Hello world");
    }
}
