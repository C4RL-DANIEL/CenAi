package com.cenai.app

import android.content.Context
import android.media.MediaPlayer
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.IOException
import java.util.UUID
import java.util.concurrent.TimeUnit

/** Authenticated Aura-1 cloud speech adapter. Text is sent only when the user invokes Read aloud. */
class CenAiVoiceClient(private val vault: SecureKeyStore) {
    private val client = OkHttpClient.Builder()
        .connectTimeout(12, TimeUnit.SECONDS)
        .readTimeout(60, TimeUnit.SECONDS)
        .callTimeout(75, TimeUnit.SECONDS)
        .followRedirects(false)
        .followSslRedirects(false)
        .build()

    suspend fun status(): JSONObject = postJson(STATUS_URL, JSONObject())

    suspend fun synthesize(text: String, language: String = "English"): ByteArray = withContext(Dispatchers.IO) {
        val clean = text.trim()
        require(clean.isNotEmpty() && clean.length <= MAX_TEXT_CHARS) {
            "Cloud voice supports 1–1,000 characters per request."
        }
        require(language == "English") { "CenAI Aura-1 currently supports English only." }
        val token = vault.getSecret("supabase_access_token")
        require(token.isNotBlank()) { "Sign in to your CenAI account to use cloud voice." }
        val body = JSONObject().put("input", clean).put("language", language).put("voice", VOICE_PROFILE)
            .toString().toRequestBody(JSON_TYPE)
        val request = Request.Builder().url(SPEECH_URL)
            .header("Authorization", "Bearer $token")
            .header("Accept", "audio/wav")
            .post(body).build()
        client.newCall(request).execute().use { response ->
            if (!response.isSuccessful) {
                val message = response.body?.string().orEmpty()
                throw IOException("CenAI Voice request failed (HTTP \${response.code}): " +
                    runCatching { JSONObject(message).optString("error") }.getOrDefault(message).take(300))
            }
            val declared = response.header("Content-Length")?.toLongOrNull() ?: 0L
            require(declared in 0..MAX_AUDIO_BYTES.toLong()) { "CenAI Voice audio exceeds the 32 MiB limit." }
            val bodyStream = response.body?.byteStream() ?: throw IOException("CenAI Voice returned an empty audio body.")
            val output = ByteArrayOutputStream()
            bodyStream.use { input ->
                val buffer = ByteArray(32 * 1024)
                var total = 0
                while (true) {
                    val count = input.read(buffer)
                    if (count < 0) break
                    total += count
                    require(total <= MAX_AUDIO_BYTES) { "CenAI Voice audio exceeds the 32 MiB limit." }
                    output.write(buffer, 0, count)
                }
            }
            val bytes = output.toByteArray()
            validateWav(bytes)
            bytes
        }
    }

    suspend fun synthesizeToCache(context: Context, text: String): File = withContext(Dispatchers.IO) {
        val bytes = synthesize(text)
        val directory = File(context.cacheDir, "cenai_voice").apply {
            check(isDirectory || mkdirs()) { "Could not create the temporary voice cache." }
        }
        val target = File(directory, UUID.randomUUID().toString() + ".wav")
        target.writeBytes(bytes)
        target
    }

    private suspend fun postJson(url: String, payload: JSONObject): JSONObject = withContext(Dispatchers.IO) {
        val token = vault.getSecret("supabase_access_token")
        require(token.isNotBlank()) { "Sign in to your CenAI account to check cloud voice." }
        val request = Request.Builder().url(url)
            .header("Authorization", "Bearer $token")
            .header("Accept", "application/json")
            .post(payload.toString().toRequestBody(JSON_TYPE))
            .build()
        client.newCall(request).execute().use { response ->
            val raw = response.body?.string().orEmpty()
            if (!response.isSuccessful) {
                throw IOException("CenAI Voice status failed (HTTP \${response.code}): " +
                    runCatching { JSONObject(raw).optString("error") }.getOrDefault(raw).take(300))
            }
            JSONObject(raw)
        }
    }

