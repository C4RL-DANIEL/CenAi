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
    private val prefs = context.getSharedPreferences("cenai_private", Context.MODE_PRIVATE)
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

    fun saveApiKey(value: String) {
        if (value.isBlank()) prefs.edit().remove("api_key").apply()
        else prefs.edit().putString("api_key", encrypt(value)).apply()
    }

    fun saveApiKey(value: String, provider: Provider) {
        val name = "api_key_${provider.name.lowercase()}"
        if (value.isBlank()) prefs.edit().remove(name).apply()
        else prefs.edit().putString(name, encrypt(value)).apply()
    }

    fun apiKey(): String = decrypt(prefs.getString("api_key", null))

    fun apiKey(provider: Provider): String {
        val stored = prefs.getString("api_key_${provider.name.lowercase()}", null)
        return decrypt(stored ?: if (provider == Provider.OPENAI) prefs.getString("api_key", null) else null)
    }

    fun putSecret(name: String, value: String) { if (value.isBlank()) prefs.edit().remove("secret_" + name).apply() else prefs.edit().putString("secret_" + name, encrypt(value)).apply() }
    fun getSecret(name: String): String = decrypt(prefs.getString("secret_" + name, null))
    fun put(name: String, value: String) { prefs.edit().putString(name, value).apply() }
    fun get(name: String, fallback: String = "") = prefs.getString(name, fallback) ?: fallback
    fun putScoped(scope: BrainContextScope, featureId: String, name: String, value: String) {
        val key = ScopedKeyNames.preferenceKey(scope.copy(featureId = featureId), name)
        prefs.edit().putString(key, value).apply()
    }

    fun getScoped(scope: BrainContextScope, featureId: String, name: String, fallback: String = ""): String {
        val key = ScopedKeyNames.preferenceKey(scope.copy(featureId = featureId), name)
        return prefs.getString(key, fallback) ?: fallback
    }

    fun putSecretScoped(scope: BrainContextScope, featureId: String, name: String, value: String) {
        val key = ScopedKeyNames.preferenceKey(scope.copy(featureId = featureId), "secret_$name")
        if (value.isBlank()) prefs.edit().remove(key).apply()
        else prefs.edit().putString(key, encrypt(value)).apply()
    }

    fun getSecretScoped(scope: BrainContextScope, featureId: String, name: String): String {
        val key = ScopedKeyNames.preferenceKey(scope.copy(featureId = featureId), "secret_$name")
        return decrypt(prefs.getString(key, null))
    }

    fun saveApiKey(value: String, provider: Provider, scope: BrainContextScope) {
        putSecretScoped(scope, "chat", "api_key_${provider.name.lowercase()}", value)
    }

    fun apiKey(provider: Provider, scope: BrainContextScope): String {
        val scoped = getSecretScoped(scope, "chat", "api_key_${provider.name.lowercase()}")
        if (scoped.isNotBlank()) return scoped
        // Existing unscoped keys belong only to the signed-out local profile.
        return if (scope.userId.startsWith("local:")) apiKey(provider) else ""
    }

}
