package com.cenai.app

import android.content.Context
import com.google.ai.edge.litertlm.Backend
import com.google.ai.edge.litertlm.Contents
import com.google.ai.edge.litertlm.Conversation
import com.google.ai.edge.litertlm.ConversationConfig
import com.google.ai.edge.litertlm.Engine
import com.google.ai.edge.litertlm.EngineConfig
import com.google.ai.edge.litertlm.Message
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import java.io.File
import java.io.FileOutputStream

/**
 * Private, on-device Lightning runtime. The pinned Qwen3-0.6B INT8 LiteRT-LM
 * asset is copied into app-private storage and never uploaded.
 */
class LocalModelRuntime(context: Context) {
    private val app = context.applicationContext
    private val modelDir = File(app.filesDir, "models")
    val modelFile: File get() = File(modelDir, "lightning.litertlm")
    private val cacheDir = File(app.cacheDir, "litertlm").apply { mkdirs() }
    private val mutex = Mutex()
    private var engine: Engine? = null
    private var loadedPath: String? = null

    fun isModelInstalled(): Boolean =
        modelFile.isFile && modelFile.length() == EXPECTED_MODEL_SIZE_BYTES

    fun modelSizeBytes(): Long = if (isModelInstalled()) modelFile.length() else 0L

    /** Installs the fixed model from APK assets, without a user import or download step. */
    suspend fun installBundledModelIfMissing(): Boolean = withContext(Dispatchers.IO) {
        mutex.withLock {
            if (isModelInstalled()) return@withLock true
            val source = try {
                app.assets.open(MODEL_ASSET_PATH)
            } catch (_: Throwable) {
                return@withLock false
            }
            modelDir.mkdirs()
            val temporary = File(modelDir, "lightning.bundle.tmp")
            try {
                var copied = 0L
                source.use { input ->
                    FileOutputStream(temporary).use { output ->
                        val buffer = ByteArray(1024 * 1024)
                        while (true) {
                            val count = input.read(buffer)
                            if (count < 0) break
                            copied += count
                            require(copied <= EXPECTED_MODEL_SIZE_BYTES) {
                                "Bundled Lightning model is larger than the pinned model."
                            }
                            output.write(buffer, 0, count)
                        }
                        output.fd.sync()
                    }
                }
                require(copied == EXPECTED_MODEL_SIZE_BYTES) {
                    "Bundled Lightning model is incomplete or has an unexpected size."
                }
                closeEngineLocked()
                if (modelFile.exists() && !modelFile.delete()) {
                    throw IllegalStateException("Could not replace the incomplete Lightning model file.")
                }
                if (!temporary.renameTo(modelFile)) {
                    throw IllegalStateException("Could not install Lightning into private app storage.")
                }
                true
            } catch (_: Throwable) {
                temporary.delete()
                false
            }
        }
    }

    suspend fun stream(messages: List<ChatMessage>, onDelta: (String) -> Unit): String =
        withContext(Dispatchers.IO) {
            val installed = installBundledModelIfMissing()
            check(installed && isModelInstalled()) {
                "The bundled Lightning model is not present or failed verification in this app build."
            }
            mutex.withLock {
                val activeEngine = ensureEngineLocked()
                val system = messages.firstOrNull { it.role == "system" }?.content.orEmpty()
                val history = messages.filter { it.role != "system" }.dropLast(1).mapNotNull { message ->
                    when (message.role) {
                        "user" -> Message.user(message.content)
                        "assistant" -> Message.model(message.content)
                        else -> null
                    }
                }
                val prompt = messages.lastOrNull { it.role == "user" }?.content
                    ?: throw IllegalArgumentException("Offline AI needs a user message.")
                val config = ConversationConfig(
                    systemInstruction = Contents.of(system.ifBlank { "You are CenAI, a helpful private on-device assistant." }),
                    initialMessages = history
                )
                val conversation: Conversation = activeEngine.createConversation(config)
                try {
                    val answer = StringBuilder()
                    conversation.sendMessageAsync(prompt).collect { part ->
                        val delta = part.toString()
                        if (delta.isNotEmpty()) {
                            answer.append(delta)
                            onDelta(delta)
                        }
                    }
                    answer.toString()
                } finally {
                    conversation.close()
                }
            }
        }

    private fun ensureEngineLocked(): Engine {
        val path = modelFile.absolutePath
        if (engine != null && loadedPath == path) return engine!!
        closeEngineLocked()
        val candidate = Engine(
            EngineConfig(
                modelPath = path,
                backend = Backend.CPU(),
                cacheDir = cacheDir.absolutePath
            )
        )
        candidate.initialize()
        engine = candidate
        loadedPath = path
        return candidate
    }

    private fun closeEngineLocked() {
        engine?.close()
        engine = null
        loadedPath = null
    }

    suspend fun release() = withContext(Dispatchers.IO) {
        mutex.withLock { closeEngineLocked() }
    }

    companion object {
        private const val MODEL_ASSET_PATH = "models/lightning.litertlm"
        private const val EXPECTED_MODEL_SIZE_BYTES = 613_722_264L
    }
}
