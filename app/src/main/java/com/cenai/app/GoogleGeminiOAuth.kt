package com.cenai.app

import android.content.Intent
import android.net.Uri
import android.util.Base64
import okhttp3.FormBody
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONObject
import java.security.MessageDigest
import java.security.SecureRandom
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Official Google OAuth authorization-code + PKCE flow for the Gemini Developer API. */
class GoogleGeminiOAuth(private val vault: SecureKeyStore) {
    companion object {
        const val REDIRECT_URI = "com.cenai.app:/oauth2redirect"
        const val SCOPE = "https://www.googleapis.com/auth/generative-language.retriever https://www.googleapis.com/auth/cloud-platform"
    }
    private val http = OkHttpClient.Builder().callTimeout(30, TimeUnit.SECONDS).build()

    fun authorizationIntent(clientId: String): Intent {
        require(clientId.endsWith(".apps.googleusercontent.com")) { "Enter a valid Google OAuth client ID." }
        val random = SecureRandom()
        val verifierBytes = ByteArray(48).also(random::nextBytes)
        val verifier = Base64.encodeToString(verifierBytes, Base64.URL_SAFE or Base64.NO_WRAP or Base64.NO_PADDING)
        val stateBytes = ByteArray(24).also(random::nextBytes)
        val state = Base64.encodeToString(stateBytes, Base64.URL_SAFE or Base64.NO_WRAP or Base64.NO_PADDING)
        val challenge = Base64.encodeToString(MessageDigest.getInstance("SHA-256").digest(verifier.toByteArray()), Base64.URL_SAFE or Base64.NO_WRAP or Base64.NO_PADDING)
        vault.putSecret("google_oauth_verifier", verifier)
        vault.putSecret("google_oauth_state", state)
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

    suspend fun completeCallback(uri: Uri): Result<Unit> = withContext(Dispatchers.IO) {
        runCatching {
            require(uri.scheme == "com.cenai.app" && uri.path == "/oauth2redirect") { "Unexpected OAuth redirect." }
            val expectedState = vault.getSecret("google_oauth_state")
            require(expectedState.isNotBlank() && uri.getQueryParameter("state") == expectedState) { "OAuth state validation failed." }
            val error = uri.getQueryParameter("error")
            require(error.isNullOrBlank()) { "Google authorization failed: " + error }
            val code = uri.getQueryParameter("code") ?: error("Google did not return an authorization code.")
            val verifier = vault.getSecret("google_oauth_verifier")
            val clientId = vault.get("google_oauth_client_id")
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
                check(response.isSuccessful) { "Google token exchange failed (HTTP " + response.code + "): " + raw.take(300) }
                val token = JSONObject(raw)
                vault.putSecret("gemini_oauth_access_token", token.getString("access_token"))
                token.optString("refresh_token").takeIf { it.isNotBlank() }?.let { vault.putSecret("gemini_oauth_refresh_token", it) }
                vault.put("gemini_oauth_expires_at", (System.currentTimeMillis() + token.optLong("expires_in", 3600L) * 1000L).toString())
                vault.putSecret("google_oauth_verifier", "")
                vault.putSecret("google_oauth_state", "")
            }
        }
    }

    suspend fun credential(): Pair<String, Boolean> = withContext(Dispatchers.IO) {
        val access = vault.getSecret("gemini_oauth_access_token")
        val expires = vault.get("gemini_oauth_expires_at", "0").toLongOrNull() ?: 0L
        if (access.isNotBlank() && expires > System.currentTimeMillis() + 60_000L) return@withContext access to true
        val refresh = vault.getSecret("gemini_oauth_refresh_token")
        val clientId = vault.get("google_oauth_client_id")
        if (refresh.isBlank() || clientId.isBlank()) return@withContext "" to false
        val body = FormBody.Builder().add("client_id", clientId).add("refresh_token", refresh).add("grant_type", "refresh_token").build()
        val request = Request.Builder().url("https://oauth2.googleapis.com/token").post(body).build()
        http.newCall(request).execute().use { response ->
            val raw = response.body?.string().orEmpty()
            if (!response.isSuccessful) return@withContext "" to false
            val token = JSONObject(raw)
            val next = token.optString("access_token")
            if (next.isNotBlank()) {
                vault.putSecret("gemini_oauth_access_token", next)
                vault.put("gemini_oauth_expires_at", (System.currentTimeMillis() + token.optLong("expires_in", 3600L) * 1000L).toString())
            }
            next to next.isNotBlank()
        }
    }

    fun disconnect() {
        vault.putSecret("gemini_oauth_access_token", "")
        vault.putSecret("gemini_oauth_refresh_token", "")
        vault.put("gemini_oauth_expires_at", "0")
    }
}
