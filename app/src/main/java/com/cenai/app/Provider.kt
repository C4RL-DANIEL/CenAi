package com.cenai.app

import org.json.JSONObject

enum class Provider {
    OPENAI,
    DEEPSEEK,
    CLAUDE,
    GEMINI,
    CENAI_CLOUD,
    CUSTOM;

    private val configuration: JSONObject by lazy(LazyThreadSafetyMode.PUBLICATION) {
        JSONObject(NativeCore.providerMetadata(name)).also { value ->
            require(value.optString("error").isBlank()) { value.optString("error", "Provider configuration is unavailable.") }
        }
    }

    val label: String get() = configuration.getString("label")
    val model: String get() = configuration.getString("model")
    val baseUrl: String get() = configuration.getString("base_url")

    companion object {
        fun parse(value: String?) = entries.firstOrNull { it.name == value } ?: OPENAI
    }
}

data class ChatMessage(val role: String, val content: String)
