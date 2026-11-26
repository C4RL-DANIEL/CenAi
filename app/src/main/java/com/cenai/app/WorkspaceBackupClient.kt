package com.cenai.app

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import java.io.IOException
import java.security.SecureRandom
import java.util.UUID
import java.util.concurrent.TimeUnit
import javax.crypto.Cipher
import javax.crypto.SecretKeyFactory
import javax.crypto.spec.GCMParameterSpec
import javax.crypto.spec.PBEKeySpec
import javax.crypto.spec.SecretKeySpec

/** End-to-end encrypted cloud backup. Supabase stores only an authenticated ciphertext envelope. */
class WorkspaceBackupClient(private val vault: SecureKeyStore, private val ownerId: String) {
    private val client = OkHttpClient.Builder()
        .connectTimeout(20, TimeUnit.SECONDS)
        .readTimeout(90, TimeUnit.SECONDS)
        .callTimeout(120, TimeUnit.SECONDS)
        .followRedirects(false)
        .followSslRedirects(false)
        .build()

    suspend fun upload(archive: ByteArray, passphrase: String) = withContext(Dispatchers.IO) {
        requireOwner()
        require(archive.isNotEmpty() && archive.size <= MAX_ARCHIVE_BYTES) { "Backup archive is empty or too large." }
        val encrypted = encrypt(archive, passphrase)
        val token = sessionToken()
        val request = Request.Builder().url(objectUrl())
            .header("apikey", CenAiCloud.PUBLISHABLE_KEY)
            .header("Authorization", "Bearer $token")
            .header("Content-Type", "application/json")
            .header("x-upsert", "true")
            .header("Cache-Control", "no-store")
            .post(encrypted.toRequestBody(JSON_TYPE))
            .build()
        client.newCall(request).execute().use { response ->
            val body = response.body?.string().orEmpty()
            if (!response.isSuccessful) throw IOException(
                "Encrypted backup upload failed (HTTP ${response.code}): " +
                    runCatching { org.json.JSONObject(body).optString("message").ifBlank { org.json.JSONObject(body).optString("error") } }
                        .getOrDefault(body).take(300)
            )
        }
    }

    suspend fun download(passphrase: String): ByteArray = withContext(Dispatchers.IO) {
        requireOwner()
        val token = sessionToken()
        val request = Request.Builder().url(objectUrl(authenticated = true))
            .header("apikey", CenAiCloud.PUBLISHABLE_KEY)
            .header("Authorization", "Bearer $token")
            .header("Accept", "application/octet-stream")
            .header("Cache-Control", "no-store")
            .get().build()
        val envelope = client.newCall(request).execute().use { response ->
            if (!response.isSuccessful) {
                val body = response.body?.string().orEmpty()
                throw IOException(
                    if (response.code == 404) "No cloud backup exists for this account yet."
                    else "Encrypted backup download failed (HTTP ${response.code}): " + body.take(300)
                )
            }
            val body = response.body ?: throw IOException("Cloud backup is empty.")
            val declared = body.contentLength()
            require(declared == -1L || declared in (HEADER_BYTES + TAG_BYTES).toLong()..(MAX_ENVELOPE_BYTES).toLong()) {
                "Cloud backup has an invalid size."
            }
            val output = java.io.ByteArrayOutputStream()
            body.byteStream().use { input ->
                val buffer = ByteArray(32 * 1024)
                var total = 0
                while (true) {
                    val count = input.read(buffer)
                    if (count < 0) break
                    total += count
                    require(total <= MAX_ENVELOPE_BYTES) { "Cloud backup exceeds the 95 MiB safety limit." }
                    output.write(buffer, 0, count)
                }
            }
            output.toByteArray()
        }
        decrypt(envelope, passphrase)
    }

    private fun encrypt(plain: ByteArray, passphrase: String): ByteArray {
        validatePassphrase(passphrase)
        val salt = ByteArray(SALT_BYTES).also(SecureRandom()::nextBytes)
        val iv = ByteArray(IV_BYTES).also(SecureRandom()::nextBytes)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply {
            init(Cipher.ENCRYPT_MODE, deriveKey(passphrase, salt), GCMParameterSpec(TAG_BITS, iv))
            updateAAD(aad())
        }
        return MAGIC + salt + iv + cipher.doFinal(plain)
    }

    private fun decrypt(envelope: ByteArray, passphrase: String): ByteArray {
        validatePassphrase(passphrase)
        require(envelope.size in (HEADER_BYTES + TAG_BYTES)..MAX_ENVELOPE_BYTES) { "Cloud backup envelope has an invalid size." }
        require(envelope.copyOfRange(0, MAGIC.size).contentEquals(MAGIC)) { "Cloud backup format is not recognized." }
        val saltStart = MAGIC.size
        val ivStart = saltStart + SALT_BYTES
        val ciphertextStart = ivStart + IV_BYTES
        val salt = envelope.copyOfRange(saltStart, ivStart)
        val iv = envelope.copyOfRange(ivStart, ciphertextStart)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply {
            init(Cipher.DECRYPT_MODE, deriveKey(passphrase, salt), GCMParameterSpec(TAG_BITS, iv))
            updateAAD(aad())
        }
        return cipher.doFinal(envelope.copyOfRange(ciphertextStart, envelope.size)).also {
            require(it.size <= MAX_ARCHIVE_BYTES) { "Decrypted backup exceeds the archive size limit." }
        }
    }

    private fun deriveKey(passphrase: String, salt: ByteArray): SecretKeySpec {
        val spec = PBEKeySpec(passphrase.toCharArray(), salt, PBKDF2_ITERATIONS, KEY_BITS)
        return try {
            val bytes = SecretKeyFactory.getInstance("PBKDF2WithHmacSHA256").generateSecret(spec).encoded
            SecretKeySpec(bytes, "AES")
        } finally {
            spec.clearPassword()
        }
    }

    private fun validatePassphrase(value: String) {
        require(value.length in 12..256) { "Use a backup passphrase between 12 and 256 characters." }
    }

    private fun requireOwner() {
        require(runCatching { UUID.fromString(ownerId).toString() == ownerId.lowercase() }.getOrDefault(false)) {
            "Sign in to a registered CenAI account before using encrypted cloud backup."
        }
    }

    private fun sessionToken(): String = vault.getUserSecret(ownerId, "supabase_access_token")
        .takeIf { it.isNotBlank() } ?: throw IOException("Your CenAI session is unavailable. Sign in again.")

    private fun objectUrl(authenticated: Boolean = false): String {
        val route = if (authenticated) "authenticated/" else ""
        return "${CenAiCloud.BASE_URL}/storage/v1/object/${route}cenai-backups/$ownerId/workspace-v1.cenai"
    }

    private fun aad(): ByteArray = "CenAI-Cloud-Backup-v1\u0000$ownerId".toByteArray(Charsets.UTF_8)

    companion object {
        private const val PBKDF2_ITERATIONS = 310_000
        private const val KEY_BITS = 256
        private const val SALT_BYTES = 16
        private const val IV_BYTES = 12
        private const val TAG_BITS = 128
        private const val TAG_BYTES = 16
        private const val HEADER_BYTES = 8 + SALT_BYTES + IV_BYTES
        private const val MAX_ARCHIVE_BYTES = 95 * 1024 * 1024
        private const val MAX_ENVELOPE_BYTES = MAX_ARCHIVE_BYTES + HEADER_BYTES + TAG_BYTES
        private val MAGIC = "CENBKUP1".toByteArray(Charsets.US_ASCII)
        private val JSON_TYPE = "application/json".toMediaType()
    }
}
