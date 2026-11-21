//! Native, offline Qwen3-TTS runtime bridge for CenAI Voice.
//!
//! The C++ model runtime is packaged as libcenai_qwen_bridge.so by the Android
//! NDK build. This module loads it lazily; it never calls a cloud speech API.

use std::path::Path;

use super::qwen_voice::{QwenLanguage, SynthesisRequest, PRIMARY_MODEL_ID};

pub const NATIVE_LIBRARY_NAME: &str = "libcenai_qwen_bridge.so";
pub const PRIMARY_GGUF_MODEL: &str = "qwen-talker-0.6b-base-Q8_0.gguf";
pub const TOKENIZER_GGUF_MODEL: &str = "qwen-tokenizer-12hz-Q8_0.gguf";
const MAX_AUDIO_SECONDS: usize = 15 * 60;

#[cfg(target_os = "android")]
mod android {
    use std::{
        ffi::{CStr, CString, c_char, c_void},
        path::Path,
        ptr,
        slice,
    };

    use super::{language_id, write_pcm16_wav, MAX_AUDIO_SECONDS, NATIVE_LIBRARY_NAME, PRIMARY_GGUF_MODEL};

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NativeResult {
        audio: *mut f32,
        audio_len: i32,
        sample_rate: i32,
        success: i32,
        error_msg: *mut c_char,
        total_ms: i64,
    }

