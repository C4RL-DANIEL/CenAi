//! MCP capability contracts. Discovery never grants permission by itself.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct McpCapability {
    pub name: String,
    pub description: String,
    pub requires_sensitive_read: bool,
    pub requires_external_write: bool,
    pub requested_scopes: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct McpServerManifest {
    pub server_id: String,
    pub version: String,
    pub endpoint_url: String,
    pub protocol_version: u16,
    pub publisher: String,
    pub publisher_verified: bool,
    pub license_id: String,
    pub declared_dependencies: Vec<String>,
    pub capabilities: Vec<McpCapability>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct McpReview {
    pub secure_transport: bool,
    pub publisher_verified: bool,
    pub capability_count: usize,
    pub sensitive_capabilities: Vec<String>,
    pub external_write_capabilities: Vec<String>,
    pub dependency_count: usize,
    pub requires_manual_review: bool,
}

impl McpServerManifest {
    pub fn validate(&self, supported_protocol: u16) -> Result<(), &'static str> {
        if self.server_id.trim().is_empty() || self.publisher.trim().is_empty() {
            return Err("Publisher and server identity are required");
        }
        if self.version.trim().is_empty() || self.license_id.trim().is_empty() {
            return Err("Server version and license must be declared");
        }
        if !self.endpoint_url.starts_with("https://") || self.endpoint_url.len() > 2048 {
            return Err("Remote MCP endpoints must use HTTPS");
        }
        if self.protocol_version != supported_protocol {
            return Err("Unsupported MCP protocol version");
        }
        if self.capabilities.is_empty() || self.capabilities.iter().any(|cap| cap.name.trim().is_empty()) {
            return Err("Server must declare valid capabilities");
        }
        let names: HashSet<&str> = self.capabilities.iter().map(|cap| cap.name.as_str()).collect();
        if names.len() != self.capabilities.len() {
            return Err("MCP capability names must be unique");
        }
        if self.capabilities.iter().any(|cap| cap.requested_scopes.iter().any(|scope| scope.trim().is_empty())) {
            return Err("Capability scopes cannot be empty");
        }
        Ok(())
    }

    pub fn review(&self) -> McpReview {
        let sensitive_capabilities = self.capabilities.iter()
            .filter(|capability| capability.requires_sensitive_read)
            .map(|capability| capability.name.clone()).collect::<Vec<_>>();
        let external_write_capabilities = self.capabilities.iter()
            .filter(|capability| capability.requires_external_write)
            .map(|capability| capability.name.clone()).collect::<Vec<_>>();
        let secure_transport = self.endpoint_url.starts_with("https://");
        let requires_manual_review = !secure_transport || !self.publisher_verified
            || !sensitive_capabilities.is_empty() || !external_write_capabilities.is_empty()
            || !self.declared_dependencies.is_empty();
        McpReview {
            secure_transport,
            publisher_verified: self.publisher_verified,
            capability_count: self.capabilities.len(),
            sensitive_capabilities,
            external_write_capabilities,
            dependency_count: self.declared_dependencies.len(),
            requires_manual_review,
        }
    }

    pub fn may_execute(&self, capability: &str, user_approved: bool) -> bool {
        self.capabilities.iter().find(|item| item.name == capability)
            .is_some_and(|item| {
                (!item.requires_external_write && !item.requires_sensitive_read) || user_approved
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_server() -> McpServerManifest {
        McpServerManifest {
            server_id: "org.example.tools".into(),
            version: "1.0.0".into(),
            endpoint_url: "https://mcp.example.com".into(),
            protocol_version: 1,
            publisher: "Example".into(),
            publisher_verified: true,
            license_id: "MIT".into(),
            declared_dependencies: vec![],
            capabilities: vec![McpCapability {
                name: "mail.send".into(),
                description: "Send mail".into(),
                requires_sensitive_read: false,
                requires_external_write: true,
                requested_scopes: vec!["mail.send".into()],
            }],
        }
    }

    #[test]
    fn external_write_capability_requires_explicit_approval() {
        let server = test_server();
        assert!(server.validate(1).is_ok());
        assert!(!server.may_execute("mail.send", false));
        assert!(server.may_execute("mail.send", true));
    }

    #[test]
    fn discovery_produces_a_security_review_but_never_implicitly_approves() {
        let mut server = test_server();
        server.publisher_verified = false;
        server.declared_dependencies.push("shell-wrapper".into());
        let review = server.review();
        assert!(review.requires_manual_review);
        assert!(!review.publisher_verified);
        assert_eq!(review.external_write_capabilities, vec!["mail.send"]);
    }

    #[test]
    fn insecure_endpoints_and_duplicate_capabilities_are_rejected() {
        let mut server = test_server();
        server.endpoint_url = "http://mcp.example.com".into();
        assert_eq!(server.validate(1), Err("Remote MCP endpoints must use HTTPS"));
        server.endpoint_url = "https://mcp.example.com".into();
        server.capabilities.push(server.capabilities[0].clone());
        assert_eq!(server.validate(1), Err("MCP capability names must be unique"));
    }
}
