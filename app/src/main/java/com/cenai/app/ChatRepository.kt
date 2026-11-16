package com.cenai.app

import kotlinx.coroutines.suspendCancellableCoroutine
import okhttp3.Call
import okhttp3.Callback
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import okio.BufferedSource
import org.json.JSONArray
import org.json.JSONObject
import java.io.IOException
import java.util.concurrent.TimeUnit
import kotlin.coroutines.resume

class ChatRepository(private val vault: SecureKeyStore) {
    private val client = OkHttpClient.Builder()
        .connectTimeout(25, TimeUnit.SECONDS)
        .readTimeout(0, TimeUnit.MILLISECONDS)
        .callTimeout(5, TimeUnit.MINUTES)
        .build()

    suspend fun stream(
        provider: Provider,
        model: String,
        baseUrl: String,
        messages: List<ChatMessage>,
        onDelta: (String) -> Unit
    ): String {
        if (provider == Provider.DEEPSEEK && model !in setOf("deepseek-flash", "deepseek-v4-flash", "deepseek-v4-flash-vision-exp") && messages.any { CENAI_IMAGE_MARKER.containsMatchIn(it.content) }) {
            throw IllegalArgumentException("DeepSeek image understanding requires the deepseek-flash model. Update the model ID in Settings.")
        }
        val oauthCredential = if (provider == Provider.GEMINI) GoogleGeminiOAuth(vault).credential() else "" to false
        val credential = when {
            provider == Provider.CENAI_CLOUD -> vault.getSecret("supabase_access_token") to false
            oauthCredential.first.isNotBlank() -> oauthCredential
            else -> vault.apiKey(provider) to false
        }
        val key = credential.first
        require(key.isNotBlank()) { "Connect a Google account or add an API key in Settings." }
        return suspendCancellableCoroutine { continuation ->
        val call = runCatching { client.newCall(buildRequest(provider, model, baseUrl, messages, key, credential.second)) }
            .getOrElse {
                continuation.resumeWith(Result.failure(it))
                return@suspendCancellableCoroutine
            }
        continuation.invokeOnCancellation { call.cancel() }
        call.enqueue(object : Callback {
            override fun onFailure(call: Call, e: IOException) {
                if (continuation.isActive) continuation.resumeWith(Result.failure(e))
            }

            override fun onResponse(call: Call, response: Response) {
                response.use { res ->
                    if (!res.isSuccessful) {
                        val raw = res.body?.string().orEmpty()
                        val message = runCatching {
                            JSONObject(raw).optJSONObject("error")?.optString("message")
                                ?.takeIf { it.isNotBlank() } ?: raw.take(500)
                        }.getOrDefault(raw.take(500))
                        if (continuation.isActive) continuation.resumeWith(
                            Result.failure(IllegalStateException("HTTP ${res.code}: ${message.ifBlank { res.message }}"))
                        )
                        return
                    }
                    try {
                        if (provider == Provider.CENAI_CLOUD) {
                            val raw = res.body?.string().orEmpty()
                            val data = JSONObject(raw)
                            val answer = data.optJSONArray("choices")?.optJSONObject(0)
                                ?.optJSONObject("message")?.optString("content").orEmpty()
                            if (answer.isBlank()) throw IOException(data.optString("error", "CenAI Cloud returned an empty response."))
                            if (continuation.isActive) onDelta(answer)
                            if (continuation.isActive) continuation.resume(answer)
                            return
                        }
                        val source = res.body?.source() ?: throw IOException("Empty response body.")
                        val answer = StringBuilder()
                        readEvents(source) { data ->
                            val delta = parseDelta(provider, data)
                            if (delta.isNotEmpty()) {
                                answer.append(delta)
                                if (continuation.isActive) onDelta(delta)
                            }
                        }
                        if (continuation.isActive) continuation.resume(answer.toString())
                    } catch (e: Exception) {
                        if (continuation.isActive) continuation.resumeWith(Result.failure(e))
                    }
                }
            }
        })
        }
    }

