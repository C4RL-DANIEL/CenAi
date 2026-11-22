package com.cenai.app

import org.json.JSONArray
import org.json.JSONObject

/**
 * Android adapter for private on-device retrieval.
 * Ranking, tokenization, filtering and excerpt selection run in the Rust core.
 */
class LocalKnowledgeStore {
    fun retrieve(
        query: String,
        conversations: List<Conversation>,
        currentMessages: List<ChatMessage>,
        maxResults: Int = 5,
        maxCharacters: Int = 4200
    ): String {
        val saved = JSONArray()
        conversations.forEach { conversation ->
            val messages = JSONArray()
            conversation.messages.forEach { message ->
                messages.put(JSONObject().put("role", message.role).put("content", message.content))
            }
            saved.put(JSONObject().put("title", conversation.title).put("messages", messages))
        }
        val current = JSONArray()
        currentMessages.forEach { message ->
            current.put(JSONObject().put("role", message.role).put("content", message.content))
        }
        val payload = JSONObject()
            .put("query", query)
            .put("conversations", saved)
            .put("current_messages", current)
        val result = NativeCore.retrieveLocalKnowledge(payload.toString(), maxResults, maxCharacters)
        require(!result.startsWith("ERROR:")) { result.removePrefix("ERROR:") }
        return result
    }
}
