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

    internal fun recordCrash(context: Context, exceptionClass: String) {
        val category = exceptionClass.takeIf { it.matches(Regex("[A-Za-z0-9_$]{1,64}")) }
            ?: "UnknownException"
        context.applicationContext.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit()
            .putLong("last_crash_at", System.currentTimeMillis())
            .putString("last_crash_category", category)
            .commit()
    }
}
