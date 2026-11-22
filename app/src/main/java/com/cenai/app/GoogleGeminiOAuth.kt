package com.cenai.app

import android.content.Intent
import android.net.Uri
import okhttp3.FormBody
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONObject
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Official Google OAuth authorization-code + PKCE flow for the Gemini Developer API. */
class GoogleGeminiOAuth(
    private val vault: SecureKeyStore,
    private val ownerId: () -> String = { vault.get("supabase_user_id", "guest").ifBlank { "guest" } }
) {
    private fun putOAuthSecret(name: String, value: String) = vault.putUserSecret(ownerId(), name, value)
    private fun getOAuthSecret(name: String) = vault.getUserSecret(ownerId(), name)
    private fun putOAuthValue(name: String, value: String) = vault.putUser(ownerId(), name, value)
    private fun getOAuthValue(name: String, fallback: String = "") = vault.getUser(ownerId(), name, fallback)
    companion object {
        const val REDIRECT_URI = "com.cenai.app:/oauth2redirect"
        const val SCOPE = "https://www.googleapis.com/auth/generative-language.retriever https://www.googleapis.com/auth/cloud-platform"
    }
    private val http = OkHttpClient.Builder().callTimeout(30, TimeUnit.SECONDS).build()

    fun authorizationIntent(clientId: String): Intent {
        val clientValidation = NativeCore.validateGoogleOAuthClientId(clientId)
        require(clientValidation == "OK") { clientValidation }
        val pkce = JSONObject(NativeCore.generateOAuthPkce())
        val pkceError = pkce.optString("error")
        require(pkceError.isBlank()) { pkceError.ifBlank { "Could not generate OAuth PKCE values." } }
        val verifier = pkce.getString("verifier")
        val state = pkce.getString("state")
        val challenge = pkce.getString("challenge")
        putOAuthValue("google_oauth_client_id", clientId)
        putOAuthSecret("google_oauth_verifier", verifier)
        putOAuthSecret("google_oauth_state", state)
        val uri = Uri.parse("https://accounts.google.com/o/oauth2/v2/auth").buildUpon()
            .appendQueryParameter("client_id", clientId)
            .appendQueryParameter("redirect_uri", REDIRECT_URI)
            .appendQueryParameter("response_type", "code")
            .appendQueryParameter("scope", SCOPE)
            .appendQueryParameter("code_challenge", challenge)
            .appendQueryParameter("code_challenge_method", "S256")
            .appendQueryParameter("state", state)
            .appendQueryParameter("access_type", "offline")
            .appendQueryParameter("prompt", "consent")
            .build()
        return Intent(Intent.ACTION_VIEW, uri)
    }

    private fun parseOAuthToken(raw: String): JSONObject {
        val token = JSONObject(NativeCore.parseGoogleOAuthToken(raw, System.currentTimeMillis()))
        val error = token.optString("error")
        check(error.isBlank()) { error.ifBlank { NativeCore.parseProviderError(raw) } }
        return token
    }

    suspend fun completeCallback(uri: Uri): Result<Unit> = withContext(Dispatchers.IO) {
        runCatching {
            val expectedState = getOAuthSecret("google_oauth_state")
            val callback = JSONObject(NativeCore.validateGoogleOAuthCallback(
                uri.scheme.orEmpty(),
                uri.path.orEmpty(),
                uri.getQueryParameter("state").orEmpty(),
                expectedState,
                uri.getQueryParameter("error").orEmpty(),
                uri.getQueryParameter("code").orEmpty()
            ))
            val callbackError = callback.optString("error")
            require(callbackError.isBlank()) { callbackError }
            val code = callback.getString("code")
            val verifier = getOAuthSecret("google_oauth_verifier")
            val clientId = getOAuthValue("google_oauth_client_id")
            require(clientId.isNotBlank() && verifier.isNotBlank()) { "OAuth setup is incomplete." }
            val body = FormBody.Builder()
                .add("client_id", clientId)
                .add("code", code)
                .add("code_verifier", verifier)
                .add("redirect_uri", REDIRECT_URI)
                .add("grant_type", "authorization_code")
                .build()
            val request = Request.Builder().url("https://oauth2.googleapis.com/token").post(body).build()
            http.newCall(request).execute().use { response ->
                val raw = response.body?.string().orEmpty()
                check(response.isSuccessful) { "Google token exchange failed (HTTP " + response.code + "): " + NativeCore.parseProviderError(raw) }
                val token = parseOAuthToken(raw)
                putOAuthSecret("gemini_oauth_access_token", token.getString("access_token"))
                token.optString("refresh_token").takeIf { it.isNotBlank() }?.let { putOAuthSecret("gemini_oauth_refresh_token", it) }
                putOAuthValue("gemini_oauth_expires_at", token.getString("expires_at"))
                putOAuthSecret("google_oauth_verifier", "")
                putOAuthSecret("google_oauth_state", "")
            }
        }
    }

    suspend fun credential(): Pair<String, Boolean> = withContext(Dispatchers.IO) {
        val access = getOAuthSecret("gemini_oauth_access_token")
        val expires = getOAuthValue("gemini_oauth_expires_at", "0")
        if (NativeCore.googleOAuthAccessTokenUsable(access, expires, System.currentTimeMillis())) return@withContext access to true
        val refresh = getOAuthSecret("gemini_oauth_refresh_token")
        val clientId = getOAuthValue("google_oauth_client_id")
        if (refresh.isBlank() || clientId.isBlank()) return@withContext "" to false
        val body = FormBody.Builder().add("client_id", clientId).add("refresh_token", refresh).add("grant_type", "refresh_token").build()
        val request = Request.Builder().url("https://oauth2.googleapis.com/token").post(body).build()
        http.newCall(request).execute().use { response ->
            val raw = response.body?.string().orEmpty()
            if (!response.isSuccessful) return@withContext "" to false
            val token = runCatching { parseOAuthToken(raw) }.getOrNull() ?: return@withContext "" to false
            val next = token.optString("access_token")
            if (next.isNotBlank()) {
                putOAuthSecret("gemini_oauth_access_token", next)
                putOAuthValue("gemini_oauth_expires_at", token.getString("expires_at"))
            }
            next to next.isNotBlank()
        }
    }

    fun disconnect() {
        putOAuthSecret("gemini_oauth_access_token", "")
        putOAuthSecret("gemini_oauth_refresh_token", "")
        putOAuthValue("gemini_oauth_expires_at", "0")
    }
}