    type Create = unsafe extern "C" fn() -> *mut c_void;
    type Destroy = unsafe extern "C" fn(*mut c_void);
    type SetThreads = unsafe extern "C" fn(i32) -> i32;
    type LoadModels = unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> i32;
    type Synthesize = unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char, i32, i32) -> NativeResult;
    type ExtractEmbedding = unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> i32;
    type FreeResult = unsafe extern "C" fn(NativeResult);
    type LastError = unsafe extern "C" fn(*mut c_void) -> *mut c_char;
    type FreeString = unsafe extern "C" fn(*mut c_char);

    pub struct QwenRuntime {
        handle: *mut c_void,
        context: *mut c_void,
        destroy: Destroy,
        synthesize: Synthesize,
        extract_embedding: ExtractEmbedding,
        free_result: FreeResult,
        last_error: LastError,
        free_string: FreeString,
    }

    // The GGML context is only accessed behind the caller's Mutex; no C++ pointer
    // is ever shared for concurrent mutation.
    unsafe impl Send for QwenRuntime {}

    impl QwenRuntime {
        pub fn load(model_directory: &Path, cpu_threads: i32) -> Result<Self, String> {
            let library_name = CString::new(NATIVE_LIBRARY_NAME).map_err(|error| error.to_string())?;
            let handle = unsafe { libc::dlopen(library_name.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
            if handle.is_null() {
                return Err("CenAI Qwen native library is missing from this APK.".into());
            }

            let loaded = unsafe {
                let create: Create = load_symbol(handle, "cenai_qwen_create")?;
                let destroy: Destroy = load_symbol(handle, "cenai_qwen_destroy")?;
                let set_threads: SetThreads = load_symbol(handle, "cenai_qwen_set_cpu_threads")?;
                let load_models: LoadModels = load_symbol(handle, "cenai_qwen_load_models")?;
                let synthesize: Synthesize = load_symbol(handle, "cenai_qwen_synthesize")?;
                let extract_embedding: ExtractEmbedding = load_symbol(handle, "cenai_qwen_extract_speaker_embedding")?;
                let free_result: FreeResult = load_symbol(handle, "cenai_qwen_free_result")?;
                let last_error: LastError = load_symbol(handle, "cenai_qwen_last_error")?;
                let free_string: FreeString = load_symbol(handle, "cenai_qwen_free_string")?;

                let context = create();
                if context.is_null() {
                    return Err("Qwen3-TTS could not allocate its native context.".into());
                }
                let directory = CString::new(model_directory.to_string_lossy().as_bytes())
                    .map_err(|_| "Model directory contains an invalid path character")?;
                let model_name = CString::new(PRIMARY_GGUF_MODEL).expect("static model name");
                let _ = set_threads(cpu_threads.max(1));
                if load_models(context, directory.as_ptr(), model_name.as_ptr()) == 0 {
                    let message_ptr = last_error(context);
                    let message = if message_ptr.is_null() {
                        "Qwen3-TTS could not load the CenAI Voice model pack.".to_owned()
                    } else {
                        let message = CStr::from_ptr(message_ptr).to_string_lossy().into_owned();
                        free_string(message_ptr);
                        message
                    };
                    destroy(context);
                    return Err(message);
                }
                Ok(Self {
                    handle,
                    context,
                    destroy,
                    synthesize,
                    extract_embedding,
                    free_result,
                    last_error,
                    free_string,
                })
            };

            match loaded {
                Ok(runtime) => Ok(runtime),
                Err(error) => {
                    unsafe { libc::dlclose(handle); }
                    Err(error)
                }
            }
        }

        pub fn synthesize_wav(
            &mut self,
            text: &str,
            language: QwenLanguage,
            speaker_embedding: Option<&Path>,
            cpu_threads: i32,
        ) -> Result<(Vec<u8>, i64), String> {
            let text = CString::new(text).map_err(|_| "Speech text contains a null byte")?;
            let embedding = speaker_embedding
                .map(|path| CString::new(path.to_string_lossy().as_bytes()))
                .transpose()
                .map_err(|_| "Speaker profile path contains a null byte")?;
            let embedding_ptr = embedding.as_ref().map_or(ptr::null(), |value| value.as_ptr());
            let result = unsafe {
                (self.synthesize)(
                    self.context,
                    text.as_ptr(),
                    embedding_ptr,
                    language_id(language),
                    cpu_threads.max(1),
                )
            };

            if result.success == 0 {
                let error = if !result.error_msg.is_null() {
                    unsafe { CStr::from_ptr(result.error_msg).to_string_lossy().into_owned() }
                } else {
                    self.read_last_error()
                };
                unsafe { (self.free_result)(result); }
                return Err(if error.is_empty() {
                    "Qwen3-TTS synthesis failed without an error message.".into()
                } else {
                    error
                });
            }

            if result.audio.is_null() || result.audio_len <= 0 || result.sample_rate <= 0 {
                unsafe { (self.free_result)(result); }
                return Err("Qwen3-TTS returned an empty or invalid audio buffer.".into());
            }
            let max_samples = result.sample_rate as usize * MAX_AUDIO_SECONDS;
            if result.audio_len as usize > max_samples {
                unsafe { (self.free_result)(result); }
                return Err("Generated speech exceeded CenAI's 15-minute safety limit.".into());
            }

            let samples = unsafe { slice::from_raw_parts(result.audio, result.audio_len as usize) };
            let wav = write_pcm16_wav(result.sample_rate as u32, samples);
            let elapsed_ms = result.total_ms;
            unsafe { (self.free_result)(result); }
            Ok((wav, elapsed_ms))
        }

        pub fn extract_speaker_embedding(
            &mut self,
            reference_wav: &Path,
            output_path: &Path,
        ) -> Result<(), String> {
            let reference = CString::new(reference_wav.to_string_lossy().as_bytes())
                .map_err(|_| "Reference audio path contains a null byte")?;
            let output = CString::new(output_path.to_string_lossy().as_bytes())
                .map_err(|_| "Voice profile path contains a null byte")?;
            let success = unsafe {
                (self.extract_embedding)(self.context, reference.as_ptr(), output.as_ptr())
            };
            if success == 0 {
                return Err(self.read_last_error());
            }
            Ok(())
        }

        fn read_last_error(&self) -> String {
            let message = unsafe { (self.last_error)(self.context) };
            if message.is_null() {
                return String::new();
            }
            let value = unsafe { CStr::from_ptr(message).to_string_lossy().into_owned() };
            unsafe { (self.free_string)(message); }
            value
        }
    }

    impl Drop for QwenRuntime {
        fn drop(&mut self) {
            unsafe {
                if !self.context.is_null() { (self.destroy)(self.context); }
                if !self.handle.is_null() { libc::dlclose(self.handle); }
            }
        }
    }

    unsafe fn load_symbol<T: Copy>(handle: *mut c_void, name: &str) -> Result<T, String> {
        let name = CString::new(name).map_err(|error| error.to_string())?;
        let symbol = libc::dlsym(handle, name.as_ptr());
        if symbol.is_null() {
            return Err(format!("CenAI Qwen bridge is missing symbol {}.", name.to_string_lossy()));
        }
        Ok(std::mem::transmute_copy(&symbol))
    }
}

#[cfg(target_os = "android")]
pub use android::QwenRuntime;

#[cfg(not(target_os = "android"))]
pub struct QwenRuntime;

#[cfg(not(target_os = "android"))]
impl QwenRuntime {
    pub fn load(_model_directory: &Path, _cpu_threads: i32) -> Result<Self, String> {
        Err("CenAI Voice native inference is available only in the Android build.".into())
    }

    pub fn synthesize_wav(
        &mut self,
        _text: &str,
        _language: QwenLanguage,
        _speaker_embedding: Option<&Path>,
        _cpu_threads: i32,
    ) -> Result<(Vec<u8>, i64), String> {
        Err("CenAI Voice native inference is unavailable on this platform.".into())
    }
}

pub fn validate_synthesis_text(text: &str, language: QwenLanguage) -> Result<SynthesisRequest, &'static str> {
    let request = SynthesisRequest {
        text: text.to_owned(),
        language,
        speaker_id: super::qwen_voice::DEFAULT_SPEAKER_ID.to_owned(),
    };
    request.validate()?;
    Ok(request)
}

