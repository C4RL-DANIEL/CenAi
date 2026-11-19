package com.cenai.app

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import java.io.IOException
import java.net.URLEncoder
import java.util.concurrent.TimeUnit
import org.json.JSONArray
import org.json.JSONObject

class ProviderModelCatalog {
    private val client = OkHttpClient.Builder()
        .connectTimeout(15, TimeUnit.SECONDS)
        .readTimeout(25, TimeUnit.SECONDS)
        .callTimeout(30, TimeUnit.SECONDS)
        .build()

    suspend fun listModels(provider: Provider, credential: String, customBaseUrl: String = ""): List<String> =
        withContext(Dispatchers.IO) {
            require(provider != Provider.CENAI_CLOUD) { "CenAI Cloud model discovery is managed by the server." }
            require(credential.isNotBlank()) { "Add an API key or connect the provider account first." }

            val url = when (provider) {
                Provider.OPENAI -> "https://api.openai.com/v1/models"
                Provider.DEEPSEEK -> "https://api.deepseek.com/models"
                Provider.CLAUDE -> "https://api.anthropic.com/v1/models"
                Provider.GEMINI -> "https://generativelanguage.googleapis.com/v1beta/models?key=" +
                    URLEncoder.encode(credential, Charsets.UTF_8.name())
                Provider.OPENROUTER -> "https://openrouter.ai/api/v1/models"
                Provider.CUSTOM -> {
                    require(customBaseUrl.startsWith("https://")) { "Custom model catalog URLs must use HTTPS." }
                    customBaseUrl.trimEnd('/') + "/models"
                }
                Provider.CENAI_CLOUD -> error("CenAI Cloud uses a server-managed model registry.")
            }

            val request = Request.Builder().url(url).get().header("Accept", "application/json").apply {
                when (provider) {
                    Provider.GEMINI -> Unit
                    Provider.CLAUDE -> header("x-api-key", credential).header("anthropic-version", "2023-06-01")
                    else -> header("Authorization", "Bearer $credential")
                }
            }.build()

            client.newCall(request).execute().use { response ->
                val raw = response.body?.string().orEmpty()
                if (!response.isSuccessful) {
                    val detail = runCatching {
                        val error = JSONObject(raw)
                        error.optJSONObject("error")?.optString("message")?.ifBlank { null }
                            ?: error.optString("message").ifBlank { null }
                    }.getOrNull()
                    throw IOException(detail ?: "Model catalog request failed (HTTP ${response.code}).")
                }
                val root = JSONObject(raw)
                val items = if (provider == Provider.GEMINI) root.optJSONArray("models") ?: JSONArray()
                    else root.optJSONArray("data") ?: JSONArray()
                val ids = LinkedHashSet<String>()
                for (index in 0 until minOf(items.length(), 500)) {
                    val item = items.optJSONObject(index) ?: continue
                    val id = item.optString(if (provider == Provider.GEMINI) "name" else "id").removePrefix("models/").trim()
                    if (id.isBlank()) continue
                    if (provider == Provider.GEMINI) {
                        val methods = item.optJSONArray("supportedGenerationMethods")
                        if (methods != null && (0 until methods.length()).none { methods.optString(it) in setOf("generateContent", "interactions") }) continue
                    }
                    ids += id
                }
                ids.sortedBy { it.lowercase() }.take(250)
            }
        }
}
