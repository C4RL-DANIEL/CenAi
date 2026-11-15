package com.cenai.app

import org.json.JSONArray
import org.json.JSONObject
import java.time.ZonedDateTime
import java.time.format.DateTimeFormatter

enum class ToolRisk { SAFE, SENSITIVE, DESTRUCTIVE }
data class AgentTool(val name: String, val description: String, val risk: ToolRisk, val parameters: JSONObject)
data class ToolExecution(val toolName: String, val arguments: JSONObject, val output: String, val requiresApproval: Boolean, val completed: Boolean)

class AgentRuntime {
    private val tools = listOf(
        AgentTool("current_time", "Get the current local date and time.", ToolRisk.SAFE, JSONObject().put("type", "object").put("properties", JSONObject())),
        AgentTool("calculate", "Evaluate a basic arithmetic expression.", ToolRisk.SAFE, JSONObject().put("type", "object").put("properties", JSONObject().put("expression", JSONObject().put("type", "string"))).put("required", JSONArray().put("expression"))),
        AgentTool("count_words", "Count words and characters in supplied text.", ToolRisk.SAFE, JSONObject().put("type", "object").put("properties", JSONObject().put("text", JSONObject().put("type", "string"))).put("required", JSONArray().put("text"))),
        AgentTool("format_json", "Validate and pretty-print JSON supplied by the user.", ToolRisk.SAFE, JSONObject().put("type", "object").put("properties", JSONObject().put("json", JSONObject().put("type", "string"))).put("required", JSONArray().put("json")))
    )
    fun availableTools(): List<AgentTool> = tools
    fun parseToolCall(response: String): Pair<String, JSONObject>? {
        val marker = "[[CENAI_TOOL_CALL]]"
        val closing = "[[/CENAI_TOOL_CALL]]"
        val start = response.indexOf(marker)
        if (start < 0) return null
        val end = response.indexOf(closing, start + marker.length)
        if (end < 0) return null
        val payload = JSONObject(response.substring(start + marker.length, end).trim())
        return payload.getString("name") to payload.optJSONObject("arguments").orEmptyJson()
    }
    private fun JSONObject?.orEmptyJson() = this ?: JSONObject()
    fun execute(name: String, args: JSONObject, approvalGranted: Boolean = false): ToolExecution {
        val tool = tools.firstOrNull { it.name == name } ?: return ToolExecution(name, args, "Tool is not allow-listed.", false, false)
        if (tool.risk != ToolRisk.SAFE && !approvalGranted) return ToolExecution(name, args, "User approval is required.", true, false)
        return try {
            val output = when (name) {
                "current_time" -> ZonedDateTime.now().format(DateTimeFormatter.ISO_OFFSET_DATE_TIME)
                "calculate" -> SafeArithmetic.evaluate(args.getString("expression")).toString()
                "count_words" -> { val value = args.getString("text"); JSONObject().put("words", value.trim().split(Regex("\\s+")).count { it.isNotBlank() }).put("characters", value.length).put("characters_without_spaces", value.count { !it.isWhitespace() }).toString() }
                "format_json" -> JSONObject(args.getString("json")).toString(2)
                else -> "No executor registered."
            }
            ToolExecution(name, args, output, false, true)
        } catch (error: Exception) { ToolExecution(name, args, "Tool error: " + (error.message ?: "invalid arguments"), false, false) }
    }
}

private object SafeArithmetic {
    fun evaluate(source: String): Double {
        require(source.length in 1..200) { "Expression length must be 1–200 characters." }
        val parser = Parser(source); val result = parser.expression(); parser.spaces(); require(parser.done()) { "Unexpected input." }; require(result.isFinite()) { "Invalid result." }; return result
    }
    private class Parser(private val s: String) {
        private var i = 0
        fun done() = i >= s.length
        fun spaces() { while (i < s.length && s[i].isWhitespace()) i++ }
        private fun take(c: Char): Boolean { if (i < s.length && s[i] == c) { i++; return true }; return false }
        fun expression(): Double { var v = term(); while (true) { spaces(); if (take('+')) v += term() else if (take('-')) v -= term() else return v } }
        private fun term(): Double { var v = factor(); while (true) { spaces(); if (take('*')) v *= factor() else if (take('/')) { val d = factor(); require(d != 0.0) { "Division by zero." }; v /= d } else if (take('%')) { val d = factor(); require(d != 0.0) { "Division by zero." }; v %= d } else return v } }
        private fun factor(): Double { spaces(); if (take('+')) return factor(); if (take('-')) return -factor(); if (take('(')) { val v = expression(); spaces(); require(take(')')) { "Missing closing parenthesis." }; return v }; val start = i; while (i < s.length && (s[i].isDigit() || s[i] == '.')) i++; require(i > start) { "Expected a number." }; return s.substring(start, i).toDouble() }
    }
}
