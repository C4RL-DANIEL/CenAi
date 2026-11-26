package com.cenai.app

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.Dns
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrl
import org.json.JSONArray
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.net.Inet4Address
import java.net.Inet6Address
import java.net.InetAddress
import java.net.UnknownHostException
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicLong

data class McpCapability(
    val name: String,
    val description: String,
    val inputSchema: JSONObject,
    val requiredScopes: Set<String>,
    val requiresSensitiveRead: Boolean = false,
    val requiresExternalWrite: Boolean = false
)

data class McpServerConfig(
    val serverId: String,
    val endpointUrl: String,
    val publisher: String,
    val publisherVerified: Boolean,
    val protocolVersion: String = McpClient.PROTOCOL_VERSION,
    val capabilities: List<McpCapability>
)

data class McpToolDefinition(
    val serverId: String,
    val name: String,
    val description: String,
    val inputSchema: JSONObject,
    val requiresApproval: Boolean,
    val requiredScopes: Set<String>
)

/** Guarded MCP Streamable HTTP transport. Discovery and sensitive execution require explicit approval. */
class McpClient(private val vault: SecureKeyStore, private val ownerId: String = vault.get("supabase_user_id").ifBlank { "guest" }) {
    private val requestIds = AtomicLong(1)
    private val jsonType = "application/json; charset=utf-8".toMediaType()

    suspend fun discover(config: McpServerConfig, serverApproved: Boolean): List<McpToolDefinition> =
        withContext(Dispatchers.IO) {
            require(serverApproved) { "Approve this MCP server before connecting." }
            validateManifest(config)
            val connection = connect(config)
            val tools = request(config, connection, "tools/list", JSONObject())
                .optJSONArray("tools") ?: JSONArray()
            require(tools.length() <= MAX_TOOLS) { "MCP server returned too many tools." }
            val declared = config.capabilities.associateBy { it.name }
            buildList {
                for (index in 0 until tools.length()) {
                    val tool = tools.optJSONObject(index) ?: throw IOException("MCP returned an invalid tool entry.")
                    val name = tool.optString("name")
                    val capability = declared[name]
                        ?: throw IOException("MCP tool '$name' was not declared in the approved server manifest.")
                    val schema = tool.optJSONObject("inputSchema")
                        ?: throw IOException("MCP tool '$name' did not provide an input schema.")
                    validateSchemaDefinition(schema, 0)
                    require(jsonEquivalent(schema, capability.inputSchema)) {
                        "MCP tool '$name' schema differs from the user-approved manifest."
                    }
                    require(schema.toString().toByteArray().size <= MAX_SCHEMA_BYTES) { "MCP tool schema is too large." }
                    add(McpToolDefinition(
                        config.serverId, name,
                        capability.description,
                        schema,
                        capability.requiresSensitiveRead || capability.requiresExternalWrite,
                        capability.requiredScopes
                    ))
                }
            }
        }

    suspend fun call(
        config: McpServerConfig,
        toolName: String,
        arguments: JSONObject,
        serverApproved: Boolean,
        approvedScopes: Set<String>,
        approveSensitiveAction: Boolean
    ): JSONObject = withContext(Dispatchers.IO) {
        require(serverApproved) { "Approve this MCP server before executing tools." }
        validateManifest(config)
        val capability = config.capabilities.firstOrNull { it.name == toolName }
            ?: throw SecurityException("MCP capability is not declared by this server.")
        require(capability.requiredScopes.all { it in approvedScopes }) {
            "This MCP capability requires additional user-approved scopes."
        }
        if (capability.requiresSensitiveRead || capability.requiresExternalWrite) {
            require(approveSensitiveAction) { "This MCP action requires your approval before execution." }
        }
        validateSchemaDefinition(capability.inputSchema, 0)
        validateValue(arguments, capability.inputSchema, 0)
        require(arguments.toString().toByteArray().size <= MAX_ARGUMENT_BYTES) {
            "MCP arguments exceed the supported size limit."
        }
        val connection = connect(config)
        request(config, connection, "tools/call", JSONObject().put("name", toolName).put("arguments", arguments))
    }

    fun saveBearerCredential(serverId: String, token: String) {
        require(serverId.matches(SERVER_ID)) { "MCP server identity is invalid." }
        require(token.length <= 8_192) { "MCP credential is too long." }
        vault.putUserSecret(ownerId, "mcp_auth_$serverId", token.trim())
    }

    fun removeBearerCredential(serverId: String) {
        require(serverId.matches(SERVER_ID)) { "MCP server identity is invalid." }
        vault.putUserSecret(ownerId, "mcp_auth_$serverId", "")
    }

