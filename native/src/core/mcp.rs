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

fn is_non_public_ip(address: std::net::IpAddr) -> bool {
    match address {
        std::net::IpAddr::V4(ip) => {
            let octets = ip.octets();
            ip.is_private() || ip.is_loopback() || ip.is_link_local() || ip.is_unspecified()
                || ip.is_multicast() || ip.is_broadcast() || octets[0] == 0 || octets[0] >= 240
                || (octets[0] == 100 && (64..=127).contains(&octets[1]))
                || (octets[0] == 192 && octets[1] == 0 && (octets[2] == 0 || octets[2] == 2))
                || (octets[0] == 198 && (octets[1] == 18 || octets[1] == 19 || (octets[1] == 51 && octets[2] == 100)))
                || (octets[0] == 203 && octets[1] == 0 && octets[2] == 113)
        }
        std::net::IpAddr::V6(ip) => {
            let segments = ip.segments();
            let octets = ip.octets();
            let mapped_v4 = segments[..5].iter().all(|segment| *segment == 0) && segments[5] == 0xffff;
            let compatible_v4 = segments[..6].iter().all(|segment| *segment == 0) && segments[6] != 0;
            if mapped_v4 || compatible_v4 {
                return is_non_public_ip(std::net::IpAddr::V4(std::net::Ipv4Addr::new(
                    octets[12], octets[13], octets[14], octets[15]
                )));
            }
            ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local()
                || ip.is_unspecified() || ip.is_multicast()
                || (segments[0] == 0x2001 && (segments[1] == 0x0db8 || segments[1] == 0x0002))
                || (segments[0] == 0x3fff && segments[1] & 0xf000 == 0)
                || (segments[0] & 0xffc0 == 0xfec0)
        },
    }
}

