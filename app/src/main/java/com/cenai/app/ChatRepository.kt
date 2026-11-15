package com.cenai.app

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.TimeUnit

class ChatRepository(private val vault: SecureKeyStore) {
    private val client = OkHttpClient.Builder()
        .connectTimeout(30, TimeUnit.SECONDS).readTimeout(120, TimeUnit.SECONDS).build()

    suspend fun send(provider: Provider, model: String, baseUrl: String, messages: List<ChatMessage>): String =
        withContext(Dispatchers.IO) {
            val key = vault.apiKey(provider)
            require(key.isNotBlank()) { "Add your API key in Settings." }
            val request = when (provider) {
                Provider.CLAUDE -> claude(model, messages, key)
                Provider.GEMINI -> gemini(model, messages, key)
                else -> compatible(provider, model, baseUrl, messages, key)
            }
            client.newCall(request).execute().use { response ->
                val raw = response.body?.string().orEmpty()
                if (!response.isSuccessful) throw IllegalStateException("HTTP ${response.code}: " +
                    runCatching { JSONObject(raw).optJSONObject("error")?.optString("message") }.getOrNull().orEmpty())
                parse(provider, raw)
            }
        }

    private fun jsonBody(value: JSONObject) = value.toString().toRequestBody("application/json".toMediaType())

    private fun compatible(p: Provider, model: String, base: String, msgs: List<ChatMessage>, key: String): Request {
        val urlBase = if (p == Provider.CUSTOM) base else p.baseUrl
        require(urlBase.startsWith("https://")) { "API URL must use HTTPS." }
        val list = JSONArray()
        msgs.forEach { list.put(JSONObject().put("role", it.role).put("content", it.content)) }
        return Request.Builder().url(urlBase.trimEnd('/') + "/chat/completions")
            .header("Authorization", "Bearer $key")
            .post(jsonBody(JSONObject().put("model", model).put("messages", list))).build()
    }

    private fun claude(model: String, msgs: List<ChatMessage>, key: String): Request {
        val list = JSONArray()
        msgs.filter { it.role != "system" }.forEach {
            list.put(JSONObject().put("role", it.role).put("content", it.content))
        }
        val payload = JSONObject().put("model", model).put("max_tokens", 2048).put("messages", list)
        msgs.firstOrNull { it.role == "system" }?.let { payload.put("system", it.content) }
        return Request.Builder().url("https://api.anthropic.com/v1/messages")
            .header("x-api-key", key).header("anthropic-version", "2023-06-01")
            .header("anthropic-dangerous-direct-browser-access", "true")
            .post(jsonBody(payload)).build()
    }

    private fun gemini(model: String, msgs: List<ChatMessage>, key: String): Request {
        val contents = JSONArray()
        msgs.filter { it.role != "system" }.forEach {
            contents.put(JSONObject().put("role", if (it.role == "assistant") "model" else "user")
                .put("parts", JSONArray().put(JSONObject().put("text", it.content))))
        }
        val url = "https://generativelanguage.googleapis.com/v1beta/models/$model:generateContent?key=$key"
        return Request.Builder().url(url).post(jsonBody(JSONObject().put("contents", contents))).build()
    }

    private fun parse(p: Provider, raw: String): String {
        val obj = JSONObject(raw)
        return when (p) {
            Provider.CLAUDE -> obj.getJSONArray("content").optJSONObject(0)?.optString("text")
            Provider.GEMINI -> obj.getJSONArray("candidates").optJSONObject(0)
                ?.getJSONObject("content")?.getJSONArray("parts")?.optJSONObject(0)?.optString("text")
            else -> obj.getJSONArray("choices").optJSONObject(0)?.getJSONObject("message")?.optString("content")
        }?.takeIf { it.isNotBlank() } ?: "Empty response from provider."
    }
}
