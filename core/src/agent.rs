//! Deterministic, bounded local agent primitives.

/// Evaluates a deliberately small arithmetic language without eval, reflection,
/// shell access, or arbitrary code execution.
pub fn evaluate_arithmetic(source: &str) -> Result<f64, String> {
    if !(1..=200).contains(&source.len()) {
        return Err("Expression length must be 1–200 characters.".to_owned());
    }
    let mut parser = Parser { source, index: 0 };
    let result = parser.expression()?;
    parser.skip_spaces();
    if !parser.done() {
        return Err("Unexpected input.".to_owned());
    }
    if !result.is_finite() {
        return Err("Invalid result.".to_owned());
    }
    Ok(result)
}

struct Parser<'a> {
    source: &'a str,
    index: usize,
}

impl Parser<'_> {
    fn done(&self) -> bool {
        self.index >= self.source.len()
    }

    fn peek(&self) -> Option<char> {
        self.source[self.index..].chars().next()
    }

    fn take(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.index += expected.len_utf8();
            true
        } else {
            false
        }
    }

    fn skip_spaces(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.index += self.peek().unwrap().len_utf8();
        }
    }

    fn expression(&mut self) -> Result<f64, String> {
        let mut value = self.term()?;
        loop {
            self.skip_spaces();
            if self.take('+') {
                value += self.term()?;
            } else if self.take('-') {
                value -= self.term()?;
            } else {
                return Ok(value);
            }
        }
    }

    fn term(&mut self) -> Result<f64, String> {
        let mut value = self.factor()?;
        loop {
            self.skip_spaces();
            if self.take('*') {
                value *= self.factor()?;
            } else if self.take('/') {
                let divisor = self.factor()?;
                if divisor == 0.0 {
                    return Err("Division by zero.".to_owned());
                }
                value /= divisor;
            } else if self.take('%') {
                let divisor = self.factor()?;
                if divisor == 0.0 {
                    return Err("Division by zero.".to_owned());
                }
                value %= divisor;
            } else {
                return Ok(value);
            }
        }
    }

    fn factor(&mut self) -> Result<f64, String> {
        self.skip_spaces();
        if self.take('+') {
            return self.factor();
        }
        if self.take('-') {
            return Ok(-self.factor()?);
        }
        if self.take('(') {
            let value = self.expression()?;
            self.skip_spaces();
            if !self.take(')') {
                return Err("Missing closing parenthesis.".to_owned());
            }
            return Ok(value);
        }

        let start = self.index;
        let mut saw_digit = false;
        let mut saw_decimal = false;
        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() {
                saw_digit = true;
                self.index += 1;
            } else if ch == '.' && !saw_decimal {
                saw_decimal = true;
                self.index += 1;
            } else {
                break;
            }
        }
        if !saw_digit {
            return Err("Expected a number.".to_owned());
        }
        self.source[start..self.index]
            .parse::<f64>()
            .map_err(|_| "Invalid number.".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::evaluate_arithmetic;

    #[test]
    fn respects_precedence_and_parentheses() {
        assert_eq!(evaluate_arithmetic("2 + 3 * 4").unwrap(), 14.0);
        assert_eq!(evaluate_arithmetic("(2 + 3) * 4").unwrap(), 20.0);
        assert_eq!(evaluate_arithmetic("-2.5 * 4").unwrap(), -10.0);
    }

    #[test]
    fn supports_division_and_remainder() {
        assert_eq!(evaluate_arithmetic("9 / 2").unwrap(), 4.5);
        assert_eq!(evaluate_arithmetic("9 % 4").unwrap(), 1.0);
    }

    #[test]
    fn rejects_invalid_or_unsafe_expressions() {
        assert!(evaluate_arithmetic("1 / 0").is_err());
        assert!(evaluate_arithmetic("1 +").is_err());
        assert!(evaluate_arithmetic("1; process").is_err());
        assert!(evaluate_arithmetic(&"1".repeat(201)).is_err());
        assert!(evaluate_arithmetic("1e9").is_err());
    }
}


/// The Rust policy registry is authoritative for tool allowlisting and approval gates.
pub fn tool_policy(name: &str, enabled_tools_json: &str) -> Result<String, String> {
    use serde_json::{json, Value};

    let risk = match name {
        "current_time" | "calculate" | "count_words" | "format_json" | "current_date"
        | "generate_uuid" | "text_stats" | "base64_encode" | "base64_decode" => "safe",
        "open_url" | "open_dialer" | "share_text" | "open_app_settings" => "sensitive",
        _ => return Ok(json!({
            "allowed": false,
            "risk": "unknown",
            "requires_approval": false
        }).to_string()),
    };

    let enabled: Value = serde_json::from_str(enabled_tools_json)
        .map_err(|_| "Enabled tool list must be valid JSON.".to_owned())?;
    let allowed = if enabled.is_null() {
        true
    } else {
        let list = enabled.as_array()
            .ok_or_else(|| "Enabled tool list must be a JSON array or null.".to_owned())?;
        list.iter().any(|item| item.as_str() == Some(name))
    };
    Ok(json!({
        "allowed": allowed,
        "risk": risk,
        "requires_approval": allowed && risk != "safe"
    }).to_string())
}

