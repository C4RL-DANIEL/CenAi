package com.cenai.app

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.io.File
import java.security.KeyStore
import java.security.MessageDigest
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** Per-account AES-256-GCM workspace storage backed by Android Keystore. */
class AccountWorkspaceStore(context: Context, val accountId: String) {
    private val root = File(context.noBackupFilesDir, "cenai_workspaces/$accountId")

    init {
        require(isUuid(accountId)) { "Workspace requires a canonical account UUID." }
    }

    fun write(relativePath: String, plaintext: ByteArray) {
        require(plaintext.size <= MAX_FILE_BYTES) { "Workspace file exceeds the 8 MiB limit." }
        val target = resolve(relativePath)
        val parent = target.parentFile ?: error("Invalid workspace path.")
        check(parent.mkdirs() || parent.isDirectory) { "Could not create workspace directory." }
        val cipher = Cipher.getInstance(TRANSFORMATION).apply {
            // Android Keystore requires randomized IV generation for GCM encryption.
            init(Cipher.ENCRYPT_MODE, key())
            updateAAD(aad(relativePath))
        }
        val iv = cipher.iv ?: error("Android Keystore did not generate a workspace nonce.")
        require(iv.size == IV_BYTES) { "Android Keystore generated an unsupported GCM nonce." }
        val envelope = MAGIC + iv + cipher.doFinal(plaintext)
        val temp = File(parent, ".${target.name}.${java.util.UUID.randomUUID()}.tmp")
        try {
            java.io.FileOutputStream(temp).use { stream ->
                stream.write(envelope)
                stream.fd.sync()
            }
            // POSIX rename replaces the previous file atomically on Android.
            android.system.Os.rename(temp.absolutePath, target.absolutePath)
        } finally {
            if (temp.exists()) temp.delete()
        }
    }

    fun read(relativePath: String): ByteArray? {
        val target = resolve(relativePath)
        if (!target.exists()) return null
        val envelope = target.readBytes()
        require(envelope.size in (MAGIC.size + IV_BYTES + TAG_BYTES)..(MAX_FILE_BYTES + MAGIC.size + IV_BYTES + TAG_BYTES)) {
            "Workspace ciphertext has an invalid size."
        }
        require(envelope.copyOfRange(0, MAGIC.size).contentEquals(MAGIC)) {
            "Workspace file format is not recognized."
        }
        val ivStart = MAGIC.size
        val cipher = Cipher.getInstance(TRANSFORMATION).apply {
            init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(TAG_BITS, envelope.copyOfRange(ivStart, ivStart + IV_BYTES)))
            updateAAD(aad(relativePath))
        }
        return cipher.doFinal(envelope.copyOfRange(ivStart + IV_BYTES, envelope.size))
    }

    fun readText(relativePath: String): String? = read(relativePath)?.toString(Charsets.UTF_8)
    fun writeText(relativePath: String, value: String) = write(relativePath, value.toByteArray(Charsets.UTF_8))

    fun delete(relativePath: String): Boolean {
        val file = resolve(relativePath)
        return !file.exists() || file.delete()
    }

    private fun resolve(relativePath: String): File {
        require(relativePath.isNotBlank() && relativePath.length <= 240) { "Workspace path is invalid." }
        val segments = relativePath.split('/')
        require(segments.all { it.isNotBlank() && it != "." && it != ".." && it.matches(Regex("[A-Za-z0-9._-]{1,80}")) }) {
            "Workspace path contains an unsafe segment."
        }
        val canonicalRoot = root.canonicalFile
        val canonicalFile = File(root, segments.joinToString(File.separator)).canonicalFile
        require(canonicalFile.path.startsWith(canonicalRoot.path + File.separator)) { "Workspace path escaped its account directory." }
        return canonicalFile
    }

    private fun aad(relativePath: String): ByteArray =
        "CenAI-Workspace-v1\u0000$accountId\u0000$relativePath".toByteArray(Charsets.UTF_8)

    private fun key(): SecretKey {
        val digest = MessageDigest.getInstance("SHA-256").digest(accountId.toByteArray(Charsets.UTF_8))
        val keyAlias = "cenai_workspace_" + digest.take(16).joinToString("") { "%02x".format(it) }
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (store.getKey(keyAlias, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        generator.init(
            KeyGenParameterSpec.Builder(keyAlias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setKeySize(256)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setRandomizedEncryptionRequired(true)
                .build()
        )
        return generator.generateKey()
    }

    private fun isUuid(value: String): Boolean =
        runCatching { java.util.UUID.fromString(value).toString() == value.lowercase() }.getOrDefault(false)

    companion object {
        private const val TRANSFORMATION = "AES/GCM/NoPadding"
        private const val IV_BYTES = 12
        private const val TAG_BITS = 128
        private const val TAG_BYTES = 16
        private const val MAX_FILE_BYTES = 8 * 1024 * 1024
        private val MAGIC = "CENAIWS1".toByteArray(Charsets.US_ASCII)
    }
}
