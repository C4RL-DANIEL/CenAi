package com.cenai.app

import android.content.Context
import android.os.Build
import android.os.SystemClock
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Local-only crash categorization for the Rust-native application.
 * Never records exception messages, stack traces, prompts, file names or credentials.
 */
object GuardianRuntimeBridge {
    private const val PREFS = "cenai_guardian_runtime"
    private val installed = AtomicBoolean(false)

    @JvmStatic
    fun install(context: Context) {
        val app = context.applicationContext
        if (!installed.compareAndSet(false, true)) return
        val previous = Thread.getDefaultUncaughtExceptionHandler()
        Thread.setDefaultUncaughtExceptionHandler { thread, throwable ->
            recordCrash(app, throwable.javaClass.simpleName)
            previous?.uncaughtException(thread, throwable)
        }
    }

    /** Records only a fixed lifecycle event name; no activity or user content is retained. */
    @JvmStatic
    fun recordLifecycle(context: Context, event: String) {
        val allowed = setOf("resumed", "paused", "stopped", "low_memory")
        if (event !in allowed) return
        val prefs = context.applicationContext.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        prefs.edit()
            .putLong("last_lifecycle_at", System.currentTimeMillis())
            .putString("last_lifecycle_event", event)
            .putInt("lifecycle_event_count", (prefs.getInt("lifecycle_event_count", 0) + 1).coerceAtMost(1_000_000))
            .commit()
    }

    /** Stores a coarse Android memory-pressure bucket, never a process dump. */
    @JvmStatic
    fun recordMemoryPressure(context: Context, level: Int) {
        val bucket = when {
            level >= 80 -> "critical"
            level >= 40 -> "high"
            level >= 10 -> "moderate"
            else -> "normal"
        }
        context.applicationContext.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit()
            .putString("memory_pressure", bucket)
            .putLong("memory_pressure_at", System.currentTimeMillis())
            .commit()
    }

    internal fun recordCrash(context: Context, exceptionClass: String) {
        val category = exceptionClass.takeIf { it.matches(Regex("[A-Za-z0-9_$]{1,64}")) }
            ?: "UnknownException"
        context.applicationContext.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit()
            .putLong("last_crash_at", System.currentTimeMillis())
            .putString("last_crash_category", category)
            .commit()
    }
}
