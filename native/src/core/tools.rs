//! Pure Rust allow-listed agent tools. Side-effecting Android actions are descriptors only;
//! they must be dispatched through the Android permission/approval bridge.
use std::collections::HashSet;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::Local;
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::core::permissions::{approval_policy, ApprovalDecision, OperationClass};

const TOOL_CALL_OPEN: &str = "[[CENAI_TOOL_CALL]]";
const TOOL_CALL_CLOSE: &str = "[[/CENAI_TOOL_CALL]]";
const MAX_TOOL_RESPONSE_BYTES: usize = 32 * 1024;
const MAX_TOOL_TEXT_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolRisk { Safe, Sensitive, Destructive }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub risk: ToolRisk,
    pub parameters: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolExecution {
    pub tool_name: String,
    pub arguments: Value,
    pub output: String,
    pub requires_approval: bool,
    pub completed: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToolCall {
    pub name: String,
    pub arguments: Value,
}

pub struct AgentToolRuntime {
    enabled_tools: Option<HashSet<String>>,
}

impl Default for AgentToolRuntime {
    fn default() -> Self { Self::new(None) }
}

impl AgentToolRuntime {
    pub fn new(enabled_tools: Option<HashSet<String>>) -> Self { Self { enabled_tools } }

    pub fn available_tools(&self) -> Vec<ToolDefinition> {
        definitions().into_iter()
            .filter(|tool| self.enabled_tools.as_ref().is_none_or(|enabled| enabled.contains(&tool.name)))
            .collect()
    }

    pub fn parse_tool_call(&self, response: &str) -> Result<Option<ToolCall>, &'static str> {
        if response.len() > MAX_TOOL_RESPONSE_BYTES { return Err("Tool response exceeds the 32 KiB limit"); }
        let response = response.trim();
        if !response.contains(TOOL_CALL_OPEN) {
            return if response.contains(TOOL_CALL_CLOSE) {
                Err("Tool call contains a closing marker without an opening marker")
            } else {
                Ok(None)
            };
        }
        if !response.starts_with(TOOL_CALL_OPEN) || !response.ends_with(TOOL_CALL_CLOSE)
            || response.matches(TOOL_CALL_OPEN).count() != 1
            || response.matches(TOOL_CALL_CLOSE).count() != 1 {
            return Err("A tool call must contain exactly one complete marker pair and no surrounding text");
        }
        let body_start = TOOL_CALL_OPEN.len();
        let raw = response[body_start..response.len() - TOOL_CALL_CLOSE.len()].trim();
        let payload: Value = serde_json::from_str(raw).map_err(|_| "Tool call JSON is invalid")?;
        let object = payload.as_object().ok_or("Tool call must be a JSON object")?;
        if object.len() != 2 || !object.contains_key("name") || !object.contains_key("arguments") {
            return Err("Tool call must contain exactly the name and arguments fields");
        }
        let name = object.get("name").and_then(Value::as_str).ok_or("Tool name is required")?;
        if name.is_empty() || name.len() > 80 { return Err("Tool name is invalid"); }
        let arguments = object.get("arguments").cloned().ok_or("Tool arguments are required")?;
        if !arguments.is_object() { return Err("Tool arguments must be a JSON object"); }
        Ok(Some(ToolCall { name: name.to_owned(), arguments }))
    }

    pub fn execute(&self, name: &str, arguments: Value, approval_granted: bool) -> ToolExecution {
        let definition = self.available_tools().into_iter().find(|tool| tool.name == name);
        let Some(definition) = definition else {
            return execution(name, arguments, "Tool is disabled or not allow-listed.", false, false);
        };
        if let Err(error) = validate_arguments(name, &arguments) {
            return execution(name, arguments, format!("Invalid tool arguments: {error}"), false, false);
        }
        let class = match definition.risk {
            ToolRisk::Safe => OperationClass::ReadOnly,
            ToolRisk::Sensitive => OperationClass::Sensitive,
            ToolRisk::Destructive => OperationClass::Destructive,
        };
        // A safe tool can run only because it is explicitly present in this
        // app-owned allow-list; sensitive/destructive tools still require a
        // separate per-action user approval.
        let explicitly_granted = definition.risk == ToolRisk::Safe || approval_granted;
        if approval_policy(class, explicitly_granted) != ApprovalDecision::Allow {
            return execution(name, arguments, "User approval is required.", true, false);
        }
        let result = match name {
            "current_time" => Ok(Local::now().to_rfc3339()),
            "current_date" => Ok(Local::now().date_naive().format("%Y-%m-%d").to_string()),
            "calculate" => required_string(&arguments, "expression").and_then(|value| SafeArithmetic::evaluate(value).map(|number| number.to_string())),
            "count_words" => required_string(&arguments, "text").map(word_stats),
            "text_stats" => required_string(&arguments, "text").map(text_stats),
            "format_json" => required_string(&arguments, "json").and_then(|value| {
                serde_json::from_str::<Value>(value)
                    .map_err(|_| "Supplied text is not valid JSON".to_owned())
                    .and_then(|parsed| serde_json::to_string_pretty(&parsed).map_err(|_| "JSON formatting failed".to_owned()))
            }),
            "generate_uuid" => Ok(random_uuid()),
            "base64_encode" => required_string(&arguments, "text").map(|value| STANDARD.encode(value.as_bytes())),
            "base64_decode" => required_string(&arguments, "value").and_then(|value| {
                STANDARD.decode(value).map_err(|_| "Base64 input is invalid".to_owned())
                    .and_then(|bytes| String::from_utf8(bytes).map_err(|_| "Base64 payload is not valid UTF-8".to_owned()))
            }),
            // These require an Android activity and an explicit approval UI; never run in this pure Rust executor.
            "open_url" | "open_dialer" | "share_text" | "open_app_settings" => {
                return execution(name, arguments, "Approved Android action must be dispatched by the foreground platform bridge.", true, false);
            }
            _ => Err("No executor is registered for this tool".to_owned()),
        };
        match result {
            Ok(output) => execution(name, arguments, output, false, true),
            Err(error) => execution(name, arguments, format!("Tool error: {error}"), false, false),
        }
    }
}

