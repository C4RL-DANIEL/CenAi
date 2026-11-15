package com.cenai.app

enum class Provider(val label: String, val model: String, val baseUrl: String) {
    OPENAI("OpenAI", "gpt-4.1-mini", "https://api.openai.com/v1"),
    DEEPSEEK("DeepSeek", "deepseek-chat", "https://api.deepseek.com"),
    CLAUDE("Claude", "claude-3-5-haiku-latest", "https://api.anthropic.com"),
    GEMINI("Gemini", "gemini-2.0-flash", "https://generativelanguage.googleapis.com"),
    CUSTOM("Custom OpenAI-compatible", "", "https://");

    companion object {
        fun parse(value: String?) = entries.firstOrNull { it.name == value } ?: OPENAI
    }
}

data class ChatMessage(val role: String, val content: String)