async fn resolve_public_addresses(host: &str, port: u16) -> Result<Vec<std::net::SocketAddr>, String> {
    let addresses = tokio::net::lookup_host((host, port)).await
        .map_err(|error| format!("MCP host DNS lookup failed: {error}"))?.collect::<Vec<_>>();
    if addresses.is_empty() { return Err("MCP host did not resolve to an address".into()); }
    if addresses.iter().any(|address| is_non_public_ip(address.ip())) {
        return Err("MCP host resolves to a private, reserved, or local network address".into());
    }
    Ok(addresses)
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
        if self.server_id.trim().is_empty() || self.server_id.len() > 128
            || !self.server_id.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
            || self.publisher.trim().is_empty() || self.publisher.len() > 160 {
            return Err("Publisher and safe server identity are required");
        }
        if self.version.trim().is_empty() || self.version.len() > 64
            || self.license_id.trim().is_empty() || self.license_id.len() > 80 {
            return Err("Server version and license must be declared within supported limits");
        }
        if self.declared_dependencies.len() > 64 || self.declared_dependencies.iter().any(|dependency| {
            dependency.trim().is_empty() || dependency.len() > 160
                || !dependency.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._@/-".contains(&byte))
        }) {
            return Err("MCP dependencies contain an invalid or oversized identifier");
        }
        let dependencies: HashSet<&str> = self.declared_dependencies.iter().map(String::as_str).collect();
        if dependencies.len() != self.declared_dependencies.len() {
            return Err("MCP dependencies must be unique");
        }
        validate_endpoint_url(&self.endpoint_url)?;
        if self.protocol_version != supported_protocol {
            return Err("Unsupported MCP protocol version");
        }
        if self.capabilities.is_empty() || self.capabilities.len() > 128
            || self.capabilities.iter().any(|cap| cap.name.trim().is_empty()
                || cap.name.len() > 128
                || !cap.name.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
                || cap.description.len() > 2000
                || cap.requested_scopes.is_empty()
                || cap.requested_scopes.len() > 32
                || cap.requested_scopes.iter().any(|scope| scope.trim().is_empty()
                    || scope.len() > 128
                    || !scope.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte)))
        }) {
            return Err("Server must declare bounded capabilities and valid non-empty scopes");
        }
        let names: HashSet<&str> = self.capabilities.iter().map(|cap| cap.name.as_str()).collect();
        if names.len() != self.capabilities.len() {
            return Err("MCP capability names must be unique");
        }
        for capability in &self.capabilities {
            let scopes: HashSet<&str> = capability.requested_scopes.iter().map(String::as_str).collect();
            if scopes.len() != capability.requested_scopes.len() {
                return Err("MCP capability scopes must be unique");
            }
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

fn validate_schema_definition(schema: &serde_json::Value, depth: usize) -> Result<(), String> {
    if depth > 12 || !schema.is_object() {
        return Err("MCP input schema is invalid or too deeply nested".into());
    }
    let kind = schema.get("type").and_then(serde_json::Value::as_str)
        .ok_or("MCP input schema must declare a supported type")?;
    if !matches!(kind, "object" | "array" | "string" | "number" | "integer" | "boolean" | "null") {
        return Err("MCP input schema uses an unsupported type".into());
    }
    if let Some(values) = schema.get("enum") {
        if !values.is_array() || values.as_array().is_some_and(|items| items.is_empty() || items.len() > 64) {
            return Err("MCP schema enum must contain 1-64 values".into());
        }
    }
    if kind == "object" {
        let properties = schema.get("properties").and_then(serde_json::Value::as_object)
            .ok_or("MCP object schema must declare properties")?;
        if properties.len() > 128 { return Err("MCP object schema has too many properties".into()); }
        if let Some(required) = schema.get("required") {
            let values = required.as_array().ok_or("MCP required must be an array")?;
            if values.len() > 128 { return Err("MCP required list is too large".into()); }
            for item in values {
                let name = item.as_str().ok_or("MCP required entries must be strings")?;
                if !properties.contains_key(name) { return Err("MCP required entry is not a declared property".into()); }
            }
        }
        if let Some(additional) = schema.get("additionalProperties") {
            if !additional.is_boolean() { return Err("MCP additionalProperties must be a boolean".into()); }
        }
        for child in properties.values() {
            validate_schema_definition(child, depth + 1)?;
        }
    }
    if kind == "array" {
        let items = schema.get("items").ok_or("MCP array schema must declare item schema")?;
        validate_schema_definition(items, depth + 1)?;
    }
    Ok(())
}

fn validate_tool_value(schema: &serde_json::Value, value: &serde_json::Value, depth: usize) -> Result<(), String> {
    if depth > 12 { return Err("MCP tool arguments exceed the supported nesting depth".into()); }
    let kind = schema.get("type").and_then(serde_json::Value::as_str).unwrap_or_default();
    let valid = match kind {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "number" => value.is_number(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => false,
    };
    if !valid { return Err(format!("MCP argument does not match the declared {kind} type")); }
    if let Some(values) = schema.get("enum").and_then(serde_json::Value::as_array) {
        if !values.iter().any(|item| item == value) { return Err("MCP argument is not an allowed enum value".into()); }
    }
    match kind {
        "object" => {
            let object = value.as_object().ok_or("Expected MCP object")?;
            if object.len() > 128 { return Err("MCP argument object has too many properties".into()); }
            let properties = schema.get("properties").and_then(serde_json::Value::as_object).ok_or("Invalid MCP object schema")?;
            if schema.get("additionalProperties").and_then(serde_json::Value::as_bool) == Some(false)
                && object.keys().any(|key| !properties.contains_key(key)) {
                return Err("MCP arguments contain undeclared properties".into());
            }
            if let Some(required) = schema.get("required").and_then(serde_json::Value::as_array) {
                for item in required {
                    let key = item.as_str().ok_or("Invalid MCP required entry")?;
                    if !object.contains_key(key) { return Err(format!("Required MCP argument '{key}' is missing")); }
                }
            }
            for (key, child) in properties {
                if let Some(argument) = object.get(key) {
                    validate_tool_value(child, argument, depth + 1)?;
                }
            }
        }
        "array" => {
            let items = value.as_array().ok_or("Expected MCP array")?;
            if items.len() > 256 { return Err("MCP argument array is too large".into()); }
            let item_schema = schema.get("items").ok_or("Invalid MCP array schema")?;
            for item in items { validate_tool_value(item_schema, item, depth + 1)?; }
            if schema.get("minItems").and_then(serde_json::Value::as_u64).is_some_and(|min| items.len() < min as usize)
                || schema.get("maxItems").and_then(serde_json::Value::as_u64).is_some_and(|max| items.len() > max as usize) {
                return Err("MCP array length is outside the allowed range".into());
            }
        }
        "string" => {
            let text = value.as_str().unwrap_or_default();
            if text.len() > 20_000
                || schema.get("minLength").and_then(serde_json::Value::as_u64).is_some_and(|min| text.chars().count() < min as usize)
                || schema.get("maxLength").and_then(serde_json::Value::as_u64).is_some_and(|max| text.chars().count() > max as usize) {
                return Err("MCP string argument is outside the allowed length".into());
            }
        }
        "number" | "integer" => {
            let number = value.as_f64().ok_or("Invalid MCP number")?;
            if schema.get("minimum").and_then(serde_json::Value::as_f64).is_some_and(|min| number < min)
                || schema.get("maximum").and_then(serde_json::Value::as_f64).is_some_and(|max| number > max) {
                return Err("MCP numeric argument is outside the allowed range".into());
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_tool_arguments(schema: &serde_json::Value, arguments: &serde_json::Value) -> Result<(), String> {
    if serde_json::to_vec(arguments).map_err(|error| error.to_string())?.len() > 64 * 1024 {
        return Err("MCP tool arguments exceed the 64 KB limit".into());
    }
    validate_tool_value(schema, arguments, 0)
}

pub struct McpHttpClient {
    client: reqwest::Client,
    manifest: McpServerManifest,
    bearer_token: Option<String>,
    session_id: tokio::sync::Mutex<Option<String>>,
    next_id: std::sync::atomic::AtomicU64,
    tools: tokio::sync::RwLock<std::collections::HashMap<String, McpTool>>,
}

impl McpHttpClient {
    pub async fn new(manifest: McpServerManifest, bearer_token: Option<String>) -> Result<Self, String> {
        manifest.validate(1).map_err(|error| format!("Invalid MCP manifest: {error:?}"))?;
        let endpoint = reqwest::Url::parse(&manifest.endpoint_url).map_err(|error| error.to_string())?;
        let host = endpoint.host_str().ok_or("MCP endpoint host is missing")?;
        let port = endpoint.port_or_known_default().ok_or("MCP endpoint port is missing")?;
        // Resolve once, reject private/reserved results, and pin the approved addresses
        // into the HTTP client to prevent DNS rebinding between validation and connect.
        let addresses = resolve_public_addresses(host, port).await?;
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(15))
            .timeout(std::time::Duration::from_secs(60))
            .redirect(reqwest::redirect::Policy::none())
            .resolve_to_addrs(host, &addresses)
            .user_agent("CenAI-MCP/0.3")
            .build().map_err(|error| error.to_string())?;
        Ok(Self {
            client,
            manifest,
            bearer_token,
            session_id: tokio::sync::Mutex::new(None),
            next_id: std::sync::atomic::AtomicU64::new(1),
            tools: tokio::sync::RwLock::new(std::collections::HashMap::new()),
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
        if !status.is_success() {
            return Err(format!("MCP server returned HTTP {}", status.as_u16()));
        }
        if response.content_length().is_some_and(|length| length > 2 * 1024 * 1024) {
            return Err("MCP response exceeds the 2 MB response limit".into());
        }
        use futures_util::StreamExt;
        let mut stream = response.bytes_stream();
        let mut response_bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| error.to_string())?;
            if response_bytes.len().saturating_add(chunk.len()) > 2 * 1024 * 1024 {
                return Err("MCP response exceeds the 2 MB response limit".into());
            }
            response_bytes.extend_from_slice(&chunk);
        }
        let raw = String::from_utf8(response_bytes).map_err(|_| "MCP response is not valid UTF-8")?;
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
        let tools: Vec<McpTool> = serde_json::from_value(result.get("tools").cloned().unwrap_or(serde_json::Value::Null))
            .map_err(|error| format!("Invalid MCP tools/list response: {error}"))?;
        if tools.len() > 128 { return Err("MCP server returned more than 128 tools".into()); }
        let mut indexed = std::collections::HashMap::new();
        for tool in &tools {
            if tool.name.trim().is_empty() || tool.name.len() > 128
                || tool.description.as_ref().is_some_and(|description| description.len() > 2000)
                || serde_json::to_vec(&tool.input_schema).map_err(|error| error.to_string())?.len() > 32 * 1024 {
                return Err("MCP tool metadata exceeds supported limits".into());
            }
            validate_schema_definition(&tool.input_schema, 0)?;
            if indexed.insert(tool.name.clone(), tool.clone()).is_some() {
                return Err("MCP server returned duplicate tool names".into());
            }
        }
        *self.tools.write().await = indexed;
        Ok(tools)
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
        let tool = self.tools.read().await.get(tool_name).cloned()
            .ok_or("MCP tool was not discovered in the active server session")?;
        validate_tool_arguments(&tool.input_schema, &arguments)?;
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
    fn manifests_reject_ambiguous_or_unbounded_capability_metadata() {
        let mut manifest = test_server();
        manifest.capabilities[0].requested_scopes.push("mail.send".into());
        assert!(manifest.validate(1).is_err());
        let mut manifest = test_server();
        manifest.capabilities[0].name = "mail send".into();
        assert!(manifest.validate(1).is_err());
        let mut manifest = test_server();
        manifest.declared_dependencies = vec!["shell-wrapper".into(), "shell-wrapper".into()];
        assert!(manifest.validate(1).is_err());
        let mut manifest = test_server();
        manifest.capabilities[0].requested_scopes.clear();
        assert!(manifest.validate(1).is_err());
    }

    #[test]
    fn dns_policy_rejects_private_and_reserved_addresses() {
        assert!(is_non_public_ip("127.0.0.1".parse().unwrap()));
        assert!(is_non_public_ip("10.0.0.8".parse().unwrap()));
        assert!(is_non_public_ip("100.64.0.1".parse().unwrap()));
        assert!(is_non_public_ip("192.0.2.10".parse().unwrap()));
        assert!(is_non_public_ip("2001:db8::1".parse().unwrap()));
        assert!(!is_non_public_ip("8.8.8.8".parse().unwrap()));
        assert!(!is_non_public_ip("2606:4700:4700::1111".parse().unwrap()));
    }

    #[test]
    fn tool_arguments_must_match_discovered_json_schema() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "to": { "type": "string", "maxLength": 200 },
                "count": { "type": "integer", "minimum": 1, "maximum": 5 }
            },
            "required": ["to"],
            "additionalProperties": false
        });
        assert!(validate_schema_definition(&schema, 0).is_ok());
        assert!(validate_tool_arguments(&schema, &serde_json::json!({"to":"team@example.com","count":2})).is_ok());
        assert!(validate_tool_arguments(&schema, &serde_json::json!({"count":2})).is_err());
        assert!(validate_tool_arguments(&schema, &serde_json::json!({"to":"team@example.com","count":9})).is_err());
        assert!(validate_tool_arguments(&schema, &serde_json::json!({"to":"team@example.com","shell":"rm -rf /"})).is_err());
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
