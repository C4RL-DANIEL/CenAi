package com.cenai.app

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class McpClientTest {
    @Test
    fun acceptsBoundedObjectSchema() {
        val schema = JSONObject()
            .put("type", "object")
            .put("additionalProperties", false)
            .put("properties", JSONObject().put("query", JSONObject().put("type", "string").put("maxLength", 200)))
            .put("required", JSONArray().put("query"))
        McpClient.validateSchemaDefinition(schema, 0)
        McpClient.validateValue(JSONObject().put("query", "hello"), schema)
    }

    @Test
    fun rejectsMissingRequiredAndWrongTypes() {
        val schema = JSONObject()
            .put("type", "object")
            .put("properties", JSONObject().put("count", JSONObject().put("type", "integer")))
            .put("required", JSONArray().put("count"))
        assertThrows(IllegalArgumentException::class.java) {
            McpClient.validateValue(JSONObject(), schema)
        }
        assertThrows(IllegalArgumentException::class.java) {
            McpClient.validateValue(JSONObject().put("count", "not-an-integer"), schema)
        }
    }

    @Test
    fun rejectsUnknownArgumentsWhenSchemaForbidsThem() {
        val schema = JSONObject()
            .put("type", "object")
            .put("additionalProperties", false)
            .put("properties", JSONObject().put("name", JSONObject().put("type", "string")))
        assertThrows(IllegalArgumentException::class.java) {
            McpClient.validateValue(JSONObject().put("name", "ok").put("extra", true), schema)
        }
    }

    @Test
    fun rejectsUnsupportedAndDeepSchemas() {
        assertThrows(IllegalArgumentException::class.java) {
            McpClient.validateSchemaDefinition(JSONObject().put("type", "function"), 0)
        }
        var nested = JSONObject().put("type", "string")
        repeat(14) { nested = JSONObject().put("type", "array").put("items", nested) }
        val deep = nested
        assertThrows(IllegalArgumentException::class.java) {
            McpClient.validateSchemaDefinition(deep, 0)
        }
        assertTrue(deep.has("items"))
    }
}
