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
 * storage and are never uploaded. The fixed Lightning model is copied from APK assets on first use.
 */
class LocalModelRuntime(context: Context) {
    private val app = context.applicationContext
    private val modelDir = File(app.filesDir, "models")
    val modelFile: File get() = File(modelDir, "lightning.litertlm")
    private val cacheDir = File(app.cacheDir, "litertlm").apply { mkdirs() }
    private val mutex = Mutex()
    private var engine: Engine? = null
    private var loadedPath: String? = null

    fun isModelInstalled(): Boolean = modelFile.isFile && modelFile.length() > 1_000_000L
    fun modelSizeBytes(): Long = if (isModelInstalled()) modelFile.length() else 0L


    /** Installs the fixed Lightning model shipped in APK assets without asking the user to import it. */
    suspend fun installBundledModelIfMissing(): Boolean = withContext(Dispatchers.IO) {
        if (isModelInstalled()) return@withContext true
        val source = try {
            app.assets.open("models/lightning.litertlm")
        } catch (_: Throwable) {
            return@withContext false
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
                        require(copied <= 2_500_000_000L) { "Bundled Lightning model exceeds the supported size." }
                        output.write(buffer, 0, count)
                    }
                    output.fd.sync()
                }
            }
            require(copied > 1_000_000L) { "Bundled Lightning model is missing or incomplete." }
            mutex.withLock {
                if (isModelInstalled()) {
                    temporary.delete()
                    true
                } else {
                    closeEngineLocked()
                    if (!temporary.renameTo(modelFile)) {
                        throw IllegalStateException("Could not install the bundled Lightning model into private app storage.")
                    }
                    true
                }
            }
        } catch (_: Throwable) {
            temporary.delete()
            false
        }
    }

    suspend fun importModel(uri: Uri, displayName: String): Long = withContext(Dispatchers.IO) {
        require(displayName.lowercase().endsWith(".litertlm")) {
            "Choose a LiteRT-LM .litertlm model file."
        }
        modelDir.mkdirs()
        val temporary = File(modelDir, "lightning.importing")
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
                // Keep the previous working model until the new import has been
                // promoted successfully. A failed rename must not destroy the
                // user's only installed offline model.
                val backup = File(modelDir, "lightning.previous")
                if (backup.exists() && !backup.delete()) {
                    throw IllegalStateException("Could not prepare the model backup.")
                }
                val hadPrevious = modelFile.exists()
                if (hadPrevious && !modelFile.renameTo(backup)) {
                    throw IllegalStateException("Could not safely stage the existing local model.")
                }
                val installed = temporary.renameTo(modelFile)
                if (!installed) {
                    if (hadPrevious && !backup.renameTo(modelFile)) {
                        throw IllegalStateException(
                            "New model installation failed and the previous model could not be restored. The backup is preserved in app storage."
                        )
                    }
                    throw IllegalStateException("Could not finish installing the local model. The previous model was restored.")
                }
                backup.delete()
            }
            copied
        } catch (error: Throwable) {
            temporary.delete()
            throw error
        }
    }

    suspend fun stream(messages: List<ChatMessage>, onDelta: (String) -> Unit): String =
        withContext(Dispatchers.IO) {
            val bundledReady = installBundledModelIfMissing()
            mutex.withLock {
                check(bundledReady || isModelInstalled()) { "The bundled Lightning model is not present in this app build." }
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
}
