package com.cenai.app

import android.content.Context
import android.util.Log
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

/**
 * Conversation history is stored inside an account-scoped encrypted workspace.
 * The old SharedPreferences history is read only for a one-time, owner-filtered
 * migration; new writes never place conversation text in plaintext preferences.
 */
class ChatStore(context: Context, vault: SecureKeyStore? = null) {
    private val appContext = context.applicationContext
    private val legacyPrefs = appContext.getSharedPreferences("cenai_history", Context.MODE_PRIVATE)
    private val vault = vault ?: SecureKeyStore(appContext)
    private fun workspace() = AccountWorkspaceStore(appContext, resolveAccountId())

    private fun resolveAccountId(): String {
        val authenticatedId = vault.get("supabase_user_id").trim().lowercase()
        if (isUuid(authenticatedId)) return authenticatedId
        val prefs = appContext.getSharedPreferences("cenai_local_identity", Context.MODE_PRIVATE)
        val existing = prefs.getString("workspace_id", null)
        if (existing != null && isUuid(existing)) return existing
        val generated = UUID.randomUUID().toString()
        prefs.edit().putString("workspace_id", generated).commit()
        return generated
    }

    fun load(): MutableList<Conversation> {
        val encrypted = runCatching { workspace().readText(CONVERSATIONS_FILE) }
            .onFailure { Log.e(TAG, "Encrypted conversation workspace could not be opened; refusing plaintext fallback.", it) }
            .getOrNull()
        if (encrypted != null) return parse(encrypted)

        // Migrate only records belonging to this signed-in account. Unsigned
        // local history is kept in its own device-local UUID namespace.
        val accountEmail = vault.get("supabase_account_email").trim().lowercase()
        val legacy = legacyPrefs.getString("conversations", "[]") ?: "[]"
        val migrated = parse(legacy).filter { conversation ->
            if (accountEmail.isBlank()) conversation.ownerEmail.isBlank()
            else conversation.ownerEmail.trim().equals(accountEmail, ignoreCase = true)
        }.toMutableList()
        if (migrated.isNotEmpty()) {
            runCatching { save(migrated) }
                .onFailure { Log.e(TAG, "Could not migrate legacy conversation history.", it) }
        }
        return migrated
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
        workspace().writeText(CONVERSATIONS_FILE, array.toString())
    }

    private fun parse(raw: String): MutableList<Conversation> = runCatching {
        val array = JSONArray(raw)
        MutableList(array.length()) { i ->
            val item = array.getJSONObject(i)
            val msgs = item.optJSONArray("messages") ?: JSONArray()
            Conversation(
                id = item.optString("id", UUID.randomUUID().toString()),
                title = item.optString("title", "New chat"),
                messages = MutableList(msgs.length()) { j ->
                    val message = msgs.getJSONObject(j)
                    ChatMessage(message.optString("role"), message.optString("content"))
                },
                updatedAt = item.optLong("updatedAt", System.currentTimeMillis()),
                ownerEmail = item.optString("ownerEmail", "")
            )
        }.sortedByDescending { it.updatedAt }.toMutableList()
    }.getOrElse {
        Log.e(TAG, "Conversation history is invalid.", it)
        mutableListOf()
    }

    private fun isUuid(value: String): Boolean =
        runCatching { UUID.fromString(value).toString() == value }.getOrDefault(false)

    companion object {
        private const val TAG = "CenAI.ChatStore"
        private const val CONVERSATIONS_FILE = "conversations.json"
    }
}
