//! Versioned, bounded request/response contract for app-to-CenAI integrations.
//!
//! Authentication identity must be supplied by the trusted transport adapter (for
//! example, a verified Supabase session), never trusted from an app-provided field.
use super::manifest::{CapabilityRegistry, UserCapabilityGrant};
use base64::engine::general_purpose::STANDARD;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};

pub const ECOSYSTEM_PROTOCOL_VERSION: u16 = 1;
pub const MAX_ECOSYSTEM_PAYLOAD_BYTES: usize = 64 * 1024;
pub const MAX_ECOSYSTEM_IN_FLIGHT_IDS: usize = 4096;
pub const MAX_ECOSYSTEM_CLOCK_SKEW_MS: u64 = 5 * 60 * 1000;

fn valid_user_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 36
        && [8, 13, 18, 23].iter().all(|index| bytes[*index] == b'-')
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| [8, 13, 18, 23].contains(&index) || byte.is_ascii_hexdigit())
}

fn valid_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EcosystemRequest {
    pub protocol_version: u16,
    pub request_id: String,
    pub app_id: String,
    pub user_scope: String,
    pub capability: String,
    pub created_at_unix_ms: u64,
    #[serde(rename = "payloadBase64", with = "base64_payload")]
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EcosystemResponse {
    pub protocol_version: u16,
    pub request_id: String,
    pub accepted: bool,
    #[serde(rename = "payloadBase64", with = "base64_payload")]
    pub payload: Vec<u8>,
    pub error_code: Option<String>,
}

mod base64_payload {
    use super::STANDARD;
    use base64::Engine as _;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        STANDARD
            .decode(encoded.as_bytes())
            .map_err(serde::de::Error::custom)
    }
}

impl EcosystemRequest {
    /// Creates a request with a cryptographically random 256-bit replay identifier.
    /// The authenticated API adapter must overwrite user_scope from its verified session.
    pub fn new(
        app_id: impl Into<String>,
        user_scope: impl Into<String>,
        capability: impl Into<String>,
        payload: Vec<u8>,
    ) -> Result<Self, &'static str> {
        let mut random = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut random);
        let request_id = random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let created_at_unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "System clock is before the Unix epoch")?
            .as_millis()
            .min(u64::MAX as u128) as u64;
        let request = Self {
            protocol_version: ECOSYSTEM_PROTOCOL_VERSION,
            request_id,
            app_id: app_id.into(),
            user_scope: user_scope.into(),
            capability: capability.into(),
            created_at_unix_ms,
            payload,
        };
        request.validate_shape()?;
        Ok(request)
    }

    pub fn validate_shape(&self) -> Result<(), &'static str> {
        if self.protocol_version != ECOSYSTEM_PROTOCOL_VERSION {
            return Err("Unsupported ecosystem protocol version");
        }
        if !valid_token(&self.request_id) || self.request_id.len() < 32 {
            return Err("Request ID must be a bounded, unguessable token");
        }
        if !valid_token(&self.app_id) || !valid_token(&self.capability) {
            return Err("App identity or capability is invalid");
        }
        if !valid_user_uuid(&self.user_scope) {
            return Err("Authenticated user scope is invalid");
        }
        if self.payload.len() > MAX_ECOSYSTEM_PAYLOAD_BYTES {
            return Err("Ecosystem request payload exceeds 64 KB");
        }
        Ok(())
    }

    /// Checks a caller-authenticated account against a currently active capability grant.
    pub fn authorize(
        &self,
        authenticated_user: &str,
        registry: &CapabilityRegistry,
        grant: &UserCapabilityGrant,
    ) -> Result<(), &'static str> {
        self.validate_shape()?;
        if authenticated_user != self.user_scope {
            return Err("Request user does not match the authenticated account");
        }
        if grant.app_id != self.app_id
            || !registry.authorize(authenticated_user, grant, &self.capability)
        {
            return Err("App capability is not currently authorized");
        }
        Ok(())
    }
}

impl EcosystemResponse {
    pub fn validate_for(&self, request: &EcosystemRequest) -> Result<(), &'static str> {
        if self.protocol_version != ECOSYSTEM_PROTOCOL_VERSION
            || self.request_id != request.request_id
        {
            return Err("Ecosystem response does not match the request");
        }
        if self.payload.len() > MAX_ECOSYSTEM_PAYLOAD_BYTES {
            return Err("Ecosystem response payload exceeds 64 KB");
        }
        if self.accepted == self.error_code.is_some() {
            return Err("Ecosystem response status and error code are inconsistent");
        }
        if self
            .error_code
            .as_ref()
            .is_some_and(|code| !valid_token(code))
        {
            return Err("Ecosystem response error code is invalid");
        }
        Ok(())
    }
}

/// Bounded replay protection. The caller must persist/coordinate this guard across
/// processes or enforce equivalent server-side uniqueness for multi-instance transport.
#[derive(Default)]
pub struct ReplayGuard {
    seen: HashMap<String, u64>,
}