#[cfg(test)]
mod policy_tests {
    use super::tool_policy;

    #[test]
    fn tool_policy_enforces_allowlists_and_approval_gates() {
        let safe: serde_json::Value = serde_json::from_str(
            &tool_policy("calculate", "null").unwrap()
        ).unwrap();
        assert_eq!(safe["allowed"], true);
        assert_eq!(safe["requires_approval"], false);

        let sensitive: serde_json::Value = serde_json::from_str(
            &tool_policy("open_dialer", "null").unwrap()
        ).unwrap();
        assert_eq!(sensitive["risk"], "sensitive");
        assert_eq!(sensitive["requires_approval"], true);

        let disabled: serde_json::Value = serde_json::from_str(
            &tool_policy("share_text", r#"["calculate"]"#).unwrap()
        ).unwrap();
        assert_eq!(disabled["allowed"], false);
        let unknown: serde_json::Value = serde_json::from_str(
            &tool_policy("run_shell", "null").unwrap()
        ).unwrap();
        assert_eq!(unknown["allowed"], false);
    }
}


/// Extracts the first bounded structured tool call from an assistant response.
pub fn parse_tool_call(response: &str) -> Result<String, String> {
    const OPEN: &str = "[[CENAI_TOOL_CALL]]";
    const CLOSE: &str = "[[/CENAI_TOOL_CALL]]";
    let Some(start) = response.find(OPEN) else { return Ok(String::new()); };
    let payload_start = start + OPEN.len();
    let Some(relative_end) = response[payload_start..].find(CLOSE) else {
        return Ok(String::new());
    };
    let payload_text = response[payload_start..payload_start + relative_end].trim();
    if payload_text.len() > 16_384 {
        return Err("Tool call payload exceeds the 16 KB safety limit.".to_owned());
    }
    let parsed: serde_json::Value = serde_json::from_str(payload_text)
        .map_err(|_| "Tool call payload is not valid JSON.".to_owned())?;
    let name = parsed.get("name").and_then(serde_json::Value::as_str)
        .ok_or_else(|| "Tool call is missing a tool name.".to_owned())?;
    let arguments = parsed.get("arguments").cloned()
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}));
    Ok(serde_json::json!({"name":name,"arguments":arguments}).to_string())
}

#[cfg(test)]
mod tool_call_tests {
    use super::parse_tool_call;

    #[test]
    fn parses_only_complete_bounded_tool_call_markers() {
        let parsed = parse_tool_call(
            r#"Plan [[CENAI_TOOL_CALL]]{"name":"calculate","arguments":{"expression":"1+2"}}[[/CENAI_TOOL_CALL]] done"#
        ).unwrap();
        let value: serde_json::Value = serde_json::from_str(&parsed).unwrap();
        assert_eq!(value["name"], "calculate");
        assert_eq!(value["arguments"]["expression"], "1+2");
        assert_eq!(parse_tool_call("no tool call").unwrap(), "");
        assert_eq!(parse_tool_call("[[CENAI_TOOL_CALL]]{").unwrap(), "");
    }
}


/// Executes deterministic safe tools in Rust. Android supplies only the local clock values.
pub fn execute_safe_tool(
    name: &str,
    arguments_json: &str,
    local_time: &str,
    local_date: &str,
) -> Result<String, String> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use serde_json::{json, Value};
    use uuid::Uuid;

    let args: Value = serde_json::from_str(arguments_json)
        .map_err(|_| "Tool arguments must be valid JSON.".to_owned())?;
    if !args.is_object() {
        return Err("Tool arguments must be a JSON object.".to_owned());
    }
    match name {
        "current_time" => Ok(local_time.to_owned()),
        "current_date" => Ok(local_date.to_owned()),
        "calculate" => {
            let expression = required_text(&args, "expression")?;
            Ok(evaluate_arithmetic(expression)?.to_string())
        }
        "count_words" => {
            let value = required_text(&args, "text")?;
            Ok(json!({
                "words": value.split_whitespace().count(),
                "characters": value.chars().count(),
                "characters_without_spaces": value.chars().filter(|ch| !ch.is_whitespace()).count()
            }).to_string())
        }
        "format_json" => {
            let value: Value = serde_json::from_str(required_text(&args, "json")?)
                .map_err(|_| "The supplied text is not valid JSON.".to_owned())?;
            serde_json::to_string_pretty(&value)
                .map_err(|_| "Could not format the supplied JSON.".to_owned())
        }
        "generate_uuid" => Ok(Uuid::new_v4().to_string()),
        "text_stats" => {
            let value = required_text(&args, "text")?;
            let lines = if value.is_empty() { 0 } else {
                value.split('\n').count()
            };
            Ok(json!({
                "lines": lines,
                "words": value.split_whitespace().count(),
                "characters": value.chars().count()
            }).to_string())
        }
        "base64_encode" => Ok(STANDARD.encode(required_text(&args, "text")?.as_bytes())),
        "base64_decode" => {
            let decoded = STANDARD.decode(required_text(&args, "value")?)
                .map_err(|_| "The supplied value is not valid Base64.".to_owned())?;
            String::from_utf8(decoded)
                .map_err(|_| "Decoded Base64 is not valid UTF-8 text.".to_owned())
        }
        _ => Err("Tool is not an approved Rust safe local tool.".to_owned()),
    }
}

