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


pub fn validate_google_oauth_client_id(client_id: &str) -> Result<(), String> {
    if client_id.len() <= 256
        && client_id.ends_with(".apps.googleusercontent.com")
        && !client_id.chars().any(char::is_whitespace)
    {
        Ok(())
    } else {
        Err("Enter a valid Google OAuth client ID.".to_owned())
    }
}

pub fn validate_google_oauth_callback(
    scheme: &str,
    path: &str,
    returned_state: &str,
    expected_state: &str,
    error: &str,
    code: &str,
) -> Result<String, String> {
    if scheme != "com.cenai.app" || path != "/oauth2redirect" {
        return Err("Unexpected OAuth redirect.".to_owned());
    }
    if expected_state.is_empty() || !constant_time_eq(returned_state.as_bytes(), expected_state.as_bytes()) {
        return Err("OAuth state validation failed.".to_owned());
    }
    if !error.is_empty() {
        return Err(format!("Google authorization failed: {}", error.chars().take(200).collect::<String>()));
    }
    if code.is_empty() {
        return Err("Google did not return an authorization code.".to_owned());
    }
    Ok(code.to_owned())
}

pub fn parse_google_oauth_token(raw_json: &str, now_ms: u64) -> Result<String, String> {
    let value: Value = serde_json::from_str(raw_json)
        .map_err(|_| "Google returned invalid OAuth token JSON.".to_owned())?;
    let access = value.get("access_token").and_then(Value::as_str).unwrap_or("");
    if access.is_empty() {
        return Err(value.get("error_description").and_then(Value::as_str)
            .unwrap_or("Google did not return an access token.").to_owned());
    }
    let refresh = value.get("refresh_token").and_then(Value::as_str).unwrap_or("");
    let expires_in = value.get("expires_in").and_then(Value::as_u64).unwrap_or(3600);
    let expires_at = now_ms.saturating_add(expires_in.saturating_mul(1000));
    Ok(json!({
        "access_token": access,
        "refresh_token": refresh,
        "expires_at": expires_at.to_string()
    }).to_string())
}

#[cfg(test)]
mod google_oauth_tests {
    use super::{parse_google_oauth_token, validate_google_oauth_callback, validate_google_oauth_client_id};
    use serde_json::Value;

    #[test]
    fn validates_google_oauth_client_and_callback_state() {
        assert!(validate_google_oauth_client_id("abc.apps.googleusercontent.com").is_ok());
        assert!(validate_google_oauth_client_id("invalid").is_err());
        assert_eq!(
            validate_google_oauth_callback("com.cenai.app", "/oauth2redirect", "s", "s", "", "c").unwrap(),
            "c"
        );
        assert!(validate_google_oauth_callback("com.cenai.app", "/oauth2redirect", "bad", "s", "", "c").is_err());
    }

    #[test]
    fn parses_google_oauth_tokens_and_expiry() {
        let token: Value = serde_json::from_str(
            &parse_google_oauth_token(r#"{"access_token":"a","refresh_token":"r","expires_in":3600}"#, 1000).unwrap()
        ).unwrap();
        assert_eq!(token["access_token"], "a");
        assert_eq!(token["expires_at"], "3601000");
        assert!(parse_google_oauth_token(r#"{"error":"invalid_grant"}"#, 1000).is_err());
    }
}


fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() { return false; }
    left.iter().zip(right.iter()).fold(0u8, |difference, (a, b)| difference | (a ^ b)) == 0
}

#[cfg(test)]
mod constant_time_state_tests {
    use super::constant_time_eq;

    #[test]
    fn compares_oauth_state_bytes() {
        assert!(constant_time_eq(b"same-state", b"same-state"));
        assert!(!constant_time_eq(b"same-state", b"different"));
    }
}
