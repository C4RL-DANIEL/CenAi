pub mod agent;
pub mod knowledge;
pub mod providers;
pub mod storage;
pub mod context;
pub mod auth;
pub mod monitor;
pub mod updates;
pub mod usage;

use jni::objects::{JClass, JString};
use jni::sys::{jboolean, jint, jlong, jstring, JNI_FALSE, JNI_TRUE};
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
    let normalized_lines = value.replace("\r\n", "\n").replace('\r', "\n");
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


#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_evaluateArithmetic(
    mut env: JNIEnv,
    _class: JClass,
    input: JString,
) -> jstring {
    let Some(expression) = read_java_string(&mut env, &input) else {
        return std::ptr::null_mut();
    };
    let result = match agent::evaluate_arithmetic(&expression) {
        Ok(value) => value.to_string(),
        Err(error) => format!("ERROR:{error}"),
    };
    match env.new_string(result) {
        Ok(value) => value.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}


#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_retrieveLocalKnowledge(
    mut env: JNIEnv,
    _class: JClass,
    payload: JString,
    max_results: jint,
    max_characters: jint,
) -> jstring {
    let Some(payload_value) = read_java_string(&mut env, &payload) else {
        return std::ptr::null_mut();
    };
    let result = (|| {
        let parsed: serde_json::Value = serde_json::from_str(&payload_value)
            .map_err(|_| "Local knowledge request could not be decoded.".to_owned())?;
        let query = parsed.get("query").and_then(serde_json::Value::as_str)
            .ok_or_else(|| "Local knowledge query is missing.".to_owned())?;
        let conversations = parsed.get("conversations").cloned().unwrap_or_else(|| serde_json::json!([]));
        let current_messages = parsed.get("current_messages").cloned().unwrap_or_else(|| serde_json::json!([]));
        knowledge::retrieve_local_knowledge(
            query,
            &conversations.to_string(),
            &current_messages.to_string(),
            max_results.max(1) as usize,
            max_characters.max(500) as usize,
        )
    })();
    let output = result.unwrap_or_else(|error| format!("ERROR:{error}"));
    match env.new_string(output) {
        Ok(value) => value.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}


#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_buildProviderPayload(
    mut env: JNIEnv,
    _class: JClass,
    provider: JString,
    model: JString,
    max_tokens: jint,
    messages_json: JString,
    custom_params_json: JString,
) -> jstring {
    let Some(provider_value) = read_java_string(&mut env, &provider) else {
        return std::ptr::null_mut();
    };
    let Some(model_value) = read_java_string(&mut env, &model) else {
        return std::ptr::null_mut();
    };
    let Some(messages_value) = read_java_string(&mut env, &messages_json) else {
        return std::ptr::null_mut();
    };
    let Some(custom_params_value) = read_java_string(&mut env, &custom_params_json) else {
        return std::ptr::null_mut();
    };
    let output = match providers::build_provider_payload(
        &provider_value,
        &model_value,
        max_tokens,
        &messages_value,
        &custom_params_value,
    ) {
        Ok(payload) => payload,
        Err(error) => serde_json::json!({ "error": error }).to_string(),
    };
    match env.new_string(output) {
        Ok(value) => value.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}


#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_evaluateToolPolicy(
    mut env: JNIEnv,
    _class: JClass,
    name: JString,
    enabled_tools_json: JString,
) -> jstring {
    let Some(name_value) = read_java_string(&mut env, &name) else {
        return std::ptr::null_mut();
    };
    let Some(enabled_value) = read_java_string(&mut env, &enabled_tools_json) else {
        return std::ptr::null_mut();
    };
    let output = match agent::tool_policy(&name_value, &enabled_value) {
        Ok(policy) => policy,
        Err(error) => serde_json::json!({ "error": error }).to_string(),
    };
    match env.new_string(output) {
        Ok(value) => value.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}


#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_parseProviderDelta(
    mut env: JNIEnv,
    _class: JClass,
    provider: JString,
    raw: JString,
) -> jstring {
    let Some(provider_value) = read_java_string(&mut env, &provider) else {
        return std::ptr::null_mut();
    };
    let Some(raw_value) = read_java_string(&mut env, &raw) else {
        return std::ptr::null_mut();
    };
    match env.new_string(providers::parse_provider_delta(&provider_value, &raw_value)) {
        Ok(value) => value.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}


#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_parseAgentToolCall(
    mut env: JNIEnv,
    _class: JClass,
    response: JString,
) -> jstring {
    let Some(response_value) = read_java_string(&mut env, &response) else {
        return std::ptr::null_mut();
    };
    let output = match agent::parse_tool_call(&response_value) {
        Ok(payload) => payload,
        Err(error) => serde_json::json!({ "error": error }).to_string(),
    };
    match env.new_string(output) {
        Ok(value) => value.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}


#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_executeSafeTool(
    mut env: JNIEnv,
    _class: JClass,
    name: JString,
    arguments_json: JString,
    local_time: JString,
    local_date: JString,
) -> jstring {
    let Some(name_value) = read_java_string(&mut env, &name) else {
        return std::ptr::null_mut();
    };
    let Some(arguments_value) = read_java_string(&mut env, &arguments_json) else {
        return std::ptr::null_mut();
    };
    let Some(time_value) = read_java_string(&mut env, &local_time) else {
        return std::ptr::null_mut();
    };
    let Some(date_value) = read_java_string(&mut env, &local_date) else {
        return std::ptr::null_mut();
    };
    let output = match agent::execute_safe_tool(
        &name_value,
        &arguments_value,
        &time_value,
        &date_value,
    ) {
        Ok(value) => serde_json::json!({ "output": value }).to_string(),
        Err(error) => serde_json::json!({ "error": error }).to_string(),
    };
    match env.new_string(output) {
        Ok(value) => value.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}


#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_listAgentTools(
    mut env: JNIEnv,
    _class: JClass,
    enabled_tools_json: JString,
) -> jstring {
    let Some(enabled_value) = read_java_string(&mut env, &enabled_tools_json) else {
        return std::ptr::null_mut();
    };
    let output = match agent::tool_catalog(&enabled_value) {
        Ok(catalog) => catalog,
        Err(error) => serde_json::json!({ "error": error }).to_string(),
    };
    match env.new_string(output) {
        Ok(value) => value.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}


#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_normalizeConversations(
    mut env: JNIEnv,
    _class: JClass,
    raw_json: JString,
) -> jstring {
    let Some(raw_value) = read_java_string(&mut env, &raw_json) else {
        return std::ptr::null_mut();
    };
    let output = match storage::normalize_conversations(&raw_value) {
        Ok(value) => value,
        Err(error) => serde_json::json!({ "error": error }).to_string(),
    };
    match env.new_string(output) {
        Ok(value) => value.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}


#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_buildSystemContext(
    mut env: JNIEnv,
    _class: JClass,
    system_prompt: JString,
    approved_memory: JString,
    local_knowledge: JString,
    tool_guide: JString,
) -> jstring {
    let Some(system_value) = read_java_string(&mut env, &system_prompt) else {
        return std::ptr::null_mut();
    };
    let Some(memory_value) = read_java_string(&mut env, &approved_memory) else {
        return std::ptr::null_mut();
    };
    let Some(knowledge_value) = read_java_string(&mut env, &local_knowledge) else {
        return std::ptr::null_mut();
    };
    let Some(tool_value) = read_java_string(&mut env, &tool_guide) else {
        return std::ptr::null_mut();
    };
    match env.new_string(context::build_system_context(
        &system_value,
        &memory_value,
        &knowledge_value,
        &tool_value,
    )) {
        Ok(value) => value.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}


#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_buildProviderRequestConfig(
    mut env: JNIEnv,
    _class: JClass,
    provider: JString,
    base_url: JString,
    api_key: JString,
    oauth: jboolean,
    project_id: JString,
    custom_headers_json: JString,
) -> jstring {
    let Some(provider_value) = read_java_string(&mut env, &provider) else {
        return std::ptr::null_mut();
    };
    let Some(base_value) = read_java_string(&mut env, &base_url) else {
        return std::ptr::null_mut();
    };
    let Some(key_value) = read_java_string(&mut env, &api_key) else {
        return std::ptr::null_mut();
    };
    let Some(project_value) = read_java_string(&mut env, &project_id) else {
        return std::ptr::null_mut();
    };
    let Some(headers_value) = read_java_string(&mut env, &custom_headers_json) else {
        return std::ptr::null_mut();
    };
    let output = match providers::build_provider_request_config(
        &provider_value,
        &base_value,
        &key_value,
        oauth != 0,
        &project_value,
        &headers_value,
    ) {
        Ok(config) => config,
        Err(error) => serde_json::json!({ "error": error }).to_string(),
    };
    match env.new_string(output) {
        Ok(value) => value.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}


#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_buildToolGuide(
    mut env: JNIEnv,
    _class: JClass,
    enabled_tools_json: JString,
    planning_enabled: jboolean,
    allow_device_actions: jboolean,
) -> jstring {
    let Some(enabled_value) = read_java_string(&mut env, &enabled_tools_json) else {
        return std::ptr::null_mut();
    };
    let output = match agent::build_tool_guide(
        &enabled_value,
        planning_enabled != 0,
        allow_device_actions != 0,
    ) {
        Ok(guide) => guide,
        Err(error) => format!("ERROR:{error}"),
    };
    match env.new_string(output) {
        Ok(value) => value.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}


#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_buildSynthesisPrompt(
    mut env: JNIEnv,
    _class: JClass,
    user_request: JString,
    answers_json: JString,
) -> jstring {
    let Some(request_value) = read_java_string(&mut env, &user_request) else {
        return std::ptr::null_mut();
    };
    let Some(answers_value) = read_java_string(&mut env, &answers_json) else {
        return std::ptr::null_mut();
    };
    let output = match providers::build_synthesis_prompt(&request_value, &answers_value) {
        Ok(prompt) => prompt,
        Err(error) => format!("ERROR:{error}"),
    };
    match env.new_string(output) {
        Ok(value) => value.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}


#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_parseAgentStep(
    mut env: JNIEnv,
    _class: JClass,
    response: JString,
) -> jstring {
    let Some(response_value) = read_java_string(&mut env, &response) else {
        return std::ptr::null_mut();
    };
    let output = match agent::parse_agent_step(&response_value) {
        Ok(value) => value,
        Err(error) => serde_json::json!({ "error": error }).to_string(),
    };
    match env.new_string(output) {
        Ok(value) => value.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}


#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_selectProviderTargets(
    mut env: JNIEnv,
    _class: JClass,
    primary_provider: JString,
    multi_agent: jboolean,
    local_model_enabled: jboolean,
    configured_json: JString,
) -> jstring {
    let Some(primary_value) = read_java_string(&mut env, &primary_provider) else {
        return std::ptr::null_mut();
    };
    let Some(configured_value) = read_java_string(&mut env, &configured_json) else {
        return std::ptr::null_mut();
    };
    let output = match providers::select_provider_targets(
        &primary_value,
        multi_agent != 0,
        local_model_enabled != 0,
        &configured_value,
    ) {
        Ok(targets) => targets,
        Err(error) => serde_json::json!({ "error": error }).to_string(),
    };
    match env.new_string(output) {
        Ok(value) => value.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}


#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_parseProviderError(
    mut env: JNIEnv,
    _class: JClass,
    raw: JString,
) -> jstring {
    let Some(raw_value) = read_java_string(&mut env, &raw) else {
        return std::ptr::null_mut();
    };
    match env.new_string(providers::parse_provider_error(&raw_value)) {
        Ok(value) => value.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}


#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_stripImageMarkers(
    mut env: JNIEnv,
    _class: JClass,
    content: JString,
) -> jstring {
    let Some(content_value) = read_java_string(&mut env, &content) else {
        return std::ptr::null_mut();
    };
    match env.new_string(providers::strip_image_markers(&content_value)) {
        Ok(value) => value.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}


#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_parseCloudResponse(
    mut env: JNIEnv,
    _class: JClass,
    raw: JString,
) -> jstring {
    let Some(raw_value) = read_java_string(&mut env, &raw) else {
        return std::ptr::null_mut();
    };
    let output = match providers::parse_cloud_response(&raw_value) {
        Ok(content) => serde_json::json!({ "content": content }).to_string(),
        Err(error) => serde_json::json!({ "error": error }).to_string(),
    };
    match env.new_string(output) {
        Ok(value) => value.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}


#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_buildAuthPayload(
    mut env: JNIEnv,
    _class: JClass,
    action: JString,
    email: JString,
    password: JString,
    refresh_token: JString,
) -> jstring {
    let Some(action_value) = read_java_string(&mut env, &action) else { return std::ptr::null_mut(); };
    let Some(email_value) = read_java_string(&mut env, &email) else { return std::ptr::null_mut(); };
    let Some(password_value) = read_java_string(&mut env, &password) else { return std::ptr::null_mut(); };
    let Some(refresh_value) = read_java_string(&mut env, &refresh_token) else { return std::ptr::null_mut(); };
    let output = match auth::build_auth_payload(&action_value, &email_value, &password_value, &refresh_value) {
        Ok(payload) => payload,
        Err(error) => serde_json::json!({ "error": error }).to_string(),
    };
    match env.new_string(output) {
        Ok(value) => value.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_parseAuthSession(
    mut env: JNIEnv,
    _class: JClass,
    raw_json: JString,
) -> jstring {
    let Some(raw_value) = read_java_string(&mut env, &raw_json) else { return std::ptr::null_mut(); };
    let output = match auth::parse_auth_session(&raw_value) {
        Ok(session) => session,
        Err(error) => serde_json::json!({ "error": error }).to_string(),
    };
    match env.new_string(output) {
        Ok(value) => value.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_validateEmailOtp(
    mut env: JNIEnv,
    _class: JClass,
    code: JString,
) -> jstring {
    let Some(code_value) = read_java_string(&mut env, &code) else { return std::ptr::null_mut(); };
    let output = auth::validate_email_otp(&code_value).err().unwrap_or_else(|| "OK".to_owned());
    match env.new_string(output) {
        Ok(value) => value.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}


#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_containsImageMarker(
    mut env: JNIEnv,
    _class: JClass,
    content: JString,
) -> jboolean {
    let Some(content_value) = read_java_string(&mut env, &content) else { return 0; };
    if providers::contains_image_marker(&content_value) { 1 } else { 0 }
}


#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_validateGoogleOAuthClientId(
    mut env: JNIEnv,
    _class: JClass,
    client_id: JString,
) -> jstring {
    let Some(value) = read_java_string(&mut env, &client_id) else { return std::ptr::null_mut(); };
    let output = auth::validate_google_oauth_client_id(&value)
        .err().unwrap_or_else(|| "OK".to_owned());
    match env.new_string(output) {
        Ok(value) => value.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_validateGoogleOAuthCallback(
    mut env: JNIEnv,
    _class: JClass,
    scheme: JString,
    path: JString,
    returned_state: JString,
    expected_state: JString,
    error: JString,
    code: JString,
) -> jstring {
    let Some(scheme_value) = read_java_string(&mut env, &scheme) else { return std::ptr::null_mut(); };
    let Some(path_value) = read_java_string(&mut env, &path) else { return std::ptr::null_mut(); };
    let Some(state_value) = read_java_string(&mut env, &returned_state) else { return std::ptr::null_mut(); };
    let Some(expected_value) = read_java_string(&mut env, &expected_state) else { return std::ptr::null_mut(); };
    let Some(error_value) = read_java_string(&mut env, &error) else { return std::ptr::null_mut(); };
    let Some(code_value) = read_java_string(&mut env, &code) else { return std::ptr::null_mut(); };
    let output = match auth::validate_google_oauth_callback(
        &scheme_value, &path_value, &state_value, &expected_value, &error_value, &code_value
    ) {
        Ok(code) => serde_json::json!({ "code": code }).to_string(),
        Err(error) => serde_json::json!({ "error": error }).to_string(),
    };
    match env.new_string(output) {
        Ok(value) => value.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_parseGoogleOAuthToken(
    mut env: JNIEnv,
    _class: JClass,
    raw_json: JString,
    now_ms: jlong,
) -> jstring {
    let Some(raw_value) = read_java_string(&mut env, &raw_json) else { return std::ptr::null_mut(); };
    let output = match auth::parse_google_oauth_token(&raw_value, now_ms.max(0) as u64) {
        Ok(token) => token,
        Err(error) => serde_json::json!({ "error": error }).to_string(),
    };
    match env.new_string(output) {
        Ok(value) => value.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}


#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_generateOAuthPkce(
    env: JNIEnv,
    _class: JClass,
) -> jstring {
    let output = match auth::generate_oauth_pkce() {
        Ok(values) => values,
        Err(error) => serde_json::json!({ "error": error }).to_string(),
    };
    match env.new_string(output) {
        Ok(value) => value.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}


#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_monitorEventKey(
    mut env: JNIEnv,
    _class: JClass,
    event: JString,
) -> jstring {
    let Some(value) = read_java_string(&mut env, &event) else { return std::ptr::null_mut(); };
    let output = monitor::event_key(&value).unwrap_or_default();
    match env.new_string(output) { Ok(value) => value.into_raw(), Err(_) => std::ptr::null_mut() }
}

#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_monitorSuccessRate(
    _env: JNIEnv,
    _class: JClass,
    succeeded: jint,
    failed: jint,
) -> jint {
    monitor::success_rate(succeeded, failed)
}

#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_monitorAverageLatency(
    _env: JNIEnv,
    _class: JClass,
    total_ms: jlong,
    completed: jint,
) -> jlong {
    monitor::average_latency(total_ms, completed)
}

#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_monitorFailureCategory(
    mut env: JNIEnv,
    _class: JClass,
    message: JString,
    class_name: JString,
) -> jstring {
    let Some(message_value) = read_java_string(&mut env, &message) else { return std::ptr::null_mut(); };
    let Some(class_value) = read_java_string(&mut env, &class_name) else { return std::ptr::null_mut(); };
    match env.new_string(monitor::failure_category(&message_value, &class_value)) {
        Ok(value) => value.into_raw(), Err(_) => std::ptr::null_mut()
    }
}


#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_providerMetadata(
    mut env: JNIEnv,
    _class: JClass,
    name: JString,
) -> jstring {
    let Some(name_value) = read_java_string(&mut env, &name) else { return std::ptr::null_mut(); };
    let output = match providers::provider_metadata(&name_value) {
        Ok(value) => value,
        Err(error) => serde_json::json!({ "error": error }).to_string(),
    };
    match env.new_string(output) { Ok(value) => value.into_raw(), Err(_) => std::ptr::null_mut() }
}


#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_compareVersions(
    mut env: JNIEnv,
    _class: JClass,
    left: JString,
    right: JString,
) -> jint {
    let Some(left_value) = read_java_string(&mut env, &left) else { return 0; };
    let Some(right_value) = read_java_string(&mut env, &right) else { return 0; };
    updates::compare_versions(&left_value, &right_value)
}


#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_providerCredentialSource(
    mut env: JNIEnv,
    _class: JClass,
    provider: JString,
    oauth_available: jboolean,
) -> jstring {
    let Some(provider_value) = read_java_string(&mut env, &provider) else { return std::ptr::null_mut(); };
    match env.new_string(providers::credential_source(&provider_value, oauth_available != 0)) {
        Ok(value) => value.into_raw(), Err(_) => std::ptr::null_mut()
    }
}


#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_evaluateDailyUsage(
    mut env: JNIEnv,
    _class: JClass,
    today: JString,
    stored_day: JString,
    stored_count: JString,
    daily_limit: JString,
) -> jstring {
    let Some(today_value) = read_java_string(&mut env, &today) else { return std::ptr::null_mut(); };
    let Some(day_value) = read_java_string(&mut env, &stored_day) else { return std::ptr::null_mut(); };
    let Some(count_value) = read_java_string(&mut env, &stored_count) else { return std::ptr::null_mut(); };
    let Some(limit_value) = read_java_string(&mut env, &daily_limit) else { return std::ptr::null_mut(); };
    match env.new_string(usage::evaluate_daily_usage(&today_value, &day_value, &count_value, &limit_value)) {
        Ok(value) => value.into_raw(), Err(_) => std::ptr::null_mut()
    }
}


