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
        val normalized = JSONArray(NativeCore.normalizeConversations("""[{"id":"x","title":"Test","messages":[{"role":"user","content":"hello"}]}]"""))
        assertEquals("x", normalized.getJSONObject(0).getString("id"))
        val count = JSONObject(NativeCore.executeSafeTool("count_words", """{"text":"one two"}""", "", ""))
        assertEquals(2, JSONObject(count.getString("output")).getInt("words"))
        assertEquals("Q2VuQUk=", JSONObject(NativeCore.executeSafeTool("base64_encode", """{"text":"CenAI"}""", "", "")).getString("output"))

        val catalog = JSONArray(NativeCore.listAgentTools("null"))
        assertTrue(catalog.length() >= 13)
        val policy = JSONObject(NativeCore.evaluateToolPolicy("open_dialer", "null"))
        assertTrue(policy.getBoolean("allowed"))
        assertTrue(policy.getBoolean("requires_approval"))

        val toolCall = JSONObject(
            NativeCore.parseAgentToolCall(
                "Thinking [[CENAI_TOOL_CALL]]{\"name\":\"calculate\",\"arguments\":{\"expression\":\"2+2\"}}[[/CENAI_TOOL_CALL]]"
            )
        )
        assertEquals("calculate", toolCall.getString("name"))
        assertEquals("2+2", toolCall.getJSONObject("arguments").getString("expression"))

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
