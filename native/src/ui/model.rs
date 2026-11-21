//! UI state model shared by the GPU scene graph and input system.
use std::collections::VecDeque;
use serde::{Deserialize, Serialize};
use crate::core::workspace::Project;

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

impl ConversationState {
    pub fn restore_messages(&mut self, messages: Vec<Message>) {
        self.next_id = messages.iter().map(|message| message.id).max().unwrap_or(0).saturating_add(1).max(1);
        self.messages = messages.into_iter().map(|mut message| { message.streaming = false; message }).collect();
        self.sending = false;
    }

    pub fn messages(&self) -> impl Iterator<Item = &Message> {
        self.messages.iter()
    }

    pub fn push(&mut self, role: MessageRole, body: impl Into<String>) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.messages.push_back(Message { id, role, body: body.into(), streaming: false });
        id
    }

    pub fn begin_stream(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.messages.push_back(Message { id, role: MessageRole::Assistant, body: String::new(), streaming: true });
        id
    }

    pub fn append_stream(&mut self, id: u64, chunk: &str) -> bool {
        match self.messages.iter_mut().find(|message| message.id == id && message.streaming) {
            Some(message) => { message.body.push_str(chunk); true }
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
pub struct AppState {
    pub destination: Destination,
    pub conversation: ConversationState,
    pub reduced_motion: bool,
    pub light_mode: bool,
    pub guardian_consent: bool,
    pub pending_guardian_consent: Option<bool>,
    pub planning_mode: bool,
    pub provider_key_provider: String,
    pub provider_key_draft: String,
    pub provider_key_status: String,
    pub pending_provider_key: Option<(String, String)>,
    pub projects: Vec<Project>,
    pub project_name_draft: String,
    pub project_description_draft: String,
    pub project_status: String,
    pub pending_project_create: Option<(String, String)>,
    pub artifact_project_id: String,
    pub artifact_name_draft: String,
    pub artifact_content_draft: String,
    pub artifact_status: String,
    pub pending_artifact: Option<(String, String, String, String)>,
    pub account_email: String,
    pub email_verified: bool,
    pub otp_draft: String,
    pub otp_status: String,
    pub pending_send_otp: bool,
    pub pending_verify_otp: Option<String>,
    pub auth_email_draft: String,
    pub auth_password_draft: String,
    pub auth_status: String,
    pub pending_sign_in: Option<(String, String)>,
    pub pending_sign_out: bool,
    pub mcp_server_id_draft: String,
    pub mcp_endpoint_draft: String,
    pub mcp_publisher_draft: String,
    pub mcp_version_draft: String,
    pub mcp_license_draft: String,
    pub mcp_capabilities_draft: String,
    pub mcp_reviewed: bool,
    pub mcp_status: String,
    pub mcp_tools: Vec<crate::core::mcp::McpTool>,
    pub pending_mcp_connect: Option<crate::core::mcp::McpServerManifest>,
    pub mcp_pending_approval: Option<String>,
    pub mcp_call_arguments_draft: String,
    pub pending_mcp_call: Option<(String, String, String)>,
    pub mcp_call_result: String,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            destination: Destination::Conversation,
            conversation: ConversationState::default(),
            reduced_motion: false,
            light_mode: false,
            guardian_consent: false,
            pending_guardian_consent: None,
            planning_mode: false,
            provider_key_provider: "openai".into(),
            provider_key_draft: String::new(),
            provider_key_status: String::new(),
            pending_provider_key: None,
            projects: Vec::new(),
            project_name_draft: String::new(),
            project_description_draft: String::new(),
            project_status: String::new(),
            pending_project_create: None,
            artifact_project_id: String::new(),
            artifact_name_draft: String::new(),
            artifact_content_draft: String::new(),
            artifact_status: String::new(),
            pending_artifact: None,
            account_email: String::new(),
            email_verified: false,
            otp_draft: String::new(),
            otp_status: String::new(),
            pending_send_otp: false,
            pending_verify_otp: None,
            auth_email_draft: String::new(),
            auth_password_draft: String::new(),
            auth_status: String::new(),
            pending_sign_in: None,
            pending_sign_out: false,
            mcp_server_id_draft: String::new(),
            mcp_endpoint_draft: String::new(),
            mcp_publisher_draft: String::new(),
            mcp_version_draft: "1.0.0".into(),
            mcp_license_draft: "MIT".into(),
            mcp_capabilities_draft: String::new(),
            mcp_reviewed: false,
            mcp_status: String::new(),
            mcp_tools: Vec::new(),
            pending_mcp_connect: None,
            mcp_pending_approval: None,
            mcp_call_arguments_draft: "{}".into(),
            pending_mcp_call: None,
            mcp_call_result: String::new(),
            mcp_server_id_draft: String::new(),
            mcp_endpoint_draft: String::new(),
            mcp_publisher_draft: String::new(),
            mcp_version_draft: "1.0.0".into(),
            mcp_license_draft: "MIT".into(),
            mcp_capabilities_draft: String::new(),
            mcp_reviewed: false,
            mcp_status: String::new(),
            mcp_tools: Vec::new(),
            pending_mcp_connect: None,
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
