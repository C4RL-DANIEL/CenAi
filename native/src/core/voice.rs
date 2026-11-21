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
    pub fn from_pcm16_le(sample_rate_hz: u32, channels: u8, bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() % 2 != 0 { return Err("PCM16 audio buffer has an incomplete sample"); }
        let samples = bytes.chunks_exact(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<_>>();
        let frame = Self { sample_rate_hz, channels, samples };
        frame.validate()?;
        Ok(frame)
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if !matches!(self.sample_rate_hz, 16_000 | 24_000 | 48_000) {
            return Err("Unsupported voice sample rate");
        }
        if self.channels != 1 { return Err("Voice capture must be mono PCM"); }
        if self.samples.is_empty() || self.samples.len() > self.sample_rate_hz as usize * 4 {
            return Err("Audio frame must contain between 1 ms and 4 seconds of samples");
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

    pub fn to_wav_bytes(&self) -> Result<Vec<u8>, &'static str> {
        if !matches!(self.sample_rate_hz, 16_000 | 24_000 | 48_000) || self.samples.is_empty() {
            return Err("Audio chunk format is invalid");
        }
        let data_length = self.samples.len().checked_mul(2).ok_or("Audio chunk is too large")?;
        if data_length > u32::MAX as usize - 36 { return Err("Audio chunk exceeds WAV limits"); }
        let data_length = data_length as u32;
        let mut wav = Vec::with_capacity(44 + data_length as usize);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36u32 + data_length).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&self.sample_rate_hz.to_le_bytes());
        wav.extend_from_slice(&(self.sample_rate_hz * 2).to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_length.to_le_bytes());
        for sample in &self.samples { wav.extend_from_slice(&sample.to_le_bytes()); }
        Ok(wav)
    }
}

/// Provider-neutral PCM16 voice activity buffer. Platform adapters supply frames;
/// the buffer emits bounded chunks after speech followed by silence or a size limit.
pub struct VoiceAudioBuffer {
    sample_rate_hz: u32,
    max_samples: usize,
    silence_samples_to_flush: usize,
    silence_samples: usize,
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
            silence_samples_to_flush: sample_rate_hz as usize * 300 / 1000,
            silence_samples: 0,
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
            self.silence_samples = 0;
        } else if self.speech_active {
            self.silence_samples = self.silence_samples.saturating_add(frame.samples.len());
        } else {
            return Ok(None);
        }
        self.samples.extend(frame.samples);
        if self.samples.len() >= self.max_samples || self.silence_samples >= self.silence_samples_to_flush {
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
        self.silence_samples = 0;
        Some(AudioChunk { sample_rate_hz: self.sample_rate_hz, samples, speech_detected })
    }
}

pub struct VoicePipeline {
    pub session: VoiceSession,
    audio: VoiceAudioBuffer,
}

