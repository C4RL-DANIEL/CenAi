//! Fixed Lightning on-device model contract.
//!
//! The Rust application owns model policy and invocation. LiteRT-LM is retained
//! only as the Android CPU inference adapter because the pinned .litertlm artifact
//! is a LiteRT-LM model format; it is never exposed as a configurable provider.

pub const ENGINE_ID: &str = "cenai.lightning";
pub const MODEL_ID: &str = "litert-community/Qwen3-0.6B";
pub const MODEL_REVISION: &str = "e6687177e00897731e7716267eebd4eba0dde7c2";
pub const MODEL_ASSET: &str = "models/lightning.litertlm";
pub const MODEL_FILE_NAME: &str = "lightning.litertlm";
pub const MODEL_BYTES: u64 = 613_722_264;
pub const MODEL_SHA256: &str = "c5f39b4191926786c3e6668b62af4d6eb15248dfc7a5348252a9b406f33f2108";
pub const RUNTIME_VERSION: &str = "0.18.1";
pub const BACKEND: &str = "cpu";
pub const LICENSE: &str = "Apache-2.0";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LightningState {
    NotInstalled,
    Verifying,
    Ready,
    Loading,
    Generating,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LightningStatus {
    pub state: LightningState,
    pub installed_bytes: u64,
    pub expected_bytes: u64,
    pub model_sha256: &'static str,
    pub error: Option<String>,
}

impl Default for LightningStatus {
    fn default() -> Self {
        Self {
            state: LightningState::NotInstalled,
            installed_bytes: 0,
            expected_bytes: MODEL_BYTES,
            model_sha256: MODEL_SHA256,
            error: None,
        }
    }
}

pub fn verify_model_metadata(bytes: u64, sha256: &str) -> Result<(), &'static str> {
    if bytes != MODEL_BYTES { return Err("Lightning model has an unexpected byte size"); }
    if !sha256.eq_ignore_ascii_case(MODEL_SHA256) { return Err("Lightning model SHA-256 does not match the pinned artifact"); }
    Ok(())
}

pub fn validate_prompt(prompt: &str) -> Result<(), &'static str> {
    let prompt = prompt.trim();
    if prompt.is_empty() { return Err("Lightning requires a non-empty prompt"); }
    if prompt.len() > 32_000 { return Err("Lightning prompt exceeds the 32 KB limit"); }
    if prompt.chars().any(|character| character == '\0') { return Err("Lightning prompt contains a null character"); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lightning_model_is_fixed_and_metadata_is_pinned() {
        assert_eq!(ENGINE_ID, "cenai.lightning");
        assert_eq!(MODEL_ID, "litert-community/Qwen3-0.6B");
        assert_eq!(MODEL_BYTES, 613_722_264);
        assert_eq!(RUNTIME_VERSION, "0.18.1");
        assert_eq!(BACKEND, "cpu");
        assert!(verify_model_metadata(MODEL_BYTES, MODEL_SHA256).is_ok());
        assert!(verify_model_metadata(MODEL_BYTES - 1, MODEL_SHA256).is_err());
        assert!(verify_model_metadata(MODEL_BYTES, "00".repeat(32).as_str()).is_err());
    }

    #[test]
    fn lightning_prompt_validation_is_bounded() {
        assert!(validate_prompt("hello").is_ok());
        assert!(validate_prompt("  ").is_err());
        assert!(validate_prompt(&"x".repeat(32_001)).is_err());
        assert!(validate_prompt("bad\\0prompt").is_ok());
        assert!(validate_prompt("bad\0prompt").is_err());
    }
}
