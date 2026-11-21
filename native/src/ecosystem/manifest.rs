//! Minimal versioned manifest validation. Authorization remains server-side.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppManifest {
    pub app_id: String,
    pub protocol_version: u16,
    pub capabilities: Vec<String>,
    pub requested_scopes: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManifestError {
    EmptyAppId,
    UnsupportedProtocol,
    EmptyCapability,
    EmptyScope,
}

impl AppManifest {
    pub fn validate(&self, supported_protocol: u16) -> Result<(), ManifestError> {
        if self.app_id.trim().is_empty() { return Err(ManifestError::EmptyAppId); }
        if self.protocol_version != supported_protocol { return Err(ManifestError::UnsupportedProtocol); }
        if self.capabilities.iter().any(|value| value.trim().is_empty()) { return Err(ManifestError::EmptyCapability); }
        if self.requested_scopes.iter().any(|value| value.trim().is_empty()) { return Err(ManifestError::EmptyScope); }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_first_party_manifest_passes_schema_checks() {
        let manifest = AppManifest {
            app_id: "com.example.notes".into(),
            protocol_version: 1,
            capabilities: vec!["notes.read".into(), "notes.search".into()],
            requested_scopes: vec!["account:current".into()],
        };
        assert_eq!(manifest.validate(1), Ok(()));
    }

    #[test]
    fn protocol_mismatch_is_rejected() {
        let manifest = AppManifest {
            app_id: "com.example.notes".into(),
            protocol_version: 2,
            capabilities: vec!["notes.read".into()],
            requested_scopes: vec![],
        };
        assert_eq!(manifest.validate(1), Err(ManifestError::UnsupportedProtocol));
    }
}
