package com.cenai.app

import android.content.Context
import android.net.Uri
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
 * Private, on-device language model runtime. Model weights remain in app-private
 * storage and are never uploaded. Import a compatible .litertlm file to use it.
 */
class LocalModelRuntime(context: Context) {
    private val app = context.applicationContext
    private val modelDir = File(app.filesDir, "models")
    val modelFile: File get() = File(modelDir, "cenai-local.litertlm")
    private val cacheDir = File(app.cacheDir, "litertlm").apply { mkdirs() }
    private val mutex = Mutex()
    private var engine: Engine? = null
    private var loadedPath: String? = null

    fun isModelInstalled(): Boolean = modelFile.isFile && modelFile.length() > 1_000_000L
    fun modelSizeBytes(): Long = if (isModelInstalled()) modelFile.length() else 0L

    suspend fun importModel(uri: Uri, displayName: String): Long = withContext(Dispatchers.IO) {
        require(displayName.lowercase().endsWith(".litertlm")) {
            "Choose a LiteRT-LM .litertlm model file."
        }
        modelDir.mkdirs()
        val temporary = File(modelDir, "cenai-local.importing")
        try {
            val input = app.contentResolver.openInputStream(uri)
                ?: throw IllegalStateException("Android could not open the selected model file.")
            var copied = 0L
            input.use { source ->
                FileOutputStream(temporary).use { output ->
                    val buffer = ByteArray(1024 * 1024)
                    while (true) {
                        val count = source.read(buffer)
                        if (count < 0) break
                        copied += count
                        require(copied <= 2_500_000_000L) { "Model is larger than the 2.5 GB import limit." }
                        output.write(buffer, 0, count)
                    }
                    output.fd.sync()
                }
            }
            require(copied > 1_000_000L) { "The selected model file is empty or incomplete." }
            mutex.withLock {
                closeEngineLocked()
                if (modelFile.exists() && !modelFile.delete()) {
                    throw IllegalStateException("Could not replace the existing local model.")
                }
                if (!temporary.renameTo(modelFile)) {
                    throw IllegalStateException("Could not finish installing the local model.")
                }
            }
            copied
        } catch (error: Throwable) {
            temporary.delete()
            throw error
        }
    }

    suspend fun stream(messages: List<ChatMessage>, onDelta: (String) -> Unit): String =
        withContext(Dispatchers.IO) {
            mutex.withLock {
                check(isModelInstalled()) { "Import a .litertlm model in Settings before using Offline AI." }
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
                        val delta = part.text
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
}
