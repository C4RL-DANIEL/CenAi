package com.cenai.app

import android.content.Context
import android.system.Os
import com.google.ai.edge.litertlm.Backend
import com.google.ai.edge.litertlm.Contents
import com.google.ai.edge.litertlm.Conversation
import com.google.ai.edge.litertlm.ConversationConfig
import com.google.ai.edge.litertlm.Engine
import com.google.ai.edge.litertlm.EngineConfig
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.takeWhile
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import java.io.File
import java.io.FileOutputStream
import java.security.MessageDigest

/** Narrow Android LiteRT-LM adapter for Rust-owned Lightning policy. */
object LightningRuntimeBridge {
    private const val ASSET_PATH = "models/lightning.litertlm"
    private const val MODEL_BYTES = 613_722_264L
    private const val MODEL_SHA256 = "c5f39b4191926786c3e6668b62af4d6eb15248dfc7a5348252a9b406f33f2108"
    private const val MAX_PROMPT_BYTES = 32_000
    internal const val MAX_RESPONSE_CHARS = 24_000
    private val mutex = Mutex()
    private var engine: Engine? = null
    private var loadedPath: String? = null
    private var verifiedFingerprint: String? = null

    private fun modelFile(context: Context) = File(context.applicationContext.filesDir, "models/lightning.litertlm")
    private fun cacheDirectory(context: Context) = File(context.applicationContext.cacheDir, "lightning-litertlm")

    @JvmStatic
    fun isReady(context: Context): Boolean {
        val file = modelFile(context)
        return file.isFile && file.length() == MODEL_BYTES && sha256(file) == MODEL_SHA256
    }

    @JvmStatic
    fun installBundledModel(context: Context): Boolean = runBlocking {
        withContext(Dispatchers.IO) { mutex.withLock { installBundledModelLocked(context.applicationContext) } }
    }

    /** Synchronous JNI boundary; Rust must call this on a dedicated blocking worker thread. */
    @JvmStatic
    fun generate(context: Context, prompt: String): String = runBlocking {
        require(prompt.isNotBlank() && prompt.toByteArray(Charsets.UTF_8).size <= MAX_PROMPT_BYTES) {
            "Lightning prompt must contain 1–32,000 UTF-8 bytes."
        }
        withContext(Dispatchers.IO) {
            mutex.withLock {
                val app = context.applicationContext
                check(installBundledModelLocked(app)) { "Bundled Lightning model is missing or failed verification." }
                val active = ensureEngineLocked(app)
                val conversation: Conversation = active.createConversation(
                    ConversationConfig(
                        systemInstruction = Contents.of("You are CenAI Lightning, a private on-device assistant. Answer helpfully and concisely. Do not claim network access or tools you do not have."),
                        initialMessages = emptyList()
                    )
                )
                try {
                    val answer = StringBuilder()
                    // Stop consuming the model stream at a hard bound instead of allowing
                    // a runaway generation to grow the app's heap without limit.
                    conversation.sendMessageAsync(prompt)
                        .takeWhile { part ->
                            val remaining = MAX_RESPONSE_CHARS - answer.length
                            if (remaining <= 0) {
                                false
                            } else {
                                answer.append(part.toString().take(remaining))
                                answer.length < MAX_RESPONSE_CHARS
                            }
                        }
                        .collect { }
                    answer.toString()
                } finally {
                    conversation.close()
                }
            }
        }
    }

    private fun installBundledModelLocked(context: Context): Boolean {
        val target = modelFile(context)
        if (target.isFile && target.length() == MODEL_BYTES) {
            val fingerprint = "${target.absolutePath}:${target.length()}:${target.lastModified()}"
            if (verifiedFingerprint == fingerprint) return true
            if (sha256(target) == MODEL_SHA256) {
                verifiedFingerprint = fingerprint
                return true
            }
        }
        val directory = target.parentFile ?: return false
        if (!directory.exists() && !directory.mkdirs()) return false
        val temporary = File(directory, "lightning.litertlm.part")
        return try {
            context.assets.open(ASSET_PATH).use { input ->
                FileOutputStream(temporary).use { output ->
                    val buffer = ByteArray(1024 * 1024)
                    var total = 0L
                    while (true) {
                        val count = input.read(buffer)
                        if (count < 0) break
                        total += count
                        check(total <= MODEL_BYTES) { "Bundled Lightning model exceeds the pinned size." }
                        output.write(buffer, 0, count)
                    }
                    check(total == MODEL_BYTES) { "Bundled Lightning model is incomplete." }
                    output.fd.sync()
                }
            }
            check(sha256(temporary) == MODEL_SHA256) { "Bundled Lightning model failed SHA-256 verification." }
            closeEngineLocked()
            // POSIX rename atomically replaces the previous model on the same filesystem.
            // A crash cannot leave the app with the old model deleted and no new model installed.
            Os.rename(temporary.absolutePath, target.absolutePath)
            verifiedFingerprint = "${target.absolutePath}:${target.length()}:${target.lastModified()}"
            true
        } catch (error: Throwable) {
            temporary.delete()
            android.util.Log.e("CenAI-Lightning", "Model installation failed", error)
            false
        }
    }

    private fun ensureEngineLocked(context: Context): Engine {
        val path = modelFile(context).absolutePath
        if (engine != null && loadedPath == path) return engine!!
        closeEngineLocked()
        val cache = cacheDirectory(context)
        if (!cache.exists() && !cache.mkdirs()) error("Could not create Lightning inference cache.")
        val candidate = Engine(EngineConfig(modelPath = path, backend = Backend.CPU(), cacheDir = cache.absolutePath))
        try {
            candidate.initialize()
        } catch (initializationError: Throwable) {
            // A partially initialized native engine may already own file descriptors,
            // mapped weights, or native buffers. Dispose it before propagating failure.
            try {
                candidate.close()
            } catch (cleanupError: Throwable) {
                initializationError.addSuppressed(cleanupError)
            }
            throw initializationError
        }
        engine = candidate
        loadedPath = path
        return candidate
    }

    private fun closeEngineLocked() {
        // Clear our references first so release remains idempotent even if native
        // teardown throws. A later request can safely construct a fresh engine.
        val previous = engine
        engine = null
        loadedPath = null
        if (previous != null) {
            try {
                previous.close()
            } catch (error: Throwable) {
                android.util.Log.w("CenAI-Lightning", "Lightning engine teardown failed", error)
            }
        }
    }

    @JvmStatic
    fun release() = runBlocking { withContext(Dispatchers.IO) { mutex.withLock { closeEngineLocked() } } }

    private fun sha256(file: File): String {
        val digest = MessageDigest.getInstance("SHA-256")
        file.inputStream().buffered().use { input ->
            val buffer = ByteArray(1024 * 1024)
            while (true) {
                val count = input.read(buffer)
                if (count < 0) break
                digest.update(buffer, 0, count)
            }
        }
        return digest.digest().joinToString("") { "%02x".format(it) }
    }
}
