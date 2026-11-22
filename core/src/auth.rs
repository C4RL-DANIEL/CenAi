//! Authentication payload validation and Supabase session decoding.

use serde_json::{json, Value};

pub fn build_auth_payload(
    action: &str,
    email: &str,
    password: &str,
    refresh_token: &str,
) -> Result<String, String> {
    match action {
        "signup" => {
            let email = email.trim();
            if !email.contains('@') || email.chars().any(char::is_whitespace) || password.chars().count() < 8 {
                return Err("Enter a valid email and a password of at least 8 characters.".to_owned());
            }
            Ok(json!({"email":email,"password":password}).to_string())
        }
        "signin" => {
            let email = email.trim();
            if !email.contains('@') || email.chars().any(char::is_whitespace) || password.is_empty() {
                return Err("Enter your email and password.".to_owned());
            }
            Ok(json!({"email":email,"password":password}).to_string())
        }
        "refresh" => {
            if refresh_token.trim().is_empty() {
                return Err("A refresh token is required.".to_owned());
            }
            Ok(json!({"refresh_token":refresh_token}).to_string())
        }
        _ => Err("Unsupported authentication operation.".to_owned()),
    }
}

pub fn parse_auth_session(raw_json: &str) -> Result<String, String> {
    let value: Value = serde_json::from_str(raw_json)
        .map_err(|_| "Authentication service returned invalid JSON.".to_owned())?;
    let access_token = value.get("access_token").and_then(Value::as_str).unwrap_or("");
    if access_token.is_empty() {
        return Err("Authentication service did not return an access token.".to_owned());
    }
    let user = value.get("user");
    let user_id = user.and_then(|item| item.get("id")).and_then(Value::as_str).unwrap_or("");
    let email = user.and_then(|item| item.get("email")).and_then(Value::as_str).unwrap_or("");
    if user_id.is_empty() {
        return Err("Authentication service did not return a user identity.".to_owned());
    }
    Ok(json!({
        "access_token":access_token,
        "refresh_token":value.get("refresh_token").and_then(Value::as_str).unwrap_or(""),
        "user_id":user_id,
        "email":email
    }).to_string())
}

pub fn validate_email_otp(code: &str) -> Result<(), String> {
    if code.len() == 6 && code.bytes().all(|byte| byte.is_ascii_digit()) {
        Ok(())
    } else {
        Err("Enter the six-digit verification code.".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::{build_auth_payload, parse_auth_session, validate_email_otp};
    use serde_json::Value;

    #[test]
    fn validates_and_builds_auth_payloads() {
        let signup: Value = serde_json::from_str(
            &build_auth_payload("signup", " user@example.com ", "password123", "").unwrap()
        ).unwrap();
        assert_eq!(signup["email"], "user@example.com");
        assert!(build_auth_payload("signup", "bad", "short", "").is_err());
        assert!(build_auth_payload("signin", "bad", "password", "").is_err());
        let refresh: Value = serde_json::from_str(
            &build_auth_payload("refresh", "", "", "refresh-token").unwrap()
        ).unwrap();
        assert_eq!(refresh["refresh_token"], "refresh-token");
    }

    #[test]
    fn validates_session_identity_and_email_otp() {
        let session: Value = serde_json::from_str(
            &parse_auth_session(r#"{"access_token":"access","refresh_token":"refresh","user":{"id":"u-1","email":"u@example.com"}}"#).unwrap()
        ).unwrap();
        assert_eq!(session["user_id"], "u-1");
        assert!(parse_auth_session(r#"{"access_token":"access"}"#).is_err());
        assert!(validate_email_otp("123456").is_ok());
        assert!(validate_email_otp("12a456").is_err());
    }
}
