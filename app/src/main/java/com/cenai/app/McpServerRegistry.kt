package com.cenai.app

import org.json.JSONArray
import org.json.JSONObject

/** Account-scoped manually approved MCP servers. No public registry or publisher claim is trusted. */
class McpServerRegistry(private val vault: SecureKeyStore, private val ownerId: String) {
    private val client = McpClient(vault, ownerId)

    fun servers(): List<McpServerConfig> = runCatching {
        val stored = vault.getUserSecret(ownerId, REGISTRY_SECRET)
        if (stored.isBlank()) return emptyList()
        val array = JSONArray(stored)
        require(array.length() <= MAX_SERVERS) { "MCP server registry exceeds its limit." }
        List(array.length()) { parseConfig(array.getJSONObject(it)) }
    }.getOrElse { emptyList() }

    fun availableTools(): List<McpToolDefinition> = servers().flatMap { config ->
        config.capabilities.map { capability ->
            McpToolDefinition(
                config.serverId,
                capability.name,
                capability.description,
                capability.inputSchema,
                capability.requiresSensitiveRead || capability.requiresExternalWrite,
                capability.requiredScopes
            )
        }
    }

    suspend fun approveAndConnect(
        serverId: String,
        endpointUrl: String,
        publisher: String,
        capabilityManifestJson: String,
        bearerToken: String
    ): List<McpToolDefinition> {
        val config = buildConfig(serverId, endpointUrl, publisher, capabilityManifestJson)
        require(servers().none { it.serverId == config.serverId }) { "An MCP server with this ID is already connected." }
        if (bearerToken.isNotBlank()) client.saveBearerCredential(config.serverId, bearerToken)
        return try {
            val discovered = client.discover(config, serverApproved = true)
            require(discovered.isNotEmpty()) { "The approved MCP server did not expose any tools." }
            persist(servers() + config)
            discovered
        } catch (error: Exception) {
            client.removeBearerCredential(config.serverId)
            throw error
        }
    }

    fun remove(serverId: String) {
        require(serverId.matches(SERVER_ID)) { "MCP server identity is invalid." }
        persist(servers().filterNot { it.serverId == serverId })
        client.removeBearerCredential(serverId)
    }

    suspend fun execute(toolName: String, arguments: JSONObject, approveSensitiveAction: Boolean = false): ToolExecution? {
        val config = servers().firstOrNull { toolName.startsWith(toolPrefix(it.serverId)) } ?: return null
        val remoteName = toolName.removePrefix(toolPrefix(config.serverId))
        val capability = config.capabilities.firstOrNull { it.name == remoteName } ?: return null
        if ((capability.requiresSensitiveRead || capability.requiresExternalWrite) && !approveSensitiveAction) {
            return ToolExecution(toolName, arguments, "This MCP capability requires your approval before it can access sensitive data or write externally.", true, false)
        }
        return try {
            val result = client.call(
                config = config,
                toolName = remoteName,
                arguments = arguments,
                serverApproved = true,
                approvedScopes = capability.requiredScopes,
                approveSensitiveAction = approveSensitiveAction
            )
            ToolExecution(toolName, arguments, result.toString().take(MAX_OUTPUT_CHARS), false, true)
        } catch (error: Exception) {
            ToolExecution(toolName, arguments, "MCP tool failed: " + (error.message ?: error.javaClass.simpleName).take(500), false, false)
        }
    }

    fun toolGuide(): String {
        val tools = availableTools()
        if (tools.isEmpty()) return ""
        return tools.joinToString("\n") { tool ->
            val name = toolName(tool.serverId, tool.name)
            val approval = if (tool.requiresApproval) " Sensitive read/external write; a separate user approval is required." else " User-approved server scope; read-only/safe capability."
            name + " — " + tool.description + ". Input schema: " + tool.inputSchema + approval
        }
    }

