use jni::objects::{JClass, JString};
use jni::sys::{jboolean, jint, jstring, JNI_FALSE, JNI_TRUE};
use jni::JNIEnv;

/// Converts a Java string without allowing a JNI conversion error to unwind across the ABI.
fn read_java_string(env: &mut JNIEnv, input: &JString) -> Option<String> {
    env.get_string(input).ok().map(Into::into)
}

/// Produces a stable prompt string for the chat pipeline:
/// - normalizes CRLF/CR to LF,
/// - removes control characters other than tabs and line breaks,
/// - trims surrounding whitespace.
fn normalize_prompt(value: &str) -> String {
    let normalized_lines = value.replace("\r\n", "\n").replace('\r', "\n');
    let filtered: String = normalized_lines
        .chars()
        .filter(|ch| !ch.is_control() || *ch == '\n' || *ch == '\t')
        .collect();
    filtered.trim().to_owned()
}

/// A deliberately conservative, language-agnostic estimate for UI hints only.
/// This is not a provider tokenizer and must never be used for billing or hard limits.
/// Sanitizes message bodies without trimming meaningful leading/trailing whitespace.
fn sanitize_message_content(value: &str) -> String {
    value
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .chars()
        .filter(|ch| !ch.is_control() || *ch == '\n' || *ch == '\t')
        .collect()
}

/// Only the roles understood by CenAI's conversation and provider adapters are accepted.
fn is_valid_chat_role(value: &str) -> bool {
    matches!(value, "system" | "user" | "assistant" | "tool")
}

fn estimate_prompt_tokens(value: &str) -> usize {
    let chars = value.chars().count();
    if chars == 0 {
        return 0;
    }

    // A rough 1 token / 4 characters estimate, with a floor for short non-empty text.
    (chars + 3) / 4
}

#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_normalizePrompt(
    mut env: JNIEnv,
    _class: JClass,
    input: JString,
) -> jstring {
    let Some(value) = read_java_string(&mut env, &input) else {
        return std::ptr::null_mut();
    };

    match env.new_string(normalize_prompt(&value)) {
        Ok(value) => value.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_normalizeMessageContent(
    mut env: JNIEnv,
    _class: JClass,
    input: JString,
) -> jstring {
    let Some(value) = read_java_string(&mut env, &input) else {
        return std::ptr::null_mut();
    };
    match env.new_string(sanitize_message_content(&value)) {
        Ok(value) => value.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_isValidChatRole(
    mut env: JNIEnv,
    _class: JClass,
    role: JString,
) -> jboolean {
    let Some(value) = read_java_string(&mut env, &role) else {
        return JNI_FALSE;
    };
    if is_valid_chat_role(&value) { JNI_TRUE } else { JNI_FALSE }
}

#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_estimatePromptTokens(
    mut env: JNIEnv,
    _class: JClass,
    input: JString,
) -> jint {
    let Some(value) = read_java_string(&mut env, &input) else {
        return 0;
    };
    estimate_prompt_tokens(&value).min(jint::MAX as usize) as jint
}

#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_isPromptWithinLimit(
    mut env: JNIEnv,
    _class: JClass,
    input: JString,
    max_chars: jint,
) -> jboolean {
    if max_chars < 0 {
        return JNI_FALSE;
    }
    let Some(value) = read_java_string(&mut env, &input) else {
        return JNI_FALSE;
    };
    if value.chars().count() <= max_chars as usize {
        JNI_TRUE
    } else {
        JNI_FALSE
    }
}

#[cfg(test)]
mod tests {
    use super::{estimate_prompt_tokens, is_valid_chat_role, normalize_prompt, sanitize_message_content};

    #[test]
    fn normalizes_line_endings_and_removes_unsafe_controls() {
        let input = "  hello\r\nworld\r\u{0007}\t!  ";
        assert_eq!(normalize_prompt(input), "hello\nworld\n\t!");
    }

    #[test]
    fn estimates_empty_and_non_empty_prompts() {
        assert_eq!(estimate_prompt_tokens(""), 0);
        assert_eq!(estimate_prompt_tokens("hello"), 2);
        assert_eq!(estimate_prompt_tokens("12345678"), 2);
    }

    #[test]
    fn validates_only_supported_conversation_roles() {
        assert!(is_valid_chat_role("system"));
        assert!(is_valid_chat_role("user"));
        assert!(is_valid_chat_role("assistant"));
        assert!(is_valid_chat_role("tool"));
        assert!(!is_valid_chat_role("developer"));
        assert!(!is_valid_chat_role("USER"));
    }

    #[test]
    fn sanitizes_message_content_without_trimming_spaces() {
        assert_eq!(sanitize_message_content("  hello\r\nworld\u{0007}  "), "  hello\nworld  ");
    }

    #[test]
    fn preserves_unicode_character_boundaries() {
        assert_eq!(estimate_prompt_tokens("你好"), 1);
        assert_eq!(normalize_prompt("  你好  "), "你好");
    }
}
