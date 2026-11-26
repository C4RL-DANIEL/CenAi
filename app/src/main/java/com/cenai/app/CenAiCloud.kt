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
                val message = NativeCore.parseProviderError(text).ifBlank { "Request failed (HTTP ${response.code})." }
                throw IOException(message)
            }
            data
        }
    }

    private fun activeOwnerId(): String = vault.get("supabase_user_id")

    private fun accessToken(ownerId: String): String =
        if (ownerId.isNotBlank() && vault.get("supabase_user_id") == ownerId)
            vault.getUserSecret(ownerId, "supabase_access_token") else ""

    private fun authPayload(action: String, email: String = "", password: String = "", refreshToken: String = ""): JSONObject {
        val payload = JSONObject(NativeCore.buildAuthPayload(action, email, password, refreshToken))
        val error = payload.optString("error")
        require(error.isBlank()) { error.ifBlank { "Authentication payload validation failed." } }
        return payload
    }

    suspend fun signUp(email: String, password: String): AccountResult {
        val data = post("$BASE_URL/auth/v1/signup", authPayload("signup", email, password))
        val access = data.optString("access_token")
        if (access.isNotBlank()) saveSession(data, email)
        return if (access.isNotBlank()) AccountResult("Account created and signed in.", true)
        else AccountResult("Account created. Check your email to confirm it, then sign in.", false)
    }

    suspend fun signIn(email: String, password: String): AccountResult {
        val data = post("$BASE_URL/auth/v1/token?grant_type=password", authPayload("signin", email, password))
        if (data.optString("access_token").isBlank()) throw IOException("Sign-in did not return a session.")
        saveSession(data, email)
        return AccountResult("Signed in as ${email.trim()}.", true)
    }

    private fun saveSession(data: JSONObject, email: String, expectedOwnerId: String? = null) {
        val session = JSONObject(NativeCore.parseAuthSession(data.toString()))
        val error = session.optString("error")
        if (error.isNotBlank()) throw IOException(error)
        val accessToken = session.optString("access_token")
        val refreshToken = session.optString("refresh_token")
        val ownerId = session.optString("user_id")
        if (ownerId.isBlank() || (expectedOwnerId != null && ownerId != expectedOwnerId)) {
            throw IOException("The refreshed CenAI session belongs to a different account. Sign in again.")
        }
        if (expectedOwnerId != null && vault.get("supabase_user_id") != expectedOwnerId) {
            throw IOException("The active CenAI account changed during session refresh. Request cancelled.")
        }
        val accountEmail = session.optString("email").ifBlank { email }
        vault.putSecret("supabase_access_token", accessToken)
        vault.putSecret("supabase_refresh_token", refreshToken)
        vault.put("supabase_user_id", ownerId)
        vault.putUserSecret(ownerId, "supabase_access_token", accessToken)
        vault.putUserSecret(ownerId, "supabase_refresh_token", refreshToken)
        vault.putUser(ownerId, "account_email", accountEmail)
        vault.put("supabase_account_email", accountEmail)
    }

    fun signOut() {
        val ownerId = vault.get("supabase_user_id")
        if (ownerId.isNotBlank()) {
            vault.putUserSecret(ownerId, "supabase_access_token", "")
            vault.putUserSecret(ownerId, "supabase_refresh_token", "")
        }
        vault.putSecret("supabase_access_token", "")
        vault.putSecret("supabase_refresh_token", "")
        vault.put("supabase_user_id", "")
        vault.put("supabase_account_email", "")
    }

    suspend fun refreshSession(expectedOwnerId: String = activeOwnerId()): Boolean {
        if (expectedOwnerId.isBlank() || activeOwnerId() != expectedOwnerId) return false
        val refresh = vault.getUserSecret(expectedOwnerId, "supabase_refresh_token")
        if (refresh.isBlank()) return false
        return try {
            val data = post("$BASE_URL/auth/v1/token?grant_type=refresh_token", authPayload("refresh", refreshToken = refresh))
            if (data.optString("access_token").isBlank()) false else {
                val parsed = JSONObject(NativeCore.parseAuthSession(data.toString()))
                if (parsed.optString("user_id") != expectedOwnerId || activeOwnerId() != expectedOwnerId) {
                    false
                } else {
                    saveSession(data, vault.getUser(expectedOwnerId, "account_email", vault.get("supabase_account_email")), expectedOwnerId)
                    true
                }
            }
        } catch (_: Exception) { false }
    }

    suspend fun paypalBilling(action: String, payload: JSONObject = JSONObject()): JSONObject {
        val ownerId = activeOwnerId()
        var token = accessToken(ownerId)
        if (token.isBlank()) throw IOException("Sign in to your CenAI account first.")
        payload.put("action", action)
        return try {
            post("$BASE_URL/functions/v1/cenai-paypal-billing", payload, token)
        } catch (error: IOException) {
            if (!refreshSession(ownerId)) throw error
            token = accessToken(ownerId)
            if (token.isBlank()) throw IOException("Your CenAI session changed. Sign in again.")
            post("$BASE_URL/functions/v1/cenai-paypal-billing", payload, token)
        }
    }

    suspend fun call(action: String, payload: JSONObject = JSONObject()): JSONObject {
        val ownerId = activeOwnerId()
        var token = accessToken(ownerId)
        if (token.isBlank()) throw IOException("Sign in to your CenAI account first.")
        return try {
            post("$BASE_URL/functions/v1/cenai-control-plane", payload.put("action", action), token)
        } catch (error: IOException) {
            if (!refreshSession(ownerId)) throw error
            token = accessToken(ownerId)
            if (token.isBlank()) throw IOException("Your CenAI session changed. Sign in again.")
            post("$BASE_URL/functions/v1/cenai-control-plane", payload.put("action", action), token)
        }
    }

    suspend fun startBillingCheckout(plan: String): String {
        require(plan in setOf("supporter", "pro")) { "Choose a supported CenAI plan." }
        val ownerId = activeOwnerId()
        var token = accessToken(ownerId)
        if (token.isBlank()) throw IOException("Sign in to your CenAI account first.")
        val payload = JSONObject().put("plan", plan)
        return try {
            post("$BASE_URL/functions/v1/cenai-billing", payload, token).optString("checkoutUrl")
                .takeIf { it.startsWith("https://checkout.stripe.com/") }
                ?: throw IOException("The billing service did not return a valid checkout link.")
        } catch (error: IOException) {
            if (!refreshSession(ownerId)) throw error
            token = accessToken(ownerId)
            if (token.isBlank()) throw IOException("Your CenAI session changed. Sign in again.")
            post("$BASE_URL/functions/v1/cenai-billing", payload, token).optString("checkoutUrl")
                .takeIf { it.startsWith("https://checkout.stripe.com/") }
                ?: throw IOException("The billing service did not return a valid checkout link.")
        }
    }

    private suspend fun telemetryCall(payload: JSONObject): JSONObject {
        val ownerId = activeOwnerId()
        var token = accessToken(ownerId)
        if (token.isBlank()) throw IOException("Sign in to your CenAI account first.")
        return try {
            post("$BASE_URL/functions/v1/cenai-telemetry", payload, token)
        } catch (error: IOException) {
            if (!refreshSession(ownerId)) throw error
            token = accessToken(ownerId)
            if (token.isBlank()) throw IOException("Your CenAI session changed. Sign in again.")
            post("$BASE_URL/functions/v1/cenai-telemetry", payload, token)
        }
    }

    suspend fun setTelemetryConsent(usageAnalytics: Boolean, guardian: Boolean) {
        telemetryCall(JSONObject(NativeCore.buildConsentTelemetryPayload(usageAnalytics, guardian)))
    }

    suspend fun recordUsageEvent(provider: String, model: String, inputTokens: Int, outputTokens: Int, latencyMs: Long, status: String, errorCode: String = "") {
        telemetryCall(JSONObject(NativeCore.buildUsageTelemetryPayload(provider, model, inputTokens, outputTokens, latencyMs, status, errorCode)))
    }

    suspend fun sendGuardianReport(appVersion: String, androidVersion: String, batteryPercent: Int, availableRamMb: Long, freeStorageMb: Long, network: String, crashCategory: String) {
        telemetryCall(JSONObject(NativeCore.buildGuardianTelemetryPayload(appVersion, androidVersion, batteryPercent, availableRamMb, freeStorageMb, network, crashCategory)))
    }

    suspend fun sendEmailOtp(): String {
        val ownerId = activeOwnerId()
        val token = accessToken(ownerId)
        if (token.isBlank()) throw IOException("Sign in to your CenAI account first.")
        val data = post("https://cenai-website.cenai-app.workers.dev/api/email-otp", JSONObject().put("action", "send"), token)
        if (activeOwnerId() != ownerId) throw IOException("The active CenAI account changed during email verification.")
        return data.optString("message", "Verification code sent.")
    }

    suspend fun verifyEmailOtp(code: String): String {
        val validation = NativeCore.validateEmailOtp(code)
        require(validation == "OK") { validation }
        val ownerId = activeOwnerId()
        val token = accessToken(ownerId)
        if (token.isBlank()) throw IOException("Sign in to your CenAI account first.")
        val data = post("https://cenai-website.cenai-app.workers.dev/api/email-otp", JSONObject().put("action", "verify").put("code", code), token)
        if (!data.optBoolean("verified")) throw IOException("Email verification was not confirmed.")
        if (activeOwnerId() != ownerId) throw IOException("The active CenAI account changed during email verification.")
        vault.putUser(ownerId, "cenai_email_verified", "true")
        return data.optString("message", "Email verified.")
    }

    fun isEmailVerified(): Boolean {
        val ownerId = vault.get("supabase_user_id")
        return ownerId.isNotBlank() && vault.getUser(ownerId, "cenai_email_verified", "false") == "true"
    }

    fun accountEmail(): String {
        val ownerId = activeOwnerId()
        return if (ownerId.isNotBlank()) vault.getUser(ownerId, "account_email", vault.get("supabase_account_email")) else ""
    }
    fun isSignedIn(): Boolean = accessToken(activeOwnerId()).isNotBlank()
}
