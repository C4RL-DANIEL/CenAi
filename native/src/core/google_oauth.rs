//! Security primitives for Google's installed-app OAuth authorization-code flow.
//! The Android adapter may launch the returned URL and forward the redirect URI;
//! PKCE generation and callback validation remain owned by Rust.
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};

pub const REDIRECT_URI: &str = "com.cenai.app:/oauth2redirect";
pub const SCOPE: &str = "https://www.googleapis.com/auth/generative-language.retriever https://www.googleapis.com/auth/cloud-platform";
const MAX_CALLBACK_BYTES: usize = 8 * 1024;
pub const MAX_AUTHORIZATION_AGE_SECONDS: i64 = 10 * 60;

/// Rejects stale authorization attempts so a copied redirect cannot be replayed indefinitely.
pub fn validate_authorization_age(started_at_unix_seconds: i64, now_unix_seconds: i64) -> Result<(), &'static str> {
    if started_at_unix_seconds <= 0 || now_unix_seconds < started_at_unix_seconds
        || now_unix_seconds.saturating_sub(started_at_unix_seconds) > MAX_AUTHORIZATION_AGE_SECONDS {
        return Err("Google sign-in request expired. Start the connection again");
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorizationRequest {
    pub url: String,
    pub state: String,
    pub verifier: String,
}

pub fn create_authorization_request(client_id: &str) -> Result<AuthorizationRequest, &'static str> {
    let client_id = client_id.trim();
    if client_id.len() > 256 || !client_id.ends_with(".apps.googleusercontent.com")
        || client_id.chars().any(|character| matches!(character, '/' | '?' | '#' | '@' | '\n' | '\r')) {
        return Err("A valid Google OAuth client ID is required");
    }
    let mut verifier_bytes = [0u8; 32];
    let mut state_bytes = [0u8; 32];
    OsRng.fill_bytes(&mut verifier_bytes);
    OsRng.fill_bytes(&mut state_bytes);
    let verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);
    let state = URL_SAFE_NO_PAD.encode(state_bytes);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let mut url = reqwest::Url::parse("https://accounts.google.com/o/oauth2/v2/auth")
        .expect("constant OAuth authorization URL");
    url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", REDIRECT_URI)
        .append_pair("response_type", "code")
        .append_pair("scope", SCOPE)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &state)
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent");
    Ok(AuthorizationRequest { url: url.to_string(), state, verifier })
}

/// Validate the exact custom-scheme redirect and one-time state before exposing a code.
pub fn validate_callback(callback: &str, expected_state: &str) -> Result<String, &'static str> {
    if callback.len() > MAX_CALLBACK_BYTES || expected_state.len() < 32 {
        return Err("OAuth callback or expected state is invalid");
    }
    let url = reqwest::Url::parse(callback).map_err(|_| "OAuth callback URL is invalid")?;
    if url.scheme() != "com.cenai.app" || url.host().is_some()
        || url.path() != "/oauth2redirect" || !url.username().is_empty()
        || url.password().is_some() || url.fragment().is_some() {
        return Err("Unexpected OAuth redirect URI");
    }
    let mut state = None;
    let mut code = None;
    let mut error = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "state" if state.is_none() => state = Some(value.into_owned()),
            "code" if code.is_none() => code = Some(value.into_owned()),
            "error" if error.is_none() => error = Some(value.into_owned()),
            "state" | "code" | "error" => return Err("OAuth callback contains duplicate parameters"),
            _ => {}
        }
    }
    if state.as_deref() != Some(expected_state) {
        return Err("OAuth state validation failed");
    }
    if error.is_some() { return Err("Google authorization was denied or failed"); }
    let code = code.ok_or("Google did not return an authorization code")?;
    if code.is_empty() || code.len() > 4096 || code.chars().any(char::is_control) {
        return Err("Google authorization code is invalid");
    }
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorization_uses_pkce_s256_and_random_state() {
        let first = create_authorization_request("123.apps.googleusercontent.com").unwrap();
        let second = create_authorization_request("123.apps.googleusercontent.com").unwrap();
        assert_ne!(first.state, second.state);
        assert_ne!(first.verifier, second.verifier);
        let url = reqwest::Url::parse(&first.url).unwrap();
        let pairs = url.query_pairs().collect::<std::collections::HashMap<_, _>>();
        assert_eq!(pairs.get("code_challenge_method").map(|v| v.as_ref()), Some("S256"));
        assert_eq!(pairs.get("redirect_uri").map(|v| v.as_ref()), Some(REDIRECT_URI));
        assert_eq!(pairs.get("state").map(|v| v.as_ref()), Some(first.state.as_str()));
        let expected = URL_SAFE_NO_PAD.encode(Sha256::digest(first.verifier.as_bytes()));
        assert_eq!(pairs.get("code_challenge").map(|v| v.as_ref()), Some(expected.as_str()));
    }

    #[test]
    fn callback_requires_exact_redirect_state_and_single_code() {
        let state = "a".repeat(43);
        assert_eq!(validate_callback(&format!("{REDIRECT_URI}?state={state}&code=abc"), &state).unwrap(), "abc");
        assert!(validate_callback(&format!("{REDIRECT_URI}?state=wrong&code=abc"), &state).is_err());
        assert!(validate_callback(&format!("https://attacker.example/?state={state}&code=abc"), &state).is_err());
        assert!(validate_callback(&format!("{REDIRECT_URI}?state={state}&code=a&code=b"), &state).is_err());
        assert!(validate_callback(&format!("{REDIRECT_URI}?state={state}&error=access_denied"), &state).is_err());
        assert!(validate_callback(&format!("com.cenai.app://evil/oauth2redirect?state={state}&code=abc"), &state).is_err());
    }

    #[test]
    fn authorization_requests_expire_after_ten_minutes() {
        assert!(validate_authorization_age(1_000, 1_599).is_ok());
        assert!(validate_authorization_age(1_000, 1_601).is_err());
        assert!(validate_authorization_age(1_000, 999).is_err());
        assert!(validate_authorization_age(0, 1_000).is_err());
    }

    #[test]
    fn invalid_client_ids_are_rejected() {
        assert!(create_authorization_request("https://evil.apps.googleusercontent.com").is_err());
        assert!(create_authorization_request("client-id").is_err());
        assert!(create_authorization_request(&format!("{} .apps.googleusercontent.com", "x".repeat(300))).is_err());
    }
}
