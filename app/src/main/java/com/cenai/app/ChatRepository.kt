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
        val credential = if (provider == Provider.GEMINI) GoogleGeminiOAuth(vault).credential() else vault.apiKey(provider) to false
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
            Provider.CLAUDE -> {
                val list = JSONArray()
                conversation.forEach { list.put(JSONObject().put("role", it.role).put("content", it.content)) }
                payload = JSONObject().put("model", model)
                    .put("max_tokens", vault.get("max_tokens", "4096").toIntOrNull() ?: 4096)
                    .put("stream", true).put("messages", list)
                if (!system.isNullOrBlank()) payload.put("system", system)
                builder = Request.Builder().url("https://api.anthropic.com/v1/messages")
                    .header("x-api-key", key).header("anthropic-version", "2023-06-01")
            }
            Provider.GEMINI -> {
                val contents = JSONArray()
                conversation.forEach {
                    contents.put(JSONObject().put("role", if (it.role == "assistant") "model" else "user")
                        .put("parts", JSONArray().put(JSONObject().put("text", it.content))))
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
                msgs.forEach { list.put(JSONObject().put("role", it.role).put("content", it.content)) }
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
