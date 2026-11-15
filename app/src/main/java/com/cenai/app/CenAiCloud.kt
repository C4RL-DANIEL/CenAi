package com.cenai.app

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONObject
import java.io.IOException
import java.util.concurrent.TimeUnit

class CenAiCloud(private val vault: SecureKeyStore) {
    companion object {
        const val BASE_URL = "https://bqkzehozezvepbthglyj.supabase.co"
        const val PUBLISHABLE_KEY = "sb_publishable_l6YDudJWsFgUJ2YZc2Xq1Q_hNatLp3C"
        private val JSON = "application/json; charset=utf-8".toMediaType()
    }
    private val client = OkHttpClient.Builder()
        .connectTimeout(20, TimeUnit.SECONDS)
        .readTimeout(30, TimeUnit.SECONDS)
        .callTimeout(40, TimeUnit.SECONDS)
        .build()

    data class AccountResult(val message: String, val signedIn: Boolean)

    private suspend fun post(url: String, body: JSONObject, token: String? = null): JSONObject = withContext(Dispatchers.IO) {
        val builder = Request.Builder().url(url)
            .header("apikey", PUBLISHABLE_KEY)
            .header("Accept", "application/json")
            .post(body.toString().toRequestBody(JSON))
        if (!token.isNullOrBlank()) builder.header("Authorization", "Bearer $token")
        client.newCall(builder.build()).execute().use { response ->
            val text = response.body?.string().orEmpty()
            val data = runCatching { JSONObject(text) }.getOrElse { JSONObject() }
            if (!response.isSuccessful) {
                val message = data.optString("msg").ifBlank { data.optString("message") }.ifBlank { data.optString("error_description") }.ifBlank { data.optString("error") }.ifBlank { "Request failed (HTTP ${response.code})." }
                throw IOException(message)
            }
            data
        }
    }

    suspend fun signUp(email: String, password: String): AccountResult {
        require(email.contains("@") && password.length >= 8) { "Enter a valid email and a password of at least 8 characters." }
        val data = post("$BASE_URL/auth/v1/signup", JSONObject().put("email", email.trim()).put("password", password))
        val access = data.optString("access_token")
        if (access.isNotBlank()) saveSession(data, email)
        return if (access.isNotBlank()) AccountResult("Account created and signed in.", true)
        else AccountResult("Account created. Check your email to confirm it, then sign in.", false)
    }

    suspend fun signIn(email: String, password: String): AccountResult {
        require(email.contains("@") && password.isNotBlank()) { "Enter your email and password." }
        val data = post("$BASE_URL/auth/v1/token?grant_type=password", JSONObject().put("email", email.trim()).put("password", password))
        if (data.optString("access_token").isBlank()) throw IOException("Sign-in did not return a session.")
        saveSession(data, email)
        return AccountResult("Signed in as ${email.trim()}.", true)
    }

    private fun saveSession(data: JSONObject, email: String) {
        vault.putSecret("supabase_access_token", data.optString("access_token"))
        vault.putSecret("supabase_refresh_token", data.optString("refresh_token"))
        vault.put("supabase_account_email", data.optJSONObject("user")?.optString("email")?.ifBlank { email } ?: email)
    }

    fun signOut() {
        vault.putSecret("supabase_access_token", "")
        vault.putSecret("supabase_refresh_token", "")
        vault.put("supabase_account_email", "")
    }

    suspend fun refreshSession(): Boolean {
        val refresh = vault.getSecret("supabase_refresh_token")
        if (refresh.isBlank()) return false
        return try {
            val data = post("$BASE_URL/auth/v1/token?grant_type=refresh_token", JSONObject().put("refresh_token", refresh))
            if (data.optString("access_token").isBlank()) false else {
                saveSession(data, vault.get("supabase_account_email"))
                true
            }
        } catch (_: Exception) { false }
    }

    suspend fun call(action: String, payload: JSONObject = JSONObject()): JSONObject {
        var token = vault.getSecret("supabase_access_token")
        if (token.isBlank()) throw IOException("Sign in to your CenAI account first.")
        return try {
            post("$BASE_URL/functions/v1/cenai-control-plane", payload.put("action", action), token)
        } catch (e: IOException) {
            if (!refreshSession()) throw e
            token = vault.getSecret("supabase_access_token")
            post("$BASE_URL/functions/v1/cenai-control-plane", payload.put("action", action), token)
        }
    }

    fun accountEmail(): String = vault.get("supabase_account_email")
    fun isSignedIn(): Boolean = vault.getSecret("supabase_access_token").isNotBlank()
}
