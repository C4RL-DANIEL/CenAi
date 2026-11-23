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
            runCatching {
                app.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit()
                    .putLong("last_crash_at", System.currentTimeMillis())
                    .putString("last_crash_category", throwable.javaClass.simpleName.take(64))
                    .putString("last_crash_thread", thread.name.take(48))
                    .commit()
            }
            previous?.uncaughtException(thread, throwable)
        }
        app.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit()
            .putString("android_release", Build.VERSION.RELEASE.orEmpty().take(32))
            .putString("device_model", Build.MODEL.orEmpty().take(64))
            .putLong("process_started_elapsed_ms", SystemClock.elapsedRealtime())
            .apply()
    }
}