#[cfg(test)]
mod safe_tool_tests {
    use super::execute_safe_tool;

    #[test]
    fn executes_safe_text_and_formatting_tools() {
        let counts = execute_safe_tool("count_words", r#"{"text":"one two"}"#, "", "").unwrap();
        let counts: serde_json::Value = serde_json::from_str(&counts).unwrap();
        assert_eq!(counts["words"], 2);
        assert_eq!(counts["characters"], 7);

        let formatted = execute_safe_tool("format_json", r#"{"json":"{\"ok\":true}"}"#, "", "").unwrap();
        assert!(formatted.contains("\n  \"ok\": true"));
        assert_eq!(execute_safe_tool("text_stats", r#"{"text":"a\nb"}"#, "", "").unwrap(),
            r#"{"characters":3,"lines":2,"words":2}"#);
    }

    #[test]
    fn executes_safe_clock_uuid_and_base64_tools() {
        assert_eq!(execute_safe_tool("current_time", "{}", "local-clock", "local-day").unwrap(), "local-clock");
        assert_eq!(execute_safe_tool("current_date", "{}", "local-clock", "local-day").unwrap(), "local-day");
        let id = execute_safe_tool("generate_uuid", "{}", "", "").unwrap();
        assert_eq!(id.len(), 36);
        assert_eq!(execute_safe_tool("base64_encode", r#"{"text":"CenAI"}"#, "", "").unwrap(), "Q2VuQUk=");
        assert_eq!(execute_safe_tool("base64_decode", r#"{"value":"Q2VuQUk="}"#, "", "").unwrap(), "CenAI");
    }

    #[test]
    fn rejects_unsafe_or_invalid_tool_arguments() {
        assert!(execute_safe_tool("run_shell", "{}", "", "").is_err());
        assert!(execute_safe_tool("base64_decode", r#"{"value":"%%%"}"#, "", "").is_err());
        assert!(execute_safe_tool("format_json", r#"{"json":"not json"}"#, "", "").is_err());
    }
}


fn required_text<'a>(args: &'a serde_json::Value, key: &str) -> Result<&'a str, String> {
    args.get(key).and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("Missing required text argument: {key}"))
}


/// Returns the Rust-owned tool catalog and JSON schemas for the requested allowlist.
pub fn tool_catalog(enabled_tools_json: &str) -> Result<String, String> {
    use serde_json::{json, Value};

    let enabled: Value = serde_json::from_str(enabled_tools_json)
        .map_err(|_| "Enabled tool list must be valid JSON.".to_owned())?;
    let allowed = |name: &str| -> bool {
        enabled.is_null() || enabled.as_array()
            .is_some_and(|items| items.iter().any(|item| item.as_str() == Some(name)))
    };
    let tools = vec![
        json!({"name":"current_time","description":"Get the current local date and time.","risk":"SAFE","parameters":{"type":"object","properties":{}}}),
        json!({"name":"calculate","description":"Evaluate a basic arithmetic expression.","risk":"SAFE","parameters":{"type":"object","properties":{"expression":{"type":"string"}},"required":["expression"]}}),
        json!({"name":"count_words","description":"Count words and characters in supplied text.","risk":"SAFE","parameters":{"type":"object","properties":{"text":{"type":"string"}},"required":["text"]}}),
        json!({"name":"format_json","description":"Validate and pretty-print JSON supplied by the user.","risk":"SAFE","parameters":{"type":"object","properties":{"json":{"type":"string"}},"required":["json"]}}),
        json!({"name":"current_date","description":"Get today's local date.","risk":"SAFE","parameters":{"type":"object","properties":{}}}),
        json!({"name":"generate_uuid","description":"Generate a random UUID.","risk":"SAFE","parameters":{"type":"object","properties":{}}}),
        json!({"name":"text_stats","description":"Return line, word and character statistics for text.","risk":"SAFE","parameters":{"type":"object","properties":{"text":{"type":"string"}},"required":["text"]}}),
        json!({"name":"base64_encode","description":"Encode supplied text as Base64.","risk":"SAFE","parameters":{"type":"object","properties":{"text":{"type":"string"}},"required":["text"]}}),
        json!({"name":"base64_decode","description":"Decode a Base64 string as UTF-8 text.","risk":"SAFE","parameters":{"type":"object","properties":{"value":{"type":"string"}},"required":["value"]}}),
        json!({"name":"open_url","description":"Open an HTTP or HTTPS link in Android after the user approves it.","risk":"SENSITIVE","parameters":{"type":"object","properties":{"url":{"type":"string"}},"required":["url"]}}),
        json!({"name":"open_dialer","description":"Open the Android dialer with a phone number filled in; does not place a call.","risk":"SENSITIVE","parameters":{"type":"object","properties":{"number":{"type":"string"}},"required":["number"]}}),
        json!({"name":"share_text","description":"Open Android's share sheet with text for the user to choose where to send.","risk":"SENSITIVE","parameters":{"type":"object","properties":{"text":{"type":"string"}},"required":["text"]}}),
        json!({"name":"open_app_settings","description":"Open CenAI's Android app settings.","risk":"SENSITIVE","parameters":{"type":"object","properties":{}}}),
    ];
    let filtered: Vec<Value> = tools.into_iter()
        .filter(|tool| allowed(tool["name"].as_str().unwrap_or("")))
        .collect();
    serde_json::to_string(&filtered)
        .map_err(|_| "Could not encode the Rust tool catalog.".to_owned())
}

#[cfg(test)]
mod catalog_tests {
    use super::tool_catalog;

    #[test]
    fn exposes_only_enabled_rust_tool_descriptors() {
        let all: serde_json::Value = serde_json::from_str(&tool_catalog("null").unwrap()).unwrap();
        assert_eq!(all.as_array().unwrap().len(), 13);
        let selected: serde_json::Value = serde_json::from_str(
            &tool_catalog(r#"["calculate","open_dialer"]"#).unwrap()
        ).unwrap();
        assert_eq!(selected.as_array().unwrap().len(), 2);
        assert_eq!(selected[0]["name"], "calculate");
        assert_eq!(selected[1]["risk"], "SENSITIVE");
    }
}


/// Builds the constrained tool-use instructions from the same Rust-owned catalog.
pub fn build_tool_guide(
    enabled_tools_json: &str,
    planning_enabled: bool,
    allow_device_actions: bool,
) -> Result<String, String> {
    if !planning_enabled {
        return Ok(String::new());
    }
    let catalog: serde_json::Value = serde_json::from_str(&tool_catalog(enabled_tools_json)?)
        .map_err(|_| "Rust tool catalog could not be decoded.".to_owned())?;
    let tools = catalog.as_array().ok_or_else(|| "Rust tool catalog is invalid.".to_owned())?;
    if tools.is_empty() {
        return Ok("No tools are enabled. Answer using conversation context only.".to_owned());
    }
    let descriptions = tools.iter().map(|tool| {
        format!(
            "{} — {}",
            tool.get("name").and_then(serde_json::Value::as_str).unwrap_or("unknown"),
            tool.get("description").and_then(serde_json::Value::as_str).unwrap_or("")
        )
    }).collect::<Vec<_>>().join(", ");
    let mut guide = format!(
        "You may use these allow-listed tools when useful: {descriptions}. To call one, output exactly [[CENAI_TOOL_CALL]] followed by a JSON object with name and arguments, then [[/CENAI_TOOL_CALL]]. Only call listed tools."
    );
    if allow_device_actions {
        guide.push_str(" Device actions are sensitive and require a separate user approval tap in CenAI before Android opens the requested screen. Never claim a device action ran before approval. Do not claim unsupported shell, arbitrary code or file-write actions were performed.");
    } else {
        guide.push_str(" This background task cannot execute Android device actions, send messages, access arbitrary files or run shell commands. Never claim external actions you cannot perform.");
    }
    guide.push_str(" Explain tool results to the user.");
    Ok(guide)
}

#[cfg(test)]
mod guide_tests {
    use super::build_tool_guide;

    #[test]
    fn builds_separate_foreground_and_background_tool_guides() {
        let foreground = build_tool_guide("null", true, true).unwrap();
        assert!(foreground.contains("approval tap"));
        let background = build_tool_guide(r#"["calculate"]"#, true, false).unwrap();
        assert!(background.contains("cannot execute Android device actions"));
        assert!(!background.contains("open_dialer"));
        assert_eq!(build_tool_guide("null", false, true).unwrap(), "");
    }
}
