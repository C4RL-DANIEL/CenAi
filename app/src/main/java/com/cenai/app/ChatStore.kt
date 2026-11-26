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

/**
 * Conversation history is stored in an AES-GCM encrypted, account-UUID workspace.
 * Existing encrypted/legacy SharedPreferences are read only as a migration source.
 */
class ChatStore(context: Context, ownerId: String = "guest") {
    private val appContext = context.applicationContext
    private val namespace = ownerId.filter { it.isLetterOrDigit() || it == '_' || it == '-' }.ifBlank { "guest" }
    private val prefs = appContext.getSharedPreferences("cenai_history_$namespace", Context.MODE_PRIVATE)
    private val legacyPrefs = appContext.getSharedPreferences("cenai_history", Context.MODE_PRIVATE)
    private val vault = SecureKeyStore(appContext)
    private val workspace = AccountWorkspaceStore(appContext, WorkspaceIdentity.resolve(appContext, ownerId))

    fun load(): MutableList<Conversation> {
        val workspaceValue = try {
            workspace.readText(CONVERSATIONS_PATH)
        } catch (_: Exception) {
            // A corrupt or unauthenticated workspace must not trigger a plaintext fallback.
            return mutableListOf()
        }
        if (workspaceValue != null) return parse(workspaceValue) ?: mutableListOf()

        var storedValue = prefs.getString("conversations", null)
        var legacyPrefsToClear: android.content.SharedPreferences? = null
        val globalOwner = vault.get("supabase_user_id")
        if (storedValue == null && globalOwner == namespace) {
            val legacy = legacyPrefs.getString("conversations", null)
            if (!legacy.isNullOrBlank()) {
                storedValue = legacy
                legacyPrefsToClear = legacyPrefs
            }
        }
        val stored = storedValue ?: "[]"
        val legacyPlaintext = stored.trimStart().startsWith("[")
        val raw = if (legacyPlaintext) stored else vault.decryptLocalData(stored).ifBlank { "[]" }
        val parsed = parse(raw) ?: return mutableListOf()
        if (storedValue != null) {
            // Commit encrypted workspace data before removing any old history.
            runCatching { save(parsed) }.onFailure { return parsed }
            legacyPrefsToClear?.edit()?.remove("conversations")?.apply()
        }
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
        workspace.writeText(CONVERSATIONS_PATH, normalized)
        // Retain an encrypted compatibility copy until older installed builds are retired.
        prefs.edit().putString("conversations", vault.encryptLocalData(normalized)).apply()
    }

    private fun parse(raw: String): MutableList<Conversation>? {
        val normalized = runCatching { NativeCore.normalizeConversations(raw) }.getOrNull() ?: return null
        val error = runCatching { JSONObject(normalized).optString("error") }.getOrDefault("")
        if (error.isNotBlank()) return null
        return runCatching {
            val array = JSONArray(normalized)
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
                    updatedAt = item.optLong("updatedAt", System.currentTimeMillis())
                )
            }
        }.getOrNull()
    }

    companion object {
        private const val CONVERSATIONS_PATH = "conversations/history.json"
    }
}
