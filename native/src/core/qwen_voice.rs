//! CenAI Voice model contract and Qwen3-TTS integration boundary.
//!
//! Qwen supplies the pretrained speech model. CenAI owns the product identity,
//! voice profile, pronunciation policy, model lifecycle, and Android integration.
//! The native inference adapter is deliberately reported unavailable until a
//! supported on-device backend has passed its compatibility and device tests.

use serde::{Deserialize, Serialize};

pub const PRODUCT_NAME: &str = "CenAI Voice";
pub const UPSTREAM_PROJECT: &str = "Qwen3-TTS";
pub const UPSTREAM_LICENSE: &str = "Apache-2.0";
pub const TOKENIZER_MODEL_ID: &str = "Qwen/Qwen3-TTS-Tokenizer-12Hz";
pub const PRIMARY_MODEL_ID: &str = "Qwen/Qwen3-TTS-12Hz-0.6B-Base";
pub const DEFAULT_SPEAKER_ID: &str = "cenai_voice_en_v1";
pub const DEFAULT_LOCALE: &str = "en-US";
pub const MAX_SYNTHESIS_TEXT_BYTES: usize = 32_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum QwenLanguage {
    Chinese,
    English,
    Japanese,
    Korean,
    German,
    French,
    Russian,
    Portuguese,
    Spanish,
    Italian,
}

impl QwenLanguage {
    pub const ALL: [Self; 10] = [
        Self::Chinese, Self::English, Self::Japanese, Self::Korean, Self::German,
        Self::French, Self::Russian, Self::Portuguese, Self::Spanish, Self::Italian,
    ];

    pub fn model_label(self) -> &'static str {
        match self {
            Self::Chinese => "Chinese",
            Self::English => "English",
            Self::Japanese => "Japanese",
            Self::Korean => "Korean",
            Self::German => "German",
            Self::French => "French",
            Self::Russian => "Russian",
            Self::Portuguese => "Portuguese",
            Self::Spanish => "Spanish",
            Self::Italian => "Italian",
        }
    }

    pub fn locale_tag(self) -> &'static str {
        match self {
            Self::Chinese => "zh",
            Self::English => "en",
            Self::Japanese => "ja",
            Self::Korean => "ko",
            Self::German => "de",
            Self::French => "fr",
            Self::Russian => "ru",
            Self::Portuguese => "pt",
            Self::Spanish => "es",
            Self::Italian => "it",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VoiceModelVariant {
    Base06B,
    CustomVoice06B,
    VoiceDesign17B,
}

impl VoiceModelVariant {
    pub fn model_id(self) -> &'static str {
        match self {
            Self::Base06B => PRIMARY_MODEL_ID,
            Self::CustomVoice06B => "Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice",
            Self::VoiceDesign17B => "Qwen/Qwen3-TTS-12Hz-1.7B-VoiceDesign",
        }
    }

    pub fn supports_fine_tuning(self) -> bool {
        matches!(self, Self::Base06B)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeState {
    ModelNotInstalled,
    NativeRuntimeNotIntegrated,
    Ready,
    IncompatibleDevice,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoiceProfile {
    pub id: String,
    pub display_name: String,
    pub language: QwenLanguage,
    pub locale: String,
    pub model_variant: VoiceModelVariant,
    pub speaker_id: String,
}

impl Default for VoiceProfile {
    fn default() -> Self {
        Self {
            id: "cenai-voice-english-v1".into(),
            display_name: "CenAI Original · English".into(),
            language: QwenLanguage::English,
            locale: DEFAULT_LOCALE.into(),
            model_variant: VoiceModelVariant::Base06B,
            speaker_id: DEFAULT_SPEAKER_ID.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoiceEngineStatus {
    pub runtime: RuntimeState,
    pub model_id: &'static str,
    pub offline_only: bool,
    pub fallback_enabled: bool,
}

impl Default for VoiceEngineStatus {
    fn default() -> Self {
        Self {
            runtime: RuntimeState::NativeRuntimeNotIntegrated,
            model_id: PRIMARY_MODEL_ID,
            offline_only: true,
            fallback_enabled: false,
        }
    }
}

impl VoiceEngineStatus {
    pub fn can_synthesize_offline(&self) -> bool {
        self.runtime == RuntimeState::Ready && self.offline_only
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SynthesisRequest {
    pub text: String,
    pub language: QwenLanguage,
    pub speaker_id: String,
}

impl SynthesisRequest {
    pub fn validate(&self) -> Result<(), &'static str> {
        let text = self.text.trim();
        if text.is_empty() {
            return Err("Speech text cannot be empty");
        }
        if text.len() > MAX_SYNTHESIS_TEXT_BYTES {
            return Err("Speech text exceeds the CenAI Voice limit");
        }
        if text.chars().any(|character| character.is_control() && !matches!(character, '\\n' | '\\r' | '\\t')) {
            return Err("Speech text contains unsupported control characters");
        }
        if self.speaker_id.trim().is_empty() || self.speaker_id.len() > 96 {
            return Err("CenAI Voice speaker profile is invalid");
        }
        Ok(())
    }
}

/// The app must fail closed until a real local model backend is installed.
pub fn offline_unavailable_reason(status: &VoiceEngineStatus) -> Option<&'static str> {
    if status.can_synthesize_offline() {
        None
    } else {
        Some("CenAI Voice's Qwen-based offline runtime is not installed yet. No cloud or Google TTS fallback is permitted.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_is_the_primary_cenai_voice_profile() {
        let profile = VoiceProfile::default();
        assert_eq!(profile.language, QwenLanguage::English);
        assert_eq!(profile.locale, "en-US");
        assert_eq!(profile.speaker_id, "cenai_voice_en_v1");
        assert_eq!(profile.model_variant, VoiceModelVariant::Base06B);
    }

    #[test]
    fn qwen_language_catalog_contains_all_ten_upstream_languages() {
        assert_eq!(QwenLanguage::ALL.len(), 10);
        assert!(QwenLanguage::ALL.contains(&QwenLanguage::English));
        assert!(QwenLanguage::ALL.contains(&QwenLanguage::Filipino) == false);
    }

    #[test]
    fn base_model_is_the_fine_tuning_source_not_the_preset_voice_model() {
        assert!(VoiceModelVariant::Base06B.supports_fine_tuning());
        assert!(!VoiceModelVariant::CustomVoice06B.supports_fine_tuning());
        assert_eq!(VoiceModelVariant::Base06B.model_id(), PRIMARY_MODEL_ID);
    }

    #[test]
    fn voice_request_rejects_empty_oversized_and_invalid_text() {
        let mut request = SynthesisRequest {
            text: "Hello from CenAI.".into(),
            language: QwenLanguage::English,
            speaker_id: DEFAULT_SPEAKER_ID.into(),
        };
        assert!(request.validate().is_ok());
        request.text.clear();
        assert!(request.validate().is_err());
        request.text = "x".repeat(MAX_SYNTHESIS_TEXT_BYTES + 1);
        assert!(request.validate().is_err());
        request.text = "bad\\u{0000}text".into();
        assert!(request.validate().is_err());
    }

    #[test]
    fn runtime_never_claims_offline_ready_or_falls_back_to_cloud() {
        let status = VoiceEngineStatus::default();
        assert!(!status.can_synthesize_offline());
        assert!(!status.fallback_enabled);
        assert!(offline_unavailable_reason(&status).is_some());
    }
}
