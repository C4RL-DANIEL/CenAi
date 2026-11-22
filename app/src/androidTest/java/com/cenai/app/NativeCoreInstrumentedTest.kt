package com.cenai.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class NativeCoreInstrumentedTest {
    @Test
    fun rustJniExportsCoverChatAgentProviderAndLocalRetrieval() {
        assertEquals("14", NativeCore.evaluateArithmetic("2 + 3 * 4"))
        val systemContext = NativeCore.buildSystemContext("rules", "memory", "facts", "tools")
        assertTrue(systemContext.indexOf("rules") < systemContext.indexOf("memory"))
        assertTrue(systemContext.contains("ON-DEVICE RETRIEVAL CONTEXT"))
        val normalized = JSONArray(NativeCore.normalizeConversations("""[{"id":"x","title":"Test","messages":[{"role":"user","content":"hello"}]}]"""))
        assertEquals("x", normalized.getJSONObject(0).getString("id"))
        val count = JSONObject(NativeCore.executeSafeTool("count_words", """{"text":"one two"}""", "", ""))
        assertEquals(2, JSONObject(count.getString("output")).getInt("words"))
        assertEquals("Q2VuQUk=", JSONObject(NativeCore.executeSafeTool("base64_encode", """{"text":"CenAI"}""", "", "")).getString("output"))

        val catalog = JSONArray(NativeCore.listAgentTools("null"))
        assertTrue(NativeCore.buildToolGuide("null", true, true).contains("approval tap"))
        assertTrue(NativeCore.buildToolGuide("""["calculate"]""", true, false).contains("cannot execute Android device actions"))
        val synthesis = NativeCore.buildSynthesisPrompt("Compare", """["A: first","B: second"]""")
        assertTrue(synthesis.contains("A: first") && synthesis.contains("B: second"))
        assertTrue(catalog.length() >= 13)
        val policy = JSONObject(NativeCore.evaluateToolPolicy("open_dialer", "null"))
        assertTrue(policy.getBoolean("allowed"))
        assertTrue(policy.getBoolean("requires_approval"))

        val stepDecision = JSONObject(NativeCore.parseAgentStep("Visible text [[CENAI_TOOL_CALL]]{\"name\":\"calculate\",\"arguments\":{\"expression\":\"1+1\"}}[[/CENAI_TOOL_CALL]]"))
        assertEquals("tool", stepDecision.getString("kind"))
        assertEquals("Visible text", stepDecision.getString("visible_text"))
        val toolCall = JSONObject(
            NativeCore.parseAgentToolCall(
                "Thinking [[CENAI_TOOL_CALL]]{\"name\":\"calculate\",\"arguments\":{\"expression\":\"2+2\"}}[[/CENAI_TOOL_CALL]]"
            )
        )
        assertEquals("calculate", toolCall.getString("name"))
        assertEquals("2+2", toolCall.getJSONObject("arguments").getString("expression"))

        val targets = JSONArray(NativeCore.selectProviderTargets("OPENAI", true, false, """["OPENAI","GEMINI"]"""))
        assertEquals(2, targets.length())
        assertEquals("OPENAI", targets.getString(0))
        val transport = JSONObject(NativeCore.buildProviderRequestConfig("GEMINI", "gemini-test", "key", false, "", "{}"))
        assertTrue(transport.getString("url").contains("gemini-test"))
        assertEquals("key", transport.getJSONObject("headers").getString("x-goog-api-key"))
        val openAiProvider = JSONObject(NativeCore.providerMetadata("OPENAI"))
        assertEquals("OpenAI", openAiProvider.getString("label"))
        assertEquals("gpt-4.1-mini", openAiProvider.getString("model"))
        assertEquals(1, NativeCore.compareVersions("v1.2.0+build.1", "1.1.9"))
        assertEquals(-1, NativeCore.compareVersions("1.0.0-beta.1", "1.0.0"))
        assertEquals(0, NativeCore.compareVersions("1.0.0+first", "1.0.0+second"))
        assertEquals("cloud_access_token", NativeCore.providerCredentialSource("CENAI_CLOUD", true))
        assertEquals("google_oauth", NativeCore.providerCredentialSource("GEMINI", true))
        assertEquals("provider_api_key", NativeCore.providerCredentialSource("OPENAI", false))
        val quotaReset = JSONObject(NativeCore.evaluateDailyUsage("2026-11-22", "2026-11-21", "9", "2"))
        assertTrue(quotaReset.getBoolean("allowed"))
        assertTrue(quotaReset.getBoolean("reset"))
        assertEquals(1, quotaReset.getInt("next_count"))
        val quotaBlocked = JSONObject(NativeCore.evaluateDailyUsage("today", "today", "2", "2"))
        assertTrue(!quotaBlocked.getBoolean("allowed"))
        assertEquals(4, NativeCore.maxAgentSteps())
        assertTrue(NativeCore.buildToolContinuationPrompt("calculate", "14", false).contains("same tool-call format"))
        assertTrue(NativeCore.buildToolContinuationPrompt("calculate", "14", true).contains("final answer"))
        assertTrue(NativeCore.agentStepLimitMessage("last", false).contains("Ask me to continue"))
        assertTrue(NativeCore.googleOAuthAccessTokenUsable("access", "61001", 1000L))
        assertTrue(!NativeCore.googleOAuthAccessTokenUsable("access", "61000", 1000L))
        assertTrue(!NativeCore.googleOAuthAccessTokenUsable("", "999999", 1000L))
        assertEquals("first title", NativeCore.conversationTitle("[{\"role\":\"user\",\"content\":\"first title\"}]"))

        val telemetryConsent = JSONObject(NativeCore.buildConsentTelemetryPayload(true, false))
        assertTrue(telemetryConsent.getBoolean("usageAnalytics"))
        assertTrue(!telemetryConsent.getBoolean("guardian"))
        val telemetryUsage = JSONObject(NativeCore.buildUsageTelemetryPayload("local", "offline", -1, 3, -1L, "success", "x".repeat(100)))
        assertEquals(0, telemetryUsage.getInt("inputTokens"))
        assertEquals(0, telemetryUsage.getLong("latencyMs"))
        assertEquals(80, telemetryUsage.getString("errorCode").length)
        val telemetryGuardian = JSONObject(NativeCore.buildGuardianTelemetryPayload("1.0", "16", 150, -1L, 200L, "wifi", "x".repeat(80)))
        assertEquals(100, telemetryGuardian.getInt("batteryPercent"))
        assertEquals(64, telemetryGuardian.getString("crashCategory").length)

        val primarySettings = JSONObject(NativeCore.resolveProviderSettings("OPENAI", "OPENAI", "custom-model", "https://custom.example/v1", "", ""))
        assertEquals("custom-model", primarySettings.getString("model"))
        val deepSeekSettings = JSONObject(NativeCore.resolveProviderSettings("DEEPSEEK", "OPENAI", "", "", "deepseek-chat", ""))
        assertEquals("deepseek-flash", deepSeekSettings.getString("model"))

        val messages = JSONArray().put(JSONObject().put("role", "user").put("content", "Hello"))
        val payload = JSONObject(
            NativeCore.buildProviderPayload("OPENAI", "test-model", 128, messages.toString(), "{}")
        )
        assertEquals("test-model", payload.getString("model"))
        assertEquals("Hello", payload.getJSONArray("messages").getJSONObject(0).getString("content"))

        assertEquals(
            "Hello",
            NativeCore.parseProviderDelta("OPENAI", """{"choices":[{"delta":{"content":"Hello"}}]}""")
        )
        assertEquals("Invalid API key", NativeCore.parseProviderError("""{"error":{"message":"Invalid API key"}}"""))
        assertEquals("Hello from cloud", JSONObject(NativeCore.parseCloudResponse("""{"choices":[{"message":{"content":"Hello from cloud"}}]}""")).getString("content"))
        assertEquals("user@example.com", JSONObject(NativeCore.buildAuthPayload("signup", " user@example.com ", "password123", "")).getString("email"))
        assertEquals("OK", NativeCore.validateEmailOtp("123456"))
        assertEquals("OK", NativeCore.validateGoogleOAuthClientId("abc.apps.googleusercontent.com"))
        val pkce = JSONObject(NativeCore.generateOAuthPkce())
        assertEquals(64, pkce.getString("verifier").length)
        assertEquals(32, pkce.getString("state").length)
        assertEquals(43, pkce.getString("challenge").length)
        val oauthCallback = JSONObject(NativeCore.validateGoogleOAuthCallback("com.cenai.app", "/oauth2redirect", "state", "state", "", "code"))
        assertEquals("code", oauthCallback.getString("code"))
        val oauthToken = JSONObject(NativeCore.parseGoogleOAuthToken("""{"access_token":"token","expires_in":3600}""", 1000L))
        assertEquals("3601000", oauthToken.getString("expires_at"))
        assertTrue(NativeCore.validateEmailOtp("12x456") != "OK")
        val session = JSONObject(NativeCore.parseAuthSession("""{"access_token":"a","refresh_token":"r","user":{"id":"u-1","email":"user@example.com"}}"""))
        assertEquals("u-1", session.getString("user_id"))
        assertEquals("Describe [Image attached] please", NativeCore.stripImageMarkers("Describe [[CENAI_IMAGE|image/png|YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXo=]] please"))
        assertTrue(NativeCore.containsImageMarker("[[CENAI_IMAGE|image/png|YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXo=]]"))

        val conversation = Conversation(
            id = "local-memory",
            title = "Private notes",
            messages = mutableListOf(
                ChatMessage("user", "My private local memory contains the phrase account separation and privacy.")
            )
        )
        val retrieved = LocalKnowledgeStore().retrieve(
            "local memory privacy",
            listOf(conversation),
            emptyList()
        )
        assertTrue(retrieved.contains("Private notes"))
        assertTrue(retrieved.contains("account separation"))
    }
}