impl ReplayGuard {
    pub fn accept(
        &mut self,
        request: &EcosystemRequest,
        now_unix_ms: u64,
    ) -> Result<(), &'static str> {
        request.validate_shape()?;
        if request.created_at_unix_ms.abs_diff(now_unix_ms) > MAX_ECOSYSTEM_CLOCK_SKEW_MS {
            return Err("Ecosystem request is expired or its clock is outside the allowed window");
        }
        self.seen.retain(|_, timestamp| {
            now_unix_ms.saturating_sub(*timestamp) <= MAX_ECOSYSTEM_CLOCK_SKEW_MS
        });
        if self.seen.contains_key(&request.request_id) {
            return Err("Ecosystem request replay detected");
        }
        if self.seen.len() >= MAX_ECOSYSTEM_IN_FLIGHT_IDS {
            return Err("Ecosystem replay guard is at capacity");
        }
        self.seen
            .insert(request.request_id.clone(), request.created_at_unix_ms);
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.seen.len()
    }
    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecosystem::manifest::{AppManifest, CapabilityRegistry};

    const USER: &str = "550e8400-e29b-41d4-a716-446655440011";

    fn request() -> EcosystemRequest {
        EcosystemRequest {
            protocol_version: ECOSYSTEM_PROTOCOL_VERSION,
            request_id: "0123456789abcdef0123456789abcdef".into(),
            app_id: "com.cenai.notes".into(),
            user_scope: USER.into(),
            capability: "notes.read".into(),
            created_at_unix_ms: 1_000_000,
            payload: br#"{"note_id":"n-1"}"#.to_vec(),
        }
    }

    fn authorized() -> (CapabilityRegistry, UserCapabilityGrant) {
        let mut registry = CapabilityRegistry::new(vec!["cenai.firstparty".into()]);
        registry
            .register(
                AppManifest {
                    app_id: "com.cenai.notes".into(),
                    publisher_id: "cenai.firstparty".into(),
                    version: "1.0.0".into(),
                    protocol_version: 1,
                    capabilities: vec!["notes.read".into()],
                    requested_scopes: vec!["notes.read".into()],
                },
                1,
            )
            .unwrap();
        let grant = registry
            .create_grant(USER, "com.cenai.notes", vec!["notes.read".into()])
            .unwrap();
        (registry, grant)
    }

    #[test]
    fn wire_format_uses_camel_case_and_base64_payloads() {
        let request = request();
        let value = serde_json::to_value(&request).unwrap();
        assert_eq!(value["protocolVersion"], ECOSYSTEM_PROTOCOL_VERSION);
        assert_eq!(value["requestId"], request.request_id);
        assert_eq!(value["payloadBase64"], "eyJub3RlX2lkIjoibi0xIn0=");
        assert!(value.get("payload").is_none());
        let restored: EcosystemRequest = serde_json::from_value(value).unwrap();
        assert_eq!(restored, request);

        let response = EcosystemResponse {
            protocol_version: ECOSYSTEM_PROTOCOL_VERSION,
            request_id: request.request_id.clone(),
            accepted: true,
            payload: br#"{"ok":true}"#.to_vec(),
            error_code: None,
        };
        let encoded = serde_json::to_value(&response).unwrap();
        let restored: EcosystemResponse = serde_json::from_value(encoded).unwrap();
        assert_eq!(restored, response);
    }

    #[test]
    fn new_requests_receive_unique_bounded_random_ids() {
        let first = EcosystemRequest::new(
            "com.cenai.notes",
            "550e8400-e29b-41d4-a716-446655440011",
            "context.read",
            b"{}".to_vec(),
        )
        .unwrap();
        let second = EcosystemRequest::new(
            "com.cenai.notes",
            "550e8400-e29b-41d4-a716-446655440011",
            "context.read",
            b"{}".to_vec(),
        )
        .unwrap();
        assert_eq!(first.request_id.len(), 64);
        assert_ne!(first.request_id, second.request_id);
        first.validate_shape().unwrap();
    }

    #[test]
    fn request_requires_matching_authenticated_identity_and_current_grant() {
        let (registry, grant) = authorized();
        let request = request();
        assert!(request.authorize(USER, &registry, &grant).is_ok());
        assert!(request
            .authorize("550e8400-e29b-41d4-a716-446655440012", &registry, &grant)
            .is_err());
        let mut revoked = grant;
        revoked.revoked = true;
        assert!(request.authorize(USER, &registry, &revoked).is_err());
    }

    #[test]
    fn replay_guard_rejects_duplicates_expiry_and_invalid_clock() {
        let mut guard = ReplayGuard::default();
        let request = request();
        assert!(guard.accept(&request, 1_000_100).is_ok());
        assert!(guard.accept(&request, 1_000_100).is_err());
        let mut stale = request.clone();
        stale.request_id = "abcdef0123456789abcdef0123456789".into();
        assert!(guard
            .accept(&stale, 1_000_000 + MAX_ECOSYSTEM_CLOCK_SKEW_MS + 1)
            .is_err());
        assert_eq!(guard.len(), 1);
    }

    #[test]
    fn payloads_and_protocol_versions_are_bounded() {
        let mut request = request();
        request.payload = vec![0; MAX_ECOSYSTEM_PAYLOAD_BYTES + 1];
        assert!(request.validate_shape().is_err());
        request.payload.clear();
        request.protocol_version += 1;
        assert!(request.validate_shape().is_err());
    }

    #[test]
    fn response_must_match_request_and_have_consistent_status() {
        let request = request();
        let response = EcosystemResponse {
            protocol_version: ECOSYSTEM_PROTOCOL_VERSION,
            request_id: request.request_id.clone(),
            accepted: true,
            payload: b"ok".to_vec(),
            error_code: None,
        };
        assert!(response.validate_for(&request).is_ok());
        let mut mismatched = response.clone();
        mismatched.request_id = "abcdef0123456789abcdef0123456789".into();
        assert!(mismatched.validate_for(&request).is_err());
        let mut inconsistent = response;
        inconsistent.error_code = Some("denied".into());
        assert!(inconsistent.validate_for(&request).is_err());
    }
}
