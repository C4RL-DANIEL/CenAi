package com.cenai.app

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AgentRuntimeTest {
    @Test
    fun sensitiveDeviceActionCannotRunWithoutApproval() {
        val runtime = AgentRuntime()
        val result = runtime.execute("open_dialer", JSONObject().put("number", "+15551234567"))
        assertTrue(result.requiresApproval)
        assertFalse(result.completed)
    }

    @Test
    fun disabledToolIsRejected() {
        val runtime = AgentRuntime(enabledTools = setOf("current_time"))
        val result = runtime.execute("share_text", JSONObject().put("text", "hello"))
        assertFalse(result.requiresApproval)
        assertFalse(result.completed)
        assertTrue(result.output.contains("disabled"))
    }

    @Test
    fun safeCalculatorExecutesAllowListedArithmetic() {
        val runtime = AgentRuntime(arithmeticEvaluator = { expression ->
            assertEquals("2 + 3 * 4", expression)
            14.0
        })
        val result = runtime.execute("calculate", JSONObject().put("expression", "2 + 3 * 4"))
        assertTrue(result.completed)
        assertEquals("14.0", result.output)
    }

    @Test
    fun toolCallsRequireTheExpectedStructuredMarker() {
        val runtime = AgentRuntime()
        val parsed = runtime.parseToolCall("thinking\n[[CENAI_TOOL_CALL]]{\"name\":\"current_date\",\"arguments\":{}}[[/CENAI_TOOL_CALL]]")
        assertNotNull(parsed)
        assertEquals("current_date", parsed!!.first)
    }
}