    private fun buildRequest(p: Provider, model: String, base: String, msgs: List<ChatMessage>, key: String, oauth: Boolean): Request {
        require(model.isNotBlank()) { "Enter a model ID in Settings." }
        val system = msgs.firstOrNull { it.role == "system" }?.content
        val conversation = msgs.filter { it.role != "system" }
        val builder: Request.Builder
        val payload: JSONObject
        when (p) {
            Provider.CENAI_CLOUD -> {
                require(!msgs.any { CENAI_IMAGE_MARKER.containsMatchIn(it.content) }) { "CenAI Cloud currently supports text-only chat. Choose another provider for image attachments." }
                val list = JSONArray()
                var remainingChars = 78000
                msgs.filter { it.role in setOf("system", "user", "assistant", "tool") }.takeLast(50).forEach { message ->
                    if (remainingChars > 0) {
                        val content = message.content.take(minOf(20000, remainingChars))
                        if (content.isNotBlank()) {
                            val role = if (message.role == "tool") "assistant" else message.role
                            list.put(JSONObject().put("role", role).put("content", content))
                            remainingChars -= content.length
                        }
                    }
                }
                payload = JSONObject().put("model", model).put("stream", false)
                    .put("max_tokens", (vault.get("max_tokens", "1024").toIntOrNull() ?: 1024).coerceIn(1, 1024))
                    .put("messages", list)
                builder = Request.Builder().url("https://cenai-website.aglibotcarldaniel.workers.dev/api/ai/chat/completions")
                    .header("Authorization", "Bearer " + key)
            }
            Provider.CLAUDE -> {
                val list = JSONArray()
                conversation.forEach { message ->
                    val blocks = JSONArray()
                    message.visionParts().forEach { (kind, value) ->
                        if (kind == "text") blocks.put(JSONObject().put("type", "text").put("text", value))
                        else blocks.put(JSONObject().put("type", "image").put("source", JSONObject().put("type", "base64").put("media_type", kind).put("data", value)))
                    }
                    list.put(JSONObject().put("role", message.role).put("content", if (blocks.length() == 1 && blocks.optJSONObject(0)?.optString("type") == "text") message.content else blocks))
                }
                payload = JSONObject().put("model", model)
                    .put("max_tokens", vault.get("max_tokens", "4096").toIntOrNull() ?: 4096)
                    .put("stream", true).put("messages", list)
                if (!system.isNullOrBlank()) payload.put("system", system)
                builder = Request.Builder().url("https://api.anthropic.com/v1/messages")
                    .header("x-api-key", key).header("anthropic-version", "2023-06-01")
            }
            Provider.GEMINI -> {
                val contents = JSONArray()
                conversation.forEach { message ->
                    val parts = JSONArray()
                    message.visionParts().forEach { (kind, value) ->
                        if (kind == "text") parts.put(JSONObject().put("text", value))
                        else parts.put(JSONObject().put("inline_data", JSONObject().put("mime_type", kind).put("data", value)))
                    }
                    contents.put(JSONObject().put("role", if (message.role == "assistant") "model" else "user").put("parts", parts))
                }
                payload = JSONObject().put("contents", contents)
                    .put("generationConfig", JSONObject().put("maxOutputTokens", vault.get("max_tokens", "4096").toIntOrNull() ?: 4096))
                if (!system.isNullOrBlank()) payload.put("systemInstruction",
                    JSONObject().put("parts", JSONArray().put(JSONObject().put("text", system))))
                builder = Request.Builder().url(if (oauth) "https://generativelanguage.googleapis.com/v1beta/models/$model:streamGenerateContent?alt=sse" else "https://generativelanguage.googleapis.com/v1beta/models/$model:streamGenerateContent?alt=sse&key=$key")
                if (oauth) builder.header("Authorization", "Bearer $key").header("x-goog-user-project", vault.get("google_cloud_project_id"))
            }
            else -> {
                val urlBase = if (p == Provider.CUSTOM) base else p.baseUrl
                require(urlBase.startsWith("https://")) { "API URL must use HTTPS." }
                val list = JSONArray()
                msgs.forEach { message ->
                    val blocks = JSONArray()
                    message.visionParts().forEach { (kind, value) ->
                        if (kind == "text") blocks.put(JSONObject().put("type", "text").put("text", value))
                        else blocks.put(JSONObject().put("type", "image_url").put("image_url", JSONObject().put("url", "data:$kind;base64,$value")))
                    }
                    val content: Any = if (blocks.length() == 1 && blocks.optJSONObject(0)?.optString("type") == "text") message.content else blocks
                    list.put(JSONObject().put("role", message.role).put("content", content))
                }
                payload = JSONObject().put("model", model).put("stream", true)
                    .put("max_tokens", vault.get("max_tokens", "4096").toIntOrNull() ?: 4096)
                    .put("messages", list)
                builder = Request.Builder().url(urlBase.trimEnd('/') + "/chat/completions")
                    .header("Authorization", "Bearer $key")
            }
        }
        if (p == Provider.CUSTOM) {
            runCatching { JSONObject(vault.get("custom_params", "{}")) }.getOrNull()?.let { params ->
                params.keys().forEach { name -> if (name !in setOf("model", "messages", "stream")) payload.put(name, params.get(name)) }
            }
        }
        if (p == Provider.CUSTOM || p == Provider.CLAUDE) {
            runCatching { JSONObject(vault.get("custom_headers", "{}")) }.getOrNull()?.let { headers ->
                headers.keys().forEach { name -> builder.header(name, headers.optString(name)) }
            }
        }
        return builder.header("Accept", "text/event-stream")
            .post(payload.toString().toRequestBody("application/json".toMediaType())).build()
    }

    private fun readEvents(source: BufferedSource, onData: (String) -> Unit) {
        while (!source.exhausted()) {
            val line = source.readUtf8Line() ?: break
            if (!line.startsWith("data:")) continue
            val data = line.removePrefix("data:").trim()
            if (data.isNotEmpty() && data != "[DONE]") onData(data)
        }
    }

    private fun parseDelta(provider: Provider, raw: String): String = runCatching {
        val obj = JSONObject(raw)
        when (provider) {
            Provider.CLAUDE -> if (obj.optString("type") == "content_block_delta")
                obj.optJSONObject("delta")?.optString("text").orEmpty() else ""
            Provider.GEMINI -> obj.optJSONArray("candidates")?.optJSONObject(0)
                ?.optJSONObject("content")?.optJSONArray("parts")?.optJSONObject(0)?.optString("text").orEmpty()
            else -> obj.optJSONArray("choices")?.optJSONObject(0)
                ?.optJSONObject("delta")?.optString("content").orEmpty()
        }
    }.getOrDefault("")
}


/** Converts user-selected image markers into provider-native vision payloads. */
private val CENAI_IMAGE_MARKER = Regex("""\[\[CENAI_IMAGE\|([^|]+)\|([A-Za-z0-9+/=]+)]]""")

private fun ChatMessage.visionParts(): List<Pair<String, String>> {
    val result = mutableListOf<Pair<String, String>>()
    var cursor = 0
    CENAI_IMAGE_MARKER.findAll(content).forEach { match ->
        val before = content.substring(cursor, match.range.first)
        if (before.isNotBlank()) result += "text" to before
        result += match.groupValues[1] to match.groupValues[2]
        cursor = match.range.last + 1
    }
    val tail = content.substring(cursor)
    if (tail.isNotBlank() || result.isEmpty()) result += "text" to tail
    return result
}
