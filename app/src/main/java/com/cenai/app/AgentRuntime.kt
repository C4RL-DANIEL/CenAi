package com.cenai.app

import org.json.JSONArray
import org.json.JSONObject
import java.time.ZonedDateTime
import java.time.LocalDate
import java.time.format.DateTimeFormatter

enum class ToolRisk { SAFE, SENSITIVE, DESTRUCTIVE }
data class AgentTool(val name: String, val description: String, val risk: ToolRisk, val parameters: JSONObject)
data class ToolExecution(val toolName: String, val arguments: JSONObject, val output: String, val requiresApproval: Boolean, val completed: Boolean)

class AgentRuntime(
    private val enabledTools: Set<String>? = null,
    private val safeToolExecutor: (String, JSONObject, String, String) -> String = { name, args, localTime, localDate ->
        val result = JSONObject(NativeCore.executeSafeTool(name, args.toString(), localTime, localDate))
        val error = result.optString("error")
        require(error.isBlank()) { error }
        result.optString("output")
    },
    private val policyEvaluator: (String, String) -> String = { name, enabledJson ->
        NativeCore.evaluateToolPolicy(name, enabledJson)
    },
    private val toolCallParser: (String) -> String = { response ->
        NativeCore.parseAgentToolCall(response)
    }
) {
    fun availableTools(): List<AgentTool> {
        val enabledJson = enabledTools?.let { JSONArray().apply { it.forEach(::put) }.toString() } ?: "null"
        val encoded = NativeCore.listAgentTools(enabledJson)
        val error = runCatching { JSONObject(encoded).optString("error") }.getOrDefault("")
        require(error.isBlank()) { error.ifBlank { "Rust tool catalog is unavailable." } }
        val catalog = JSONArray(encoded)
        return List(catalog.length()) { index ->
            val item = catalog.getJSONObject(index)
            AgentTool(
                item.getString("name"),
                item.optString("description"),
                ToolRisk.valueOf(item.optString("risk", "SAFE")),
                item.optJSONObject("parameters") ?: JSONObject()
            )
        }
    }
    fun parseToolCall(response: String): Pair<String, JSONObject>? {
        val encoded = runCatching { toolCallParser(response) }.getOrDefault("")
        if (encoded.isBlank()) return null
        val payload = runCatching { JSONObject(encoded) }.getOrNull() ?: return null
        if (payload.optString("error").isNotBlank()) return null
        val name = payload.optString("name").takeIf { it.isNotBlank() } ?: return null
        return name to payload.optJSONObject("arguments").orEmptyJson()
    }
    private fun JSONObject?.orEmptyJson() = this ?: JSONObject()
    fun execute(name: String, args: JSONObject, approvalGranted: Boolean = false): ToolExecution {
        val enabledJson = enabledTools?.let { org.json.JSONArray().apply { it.forEach(::put) }.toString() } ?: "null"
        val policy = runCatching { JSONObject(policyEvaluator(name, enabledJson)) }.getOrElse {
            return ToolExecution(name, args, "Tool policy could not be verified. Action blocked.", false, false)
        }
        if (!policy.optBoolean("allowed")) return ToolExecution(name, args, "Tool is disabled or not allow-listed.", false, false)
        val requiresApproval = policy.optBoolean("requires_approval")
        if (requiresApproval && !approvalGranted) return ToolExecution(name, args, "User approval is required.", true, false)
        return try {
            val localTime = ZonedDateTime.now().format(DateTimeFormatter.ISO_OFFSET_DATE_TIME)
            val localDate = LocalDate.now().toString()
            val output = when (name) {
                "current_time", "calculate", "count_words", "format_json", "current_date",
                "generate_uuid", "text_stats", "base64_encode", "base64_decode" ->
                    safeToolExecutor(name, args, localTime, localDate)
                else -> "No executor registered."
            }
            ToolExecution(name, args, output, false, true)
        } catch (error: Exception) { ToolExecution(name, args, "Tool error: " + (error.message ?: "invalid arguments"), false, false) }
    }
}