impl VoicePipeline {
    pub fn new(mode: VoiceMode, sample_rate_hz: u32, max_chunk_ms: u32) -> Result<Self, &'static str> {
        Ok(Self { session: VoiceSession::new(mode), audio: VoiceAudioBuffer::new(sample_rate_hz, max_chunk_ms)? })
    }

    pub fn start(&mut self) -> Result<VoiceEvent, &'static str> { self.session.start() }

    pub fn permission_result(&mut self, granted: bool) -> Result<VoiceEvent, &'static str> {
        self.session.permission_result(granted)
    }

    pub fn provider_connected(&mut self) -> Result<VoiceEvent, &'static str> {
        self.session.provider_connected()
    }

    pub fn ingest_frame(&mut self, frame: AudioFrame) -> Result<Option<AudioChunk>, &'static str> {
        if self.session.state != VoiceState::Listening {
            return Err("Audio frames can only be accepted while the voice session is listening");
        }
        self.audio.push_frame(frame)
    }

    pub fn finish_capture(&mut self) -> Result<Option<AudioChunk>, &'static str> {
        if self.session.state != VoiceState::Listening {
            return Err("Voice session is not listening");
        }
        Ok(self.audio.finish())
    }

    pub fn stop(&mut self) -> VoiceEvent { self.session.stop() }
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
        if !matches!(self.state, VoiceState::Idle | VoiceState::Ended | VoiceState::Failed) {
            return Err("Voice session is already active");
        }
        self.microphone_granted = false;
        self.output_active = false;
        self.last_error = None;
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
    fn audio_frame_decodes_little_endian_pcm16() {
        let frame = AudioFrame::from_pcm16_le(16_000, 1, &[0x01, 0x00, 0xff, 0xff]).unwrap();
        assert_eq!(frame.samples, vec![1, -1]);
        assert!(AudioFrame::from_pcm16_le(16_000, 1, &[0x01]).is_err());
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
    fn voice_activity_silence_timeout_is_sample_based_not_frame_count_based() {
        let mut buffer = VoiceAudioBuffer::new(16_000, 5_000).unwrap();
        let speech = AudioFrame { sample_rate_hz: 16_000, channels: 1, samples: vec![1200; 800] };
        assert!(buffer.push_frame(speech).unwrap().is_none());
        let short_silence = AudioFrame { sample_rate_hz: 16_000, channels: 1, samples: vec![0; 800] };
        assert!(buffer.push_frame(short_silence.clone()).unwrap().is_none());
        assert!(buffer.push_frame(short_silence.clone()).unwrap().is_none());
        assert!(buffer.push_frame(short_silence.clone()).unwrap().is_none());
        assert!(buffer.push_frame(short_silence.clone()).unwrap().is_none());
        let flushed = buffer.push_frame(short_silence).unwrap().unwrap();
        assert!(flushed.speech_detected);
        assert_eq!(flushed.duration_ms(), 300);
    }

    #[test]
    fn voice_audio_buffer_enforces_frame_format_and_chunk_size() {
        let mut buffer = VoiceAudioBuffer::new(16_000, 500).unwrap();
        assert!(buffer.push_frame(AudioFrame { sample_rate_hz: 48_000, channels: 1, samples: vec![900; 1600] }).is_err());
        assert!(buffer.push_frame(AudioFrame { sample_rate_hz: 16_000, channels: 2, samples: vec![900; 1600] }).is_err());
        assert!(AudioFrame { sample_rate_hz: 16_000, channels: 1, samples: vec![0; 32_000] }.validate().is_ok());
        assert!(AudioFrame { sample_rate_hz: 16_000, channels: 1, samples: vec![0; 64_001] }.validate().is_err());
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
    fn audio_chunk_encodes_valid_pcm16_wav_header() {
        let chunk = AudioChunk { sample_rate_hz: 16_000, samples: vec![1, -1, 300], speech_detected: true };
        let wav = chunk.to_wav_bytes().unwrap();
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 6);
        assert_eq!(wav.len(), 50);
    }

    #[test]
    fn voice_pipeline_accepts_audio_only_after_permission_and_connection() {
        let mut pipeline = VoicePipeline::new(VoiceMode::PushToTalk, 16_000, 5_000).unwrap();
        assert!(pipeline.ingest_frame(frame(1200)).is_err());
        pipeline.start().unwrap();
        pipeline.permission_result(true).unwrap();
        pipeline.provider_connected().unwrap();
        assert!(pipeline.ingest_frame(frame(1200)).unwrap().is_none());
        let chunk = pipeline.finish_capture().unwrap().unwrap();
        assert!(chunk.speech_detected);
        assert_eq!(chunk.duration_ms(), 100);
        assert_eq!(pipeline.stop(), VoiceEvent::CloseSession);
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
    fn voice_session_can_retry_after_permission_denial() {
        let mut session = VoiceSession::new(VoiceMode::PushToTalk);
        session.start().unwrap();
        assert!(session.permission_result(false).is_err());
        assert_eq!(session.state, VoiceState::Failed);
        assert_eq!(session.start(), Ok(VoiceEvent::RequestMicrophonePermission));
        assert_eq!(session.state, VoiceState::AwaitingPermission);
        assert!(session.last_error.is_none());
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