fn validate_arguments(name: &str, arguments: &Value) -> Result<(), String> {
    let object = arguments.as_object().ok_or("Tool arguments must be a JSON object")?;
    let expected: &[(&str, bool)] = match name {
        "current_time" | "current_date" | "generate_uuid" | "open_app_settings" => &[],
        "calculate" => &[("expression", true)],
        "count_words" | "text_stats" | "base64_encode" | "share_text" => &[("text", true)],
        "format_json" => &[("json", true)],
        "base64_decode" => &[("value", true)],
        "open_url" => &[("url", true)],
        "open_dialer" => &[("number", true)],
        _ => return Err("No argument contract is registered for this tool".into()),
    };
    let expected_names: HashSet<&str> = expected.iter().map(|(key, _)| *key).collect();
    if object.len() != expected.len()
        || expected.iter().any(|(key, required)| *required && !object.contains_key(*key))
        || object.keys().any(|actual| !expected_names.contains(actual.as_str())) {
        return Err("Tool arguments do not match the exact allow-listed schema".into());
    }
    for (key, _) in expected {
        if !object.get(*key).is_some_and(Value::is_string) {
            return Err(format!("Tool argument '{key}' must be a string"));
        }
        if object.get(*key).and_then(Value::as_str).is_some_and(|value| value.len() > MAX_TOOL_TEXT_BYTES) {
            return Err("Tool text argument exceeds 64 KiB".into());
        }
    }
    Ok(())
}

