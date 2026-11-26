package com.cenai.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.util.UUID

@RunWith(AndroidJUnit4::class)
class McpServerRegistryInstrumentedTest {
    @Test
    fun savedServersAndBearerCredentialsAreAccountScoped() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val vault = SecureKeyStore(context)
        val firstId = UUID.randomUUID().toString()
        val secondId = UUID.randomUUID().toString()
        val schema = JSONObject()
            .put("type", "object")
            .put("properties", JSONObject().put("query", JSONObject().put("type", "string")))
            .put("required", JSONArray().put("query"))
        val capability = JSONObject()
            .put("name", "search")
            .put("description", "Search user-approved records")
            .put("inputSchema", schema)
            .put("requiredScopes", JSONArray().put("read"))
            .put("requiresSensitiveRead", false)
            .put("requiresExternalWrite", false)
        val registry = JSONArray().put(
            JSONObject()
                .put("serverId", "private-docs")
                .put("endpointUrl", "https://mcp.example.com/mcp")
                .put("publisher", "Example Publisher")
                .put("publisherVerified", false)
                .put("protocolVersion", McpClient.PROTOCOL_VERSION)
                .put("capabilities", JSONArray().put(capability))
        ).toString()
        try {
            vault.putUserSecret(firstId, "mcp_server_registry_v1", registry)
            vault.putUserSecret(firstId, "mcp_auth_private-docs", "first-account-token")
            val first = McpServerRegistry(vault, firstId)
            val second = McpServerRegistry(vault, secondId)
            assertEquals(1, first.servers().size)
            assertEquals("private-docs", first.servers().single().serverId)
            assertEquals("search", first.availableTools().single().name)
            assertTrue(second.servers().isEmpty())
            assertEquals("mcp__private-docs__search", McpServerRegistry.toolName("private-docs", "search"))
            assertEquals("first-account-token", vault.getUserSecret(firstId, "mcp_auth_private-docs"))
            assertEquals("", vault.getUserSecret(secondId, "mcp_auth_private-docs"))
        } finally {
            vault.putUserSecret(firstId, "mcp_server_registry_v1", "")
            vault.putUserSecret(firstId, "mcp_auth_private-docs", "")
        }
    }
}
