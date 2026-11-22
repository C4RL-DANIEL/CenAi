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

/// Sanitizes message bodies without trimming meaningful leading/trailing whitespace.
fn sanitize_message_content(value: &str) -> String {
    let carriage_return = char::from(13);
    let newline = char::from(10);
    let tab = char::from(9);
    let mut chars = value.chars().peekable();
    let mut output = String::with_capacity(value.len());
    while let Some(ch) = chars.next() {
        if ch == carriage_return {
            if chars.peek() == Some(&newline) {
                chars.next();
            }
            output.push(newline);
        } else if !ch.is_control() || ch == newline || ch == tab {
            output.push(ch);
        }
    }
    output
}

/// Only the roles understood by CenAI's conversation and provider adapters are accepted.
fn is_valid_chat_role(value: &str) -> bool {
    matches!(value, "system" | "user" | "assistant" | "tool")
}


/// Builds a text-only OpenAI-compatible chat payload in Rust.
fn build_openai_chat_payload(model: &str, max_tokens: i32, messages_json: &str) -> Result<String, String> {
    use serde_json::{json, Value};

    if model.trim().is_empty() {
        return Err("Enter a model ID in Settings.".to_owned());
    }
    let parsed: Value = serde_json::from_str(messages_json)
        .map_err(|_| "Conversation messages could not be decoded.".to_owned())?;
    let messages = parsed.as_array()
        .ok_or_else(|| "Conversation messages must be a JSON array.".to_owned())?;
    if messages.is_empty() {
        return Err("A conversation must contain at least one message.".to_owned());
    }
    let mut normalized = Vec::with_capacity(messages.len());
    for message in messages {
        let role = message.get("role").and_then(Value::as_str)
            .ok_or_else(|| "A conversation message is missing its role.".to_owned())?;
        if !is_valid_chat_role(role) {
            return Err(format!("Unsupported conversation role: {role}"));
        }
        let content = message.get("content").and_then(Value::as_str)
            .ok_or_else(|| "A conversation message is missing text content.".to_owned())?;
        normalized.push(json!({
            "role": role,
            "content": sanitize_message_content(content)
        }));
    }
    serde_json::to_string(&json!({
        "model": model,
        "stream": true,
        "max_tokens": max_tokens.clamp(1, 131072),
        "messages": normalized
    })).map_err(|_| "Could not encode the provider request.".to_owned())
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
pub extern "system" fn Java_com_cenai_app_NativeCore_buildOpenAiChatPayload(
    mut env: JNIEnv,
    _class: JClass,
    model: JString,
    max_tokens: jint,
    messages_json: JString,
) -> jstring {
    let Some(model_value) = read_java_string(&mut env, &model) else {
        return std::ptr::null_mut();
    };
    let Some(messages_value) = read_java_string(&mut env, &messages_json) else {
        return std::ptr::null_mut();
    };
    match build_openai_chat_payload(&model_value, max_tokens, &messages_value) {
        Ok(payload) => match env.new_string(payload) {
            Ok(value) => value.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        Err(error) => match env.new_string(serde_json::json!({ "error": error }).to_string()) {
            Ok(value) => value.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
    }
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
    use super::{build_openai_chat_payload, estimate_prompt_tokens, is_valid_chat_role, normalize_prompt, sanitize_message_content};

    #[test]
    fn normalizes_line_endings_and_removes_unsafe_controls() {
        let cr = char::from(13);
        let lf = char::from(10);
        let tab = char::from(9);
        let bell = char::from(7);
        let input = format!("  hello{cr}{lf}world{cr}{bell}{tab}!  ");
        let expected = format!("hello{lf}world{lf}{tab}!");
        assert_eq!(normalize_prompt(&input), expected);
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
        let cr = char::from(13);
        let lf = char::from(10);
        let bell = char::from(7);
        let input = format!("  hello{cr}{lf}world{bell}  ");
        let expected = format!("  hello{lf}world  ");
        assert_eq!(sanitize_message_content(&input), expected);
    }

    #[test]
    fn builds_valid_openai_payload_and_sanitizes_message_content() {
        let cr = char::from(13);
        let lf = char::from(10);
        let content = format!("hello{cr}{lf}world");
        let messages = serde_json::json!([{"role":"user","content":content}]).to_string();
        let payload = build_openai_chat_payload("gpt-test", 2048, &messages).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(parsed["model"], "gpt-test");
        assert_eq!(parsed["stream"], true);
        assert_eq!(parsed["max_tokens"], 2048);
        assert_eq!(parsed["messages"][0]["content"], format!("hello{lf}world"));
    }

    #[test]
    fn rejects_unsupported_roles_in_provider_payload() {
        let messages = serde_json::json!([{"role":"developer","content":"no"}]).to_string();
        assert!(build_openai_chat_payload("gpt-test", 100, &messages).is_err());
    }

    #[test]
    fn preserves_unicode_character_boundaries() {
        assert_eq!(estimate_prompt_tokens("你好"), 1);
        assert_eq!(normalize_prompt("  你好  "), "你好");
    }
}
