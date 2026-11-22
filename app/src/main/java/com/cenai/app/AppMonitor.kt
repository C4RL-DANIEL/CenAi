package com.cenai.app

import android.content.Context

data class MonitorSnapshot(
    val started: Int,
    val succeeded: Int,
    val failed: Int,
    val cancelled: Int,
    val averageLatencyMs: Long,
    val lastSuccessAt: Long,
    val lastFailureCategory: String,
    val appStarts: Int,
    val voiceInputs: Int,
    val voiceOutputs: Int,
    val documentsAttached: Int,
    val updateChecks: Int,
    val providerTests: Int
) {
    val successRate: Int
        get() = NativeCore.monitorSuccessRate(succeeded, failed)
}

/**
 * Local privacy-first monitor. It records aggregate request and feature activity
 * only; prompts, responses, documents, provider keys and tokens are never stored.
 */
class AppMonitor(context: Context) {
    private val prefs = context.getSharedPreferences("cenai_app_monitor", Context.MODE_PRIVATE)
    @Synchronized
    fun recordEvent(event: String) {
        val key = NativeCore.monitorEventKey(event).ifBlank { return }
        prefs.edit().putInt(key, prefs.getInt(key, 0) + 1).apply()
    }

    @Synchronized
    fun recordStarted() {
        prefs.edit().putInt("started", prefs.getInt("started", 0) + 1).apply()
    }

    @Synchronized
    fun recordSucceeded(durationMs: Long) {
        val count = prefs.getInt("succeeded", 0) + 1
        val total = prefs.getLong("latency_total", 0L) + durationMs.coerceAtLeast(0L)
        prefs.edit().putInt("succeeded", count).putLong("latency_total", total)
            .putLong("last_success_at", System.currentTimeMillis()).apply()
    }

    @Synchronized
    fun recordFailed(durationMs: Long, error: Throwable) {
        val count = prefs.getInt("failed", 0) + 1
        val total = prefs.getLong("latency_total", 0L) + durationMs.coerceAtLeast(0L)
        val message = error.message.orEmpty()
        val category = NativeCore.monitorFailureCategory(message, error.javaClass.simpleName)
        prefs.edit().putInt("failed", count).putLong("latency_total", total)
            .putString("last_failure_category", category).apply()
    }

    @Synchronized
    fun recordCancelled() {
        prefs.edit().putInt("cancelled", prefs.getInt("cancelled", 0) + 1).apply()
    }

    @Synchronized
    fun snapshot(): MonitorSnapshot {
        val succeeded = prefs.getInt("succeeded", 0)
        val failed = prefs.getInt("failed", 0)
        val completed = succeeded + failed
        val total = prefs.getLong("latency_total", 0L)
        return MonitorSnapshot(
            started = prefs.getInt("started", 0),
            succeeded = succeeded,
            failed = failed,
            cancelled = prefs.getInt("cancelled", 0),
            averageLatencyMs = NativeCore.monitorAverageLatency(total, completed),
            lastSuccessAt = prefs.getLong("last_success_at", 0L),
            lastFailureCategory = prefs.getString("last_failure_category", "").orEmpty(),
            appStarts = prefs.getInt("app_starts", 0),
            voiceInputs = prefs.getInt("voice_inputs", 0),
            voiceOutputs = prefs.getInt("voice_outputs", 0),
            documentsAttached = prefs.getInt("documents_attached", 0),
            updateChecks = prefs.getInt("update_checks", 0),
            providerTests = prefs.getInt("provider_tests", 0)
        )
    }

    @Synchronized
    fun reset() {
        prefs.edit().clear().apply()
    }
}
