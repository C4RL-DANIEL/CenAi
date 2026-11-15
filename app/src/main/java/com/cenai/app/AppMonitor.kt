package com.cenai.app

import android.content.Context

data class MonitorSnapshot(
    val started: Int,
    val succeeded: Int,
    val failed: Int,
    val cancelled: Int,
    val averageLatencyMs: Long,
    val lastSuccessAt: Long,
    val lastFailureCategory: String
) {
    val successRate: Int
        get() = if (succeeded + failed == 0) 0 else (succeeded * 100 / (succeeded + failed))
}

/**
 * Local privacy-first runtime monitor. Stores aggregate counters and failure
 * categories only; never stores prompts, responses, provider keys or tokens.
 */
class AppMonitor(context: Context) {
    private val prefs = context.getSharedPreferences("cenai_app_monitor", Context.MODE_PRIVATE)

    @Synchronized
    fun recordStarted() {
        prefs.edit().putInt("started", prefs.getInt("started", 0) + 1).apply()
    }

    @Synchronized
    fun recordSucceeded(durationMs: Long) {
        val count = prefs.getInt("succeeded", 0) + 1
        val total = prefs.getLong("latency_total", 0L) + durationMs.coerceAtLeast(0L)
        prefs.edit()
            .putInt("succeeded", count)
            .putLong("latency_total", total)
            .putLong("last_success_at", System.currentTimeMillis())
            .apply()
    }

    @Synchronized
    fun recordFailed(durationMs: Long, error: Throwable) {
        val count = prefs.getInt("failed", 0) + 1
        val total = prefs.getLong("latency_total", 0L) + durationMs.coerceAtLeast(0L)
        val message = error.message.orEmpty()
        val category = Regex("HTTP ([0-9]{3})").find(message)?.groupValues?.get(1)
            ?.let { "HTTP $it" } ?: error.javaClass.simpleName.take(48)
        prefs.edit()
            .putInt("failed", count)
            .putLong("latency_total", total)
            .putString("last_failure_category", category)
            .apply()
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
            averageLatencyMs = if (completed == 0) 0L else total / completed,
            lastSuccessAt = prefs.getLong("last_success_at", 0L),
            lastFailureCategory = prefs.getString("last_failure_category", "").orEmpty()
        )
    }

    @Synchronized
    fun reset() {
        prefs.edit().clear().apply()
    }
}