    private fun buildConfig(
        serverId: String,
        endpointUrl: String,
        publisher: String,
        manifestJson: String
    ): McpServerConfig {
        require(serverId.matches(SERVER_ID)) { "MCP server ID must be 1–128 safe characters." }
        require(endpointUrl.length <= 2_048) { "MCP endpoint URL is too long." }
        require(publisher.isNotBlank() && publisher.length <= 160) { "MCP publisher name is required." }
        require(manifestJson.length in 2..128_000) { "MCP capability manifest is empty or too large." }
        val array = JSONArray(manifestJson)
        require(array.length() in 1..MAX_TOOLS) { "Declare between 1 and $MAX_TOOLS MCP tools." }
        val capabilities = List(array.length()) { index ->
            val item = array.getJSONObject(index)
            val scopesJson = item.optJSONArray("requiredScopes") ?: JSONArray()
            val scopes = (0 until scopesJson.length()).map { scopesJson.getString(it) }.toSet()
            McpCapability(
                name = item.getString("name"),
                description = item.optString("description").take(2_000),
                inputSchema = item.getJSONObject("inputSchema"),
                requiredScopes = scopes,
                requiresSensitiveRead = item.optBoolean("requiresSensitiveRead"),
                requiresExternalWrite = item.optBoolean("requiresExternalWrite")
            )
        }
        return McpServerConfig(
            serverId = serverId,
            endpointUrl = endpointUrl.trim(),
            publisher = publisher.trim(),
            publisherVerified = false,
            capabilities = capabilities
        )
    }

    private fun parseConfig(item: JSONObject): McpServerConfig {
        val capabilitiesJson = item.getJSONArray("capabilities")
        val capabilities = List(capabilitiesJson.length()) { index ->
            val capability = capabilitiesJson.getJSONObject(index)
            val scopesJson = capability.getJSONArray("requiredScopes")
            McpCapability(
                capability.getString("name"),
                capability.optString("description"),
                capability.getJSONObject("inputSchema"),
                (0 until scopesJson.length()).map { scopesJson.getString(it) }.toSet(),
                capability.optBoolean("requiresSensitiveRead"),
                capability.optBoolean("requiresExternalWrite")
            )
        }
        return McpServerConfig(
            item.getString("serverId"),
            item.getString("endpointUrl"),
            item.getString("publisher"),
            false,
            item.optString("protocolVersion", McpClient.PROTOCOL_VERSION),
            capabilities
        )
    }

    private fun persist(configs: List<McpServerConfig>) {
        require(configs.size <= MAX_SERVERS) { "You can connect up to $MAX_SERVERS MCP servers." }
        val array = JSONArray()
        configs.forEach { config ->
            val capabilities = JSONArray()
            config.capabilities.forEach { capability ->
                capabilities.put(JSONObject()
                    .put("name", capability.name)
                    .put("description", capability.description)
                    .put("inputSchema", capability.inputSchema)
                    .put("requiredScopes", JSONArray().apply { capability.requiredScopes.forEach(::put) })
                    .put("requiresSensitiveRead", capability.requiresSensitiveRead)
                    .put("requiresExternalWrite", capability.requiresExternalWrite))
            }
            array.put(JSONObject()
                .put("serverId", config.serverId)
                .put("endpointUrl", config.endpointUrl)
                .put("publisher", config.publisher)
                .put("publisherVerified", false)
                .put("protocolVersion", config.protocolVersion)
                .put("capabilities", capabilities))
        }
        vault.putUserSecret(ownerId, REGISTRY_SECRET, array.toString())
    }

    companion object {
        private const val REGISTRY_SECRET = "mcp_server_registry_v1"
        private const val MAX_SERVERS = 10
        private const val MAX_TOOLS = 64
        private const val MAX_OUTPUT_CHARS = 8_000
        private val SERVER_ID = Regex("[A-Za-z0-9][A-Za-z0-9._-]{0,127}")

        fun toolName(serverId: String, capability: String): String = "mcp__" + serverId + "__" + capability
        private fun toolPrefix(serverId: String) = "mcp__" + serverId + "__"
    }
}
