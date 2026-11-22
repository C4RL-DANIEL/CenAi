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
