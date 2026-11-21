//! Versioned first-party app identity and per-user capability authorization contracts.
use std::collections::{HashMap, HashSet};

fn valid_user_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 36
        && [8, 13, 18, 23].iter().all(|index| bytes[*index] == b'-')
        && bytes.iter().enumerate().all(|(index, byte)| {
            [8, 13, 18, 23].contains(&index) || byte.is_ascii_hexdigit()
        })
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
    EmptyPublisher,
    UntrustedPublisher,
    EmptyVersion,
    UnsupportedProtocol,
    EmptyCapability,
    DuplicateCapability,
    EmptyScope,
    DuplicateScope,
    AppIdOwnedByAnotherPublisher,
}

impl AppManifest {
    pub fn validate(&self, supported_protocol: u16) -> Result<(), ManifestError> {
        if self.app_id.trim().is_empty() { return Err(ManifestError::EmptyAppId); }
        if self.publisher_id.trim().is_empty() { return Err(ManifestError::EmptyPublisher); }
        if self.version.trim().is_empty() { return Err(ManifestError::EmptyVersion); }
        if self.protocol_version != supported_protocol { return Err(ManifestError::UnsupportedProtocol); }
        if self.capabilities.iter().any(|value| value.trim().is_empty()) {
            return Err(ManifestError::EmptyCapability);
        }
        let unique_capabilities: HashSet<&str> =
            self.capabilities.iter().map(String::as_str).collect();
        if unique_capabilities.len() != self.capabilities.len() {
            return Err(ManifestError::DuplicateCapability);
        }
        if self.requested_scopes.iter().any(|value| value.trim().is_empty()) {
            return Err(ManifestError::EmptyScope);
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
        if self.manifests.get(&manifest.app_id)
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
        assert!(registry.authorize("user-a", &grant, "notes.read"));
        assert!(!registry.authorize("user-b", &grant, "notes.read"));
        let mut revoked = grant;
        revoked.revoked = true;
        assert!(!registry.authorize("user-a", &revoked, "notes.read"));
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
        assert!(registry.authorize("user-a", &grant(), "notes.read"));
    }

    #[test]
    fn duplicate_requested_scopes_are_rejected() {
        let mut app = manifest();
        app.requested_scopes.push("notes.read".into());
        assert_eq!(app.validate(1), Err(ManifestError::DuplicateScope));
    }
}