#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_maxAgentSteps(
    _env: JNIEnv,
    _class: JClass,
) -> jint {
    agent::MAX_AGENT_STEPS
}

#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_buildToolContinuationPrompt(
    mut env: JNIEnv,
    _class: JClass,
    tool_name: JString,
    output: JString,
    scheduled: jboolean,
) -> jstring {
    let Some(name_value) = read_java_string(&mut env, &tool_name) else { return std::ptr::null_mut(); };
    let Some(output_value) = read_java_string(&mut env, &output) else { return std::ptr::null_mut(); };
    match env.new_string(agent::tool_continuation_prompt(&name_value, &output_value, scheduled != 0)) {
        Ok(value) => value.into_raw(), Err(_) => std::ptr::null_mut()
    }
}

#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_agentStepLimitMessage(
    mut env: JNIEnv,
    _class: JClass,
    last_output: JString,
    scheduled: jboolean,
) -> jstring {
    let Some(output_value) = read_java_string(&mut env, &last_output) else { return std::ptr::null_mut(); };
    match env.new_string(agent::step_limit_message(&output_value, scheduled != 0)) {
        Ok(value) => value.into_raw(), Err(_) => std::ptr::null_mut()
    }
}


#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_googleOAuthAccessTokenUsable(
    mut env: JNIEnv,
    _class: JClass,
    access_token: JString,
    expires_at: JString,
    now_ms: jlong,
) -> jboolean {
    let Some(access_value) = read_java_string(&mut env, &access_token) else { return 0; };
    let Some(expiry_value) = read_java_string(&mut env, &expires_at) else { return 0; };
    if auth::google_oauth_access_token_usable(&access_value, &expiry_value, now_ms.max(0) as u64) { 1 } else { 0 }
}


#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_conversationTitle(
    mut env: JNIEnv,
    _class: JClass,
    messages_json: JString,
) -> jstring {
    let Some(raw) = read_java_string(&mut env, &messages_json) else { return std::ptr::null_mut(); };
    let title = storage::conversation_title(&raw).unwrap_or_else(|_| "New chat".to_owned());
    match env.new_string(title) { Ok(value) => value.into_raw(), Err(_) => std::ptr::null_mut() }
}