    private data class Connection(val url: HttpUrl, val addresses: List<InetAddress>, var sessionId: String? = null)

    private fun connect(config: McpServerConfig): Connection {
        val url = config.endpointUrl.toHttpUrl()
        val connection = Connection(url, resolvePublicEndpoint(url))
        val initialized = request(
            config, connection, "initialize",
            JSONObject()
                .put("protocolVersion", PROTOCOL_VERSION)
                .put("capabilities", JSONObject())
                .put("clientInfo", JSONObject().put("name", "CenAI Android").put("version", "0.4"))
        )
        val negotiated = initialized.optString("protocolVersion")
        require(negotiated == PROTOCOL_VERSION || negotiated == "2024-11-05") {
            "MCP server selected an unsupported protocol version."
        }
        notifyInitialized(config, connection)
        return connection
    }

    private fun notifyInitialized(config: McpServerConfig, connection: Connection) {
        val body = JSONObject().put("jsonrpc", "2.0").put("method", "notifications/initialized")
        val request = requestBuilder(config, connection).post(body.toString().toRequestBody(jsonType)).build()
        clientFor(connection).newCall(request).execute().use { response ->
            if (!response.isSuccessful) throw IOException("MCP initialization notification failed (HTTP ${response.code}).")
        }
    }

    private fun request(config: McpServerConfig, connection: Connection, method: String, params: JSONObject): JSONObject {
        val body = JSONObject().put("jsonrpc", "2.0").put("id", requestIds.getAndIncrement())
            .put("method", method).put("params", params)
        val request = requestBuilder(config, connection).post(body.toString().toRequestBody(jsonType)).build()
        clientFor(connection).newCall(request).execute().use { response ->
            val raw = readBodyLimited(response)
            if (!response.isSuccessful) throw IOException("MCP request failed (HTTP ${response.code}): ${raw.take(400)}")
            connection.sessionId = response.header("Mcp-Session-Id") ?: connection.sessionId
            val envelope = parseJsonOrSse(raw)
            val error = envelope.optJSONObject("error")
            if (error != null) throw IOException("MCP ${method} error: ${error.optString("message", "Unknown server error").take(400)}")
            return envelope.optJSONObject("result") ?: throw IOException("MCP ${method} returned no result.")
        }
    }

    private fun requestBuilder(config: McpServerConfig, connection: Connection): Request.Builder {
        val builder = Request.Builder().url(connection.url)
            .header("Accept", "application/json, text/event-stream")
            .header("Content-Type", "application/json")
            .header("MCP-Protocol-Version", config.protocolVersion)
        connection.sessionId?.let { builder.header("Mcp-Session-Id", it) }
        vault.getUserSecret(ownerId, "mcp_auth_${config.serverId}").takeIf { it.isNotBlank() }?.let {
            builder.header("Authorization", "Bearer $it")
        }
        return builder
    }

    private fun clientFor(connection: Connection): OkHttpClient =
        OkHttpClient.Builder()
            .dns(object : Dns {
                override fun lookup(hostname: String): List<InetAddress> {
                    if (!hostname.equals(connection.url.host, ignoreCase = true)) {
                        throw UnknownHostException("MCP attempted to resolve an unapproved host.")
                    }
                    return connection.addresses
                }
            })
            .connectTimeout(12, TimeUnit.SECONDS)
            .readTimeout(45, TimeUnit.SECONDS)
            .callTimeout(60, TimeUnit.SECONDS)
            .followRedirects(false)
            .followSslRedirects(false)
            .retryOnConnectionFailure(false)
            .build()

    private fun validateManifest(config: McpServerConfig) {
        require(config.serverId.matches(SERVER_ID)) { "MCP server identity is invalid." }
        require(config.publisher.isNotBlank() && config.publisher.length <= 160) { "MCP publisher is required." }
        require(config.protocolVersion == PROTOCOL_VERSION || config.protocolVersion == "2024-11-05") {
            "Unsupported MCP protocol version."
        }
        require(config.capabilities.isNotEmpty() && config.capabilities.size <= MAX_TOOLS) {
            "MCP manifest must declare between 1 and $MAX_TOOLS capabilities."
        }
        require(config.capabilities.map { it.name }.distinct().size == config.capabilities.size) {
            "MCP capability names must be unique."
        }
        config.capabilities.forEach { capability ->
            require(capability.name.matches(TOOL_NAME) && capability.requiredScopes.isNotEmpty()) {
                "MCP capability names and scopes must be valid."
            }
            require(capability.description.length <= 2_000 && capability.requiredScopes.size <= 32) {
                "MCP capability metadata exceeds supported limits."
            }
            validateSchemaDefinition(capability.inputSchema, 0)
        }
        config.endpointUrl.toHttpUrl()
    }

