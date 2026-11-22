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
    private fun ownerId(): String = vault.get("supabase_user_id", "guest").ifBlank { "guest" }
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
        // Rust owns canonical message validation and sanitation before provider serialization.
        val preparedMessages = messages.map { message ->
            require(NativeCore.isValidChatRole(message.role)) {
                "Unsupported conversation role: ${message.role}"
            }
            ChatMessage(message.role, NativeCore.normalizeMessageContent(message.content))
        }
        require(preparedMessages.all { NativeCore.isPromptWithinLimit(it.content, 3_000_000) }) {
            "A conversation message exceeds CenAI's 3,000,000-character safety limit."
        }
        if (provider == Provider.DEEPSEEK && model !in setOf("deepseek-flash", "deepseek-v4-flash", "deepseek-v4-flash-vision-exp") && preparedMessages.any { CENAI_IMAGE_MARKER.containsMatchIn(it.content) }) {
            throw IllegalArgumentException("DeepSeek image understanding requires the deepseek-flash model. Update the model ID in Settings.")
        }
        val oauthCredential = if (provider == Provider.GEMINI) GoogleGeminiOAuth(vault).credential() else "" to false
        val credential = when {
            provider == Provider.CENAI_CLOUD -> vault.getSecret("supabase_access_token") to false
            oauthCredential.first.isNotBlank() -> oauthCredential
            else -> vault.apiKey(provider, ownerId()) to false
        }
        val key = credential.first
        require(key.isNotBlank()) { "Connect a Google account or add an API key in Settings." }
        return suspendCancellableCoroutine { continuation ->
        val call = runCatching { client.newCall(buildRequest(provider, model, baseUrl, preparedMessages, key, credential.second)) }
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
                            JSONObject(raw).optJSONObject("error")?.stringOrEmpty("message")
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
                                ?.optJSONObject("message")?.stringOrEmpty("content").orEmpty()
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
        val accountId = ownerId()
        val maxTokens = vault.getUser(accountId, "max_tokens", "4096").toIntOrNull() ?: 4096
        val messagesJson = JSONArray().apply {
            msgs.forEach { message ->
                put(JSONObject().put("role", message.role).put("content", message.content))
            }
        }
        val encoded = NativeCore.buildProviderPayload(
            p.name,
            model,
            maxTokens,
            messagesJson.toString(),
            vault.getUser(accountId, "custom_params", "{}")
        )
        val error = runCatching { JSONObject(encoded).optString("error") }.getOrDefault("")
        require(error.isBlank()) { error.ifBlank { "Rust could not build the provider request." } }
        val payload = JSONObject(encoded)

        val builder = when (p) {
            Provider.CENAI_CLOUD -> Request.Builder().url("https://cenai-website.aglibotcarldaniel.workers.dev/api/ai/chat/completions")
                .header("Authorization", "Bearer " + key)
            Provider.CLAUDE -> Request.Builder().url("https://api.anthropic.com/v1/messages")
                .header("x-api-key", key).header("anthropic-version", "2023-06-01")
            Provider.GEMINI -> {
                val request = Request.Builder().url(
                    if (oauth) "https://generativelanguage.googleapis.com/v1beta/models/$model:streamGenerateContent?alt=sse"
                    else "https://generativelanguage.googleapis.com/v1beta/models/$model:streamGenerateContent?alt=sse&key=$key"
                )
                if (oauth) request.header("Authorization", "Bearer $key")
                    .header("x-goog-user-project", vault.get("google_cloud_project_id"))
                request
            }
            else -> {
                val urlBase = if (p == Provider.CUSTOM) base else p.baseUrl
                require(urlBase.startsWith("https://")) { "API URL must use HTTPS." }
                Request.Builder().url(urlBase.trimEnd('/') + "/chat/completions")
                    .header("Authorization", "Bearer $key")
            }
        }

        if (p == Provider.CUSTOM || p == Provider.CLAUDE) {
            runCatching { JSONObject(vault.getUser(accountId, "custom_headers", "{}")) }.getOrNull()?.let { headers ->
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
            Provider.CLAUDE -> if (obj.stringOrEmpty("type") == "content_block_delta")
                obj.optJSONObject("delta")?.stringOrEmpty("text").orEmpty() else ""
            Provider.GEMINI -> {
                val parts = obj.optJSONArray("candidates")?.optJSONObject(0)
                    ?.optJSONObject("content")?.optJSONArray("parts") ?: return@runCatching ""
                buildString {
                    for (index in 0 until parts.length()) {
                        append(parts.optJSONObject(index)?.stringOrEmpty("text").orEmpty())
                    }
                }
            }
            else -> obj.optJSONArray("choices")?.optJSONObject(0)
                ?.optJSONObject("delta")?.stringOrEmpty("content").orEmpty()
        }
    }.getOrDefault("").takeUnless { it == "null" }.orEmpty()
}

/** JSONObject.optString() may stringify JSONObject.NULL as the literal word "null". */
private fun JSONObject.stringOrEmpty(name: String): String =
    (opt(name) as? String).orEmpty()

/** Marker used only for fast capability checks; Rust performs provider payload parsing. */
private val CENAI_IMAGE_MARKER = Regex("""\[\[CENAI_IMAGE\|([^|]+)\|([A-Za-z0-9+/=]+)]]""")
