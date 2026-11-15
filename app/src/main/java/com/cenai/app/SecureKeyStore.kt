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

    fun saveApiKey(value: String) {
        if (value.isBlank()) { prefs.edit().remove("api_key").apply(); return }
        val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, key()) }
        val packed = cipher.iv + cipher.doFinal(value.toByteArray(Charsets.UTF_8))
        prefs.edit().putString("api_key", Base64.encodeToString(packed, Base64.NO_WRAP)).apply()
    }

    fun apiKey(): String {
        return try {
            val encoded = prefs.getString("api_key", null) ?: return ""
            val packed = Base64.decode(encoded, Base64.NO_WRAP)
            require(packed.size > 12) { "Invalid encrypted key payload" }
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, packed.copyOfRange(0, 12)))
            String(cipher.doFinal(packed.copyOfRange(12, packed.size)), Charsets.UTF_8)
        } catch (_: Exception) { "" }
    }

    fun put(name: String, value: String) { prefs.edit().putString(name, value).apply() }
    fun get(name: String, fallback: String = "") = prefs.getString(name, fallback) ?: fallback
}
