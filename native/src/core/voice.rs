//! Voice-session state machine. Audio capture and provider transport remain platform adapters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VoiceMode { PushToTalk, RealtimeSpeech }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VoiceState { Idle, AwaitingPermission, Connecting, Listening, Speaking, Interrupted, Ended, Failed }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VoiceEvent { RequestMicrophonePermission, ConnectProvider, StartCapture, StopCapture, InterruptPlayback, CloseSession }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioFrame {
    pub sample_rate_hz: u32,
    pub channels: u8,
    pub samples: Vec<i16>,
}

impl AudioFrame {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !matches!(self.sample_rate_hz, 16_000 | 24_000 | 48_000) {
            return Err("Unsupported voice sample rate");
        }
        if self.channels != 1 { return Err("Voice capture must be mono PCM"); }
        if self.samples.is_empty() || self.samples.len() > self.sample_rate_hz as usize {
            return Err("Audio frame must contain between 1 ms and 1 second of samples");
        }
        Ok(())
    }

    fn rms_level(&self) -> u32 {
        if self.samples.is_empty() { return 0; }
        let sum = self.samples.iter().map(|sample| {
            let value = i64::from(*sample);
            (value * value) as u64
        }).sum::<u64>();
        ((sum / self.samples.len() as u64) as f64).sqrt() as u32
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioChunk {
    pub sample_rate_hz: u32,
    pub samples: Vec<i16>,
    pub speech_detected: bool,
}

impl AudioChunk {
    pub fn duration_ms(&self) -> u64 {
        self.samples.len() as u64 * 1000 / self.sample_rate_hz.max(1) as u64
    }
}

/// Provider-neutral PCM16 voice activity buffer. Platform adapters supply frames;
/// the buffer emits bounded chunks after speech followed by silence or a size limit.
pub struct VoiceAudioBuffer {
    sample_rate_hz: u32,
    max_samples: usize,
    silence_frames_to_flush: u8,
    silence_frames: u8,
    speech_active: bool,
    samples: Vec<i16>,
}

impl VoiceAudioBuffer {
    pub fn new(sample_rate_hz: u32, max_chunk_ms: u32) -> Result<Self, &'static str> {
        if !matches!(sample_rate_hz, 16_000 | 24_000 | 48_000) {
            return Err("Unsupported voice sample rate");
        }
        if !(500..=15_000).contains(&max_chunk_ms) {
            return Err("Voice chunk limit must be between 500 ms and 15 seconds");
        }
        Ok(Self {
            sample_rate_hz,
            max_samples: sample_rate_hz as usize * max_chunk_ms as usize / 1000,
            silence_frames_to_flush: 3,
            silence_frames: 0,
            speech_active: false,
            samples: Vec::new(),
        })
    }

    pub fn push_frame(&mut self, frame: AudioFrame) -> Result<Option<AudioChunk>, &'static str> {
        frame.validate()?;
        if frame.sample_rate_hz != self.sample_rate_hz {
            return Err("Audio frame sample rate changed during the voice session");
        }
        if frame.rms_level() >= 500 {
            self.speech_active = true;
            self.silence_frames = 0;
        } else if self.speech_active {
            self.silence_frames = self.silence_frames.saturating_add(1);
        } else {
            return Ok(None);
        }
        self.samples.extend(frame.samples);
        if self.samples.len() >= self.max_samples || self.silence_frames >= self.silence_frames_to_flush {
            return Ok(self.take_chunk());
        }
        Ok(None)
    }

    pub fn finish(&mut self) -> Option<AudioChunk> {
        self.take_chunk()
    }

    fn take_chunk(&mut self) -> Option<AudioChunk> {
        if self.samples.is_empty() { return None; }
        let samples = std::mem::take(&mut self.samples);
        let speech_detected = std::mem::replace(&mut self.speech_active, false);
        self.silence_frames = 0;
        Some(AudioChunk { sample_rate_hz: self.sample_rate_hz, samples, speech_detected })
    }
}

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

    fn frame(level: i16) -> AudioFrame {
        AudioFrame { sample_rate_hz: 16_000, channels: 1, samples: vec![level; 1_600] }
    }

    #[test]
    fn voice_audio_buffer_ignores_leading_silence_and_flushes_after_speech() {
        let mut buffer = VoiceAudioBuffer::new(16_000, 5_000).unwrap();
        assert!(buffer.push_frame(frame(0)).unwrap().is_none());
        assert!(buffer.push_frame(frame(1200)).unwrap().is_none());
        assert!(buffer.push_frame(frame(0)).unwrap().is_none());
        assert!(buffer.push_frame(frame(0)).unwrap().is_none());
        let chunk = buffer.push_frame(frame(0)).unwrap().unwrap();
        assert!(chunk.speech_detected);
        assert_eq!(chunk.duration_ms(), 400);
        assert!(buffer.finish().is_none());
    }

    #[test]
    fn voice_audio_buffer_enforces_frame_format_and_chunk_size() {
        let mut buffer = VoiceAudioBuffer::new(16_000, 500).unwrap();
        assert!(buffer.push_frame(AudioFrame { sample_rate_hz: 48_000, channels: 1, samples: vec![900; 1600] }).is_err());
        assert!(buffer.push_frame(AudioFrame { sample_rate_hz: 16_000, channels: 2, samples: vec![900; 1600] }).is_err());
        let mut completed = None;
        for _ in 0..5 {
            completed = buffer.push_frame(frame(1000)).unwrap();
        }
        let chunk = completed.unwrap();
        assert_eq!(chunk.duration_ms(), 500);
        assert!(chunk.speech_detected);
        assert!(VoiceAudioBuffer::new(16_000, 100).is_err());
    }

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