    private fun resolvePublicEndpoint(url: HttpUrl): List<InetAddress> {
        require(url.isHttps && url.port == 443 && url.username.isEmpty() && url.password.isEmpty() &&
            url.query == null && url.fragment == null) {
            "MCP servers must use public HTTPS on port 443 without URL credentials, query strings or fragments."
        }
        val host = url.host.lowercase()
        require(host.contains('.') && !host.endsWith(".local") && !host.endsWith(".localhost") &&
            !host.equals("localhost", true) && !host.matches(Regex("[0-9.]+")) && !host.contains(':')) {
            "MCP endpoint must use a public DNS hostname."
        }
        val addresses = InetAddress.getAllByName(host).toList()
        require(addresses.isNotEmpty() && addresses.size <= 16 && addresses.none(::isNonPublicAddress)) {
            "MCP endpoint resolves to a private, reserved or local network address."
        }
        return addresses
    }

    private fun isNonPublicAddress(address: InetAddress): Boolean {
        if (address.isAnyLocalAddress || address.isLoopbackAddress || address.isLinkLocalAddress ||
            address.isSiteLocalAddress || address.isMulticastAddress) return true
        return when (address) {
            is Inet4Address -> {
                val b = address.address.map { it.toInt() and 0xff }
                b[0] == 0 || b[0] == 10 || b[0] == 127 || b[0] >= 224 ||
                    (b[0] == 100 && b[1] in 64..127) ||
                    (b[0] == 169 && b[1] == 254) ||
                    (b[0] == 172 && b[1] in 16..31) ||
                    (b[0] == 192 && b[1] == 168) ||
                    (b[0] == 192 && b[1] == 0 && b[2] == 0) ||
                    (b[0] == 192 && b[1] == 0 && b[2] == 2) ||
                    (b[0] == 198 && b[1] in 18..19) ||
                    (b[0] == 198 && b[1] == 51 && b[2] == 100) ||
                    (b[0] == 203 && b[1] == 0 && b[2] == 113)
            }
            is Inet6Address -> {
                val bytes = address.address
                val mappedV4 = bytes.take(10).all { it == 0.toByte() } &&
                    bytes[10] == 0xff.toByte() && bytes[11] == 0xff.toByte()
                val first = bytes[0].toInt() and 0xff
                val globalUnicast = first in 0x20..0x3f
                val uniqueLocal = (first and 0xfe) == 0xfc
                val documentation = bytes[0] == 0x20.toByte() && bytes[1] == 0x01.toByte() &&
                    bytes[2] == 0x0d.toByte() && bytes[3] == 0xb8.toByte()
                val special2001 = bytes[0] == 0x20.toByte() && bytes[1] == 0x01.toByte() &&
                    (bytes[2].toInt() and 0xf0) == 0
                val sixToFour = bytes[0] == 0x20.toByte() && bytes[1] == 0x02.toByte()
                mappedV4 || !globalUnicast || uniqueLocal || documentation || special2001 || sixToFour
            }
            else -> true
        }
    }

    private fun readBodyLimited(response: okhttp3.Response): String {
        val body = response.body ?: return ""
        val declared = body.contentLength()
        require(declared < 0 || declared <= MAX_RESPONSE_BYTES) { "MCP response exceeds the 1 MiB safety limit." }
        val output = ByteArrayOutputStream()
        body.byteStream().use { input ->
            val buffer = ByteArray(16 * 1024)
            var total = 0
            while (true) {
                val count = input.read(buffer)
                if (count < 0) break
                total += count
                require(total <= MAX_RESPONSE_BYTES) { "MCP response exceeds the 1 MiB safety limit." }
                output.write(buffer, 0, count)
            }
        }
        return output.toString(Charsets.UTF_8.name())
    }

    private fun parseJsonOrSse(raw: String): JSONObject {
        val trimmed = raw.trim()
        if (trimmed.startsWith("{")) return JSONObject(trimmed)
        val data = trimmed.lineSequence().filter { it.startsWith("data:") }
            .map { it.removePrefix("data:").trim() }.filter { it.isNotBlank() && it != "[DONE]" }.toList()
        for (line in data.asReversed()) runCatching { JSONObject(line) }.getOrNull()?.let { return it }
        throw IOException("MCP server returned an invalid JSON or SSE response.")
    }