fn definitions() -> Vec<ToolDefinition> {
    let no_args = json!({"type":"object","properties":{},"additionalProperties":false});
    let string_arg = |name: &str| json!({
        "type":"object",
        "properties":{(name):{"type":"string","maxLength":MAX_TOOL_TEXT_BYTES}},
        "required":[name],
        "additionalProperties":false
    });
    [
        ("current_time", "Get the current local date and time.", ToolRisk::Safe, no_args.clone()),
        ("current_date", "Get today's local date.", ToolRisk::Safe, no_args.clone()),
        ("calculate", "Evaluate a basic arithmetic expression.", ToolRisk::Safe, string_arg("expression")),
        ("count_words", "Count words and characters in supplied text.", ToolRisk::Safe, string_arg("text")),
        ("format_json", "Validate and pretty-print JSON supplied by the user.", ToolRisk::Safe, string_arg("json")),
        ("generate_uuid", "Generate a random UUID.", ToolRisk::Safe, no_args.clone()),
        ("text_stats", "Return line, word and character statistics for text.", ToolRisk::Safe, string_arg("text")),
        ("base64_encode", "Encode supplied text as Base64.", ToolRisk::Safe, string_arg("text")),
        ("base64_decode", "Decode Base64 as UTF-8 text.", ToolRisk::Safe, string_arg("value")),
        ("open_url", "Open an HTTP or HTTPS link after user approval.", ToolRisk::Sensitive, string_arg("url")),
        ("open_dialer", "Open Android dialer with a number; does not place a call.", ToolRisk::Sensitive, string_arg("number")),
        ("share_text", "Open Android share sheet after user approval.", ToolRisk::Sensitive, string_arg("text")),
        ("open_app_settings", "Open CenAI Android app settings.", ToolRisk::Sensitive, no_args),
    ].into_iter().map(|(name, description, risk, parameters)| ToolDefinition {
        name: name.into(), description: description.into(), risk, parameters,
    }).collect()
}

fn execution(name: &str, arguments: Value, output: impl Into<String>, requires_approval: bool, completed: bool) -> ToolExecution {
    ToolExecution { tool_name: name.into(), arguments, output: output.into(), requires_approval, completed }
}

fn required_string<'a>(arguments: &'a Value, key: &str) -> Result<&'a str, String> {
    let value = arguments.get(key).and_then(Value::as_str).ok_or_else(|| format!("Argument '{key}' must be a string"))?;
    if value.len() > MAX_TOOL_TEXT_BYTES { return Err("Tool text argument exceeds 64 KiB".into()); }
    Ok(value)
}

fn word_stats(value: &str) -> String {
    let words = value.split_whitespace().count();
    let characters = value.chars().count();
    let without_spaces = value.chars().filter(|character| !character.is_whitespace()).count();
    json!({"words":words,"characters":characters,"characters_without_spaces":without_spaces}).to_string()
}

fn text_stats(value: &str) -> String {
    let lines = if value.is_empty() { 0 } else { value.lines().count() };
    json!({"lines":lines,"words":value.split_whitespace().count(),"characters":value.chars().count()}).to_string()
}

fn random_uuid() -> String {
    let mut bytes = [0u8; 16];
    OsRng.fill_bytes(&mut bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],bytes[1],bytes[2],bytes[3],bytes[4],bytes[5],bytes[6],bytes[7],
        bytes[8],bytes[9],bytes[10],bytes[11],bytes[12],bytes[13],bytes[14],bytes[15]
    )
}

struct SafeArithmetic<'a> { source: &'a [u8], cursor: usize }

