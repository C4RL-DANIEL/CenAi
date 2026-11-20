package com.cenai.app

import android.content.Context
import java.security.MessageDigest
import java.util.UUID
import java.util.Base64
import org.json.JSONObject

/**
 * Identifies the active CenAI principal without using an email address as an ID.
 * A signed-in Supabase user gets a stable account namespace. Signed-out use gets
 * a device-local namespace that is never merged into an account automatically.
 */
class CenAiIdentityStore(context: Context, private val vault: SecureKeyStore) {
    private val prefs = context.applicationContext.getSharedPreferences(
        "cenai_identity", Context.MODE_PRIVATE
    )

    fun activeUserScopeId(): String {
        var accountId = vault.get("supabase_account_id").trim()
        if (!accountId.matches(Regex("[A-Za-z0-9_-]{8,128}"))) {
            // Older installs saved the encrypted Supabase JWT before persisting
            // the account UUID separately. Recover its subject only for local
            // partition selection; Supabase still validates every server request.
            val token = vault.getSecret("supabase_access_token")
            val payload = token.split(".").getOrNull(1).orEmpty()
            accountId = runCatching {
                JSONObject(String(Base64.getUrlDecoder().decode(payload), Charsets.UTF_8))
                    .optString("sub")
            }.getOrDefault("")
            if (accountId.matches(Regex("[A-Za-z0-9_-]{8,128}"))) {
                vault.put("supabase_account_id", accountId)
            }
        }
        if (accountId.matches(Regex("[A-Za-z0-9_-]{8,128}"))) return "account:$accountId"
        val localId = prefs.getString("local_profile_id", null) ?: UUID.randomUUID().toString().also {
            prefs.edit().putString("local_profile_id", it).apply()
        }
        return "local:$localId"
    }
}

/** A context namespace is always a verified principal plus one feature boundary. */
data class BrainContextScope(val userId: String, val featureId: String) {
    init {
        val principalId = when {
            userId.startsWith("account:") -> userId.removePrefix("account:")
            userId.startsWith("local:") -> userId.removePrefix("local:")
            else -> ""
        }
        require(principalId.matches(Regex("[A-Za-z0-9_-]{8,128}"))) {
            "A CenAI context must belong to a valid account or local profile."
        }
        require(userId.length <= 160 && featureId.matches(Regex("[a-z][a-z0-9_-]{0,63}"))) {
            "Invalid CenAI context namespace."
        }
    }

    val storageNamespace: String
        get() = "u_${ScopedKeyNames.hash(userId)}__f_${ScopedKeyNames.hash(featureId)}"
}

object ScopedKeyNames {
    fun hash(value: String): String = MessageDigest.getInstance("SHA-256")
        .digest(value.toByteArray(Charsets.UTF_8))
        .take(16)
        .joinToString("") { "%02x".format(it.toInt() and 0xff) }

    fun preferenceKey(scope: BrainContextScope, key: String): String {
        require(key.matches(Regex("[A-Za-z0-9_.-]{1,100}"))) { "Invalid scoped key." }
        return "${scope.storageNamespace}__${key}"
    }
}
