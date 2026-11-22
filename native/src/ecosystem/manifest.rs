//! Versioned first-party app identity and per-user capability authorization contracts.
use std::collections::{HashMap, HashSet};

const MAX_ID_BYTES: usize = 128;
const MAX_VERSION_BYTES: usize = 64;
const MAX_DECLARATIONS: usize = 64;

fn valid_user_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 36
        && [8, 13, 18, 23].iter().all(|index| bytes[*index] == b'-')
        && bytes.iter().enumerate().all(|(index, byte)| {
            [8, 13, 18, 23].contains(&index) || byte.is_ascii_hexdigit()
        })
}

fn valid_dotted_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value.split('.').all(|part| {
            !part.is_empty()
                && part.len() <= 63
                && part.as_bytes()[0].is_ascii_alphanumeric()
                && part.as_bytes().last().is_some_and(u8::is_ascii_alphanumeric)
                && part.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
}

fn valid_scope_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._:-".contains(&byte))
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value.as_bytes().last().is_some_and(u8::is_ascii_alphanumeric)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppManifest {
    pub app_id: String,
    pub publisher_id: String,
    pub version: String,
    pub protocol_version: u16,
    pub capabilities: Vec<String>,
    pub requested_scopes: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManifestError {
    EmptyAppId,
    InvalidAppId,
    EmptyPublisher,
    InvalidPublisher,
    UntrustedPublisher,
    EmptyVersion,
    InvalidVersion,
    UnsupportedProtocol,
    TooManyCapabilities,
    EmptyCapability,
    InvalidCapability,
    DuplicateCapability,
    TooManyScopes,
    EmptyScope,
    InvalidScope,
    DuplicateScope,
    AppIdOwnedByAnotherPublisher,
}

impl AppManifest {
    pub fn validate(&self, supported_protocol: u16) -> Result<(), ManifestError> {
        if self.app_id.trim().is_empty() {
            return Err(ManifestError::EmptyAppId);
        }
        if !valid_dotted_id(&self.app_id) {
            return Err(ManifestError::InvalidAppId);
        }
        if self.publisher_id.trim().is_empty() {
            return Err(ManifestError::EmptyPublisher);
        }
        if !valid_dotted_id(&self.publisher_id) {
            return Err(ManifestError::InvalidPublisher);
        }
        if self.version.trim().is_empty() {
            return Err(ManifestError::EmptyVersion);
        }
        if self.version.len() > MAX_VERSION_BYTES
            || !self.version.bytes().all(|byte| byte.is_ascii_alphanumeric() || b".+-".contains(&byte))
        {
            return Err(ManifestError::InvalidVersion);
        }
        if self.protocol_version != supported_protocol {
            return Err(ManifestError::UnsupportedProtocol);
        }
        if self.capabilities.len() > MAX_DECLARATIONS {
            return Err(ManifestError::TooManyCapabilities);
        }
        if self.capabilities.iter().any(|value| value.trim().is_empty()) {
            return Err(ManifestError::EmptyCapability);
        }
        if self.capabilities.iter().any(|value| !valid_scope_token(value)) {
            return Err(ManifestError::InvalidCapability);
        }
        let unique_capabilities: HashSet<&str> =
            self.capabilities.iter().map(String::as_str).collect();
        if unique_capabilities.len() != self.capabilities.len() {
            return Err(ManifestError::DuplicateCapability);
        }
        if self.requested_scopes.len() > MAX_DECLARATIONS {
            return Err(ManifestError::TooManyScopes);
        }
        if self.requested_scopes.iter().any(|value| value.trim().is_empty()) {
            return Err(ManifestError::EmptyScope);
        }
        if self.requested_scopes.iter().any(|value| !valid_scope_token(value)) {
            return Err(ManifestError::InvalidScope);
        }
        let unique_scopes: HashSet<&str> =
            self.requested_scopes.iter().map(String::as_str).collect();
        if unique_scopes.len() != self.requested_scopes.len() {
            return Err(ManifestError::DuplicateScope);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserCapabilityGrant {
    /// Immutable authenticated account identifier; never an email or app-provided label.
    pub user_scope: String,
    pub app_id: String,
    pub granted_scopes: HashSet<String>,
    pub revoked: bool,
}

#[derive(Default)]
pub struct CapabilityRegistry {
    trusted_publishers: HashSet<String>,
    manifests: HashMap<String, AppManifest>,
}

impl CapabilityRegistry {
    pub fn new(trusted_publishers: impl IntoIterator<Item = String>) -> Self {
        Self {
            trusted_publishers: trusted_publishers.into_iter().collect(),
            manifests: HashMap::new(),
        }
    }

    pub fn register(
        &mut self,
        manifest: AppManifest,
        supported_protocol: u16,
    ) -> Result<(), ManifestError> {
        manifest.validate(supported_protocol)?;
        if !self.trusted_publishers.contains(&manifest.publisher_id) {
            return Err(ManifestError::UntrustedPublisher);
        }
        // An app ID is permanently bound to its first trusted publisher. A different
        // publisher cannot replace the manifest and inherit existing user grants.
        if self
            .manifests
            .get(&manifest.app_id)
            .is_some_and(|existing| existing.publisher_id != manifest.publisher_id)
        {
            return Err(ManifestError::AppIdOwnedByAnotherPublisher);
        }
        self.manifests.insert(manifest.app_id.clone(), manifest);
        Ok(())
    }

    pub fn authorize(
        &self,
        user_scope: &str,
        grant: &UserCapabilityGrant,
        capability: &str,
    ) -> bool {
        if !valid_user_uuid(user_scope)
            || !valid_user_uuid(&grant.user_scope)
            || grant.revoked
            || grant.user_scope != user_scope
            || !grant.granted_scopes.contains(capability)
        {
            return false;
        }
        self.manifests.get(&grant.app_id).is_some_and(|manifest| {
            manifest.capabilities.iter().any(|item| item == capability)
                && manifest.requested_scopes.iter().any(|scope| scope == capability)
        })
    }

    pub fn registered_app_count(&self) -> usize {
        self.manifests.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> AppManifest {
        AppManifest {
            app_id: "com.cenai.notes".into(),
            publisher_id: "cenai.firstparty".into(),
            version: "1.0.0".into(),
            protocol_version: 1,
            capabilities: vec!["notes.read".into(), "notes.search".into()],
            requested_scopes: vec!["account:current".into(), "notes.read".into(), "notes.search".into()],
        }
    }

    fn grant() -> UserCapabilityGrant {
        UserCapabilityGrant {
            user_scope: "550e8400-e29b-41d4-a716-446655440011".into(),
            app_id: "com.cenai.notes".into(),
            granted_scopes: HashSet::from(["notes.read".into()]),
            revoked: false,
        }
    }

    #[test]
    fn trusted_first_party_manifest_registers() {
        let mut registry = CapabilityRegistry::new(vec!["cenai.firstparty".into()]);
        assert_eq!(registry.register(manifest(), 1), Ok(()));
        assert_eq!(registry.registered_app_count(), 1);
    }

    #[test]
    fn untrusted_publishers_cannot_register() {
        let mut registry = CapabilityRegistry::new(Vec::new());
        assert_eq!(registry.register(manifest(), 1), Err(ManifestError::UntrustedPublisher));
    }

    #[test]
    fn user_grants_are_isolated_and_revocable() {
        let mut registry = CapabilityRegistry::new(vec!["cenai.firstparty".into()]);
        registry.register(manifest(), 1).unwrap();
        let grant = grant();
        assert!(registry.authorize("550e8400-e29b-41d4-a716-446655440011", &grant, "notes.read"));
        assert!(!registry.authorize("550e8400-e29b-41d4-a716-446655440012", &grant, "notes.read"));
        let mut revoked = grant;
        revoked.revoked = true;
        assert!(!registry.authorize("550e8400-e29b-41d4-a716-446655440011", &revoked, "notes.read"));
    }

    #[test]
    fn grant_cannot_exceed_manifest_requested_scopes() {
        let mut registry = CapabilityRegistry::new(vec!["cenai.firstparty".into()]);
        let mut app = manifest();
        app.requested_scopes.retain(|scope| scope != "notes.read");
        registry.register(app, 1).unwrap();
        assert!(!registry.authorize("550e8400-e29b-41d4-a716-446655440011", &grant(), "notes.read"));
    }

    #[test]
    fn publisher_cannot_take_over_an_existing_app_id() {
        let mut registry = CapabilityRegistry::new(vec![
            "cenai.firstparty".into(),
            "another.trusted.publisher".into(),
        ]);
        registry.register(manifest(), 1).unwrap();
        let mut takeover = manifest();
        takeover.publisher_id = "another.trusted.publisher".into();
        takeover.capabilities = vec!["notes.read".into()];
        takeover.requested_scopes = vec!["notes.read".into()];
        assert_eq!(
            registry.register(takeover, 1),
            Err(ManifestError::AppIdOwnedByAnotherPublisher)
        );
        assert!(registry.authorize("550e8400-e29b-41d4-a716-446655440011", &grant(), "notes.read"));
    }

    #[test]
    fn duplicate_requested_scopes_are_rejected() {
        let mut app = manifest();
        app.requested_scopes.push("notes.read".into());
        assert_eq!(app.validate(1), Err(ManifestError::DuplicateScope));
    }

    #[test]
    fn identifiers_and_declarations_are_strictly_bounded() {
        let mut app = manifest();
        app.app_id = "com..notes".into();
        assert_eq!(app.validate(1), Err(ManifestError::InvalidAppId));

        let mut app = manifest();
        app.publisher_id = "CenAI.publisher".into();
        assert_eq!(app.validate(1), Err(ManifestError::InvalidPublisher));

        let mut app = manifest();
        app.capabilities = vec!["*".into()];
        assert_eq!(app.validate(1), Err(ManifestError::InvalidCapability));

        let mut app = manifest();
        app.requested_scopes = vec!["notes.read\nadmin".into()];
        assert_eq!(app.validate(1), Err(ManifestError::InvalidScope));

        let mut app = manifest();
        app.capabilities = (0..=MAX_DECLARATIONS).map(|index| format!("capability.{index}")).collect();
        assert_eq!(app.validate(1), Err(ManifestError::TooManyCapabilities));
    }
}