fn language_id(language: QwenLanguage) -> i32 {
    match language {
        QwenLanguage::English => 2050,
        // Qwen's C++ runtime accepts -1 for automatic language detection.
        _ => -1,
    }
}

fn write_pcm16_wav(sample_rate: u32, samples: &[f32]) -> Vec<u8> {
    let data_len = samples.len().saturating_mul(2).min(u32::MAX as usize - 36) as u32;
    let mut wav = Vec::with_capacity(44 + data_len as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36u32 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&(sample_rate.saturating_mul(2)).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples.iter().take(data_len as usize / 2) {
        let normalized = sample.clamp(-1.0, 1.0);
        let pcm = (normalized * i16::MAX as f32).round() as i16;
        wav.extend_from_slice(&pcm.to_le_bytes());
    }
    wav
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_qwen_voice_model_is_a_compact_english_first_gguf_pack() {
        assert_eq!(PRIMARY_GGUF_MODEL, "qwen-talker-0.6b-base-Q8_0.gguf");
        assert_eq!(TOKENIZER_GGUF_MODEL, "qwen-tokenizer-12hz-Q8_0.gguf");
        assert_eq!(NATIVE_LIBRARY_NAME, "libcenai_qwen_bridge.so");
    }

    #[test]
    fn generated_float_audio_is_wrapped_as_mono_pcm16_wav() {
        let wav = write_pcm16_wav(24_000, &[0.0, 0.5, -0.5, 1.0, -1.0]);
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 24_000);
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 10);
        assert_eq!(i16::from_le_bytes([wav[46], wav[47]]), (0.5 * i16::MAX as f32) as i16);
    }

    #[test]
    fn synthesis_text_is_validated_before_entering_native_code() {
        assert!(validate_synthesis_text("Hello, CenAI.", QwenLanguage::English).is_ok());
        assert!(validate_synthesis_text("  ", QwenLanguage::English).is_err());
    }
}
