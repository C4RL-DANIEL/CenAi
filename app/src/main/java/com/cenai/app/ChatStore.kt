package com.cenai.app

import android.content.Context
import org.json.JSONArray
import org.json.JSONObject
import java.util.UUID

data class Conversation(
    val id: String = UUID.randomUUID().toString(),
    var title: String = "New chat",
    val messages: MutableList<ChatMessage> = mutableListOf(),
    var updatedAt: Long = System.currentTimeMillis()
)

class ChatStore(context: Context, ownerId: String = "guest") {
    private val appContext = context.applicationContext
    private val namespace = ownerId.filter { it.isLetterOrDigit() || it == '_' || it == '-' }.ifBlank { "guest" }
    private val prefs = appContext.getSharedPreferences("cenai_history_$namespace", Context.MODE_PRIVATE)
    private val vault = SecureKeyStore(appContext)

    fun load(): MutableList<Conversation> {
        var storedValue = prefs.getString("conversations", null)
        var legacyPrefsToClear: android.content.SharedPreferences? = null
        val globalOwner = vault.get("supabase_user_id")
        if (storedValue == null && globalOwner == namespace) {
            val legacyPrefs = appContext.getSharedPreferences("cenai_history", Context.MODE_PRIVATE)
            val legacy = legacyPrefs.getString("conversations", null)
            if (!legacy.isNullOrBlank()) {
                storedValue = legacy
                legacyPrefsToClear = legacyPrefs
            }
        }
        val stored = storedValue ?: "[]"
        val legacyPlaintext = stored.trimStart().startsWith("[")
        val raw = if (legacyPlaintext) stored else vault.decryptLocalData(stored).ifBlank { "[]" }
        val normalized = runCatching { NativeCore.normalizeConversations(raw) }.getOrNull()
            ?: return mutableListOf()
        val error = runCatching { JSONObject(normalized).optString("error") }.getOrDefault("")
        if (error.isNotBlank()) return mutableListOf()
        val parsed = runCatching {
            val array = JSONArray(normalized)
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
                    updatedAt = item.optLong("updatedAt", System.currentTimeMillis())
                )
            }
        }.getOrNull() ?: return mutableListOf()
        if (legacyPlaintext) save(parsed)
        legacyPrefsToClear?.edit()?.remove("conversations")?.apply()
        return parsed
    }

    fun save(items: List<Conversation>) {
        val array = JSONArray()
        items.forEach { conversation ->
            val messages = JSONArray()
            conversation.messages.forEach { messages.put(JSONObject().put("role", it.role).put("content", it.content)) }
            array.put(JSONObject().put("id", conversation.id).put("title", conversation.title)
                .put("updatedAt", conversation.updatedAt).put("messages", messages))
        }
        val normalized = NativeCore.normalizeConversations(array.toString())
        val error = runCatching { JSONObject(normalized).optString("error") }.getOrDefault("")
        require(error.isBlank()) { error.ifBlank { "Rust could not normalize saved conversations." } }
        prefs.edit().putString("conversations", vault.encryptLocalData(normalized)).apply()
    }
}
