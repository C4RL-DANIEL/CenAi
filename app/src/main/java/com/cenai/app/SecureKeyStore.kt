package com.cenai.app

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

class SecureKeyStore(context: Context) {
    private val appContext = context.applicationContext
    private val prefs = appContext.getSharedPreferences("cenai_private", Context.MODE_PRIVATE)
    private fun accountPrefs(ownerId: String) = appContext.getSharedPreferences("cenai_private_" + ownerId.filter { it.isLetterOrDigit() || it == '_' || it == '-' }.ifBlank { "guest" }, Context.MODE_PRIVATE)
    private val alias = "cenai_aes_key_v1"

    private fun key(): SecretKey {
        val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (ks.getKey(alias, null) as? SecretKey)?.let { return it }
        val gen = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        gen.init(KeyGenParameterSpec.Builder(alias,
            KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setRandomizedEncryptionRequired(true).build())
        return gen.generateKey()
    }

    private fun encrypt(value: String): String {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, key()) }
        val packed = cipher.iv + cipher.doFinal(value.toByteArray(Charsets.UTF_8))
        return Base64.encodeToString(packed, Base64.NO_WRAP)
    }

    fun encryptLocalData(value: String): String = encrypt(value)

    fun decryptLocalData(value: String): String = decrypt(value)

    private fun decrypt(encoded: String?): String {
        if (encoded.isNullOrBlank()) return ""
        return runCatching {
            val packed = Base64.decode(encoded, Base64.NO_WRAP)
            require(packed.size > 12)
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, packed.copyOfRange(0, 12)))
            String(cipher.doFinal(packed.copyOfRange(12, packed.size)), Charsets.UTF_8)
        }.getOrDefault("")
    }

    /**
     * One-time migration of legacy global settings to the currently authenticated account.
     * It deliberately refuses to migrate when the stored Supabase identity is absent or different,
     * preventing old account data from being copied into guest mode or a newly signed-in account.
     */
    fun migrateLegacyAccountData(ownerId: String) {
        if (prefs.getBoolean("legacy_account_migration_v1_done", false)) return
        val storedOwner = prefs.getString("supabase_user_id", "").orEmpty()
        if (ownerId.isBlank() || ownerId == "guest" || storedOwner != ownerId) {
            prefs.edit().putBoolean("legacy_account_migration_v1_done", true).apply()
            return
        }
        val scoped = accountPrefs(ownerId)
        val plainKeys = setOf(
            "provider", "model", "base_url", "multi_agent", "agent_planning",
            "daily_request_limit", "speech_rate", "speech_pitch", "speech_language",
            "speech_input_language", "local_knowledge_enabled", "google_oauth_client_id",
            "google_cloud_project_id", "beta_display_name", "cloud_usage_analytics_consent",
            "guardian_cloud_consent", "usage_day", "usage_count", "usage_estimated_tokens",
            "cenai_email_verified", "max_tokens", "gemini_oauth_expires_at"
        )
        val all = prefs.all
        for ((name, value) in all) {
            val accountSetting = name in plainKeys ||
                name.startsWith("model_") || name.startsWith("base_url_") || name.startsWith("tool_enabled_")
            if (accountSetting && value is String && scoped.getString(name, null) == null) {
                scoped.edit().putString(name, value).apply()
            }
        }

        for (name in listOf("system_prompt", "custom_headers", "custom_params")) {
            val legacy = prefs.getString(name, null)
            if (!legacy.isNullOrBlank() && scoped.getString("secret_$name", null) == null) {
                putUserSecret(ownerId, name, legacy)
            }
        }
        val legacyMemoryName = "agent_memory_$ownerId"
        val secretNames = setOf(
            "supabase_access_token", "supabase_refresh_token",
            "gemini_oauth_access_token", "gemini_oauth_refresh_token",
            "google_oauth_verifier", "google_oauth_state", legacyMemoryName
        )
        for (name in secretNames) {
            val legacy = prefs.getString("secret_$name", null) ?: continue
            val targetName = if (name == legacyMemoryName) "agent_memory" else name
            if (scoped.getString("secret_$targetName", null) == null) {
                val clear = decrypt(legacy)
                if (clear.isNotBlank()) putUserSecret(ownerId, targetName, clear)
            }
        }
        for (provider in Provider.entries) {
            val name = "api_key_${provider.name.lowercase()}"
            if (scoped.getString(name, null) != null) continue
            val legacy = prefs.getString(name, null)
                ?: if (provider == Provider.OPENAI) prefs.getString("api_key", null) else null
                ?: continue
            val clear = decrypt(legacy)
            if (clear.isNotBlank()) saveApiKey(clear, provider, ownerId)
        }
        prefs.getString("supabase_account_email", null)?.let { email ->
            if (scoped.getString("account_email", null) == null) scoped.edit().putString("account_email", email).apply()
        }
        prefs.edit().putBoolean("legacy_account_migration_v1_done", true).apply()
    }

    fun saveApiKey(value: String) {
        if (value.isBlank()) prefs.edit().remove("api_key").apply()
        else prefs.edit().putString("api_key", encrypt(value)).apply()
    }

    fun saveApiKey(value: String, provider: Provider, ownerId: String = "guest") {
        val scoped = accountPrefs(ownerId)
        val name = "api_key_${provider.name.lowercase()}"
        if (value.isBlank()) scoped.edit().remove(name).apply()
        else scoped.edit().putString(name, encrypt(value)).apply()
    }

    fun apiKey(provider: Provider, ownerId: String = "guest"): String {
        val scoped = accountPrefs(ownerId)
        val stored = scoped.getString("api_key_${provider.name.lowercase()}", null)
        if (stored != null) return decrypt(stored)
        // Preserve legacy unscoped credentials for guest mode only; never leak them into another account.
        return if (ownerId == "guest") decrypt(if (provider == Provider.OPENAI) prefs.getString("api_key", null) else prefs.getString("api_key_${provider.name.lowercase()}", null)) else ""
    }

    fun putUserSecret(ownerId: String, name: String, value: String) {
        val scoped = accountPrefs(ownerId)
        if (value.isBlank()) scoped.edit().remove("secret_" + name).apply()
        else scoped.edit().putString("secret_" + name, encrypt(value)).apply()
    }

    fun getUserSecret(ownerId: String, name: String): String =
        decrypt(accountPrefs(ownerId).getString("secret_" + name, null))

    fun putUser(ownerId: String, name: String, value: String) {
        accountPrefs(ownerId).edit().putString(name, value).apply()
    }

    fun getUser(ownerId: String, name: String, fallback: String = ""): String =
        accountPrefs(ownerId).getString(name, fallback) ?: fallback

    fun putSecret(name: String, value: String) { if (value.isBlank()) prefs.edit().remove("secret_" + name).apply() else prefs.edit().putString("secret_" + name, encrypt(value)).apply() }
    fun getSecret(name: String): String = decrypt(prefs.getString("secret_" + name, null))
    fun put(name: String, value: String) { prefs.edit().putString(name, value).apply() }
    fun get(name: String, fallback: String = "") = prefs.getString(name, fallback) ?: fallback
}
