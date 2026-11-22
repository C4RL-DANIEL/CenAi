package com.cenai.app

import org.json.JSONArray
import org.json.JSONObject
import java.time.ZonedDateTime
import java.time.LocalDate
import java.util.UUID
import java.util.Base64
import java.time.format.DateTimeFormatter

enum class ToolRisk { SAFE, SENSITIVE, DESTRUCTIVE }
data class AgentTool(val name: String, val description: String, val risk: ToolRisk, val parameters: JSONObject)
data class ToolExecution(val toolName: String, val arguments: JSONObject, val output: String, val requiresApproval: Boolean, val completed: Boolean)

class AgentRuntime(
    private val enabledTools: Set<String>? = null,
    private val arithmeticEvaluator: (String) -> Double = SafeArithmetic::evaluate,
    private val policyEvaluator: (String, String) -> String = { name, enabledJson ->
        NativeCore.evaluateToolPolicy(name, enabledJson)
    },
    private val toolCallParser: (String) -> String = { response ->
        NativeCore.parseAgentToolCall(response)
    }
) {
    private val tools = listOf(
        AgentTool("current_time", "Get the current local date and time.", ToolRisk.SAFE, JSONObject().put("type", "object").put("properties", JSONObject())),
        AgentTool("calculate", "Evaluate a basic arithmetic expression.", ToolRisk.SAFE, JSONObject().put("type", "object").put("properties", JSONObject().put("expression", JSONObject().put("type", "string"))).put("required", JSONArray().put("expression"))),
        AgentTool("count_words", "Count words and characters in supplied text.", ToolRisk.SAFE, JSONObject().put("type", "object").put("properties", JSONObject().put("text", JSONObject().put("type", "string"))).put("required", JSONArray().put("text"))),
        AgentTool("format_json", "Validate and pretty-print JSON supplied by the user.", ToolRisk.SAFE, JSONObject().put("type", "object").put("properties", JSONObject().put("json", JSONObject().put("type", "string"))).put("required", JSONArray().put("json"))),
        AgentTool("current_date", "Get today's local date.", ToolRisk.SAFE, JSONObject().put("type", "object").put("properties", JSONObject())),
        AgentTool("generate_uuid", "Generate a random UUID.", ToolRisk.SAFE, JSONObject().put("type", "object").put("properties", JSONObject())),
        AgentTool("text_stats", "Return line, word and character statistics for text.", ToolRisk.SAFE, JSONObject().put("type", "object").put("properties", JSONObject().put("text", JSONObject().put("type", "string"))).put("required", JSONArray().put("text"))),
        AgentTool("base64_encode", "Encode supplied text as Base64.", ToolRisk.SAFE, JSONObject().put("type", "object").put("properties", JSONObject().put("text", JSONObject().put("type", "string"))).put("required", JSONArray().put("text"))),
        AgentTool("base64_decode", "Decode a Base64 string as UTF-8 text.", ToolRisk.SAFE, JSONObject().put("type", "object").put("properties", JSONObject().put("value", JSONObject().put("type", "string"))).put("required", JSONArray().put("value")))
,
        AgentTool("open_url", "Open an HTTP or HTTPS link in Android after the user approves it.", ToolRisk.SENSITIVE, JSONObject().put("type", "object").put("properties", JSONObject().put("url", JSONObject().put("type", "string"))).put("required", JSONArray().put("url"))),
        AgentTool("open_dialer", "Open the Android dialer with a phone number filled in; does not place a call.", ToolRisk.SENSITIVE, JSONObject().put("type", "object").put("properties", JSONObject().put("number", JSONObject().put("type", "string"))).put("required", JSONArray().put("number"))),
        AgentTool("share_text", "Open Android's share sheet with text for the user to choose where to send.", ToolRisk.SENSITIVE, JSONObject().put("type", "object").put("properties", JSONObject().put("text", JSONObject().put("type", "string"))).put("required", JSONArray().put("text"))),
        AgentTool("open_app_settings", "Open CenAI's Android app settings.", ToolRisk.SENSITIVE, JSONObject().put("type", "object").put("properties", JSONObject()))
    )
    fun availableTools(): List<AgentTool> = tools.filter { enabledTools == null || it.name in enabledTools }
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
            val output = when (name) {
                "current_time" -> ZonedDateTime.now().format(DateTimeFormatter.ISO_OFFSET_DATE_TIME)
                "calculate" -> arithmeticEvaluator(args.getString("expression")).toString()
                "count_words" -> { val value = args.getString("text"); JSONObject().put("words", value.trim().split(Regex("\\s+")).count { it.isNotBlank() }).put("characters", value.length).put("characters_without_spaces", value.count { !it.isWhitespace() }).toString() }
                "format_json" -> JSONObject(args.getString("json")).toString(2)
                "current_date" -> LocalDate.now().toString()
                "generate_uuid" -> UUID.randomUUID().toString()
                "text_stats" -> {
                    val value = args.getString("text")
                    JSONObject().put("lines", if (value.isEmpty()) 0 else value.lines().size)
                        .put("words", value.trim().split(Regex("\\s+")).count { it.isNotBlank() })
                        .put("characters", value.length).toString()
                }
                "base64_encode" -> Base64.getEncoder().encodeToString(args.getString("text").toByteArray(Charsets.UTF_8))
                "base64_decode" -> String(Base64.getDecoder().decode(args.getString("value")), Charsets.UTF_8)
                else -> "No executor registered."
            }
            ToolExecution(name, args, output, false, true)
        } catch (error: Exception) { ToolExecution(name, args, "Tool error: " + (error.message ?: "invalid arguments"), false, false) }
    }
}

private object SafeArithmetic {
    fun evaluate(source: String): Double {
        val result = NativeCore.evaluateArithmetic(source)
        require(!result.startsWith("ERROR:")) { result.removePrefix("ERROR:") }
        return result.toDouble()
    }
}
