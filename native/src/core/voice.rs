//! Voice-session state machine. Audio capture and provider transport remain platform adapters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VoiceMode { PushToTalk, RealtimeSpeech }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VoiceState { Idle, AwaitingPermission, Connecting, Listening, Speaking, Interrupted, Ended, Failed }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VoiceEvent { RequestMicrophonePermission, ConnectProvider, StartCapture, StopCapture, InterruptPlayback, CloseSession }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoiceSession {
    pub mode: VoiceMode,
    pub state: VoiceState,
    pub microphone_granted: bool,
    pub output_active: bool,
    pub last_error: Option<String>,
}

impl VoiceSession {
    pub fn new(mode: VoiceMode) -> Self {
        Self { mode, state: VoiceState::Idle, microphone_granted: false, output_active: false, last_error: None }
    }

    pub fn start(&mut self) -> Result<VoiceEvent, &'static str> {
        if self.state != VoiceState::Idle && self.state != VoiceState::Ended {
            return Err("Voice session is already active");
        }
        self.state = VoiceState::AwaitingPermission;
        Ok(VoiceEvent::RequestMicrophonePermission)
    }

    pub fn permission_result(&mut self, granted: bool) -> Result<VoiceEvent, &'static str> {
        if self.state != VoiceState::AwaitingPermission { return Err("Microphone permission was not requested"); }
        self.microphone_granted = granted;
        if !granted {
            self.state = VoiceState::Failed;
            self.last_error = Some("Microphone permission was denied".into());
            return Err("Microphone permission is required for voice input");
        }
        self.state = VoiceState::Connecting;
        Ok(VoiceEvent::ConnectProvider)
    }

    pub fn provider_connected(&mut self) -> Result<VoiceEvent, &'static str> {
        if self.state != VoiceState::Connecting { return Err("Voice provider is not connecting"); }
        self.state = VoiceState::Listening;
        Ok(VoiceEvent::StartCapture)
    }

    pub fn assistant_started_speaking(&mut self) -> Result<(), &'static str> {
        if self.state != VoiceState::Listening { return Err("Voice session is not listening"); }
        self.output_active = true;
        self.state = VoiceState::Speaking;
        Ok(())
    }

    pub fn user_interrupted(&mut self) -> Result<VoiceEvent, &'static str> {
        if self.state != VoiceState::Speaking { return Err("There is no active assistant speech to interrupt"); }
        self.output_active = false;
        self.state = VoiceState::Interrupted;
        Ok(VoiceEvent::InterruptPlayback)
    }

    pub fn resume_listening(&mut self) -> Result<VoiceEvent, &'static str> {
        if self.state != VoiceState::Interrupted && self.state != VoiceState::Speaking {
            return Err("Voice session cannot resume from its current state");
        }
        self.output_active = false;
        self.state = VoiceState::Listening;
        Ok(VoiceEvent::StartCapture)
    }

    pub fn stop(&mut self) -> VoiceEvent {
        self.output_active = false;
        self.state = VoiceState::Ended;
        VoiceEvent::CloseSession
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_session_requires_permission_and_provider_connection() {
        let mut session = VoiceSession::new(VoiceMode::RealtimeSpeech);
        assert_eq!(session.start(), Ok(VoiceEvent::RequestMicrophonePermission));
        assert_eq!(session.permission_result(true), Ok(VoiceEvent::ConnectProvider));
        assert_eq!(session.provider_connected(), Ok(VoiceEvent::StartCapture));
        assert_eq!(session.state, VoiceState::Listening);
    }

    #[test]
    fn barge_in_interrupts_assistant_output_and_returns_to_listening() {
        let mut session = VoiceSession::new(VoiceMode::RealtimeSpeech);
        session.start().unwrap();
        session.permission_result(true).unwrap();
        session.provider_connected().unwrap();
        session.assistant_started_speaking().unwrap();
        assert_eq!(session.user_interrupted(), Ok(VoiceEvent::InterruptPlayback));
        assert_eq!(session.resume_listening(), Ok(VoiceEvent::StartCapture));
        assert_eq!(session.state, VoiceState::Listening);
    }

    #[test]
    fn denied_permission_never_starts_capture() {
        let mut session = VoiceSession::new(VoiceMode::PushToTalk);
        session.start().unwrap();
        assert!(session.permission_result(false).is_err());
        assert_eq!(session.state, VoiceState::Failed);
        assert!(!session.microphone_granted);
    }
}
