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

fn validate_endpoint_url(endpoint: &str) -> Result<(), &'static str> {
    if endpoint.len() > 2048 {
        return Err("MCP endpoint URL exceeds the supported size");
    }
    let url = reqwest::Url::parse(endpoint).map_err(|_| "MCP endpoint URL is invalid")?;
    if url.scheme() != "https" || url.host_str().is_none() {
        return Err("Remote MCP endpoints must use HTTPS");
    }
    if !url.username().is_empty() || url.password().is_some() || url.query().is_some() || url.fragment().is_some() {
        return Err("MCP endpoint URLs cannot contain credentials, query parameters or fragments");
    }
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    if host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local") {
        return Err("Local-only MCP hosts are not accepted by the remote connector");
    }
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        let is_private = match ip {
            std::net::IpAddr::V4(ip) => ip.is_private() || ip.is_loopback() || ip.is_link_local() || ip.is_unspecified() || ip.is_multicast(),
            std::net::IpAddr::V6(ip) => ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local() || ip.is_unspecified() || ip.is_multicast(),
        };
        if is_private {
            return Err("Private, loopback and link-local MCP addresses are blocked");
        }
    }
    Ok(())
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
        validate_endpoint_url(&self.endpoint_url)?;
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


#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

pub struct McpHttpClient {
    client: reqwest::Client,
    manifest: McpServerManifest,
    bearer_token: Option<String>,
    session_id: tokio::sync::Mutex<Option<String>>,
    next_id: std::sync::atomic::AtomicU64,
}

impl McpHttpClient {
    pub fn new(manifest: McpServerManifest, bearer_token: Option<String>) -> Result<Self, String> {
        manifest.validate(1).map_err(|error| format!("Invalid MCP manifest: {error:?}"))?;
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(15))
            .timeout(std::time::Duration::from_secs(60))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("CenAI-MCP/0.3")
            .build().map_err(|error| error.to_string())?;
        Ok(Self {
            client,
            manifest,
            bearer_token,
            session_id: tokio::sync::Mutex::new(None),
            next_id: std::sync::atomic::AtomicU64::new(1),
        })
    }

    async fn rpc(&self, method: &str, params: serde_json::Value, notification: bool) -> Result<(serde_json::Value, Option<String>), String> {
        use reqwest::header::{ACCEPT, CONTENT_TYPE};
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });
        if !notification {
            body["id"] = serde_json::json!(id);
        }
        let mut request = self.client.post(&self.manifest.endpoint_url)
            .header(ACCEPT, "application/json, text/event-stream")
            .header(CONTENT_TYPE, "application/json")
            .header("MCP-Protocol-Version", "2025-03-26")
            .json(&body);
        if let Some(token) = &self.bearer_token {
            request = request.bearer_auth(token);
        }
        if let Some(session) = self.session_id.lock().await.as_ref() {
            request = request.header("Mcp-Session-Id", session);
        }
        let response = request.send().await.map_err(|error| error.to_string())?;
        let status = response.status();
        let session = response.headers().get("Mcp-Session-Id")
            .and_then(|value| value.to_str().ok()).map(str::to_owned);
        let content_type = response.headers().get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()).unwrap_or_default().to_owned();
        let raw = response.text().await.map_err(|error| error.to_string())?;
        if !status.is_success() {
            return Err(format!("MCP HTTP {}: {}", status.as_u16(), raw.chars().take(300).collect::<String>()));
        }
        if notification || raw.trim().is_empty() {
            return Ok((serde_json::Value::Null, session));
        }
        let payload = if content_type.contains("text/event-stream") {
            raw.lines().filter_map(|line| line.trim().strip_prefix("data:"))
                .map(str::trim).rfind(|line| !line.is_empty() && *line != "[DONE]").ok_or("MCP server returned an empty event stream")?
                .to_owned()
        } else {
            raw
        };
        let value: serde_json::Value = serde_json::from_str(&payload)
            .map_err(|error| format!("Invalid MCP JSON-RPC response: {error}"))?;
        if let Some(error) = value.get("error") {
            return Err(format!("MCP method {method} failed: {error}"));
        }
        Ok((value.get("result").cloned().unwrap_or(value), session))
    }

    pub async fn initialize(&self) -> Result<serde_json::Value, String> {
        let (result, session) = self.rpc("initialize", serde_json::json!({
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": { "name": "CenAI", "version": "0.3.0" }
        }), false).await?;
        if let Some(session) = session {
            *self.session_id.lock().await = Some(session);
        }
        let _ = self.rpc("notifications/initialized", serde_json::json!({}), true).await?;
        Ok(result)
    }

    pub async fn list_tools(&self) -> Result<Vec<McpTool>, String> {
        let (result, _) = self.rpc("tools/list", serde_json::json!({}), false).await?;
        serde_json::from_value(result.get("tools").cloned().unwrap_or(serde_json::Value::Null))
            .map_err(|error| format!("Invalid MCP tools/list response: {error}"))
    }

    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
        user_approved: bool,
    ) -> Result<serde_json::Value, String> {
        if !self.manifest.may_execute(tool_name, user_approved) {
            return Err("This MCP action requires explicit user approval.".into());
        }
        let (result, _) = self.rpc("tools/call", serde_json::json!({
            "name": tool_name,
            "arguments": arguments
        }), false).await?;
        Ok(result)
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
        server.endpoint_url = "https://user:password@mcp.example.com".into();
        assert!(server.validate(1).is_err());
        server.endpoint_url = "https://127.0.0.1/mcp".into();
        assert!(server.validate(1).is_err());
        server.endpoint_url = "https://mcp.example.com?token=secret".into();
        assert!(server.validate(1).is_err());
        server.endpoint_url = "https://mcp.example.com".into();
        server.capabilities.push(server.capabilities[0].clone());
        assert_eq!(server.validate(1), Err("MCP capability names must be unique"));
    }
}
