package com.cenai.app

import android.content.Context
import org.json.JSONArray
import org.json.JSONObject
import java.util.UUID

data class Conversation(
    val id: String = UUID.randomUUID().toString(),
    var title: String = "New chat",
    val messages: MutableList<ChatMessage> = mutableListOf(),
    var updatedAt: Long = System.currentTimeMillis(),
    val ownerEmail: String = ""
)

class ChatStore(context: Context) {
    private val prefs = context.getSharedPreferences("cenai_history", Context.MODE_PRIVATE)

    fun load(): MutableList<Conversation> {
        val raw = prefs.getString("conversations", "[]") ?: "[]"
        return runCatching {
            val array = JSONArray(raw)
            MutableList(array.length()) { i ->
                val item = array.getJSONObject(i)
                val msgs = item.optJSONArray("messages") ?: JSONArray()
                Conversation(
                    id = item.optString("id", UUID.randomUUID().toString()),
                    title = item.optString("title", "New chat"),
                    messages = MutableList(msgs.length()) { j ->
                        val m = msgs.getJSONObject(j)
                        ChatMessage(m.optString("role"), m.optString("content"))
                    },
                    updatedAt = item.optLong("updatedAt", System.currentTimeMillis()),
                    ownerEmail = item.optString("ownerEmail", "")
                )
            }.sortedByDescending { it.updatedAt }.toMutableList()
        }.getOrDefault(mutableListOf())
    }

    fun save(items: List<Conversation>) {
        val array = JSONArray()
        items.forEach { conversation ->
            val messages = JSONArray()
            conversation.messages.forEach { messages.put(JSONObject().put("role", it.role).put("content", it.content)) }
            array.put(JSONObject().put("id", conversation.id).put("title", conversation.title)
                .put("updatedAt", conversation.updatedAt).put("ownerEmail", conversation.ownerEmail)
                .put("messages", messages))
        }
        prefs.edit().putString("conversations", array.toString()).apply()
    }
}
