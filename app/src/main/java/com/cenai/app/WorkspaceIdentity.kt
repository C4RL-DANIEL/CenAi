package com.cenai.app

import android.content.Context
import java.security.MessageDigest
import java.util.UUID

/** Maps legacy test/guest labels to a stable, opaque UUID without using them as storage paths. */
object WorkspaceIdentity {
    fun resolve(context: Context, ownerId: String): String {
        val owner = ownerId.trim().ifBlank { "guest" }
        val normalized = owner.lowercase()
        val direct = runCatching { UUID.fromString(normalized).toString() }.getOrNull()
        if (direct == normalized) return normalized
        val digest = MessageDigest.getInstance("SHA-256")
            .digest(owner.toByteArray(Charsets.UTF_8))
            .take(16).joinToString("") { "%02x".format(it) }
        val key = "owner_$digest"
        val prefs = context.applicationContext.getSharedPreferences("cenai_workspace_identities", Context.MODE_PRIVATE)
        val existing = prefs.getString(key, null)
        if (existing != null && runCatching { UUID.fromString(existing).toString() == existing }.getOrDefault(false)) return existing
        val generated = UUID.randomUUID().toString()
        check(prefs.edit().putString(key, generated).commit()) { "Could not persist the workspace identity." }
        return generated
    }
}