    companion object {
        const val PROTOCOL_VERSION = "2025-03-26"
        private const val MAX_TOOLS = 128
        private const val MAX_SCHEMA_BYTES = 64 * 1024
        private const val MAX_ARGUMENT_BYTES = 256 * 1024
        private const val MAX_RESPONSE_BYTES = 1024 * 1024
        private val SERVER_ID = Regex("[A-Za-z0-9][A-Za-z0-9._-]{0,127}")
        private val TOOL_NAME = Regex("[A-Za-z0-9][A-Za-z0-9._:-]{0,127}")

        private fun jsonEquivalent(first: Any?, second: Any?): Boolean {
            if (first is JSONObject && second is JSONObject) {
                val firstKeys = first.keys().asSequence().toSet()
                val secondKeys = second.keys().asSequence().toSet()
                return firstKeys == secondKeys && firstKeys.all { key ->
                    jsonEquivalent(first.opt(key), second.opt(key))
                }
            }
            if (first is JSONArray && second is JSONArray) {
                if (first.length() != second.length()) return false
                return (0 until first.length()).all { index -> jsonEquivalent(first.opt(index), second.opt(index)) }
            }
            return first == second
        }

        internal fun validateSchemaDefinition(schema: JSONObject, depth: Int) {
            require(depth <= 12) { "MCP schema is nested too deeply." }
            val type = schema.optString("type")
            require(type in setOf("object", "array", "string", "number", "integer", "boolean", "null")) {
                "MCP schema must declare a supported type."
            }
            schema.optJSONArray("enum")?.let { require(it.length() in 1..64) { "MCP schema enum is invalid." } }
            when (type) {
                "object" -> {
                    val properties = schema.optJSONObject("properties")
                        ?: throw IllegalArgumentException("MCP object schema must declare properties.")
                    require(properties.length() <= 128) { "MCP schema has too many properties." }
                    val required = schema.optJSONArray("required") ?: JSONArray()
                    require(required.length() <= 128) { "MCP schema required list is too large." }
                    for (i in 0 until required.length()) {
                        val name = required.optString(i)
                        require(name.isNotBlank() && properties.has(name)) { "MCP schema has an undeclared required property." }
                    }
                    val keys = properties.keys()
                    while (keys.hasNext()) {
                        val child = properties.optJSONObject(keys.next())
                            ?: throw IllegalArgumentException("MCP property schema must be an object.")
                        validateSchemaDefinition(child, depth + 1)
                    }
                }
                "array" -> {
                    val items = schema.optJSONObject("items")
                        ?: throw IllegalArgumentException("MCP array schema must declare items.")
                    validateSchemaDefinition(items, depth + 1)
                }
            }
        }

        internal fun validateValue(value: Any?, schema: JSONObject, depth: Int = 0) {
            require(depth <= 12) { "MCP arguments are nested too deeply." }
            val type = schema.getString("type")
            val valid = when (type) {
                "object" -> value is JSONObject
                "array" -> value is JSONArray
                "string" -> value is String
                "number" -> value is Number
                "integer" -> value is Int || value is Long
                "boolean" -> value is Boolean
                "null" -> value == JSONObject.NULL || value == null
                else -> false
            }
            require(valid) { "MCP argument does not match schema type '$type'." }
            schema.optJSONArray("enum")?.let { allowed ->
                require((0 until allowed.length()).any { allowed.opt(it) == value }) { "MCP argument is not an allowed enum value." }
            }
            when (value) {
                is JSONObject -> {
                    val properties = schema.optJSONObject("properties") ?: JSONObject()
                    val required = schema.optJSONArray("required") ?: JSONArray()
                    for (i in 0 until required.length()) {
                        require(value.has(required.getString(i))) { "Required MCP argument '${required.getString(i)}' is missing." }
                    }
                    val keys = value.keys()
                    while (keys.hasNext()) {
                        val name = keys.next()
                        val childSchema = properties.optJSONObject(name)
                        if (childSchema == null) {
                            require(!schema.optBoolean("additionalProperties", true)) { "Undeclared MCP argument '$name' is not allowed." }
                        } else validateValue(value.opt(name), childSchema, depth + 1)
                    }
                }
                is JSONArray -> {
                    val items = schema.optJSONObject("items") ?: JSONObject()
                    require(value.length() <= schema.optInt("maxItems", 256)) { "MCP array argument is too large." }
                    for (i in 0 until value.length()) validateValue(value.opt(i), items, depth + 1)
                }
                is String -> require(value.length >= schema.optInt("minLength", 0) &&
                    value.length <= schema.optInt("maxLength", 16_384)) {
                    "MCP string argument length is outside the allowed range."
                }
                is Number -> {
                    schema.optDouble("minimum").takeIf { !it.isNaN() }?.let { require(value.toDouble() >= it) { "MCP number is below its minimum." } }
                    schema.optDouble("maximum").takeIf { !it.isNaN() }?.let { require(value.toDouble() <= it) { "MCP number exceeds its maximum." } }
                }
            }
        }
    }
}
