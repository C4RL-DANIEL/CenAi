package com.cenai.app

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AgentRuntimeTest {
    private fun testPolicy(name: String, enabledJson: String): String {
        val enabled = if (enabledJson == "null") null else org.json.JSONArray(enabledJson)
        val allowedByName = name in setOf("current_time", "calculate", "count_words", "format_json", "current_date", "generate_uuid", "text_stats", "base64_encode", "base64_decode", "open_url", "open_dialer", "share_text", "open_app_settings")
        val enabledForUser = enabled == null || (0 until enabled.length()).any { enabled.optString(it) == name }
        val sensitive = name in setOf("open_url", "open_dialer", "share_text", "open_app_settings")
        return JSONObject().put("allowed", allowedByName && enabledForUser)
            .put("risk", if (sensitive) "sensitive" else "safe")
            .put("requires_approval", allowedByName && enabledForUser && sensitive).toString()
    }

    @Test
    fun sensitiveDeviceActionCannotRunWithoutApproval() {
        val runtime = AgentRuntime(policyEvaluator = ::testPolicy)
        val result = runtime.execute("open_dialer", JSONObject().put("number", "+15551234567"))
        assertTrue(result.requiresApproval)
        assertFalse(result.completed)
    }

    @Test
    fun disabledToolIsRejected() {
        val runtime = AgentRuntime(enabledTools = setOf("current_time"), policyEvaluator = ::testPolicy)
        val result = runtime.execute("share_text", JSONObject().put("text", "hello"))
        assertFalse(result.requiresApproval)
        assertFalse(result.completed)
        assertTrue(result.output.contains("disabled"))
    }

    @Test
    fun safeCalculatorExecutesAllowListedArithmetic() {
        val runtime = AgentRuntime(safeToolExecutor = { name, args, _, _ ->
            assertEquals("calculate", name)
            assertEquals("2 + 3 * 4", args.getString("expression"))
            "14.0"
        }, policyEvaluator = ::testPolicy)
        val result = runtime.execute("calculate", JSONObject().put("expression", "2 + 3 * 4"))
        assertTrue(result.completed)
        assertEquals("14.0", result.output)
    }

    @Test
    fun toolCallsRequireTheExpectedStructuredMarker() {
        val runtime = AgentRuntime(
            toolCallParser = { response ->
                if (response.contains("[[CENAI_TOOL_CALL]]")) {
                    JSONObject().put("name", "current_date").put("arguments", JSONObject()).toString()
                } else ""
            }
        )
        val parsed = runtime.parseToolCall("thinking\n[[CENAI_TOOL_CALL]]{\"name\":\"current_date\",\"arguments\":{}}[[/CENAI_TOOL_CALL]]")
        assertNotNull(parsed)
        assertEquals("current_date", parsed!!.first)
    }
}
