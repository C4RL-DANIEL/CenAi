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
    var ownerUserId: String = "",
    var ownerFeatureId: String = ""
)

/**
 * Conversation storage is partitioned by both principal and feature.
 * Legacy unscoped history is migrated only into the signed-out local profile;
 * it is never copied into a newly signed-in account.
 */
class ChatStore(
    context: Context,
    private val contextScope: BrainContextScope = BrainContextScope(
        CenAiIdentityStore(context, SecureKeyStore(context)).activeUserScopeId(), "chat"
    )
) {
    private val appContext = context.applicationContext
    private val prefs = appContext.getSharedPreferences(
        "cenai_history_${contextScope.storageNamespace}", Context.MODE_PRIVATE
    )

    init {
        migrateLegacyHistoryForLocalProfile()
    }

    private fun migrateLegacyHistoryForLocalProfile() {
        if (!contextScope.userId.startsWith("local:")) return
        if (prefs.getBoolean("legacy_history_migrated", false)) return
        val legacy = appContext.getSharedPreferences("cenai_history", Context.MODE_PRIVATE)
            .getString("conversations", null)
        if (!legacy.isNullOrBlank() && prefs.getString("conversations", null).isNullOrBlank()) {
            prefs.edit().putString("conversations", legacy).apply()
        }
        prefs.edit().putBoolean("legacy_history_migrated", true).apply()
    }

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
                    ownerUserId = item.optString("ownerUserId", contextScope.userId),
                    ownerFeatureId = item.optString("ownerFeatureId", contextScope.featureId)
                )
            }.filter { BrainContextOwnership.owns(contextScope, it.ownerUserId, it.ownerFeatureId) }
                .sortedByDescending { it.updatedAt }.toMutableList()
        }.getOrDefault(mutableListOf())
    }

    fun save(items: List<Conversation>) {
        val array = JSONArray()
        items.forEach { conversation ->
            BrainContextOwnership.requireWritable(contextScope, conversation.ownerUserId, conversation.ownerFeatureId)
            conversation.ownerUserId = contextScope.userId
            conversation.ownerFeatureId = contextScope.featureId
            val messages = JSONArray()
            conversation.messages.forEach { messages.put(JSONObject().put("role", it.role).put("content", it.content)) }
            array.put(JSONObject().put("id", conversation.id).put("title", conversation.title)
                .put("updatedAt", conversation.updatedAt)
                .put("ownerUserId", contextScope.userId)
                .put("ownerFeatureId", contextScope.featureId)
                .put("messages", messages))
        }
        prefs.edit().putString("conversations", array.toString()).apply()
    }
}
