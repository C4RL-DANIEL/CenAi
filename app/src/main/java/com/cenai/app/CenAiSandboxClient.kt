package com.cenai.app

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONArray
import org.json.JSONObject
import java.io.IOException
import java.util.concurrent.TimeUnit

data class CenAiSandboxResult(
    val runId: String,
    val projectId: String,
    val status: String,
    val exitCode: Int,
    val durationMs: Long,
    val stdout: String,
    val stderr: String
)

/** User-initiated isolated cloud execution. No shell is run on the Android device. */
class CenAiSandboxClient(private val vault: SecureKeyStore) {
    private val client = OkHttpClient.Builder()
        .connectTimeout(12, TimeUnit.SECONDS)
        .readTimeout(75, TimeUnit.SECONDS)
        .callTimeout(80, TimeUnit.SECONDS)
        .followRedirects(false)
        .followSslRedirects(false)
        .build()

    suspend fun run(
        command: String,
        arguments: List<String>,
        projectId: String = "default",
        workingDirectory: String = "."
    ): CenAiSandboxResult = withContext(Dispatchers.IO) {
        validateRequest(command, arguments, projectId, workingDirectory)
        val token = vault.getSecret("supabase_access_token")
        require(token.isNotBlank()) { "Sign in to your CenAI account before using the cloud sandbox." }
        val args = JSONArray().apply { arguments.forEach(::put) }
        val payload = JSONObject()
            .put("command", command)
            .put("arguments", args)
            .put("projectId", projectId)
            .put("workingDirectory", workingDirectory)
            .toString()
            .toRequestBody(JSON_TYPE)
        val request = Request.Builder().url(SANDBOX_URL)
            .header("Authorization", "Bearer $token")
            .header("Accept", "application/json")
            .post(payload)
            .build()
        client.newCall(request).execute().use { response ->
            val raw = response.body?.string().orEmpty()
            val data = runCatching { JSONObject(raw) }.getOrElse { JSONObject() }
            if (!response.isSuccessful) {
                throw IOException(data.optString("error").ifBlank { "Cloud sandbox failed (HTTP ${response.code})." }.take(500))
            }
            CenAiSandboxResult(
                data.optString("runId"),
                data.optString("projectId"),
                data.optString("status"),
                data.optInt("exitCode", 1),
                data.optLong("durationMs"),
                data.optString("stdout").take(MAX_OUTPUT_CHARS),
                data.optString("stderr").take(MAX_OUTPUT_CHARS)
            )
        }
    }

    companion object {
        private const val SANDBOX_URL = "https://cenai-website.cenai-app.workers.dev/api/sandbox"
        private const val MAX_OUTPUT_CHARS = 12 * 1024
        private val JSON_TYPE = "application/json; charset=utf-8".toMediaType()
        private val ALLOWED_COMMANDS = setOf(
            "bash", "sh", "cargo", "rustc", "python3", "python", "node", "npm", "npx", "git",
            "go", "make", "cmake", "java", "javac", "gradle", "ls", "cat", "find", "grep",
            "sed", "awk", "pwd", "mkdir", "cp", "mv", "rm", "touch", "tar", "unzip", "zip",
            "which", "head", "tail", "wc", "file"
        )
        private val CREDENTIAL_PATTERN = Regex(
            "(?i)authorization\\s*:|bearer\\s+|(?:api[_ -]?key|access[_ -]?token|refresh[_ -]?token|password|passphrase|client[_ -]?secret|service[_ -]?role|private[_ -]?key|secret)\\s*[:=]|-----BEGIN |\\bsk-[A-Za-z0-9_-]{12,}|\\bsb_secret_|\\beyJ[A-Za-z0-9_-]{10,}\\.[A-Za-z0-9_-]{10,}\\."
        )

        internal fun validateRequest(command: String, arguments: List<String>, projectId: String, workingDirectory: String) {
            require(command in ALLOWED_COMMANDS) { "Command is not in the CenAI sandbox allowlist." }
            require(arguments.size <= 128 && arguments.all { it.length <= 4_096 && '\u0000' !in it && !CREDENTIAL_PATTERN.containsMatchIn(it) }) {
                "Sandbox arguments exceed the allowed limits or contain credential-like material."
            }
            require(projectId.matches(Regex("[A-Za-z0-9_-]{1,80}"))) { "Sandbox project ID is invalid." }
            require(workingDirectory.length <= 512 && !workingDirectory.startsWith("/") &&
                '\\' !in workingDirectory && '\u0000' !in workingDirectory) {
                "Working directory must remain inside the project workspace."
            }
            val segments = workingDirectory.split('/')
            require(segments.none { it == ".." }) { "Working directory cannot traverse outside the project workspace." }
        }
    }
}