    companion object {
        const val VOICE_PROFILE = "cenai-en-v1"
        const val MAX_TEXT_CHARS = 1_000
        private const val MAX_AUDIO_BYTES = 32 * 1024 * 1024
        private const val STATUS_URL = "https://cenai-website.cenai-app.workers.dev/api/voice/status"
        private const val SPEECH_URL = "https://cenai-website.cenai-app.workers.dev/api/voice/speech"
        private val JSON_TYPE = "application/json; charset=utf-8".toMediaType()

        internal fun validateWav(bytes: ByteArray) {
            require(bytes.size in 44..MAX_AUDIO_BYTES) { "CenAI Voice returned an invalid WAV size." }
            fun ascii(offset: Int, value: String) {
                require(bytes.copyOfRange(offset, offset + value.length).toString(Charsets.US_ASCII) == value) {
                    "CenAI Voice did not return a WAV audio file."
                }
            }
            ascii(0, "RIFF")
            ascii(8, "WAVE")
            var offset = 12
            var format: Int? = null
            var channels: Int? = null
            var sampleRate: Long? = null
            var bits: Int? = null
            var dataBytes = 0
            while (offset + 8 <= bytes.size) {
                val chunkId = bytes.copyOfRange(offset, offset + 4).toString(Charsets.US_ASCII)
                val length = littleEndianInt(bytes, offset + 4)
                require(length >= 0 && offset.toLong() + 8L + length <= bytes.size) { "CenAI Voice WAV contains a truncated chunk." }
                when (chunkId) {
                    "fmt " -> {
                        require(length >= 16) { "CenAI Voice WAV format header is incomplete." }
                        format = littleEndianShort(bytes, offset + 8)
                        channels = littleEndianShort(bytes, offset + 10)
                        sampleRate = littleEndianInt(bytes, offset + 12).toLong() and 0xffffffffL
                        bits = littleEndianShort(bytes, offset + 22)
                    }
                    "data" -> dataBytes += length
                }
                offset += 8 + length + (length and 1)
            }
            require(format == 1 && channels == 1 && sampleRate != null && sampleRate in setOf(8_000L, 16_000L, 24_000L, 48_000L) &&
                bits == 16 && dataBytes > 0 && dataBytes % 2 == 0) {
                "CenAI Voice WAV uses an unsupported audio encoding."
            }
        }

        private fun littleEndianShort(bytes: ByteArray, offset: Int): Int =
            (bytes[offset].toInt() and 0xff) or ((bytes[offset + 1].toInt() and 0xff) shl 8)

        private fun littleEndianInt(bytes: ByteArray, offset: Int): Int =
            (bytes[offset].toInt() and 0xff) or
                ((bytes[offset + 1].toInt() and 0xff) shl 8) or
                ((bytes[offset + 2].toInt() and 0xff) shl 16) or
                ((bytes[offset + 3].toInt() and 0xff) shl 24)
    }
}

/** Plays only a WAV file produced by the authenticated CenAI Voice client. */
class CenAiVoicePlayback {
    private var player: MediaPlayer? = null

    fun play(file: File, onComplete: () -> Unit = {}, onError: (String) -> Unit = {}) {
        stop()
        val next = MediaPlayer()
        player = next
        next.setOnCompletionListener {
            stop()
            file.delete()
            onComplete()
        }
        next.setOnErrorListener { _, what, extra ->
            stop()
            file.delete()
            onError("Audio playback failed (\${what}/\${extra}).")
            true
        }
        try {
            next.setDataSource(file.absolutePath)
            next.setOnPreparedListener { it.start() }
            next.prepareAsync()
        } catch (error: Exception) {
            stop()
            file.delete()
            onError(error.message ?: "Could not start audio playback.")
        }
    }

    fun stop() {
        val current = player
        player = null
        runCatching { current?.stop() }
        runCatching { current?.release() }
    }
}