impl<'a> SafeArithmetic<'a> {
    fn evaluate(source: &'a str) -> Result<f64, String> {
        if source.is_empty() || source.len() > 200 { return Err("Expression length must be 1–200 characters.".into()); }
        let mut parser = Self { source: source.as_bytes(), cursor: 0 };
        let result = parser.expression()?;
        parser.spaces();
        if parser.cursor != parser.source.len() { return Err("Unexpected input.".into()); }
        if !result.is_finite() { return Err("Invalid result.".into()); }
        Ok(result)
    }
    fn spaces(&mut self) { while self.source.get(self.cursor).is_some_and(u8::is_ascii_whitespace) { self.cursor += 1; } }
    fn take(&mut self, byte: u8) -> bool {
        if self.source.get(self.cursor) == Some(&byte) { self.cursor += 1; true } else { false }
    }
    fn expression(&mut self) -> Result<f64, String> {
        let mut value = self.term()?;
        loop {
            self.spaces();
            if self.take(b'+') { value += self.term()?; }
            else if self.take(b'-') { value -= self.term()?; }
            else { return Ok(value); }
        }
    }
    fn term(&mut self) -> Result<f64, String> {
        let mut value = self.factor()?;
        loop {
            self.spaces();
            if self.take(b'*') { value *= self.factor()?; }
            else if self.take(b'/') || self.take(b'%') {
                let operator = self.source[self.cursor - 1];
                let divisor = self.factor()?;
                if divisor == 0.0 { return Err("Division by zero.".into()); }
                if operator == b'/' { value /= divisor; } else { value %= divisor; }
            } else { return Ok(value); }
        }
    }
    fn factor(&mut self) -> Result<f64, String> {
        self.spaces();
        if self.take(b'+') { return self.factor(); }
        if self.take(b'-') { return Ok(-self.factor()?); }
        if self.take(b'(') {
            let value = self.expression()?;
            self.spaces();
            if !self.take(b')') { return Err("Missing closing parenthesis.".into()); }
            return Ok(value);
        }
        let start = self.cursor;
        while self.source.get(self.cursor).is_some_and(|byte| byte.is_ascii_digit() || *byte == b'.') {
            self.cursor += 1;
        }
        if self.cursor == start { return Err("Expected a number.".into()); }
        let raw = std::str::from_utf8(&self.source[start..self.cursor]).map_err(|_| "Invalid number.".to_owned())?;
        raw.parse::<f64>().map_err(|_| "Invalid number.".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allow_list_and_safe_tools_work() {
        let runtime = AgentToolRuntime::default();
        assert_eq!(runtime.execute("calculate", json!({"expression":"(2 + 3) * 4 / 2"}), false).output, "10");
        assert!(runtime.execute("generate_uuid", json!({}), false).output.contains('-'));
        assert_eq!(runtime.execute("base64_encode", json!({"text":"CenAI"}), false).output, "Q2VuQUk=");
        assert_eq!(runtime.execute("base64_decode", json!({"value":"Q2VuQUk="}), false).output, "CenAI");
    }

    #[test]
    fn parser_rejects_unmatched_close_and_extra_top_level_fields() {
        let runtime = AgentToolRuntime::default();
        assert!(runtime.parse_tool_call("orphan [[/CENAI_TOOL_CALL]]").is_err());
        assert!(runtime.parse_tool_call(r#"[[CENAI_TOOL_CALL]]{"name":"current_time","arguments":{},"extra":true}[[/CENAI_TOOL_CALL]]"#).is_err());
        assert!(runtime.parse_tool_call(r#"[[CENAI_TOOL_CALL]]{"name":"current_time"}[[/CENAI_TOOL_CALL]]"#).is_err());
    }

    #[test]
    fn malformed_math_and_json_are_rejected() {
        let runtime = AgentToolRuntime::default();
        assert!(!runtime.execute("calculate", json!({"expression":"1 / 0"}), false).completed);
        assert!(!runtime.execute("format_json", json!({"json":"{bad"}), false).completed);
        assert!(!runtime.execute("calculate", json!({"expression":"1 + 2 garbage"}), false).completed);
    }

    #[test]
    fn tools_reject_extra_and_missing_arguments() {
        let runtime = AgentToolRuntime::default();
        assert!(!runtime.execute("calculate", json!({"expression":"1+1","unexpected":"ignored"}), false).completed);
        assert!(!runtime.execute("current_time", json!({"unexpected":true}), false).completed);
        assert!(!runtime.execute("base64_encode", json!({}), false).completed);
    }

    #[test]
    fn sensitive_actions_never_execute_without_approval() {
        let runtime = AgentToolRuntime::default();
        let result = runtime.execute("open_url", json!({"url":"https://example.com"}), false);
        assert!(result.requires_approval);
        assert!(!result.completed);
    }

    #[test]
    fn disabled_tools_cannot_be_called() {
        let runtime = AgentToolRuntime::new(Some(["current_date".to_owned()].into_iter().collect()));
        assert!(runtime.execute("calculate", json!({"expression":"1+1"}), false).output.contains("not allow-listed"));
    }

    #[test]
    fn tool_call_parser_requires_valid_object_arguments() {
        let runtime = AgentToolRuntime::default();
        let call = runtime.parse_tool_call(r#"[[CENAI_TOOL_CALL]]{"name":"calculate","arguments":{"expression":"2+2"}}[[/CENAI_TOOL_CALL]]"#).unwrap().unwrap();
        assert_eq!(call.name, "calculate");
        assert_eq!(call.arguments["expression"], "2+2");
        assert!(runtime.parse_tool_call(r#"prefix [[CENAI_TOOL_CALL]]{"name":"calculate","arguments":{"expression":"2+2"}}[[/CENAI_TOOL_CALL]]"#).is_err());
        assert!(runtime.parse_tool_call("[[CENAI_TOOL_CALL]][][[/CENAI_TOOL_CALL]]").is_err());
    }
}
