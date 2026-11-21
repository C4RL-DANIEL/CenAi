//! Versioned first-party app identity and per-user capability authorization contracts.
use std::collections::{HashMap, HashSet};

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
}

impl AppManifest {
    pub fn validate(&self, supported_protocol: u16) -> Result<(), ManifestError> {
        if self.app_id.trim().is_empty() { return Err(ManifestError::EmptyAppId); }
        if self.publisher_id.trim().is_empty() { return Err(ManifestError::EmptyPublisher); }
        if self.version.trim().is_empty() { return Err(ManifestError::EmptyVersion); }
        if self.protocol_version != supported_protocol { return Err(ManifestError::UnsupportedProtocol); }
        if self.capabilities.iter().any(|value| value.trim().is_empty()) { return Err(ManifestError::EmptyCapability); }
        let unique: HashSet<&str> = self.capabilities.iter().map(String::as_str).collect();
        if unique.len() != self.capabilities.len() { return Err(ManifestError::DuplicateCapability); }
        if self.requested_scopes.iter().any(|value| value.trim().is_empty()) { return Err(ManifestError::EmptyScope); }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserCapabilityGrant {
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
        Self { trusted_publishers: trusted_publishers.into_iter().collect(), manifests: HashMap::new() }
    }

    pub fn register(&mut self, manifest: AppManifest, supported_protocol: u16) -> Result<(), ManifestError> {
        manifest.validate(supported_protocol)?;
        if !self.trusted_publishers.contains(&manifest.publisher_id) {
            return Err(ManifestError::UntrustedPublisher);
        }
        self.manifests.insert(manifest.app_id.clone(), manifest);
        Ok(())
    }

    pub fn authorize(&self, user_scope: &str, grant: &UserCapabilityGrant, capability: &str) -> bool {
        if grant.revoked || grant.user_scope != user_scope || !grant.granted_scopes.contains(capability) {
            return false;
        }
        self.manifests.get(&grant.app_id)
            .is_some_and(|manifest| manifest.capabilities.iter().any(|item| item == capability))
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
            requested_scopes: vec!["account:current".into()],
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
        let grant = UserCapabilityGrant {
            user_scope: "user-a".into(),
            app_id: "com.cenai.notes".into(),
            granted_scopes: HashSet::from(["notes.read".into()]),
            revoked: false,
        };
        assert!(registry.authorize("user-a", &grant, "notes.read"));
        assert!(!registry.authorize("user-b", &grant, "notes.read"));
        let mut revoked = grant;
        revoked.revoked = true;
        assert!(!registry.authorize("user-a", &revoked, "notes.read"));
    }
}
