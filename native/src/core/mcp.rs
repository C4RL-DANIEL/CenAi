//! MCP capability contracts. Discovery never grants permission by itself.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct McpCapability {
    pub name: String,
    pub description: String,
    pub requires_external_write: bool,
    pub requested_scopes: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct McpServerManifest {
    pub server_id: String,
    pub protocol_version: u16,
    pub publisher: String,
    pub capabilities: Vec<McpCapability>,
}

impl McpServerManifest {
    pub fn validate(&self, supported_protocol: u16) -> Result<(), &'static str> {
        if self.server_id.trim().is_empty() || self.publisher.trim().is_empty() { return Err("Publisher and server identity are required"); }
        if self.protocol_version != supported_protocol { return Err("Unsupported MCP protocol version"); }
        if self.capabilities.is_empty() || self.capabilities.iter().any(|cap| cap.name.trim().is_empty()) {
            return Err("Server must declare valid capabilities");
        }
        if self.capabilities.iter().any(|cap| cap.requested_scopes.iter().any(|scope| scope.trim().is_empty())) {
            return Err("Capability scopes cannot be empty");
        }
        Ok(())
    }

    pub fn may_execute(&self, capability: &str, user_approved: bool) -> bool {
        self.capabilities.iter().find(|item| item.name == capability)
            .is_some_and(|item| !item.requires_external_write || user_approved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_write_capability_requires_explicit_approval() {
        let server = McpServerManifest {
            server_id: "org.example.tools".into(), protocol_version: 1, publisher: "Example".into(),
            capabilities: vec![McpCapability { name: "mail.send".into(), description: "Send mail".into(), requires_external_write: true, requested_scopes: vec!["mail.send".into()] }],
        };
        assert!(server.validate(1).is_ok());
        assert!(!server.may_execute("mail.send", false));
        assert!(server.may_execute("mail.send", true));
    }
}
