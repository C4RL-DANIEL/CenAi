package com.cenai.app

import android.app.ActivityManager
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.os.BatteryManager
import android.os.Build
import android.os.StatFs
import android.os.SystemClock
import java.util.concurrent.atomic.AtomicBoolean

data class GuardianSnapshot(
    val availableRamMb: Long,
    val totalRamMb: Long,
    val appHeapUsedMb: Long,
    val appHeapMaxMb: Long,
    val freeStorageMb: Long,
    val totalStorageMb: Long,
    val batteryPercent: Int,
    val charging: Boolean,
    val network: String,
    val androidVersion: String,
    val device: String,
    val uptimeMinutes: Long,
    val lastCrashAt: Long,
    val lastCrashCategory: String
)

/** Local device health and crash-category diagnostics. No prompts, tokens or files are collected. */
class GuardianMonitor(private val context: Context) {
    private val prefs = context.getSharedPreferences("cenai_guardian", Context.MODE_PRIVATE)

    fun snapshot(): GuardianSnapshot {
        val activity = context.getSystemService(Context.ACTIVITY_SERVICE) as ActivityManager
        val memory = ActivityManager.MemoryInfo().also(activity::getMemoryInfo)
        val stat = StatFs(context.filesDir.absolutePath)
        val batteryManager = context.getSystemService(Context.BATTERY_SERVICE) as BatteryManager
        val battery = batteryManager.getIntProperty(BatteryManager.BATTERY_PROPERTY_CAPACITY).coerceIn(0, 100)
        val batteryIntent = context.registerReceiver(null, IntentFilter(Intent.ACTION_BATTERY_CHANGED))
        val status = batteryIntent?.getIntExtra(BatteryManager.EXTRA_STATUS, -1) ?: -1
        val connectivity = context.getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager
        val caps = connectivity.activeNetwork?.let(connectivity::getNetworkCapabilities)
        val network = when {
            caps == null -> "Offline"
            caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) -> "Wi-Fi"
            caps.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) -> "Mobile data"
            caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET) -> "Ethernet"
            else -> "Connected"
        }
        val runtime = Runtime.getRuntime()
        return GuardianSnapshot(
            availableRamMb = memory.availMem / 1_048_576L,
            totalRamMb = memory.totalMem / 1_048_576L,
            appHeapUsedMb = (runtime.totalMemory() - runtime.freeMemory()) / 1_048_576L,
            appHeapMaxMb = runtime.maxMemory() / 1_048_576L,
            freeStorageMb = stat.availableBytes / 1_048_576L,
            totalStorageMb = stat.totalBytes / 1_048_576L,
            batteryPercent = battery,
            charging = status == BatteryManager.BATTERY_STATUS_CHARGING || status == BatteryManager.BATTERY_STATUS_FULL,
            network = network,
            androidVersion = Build.VERSION.RELEASE ?: "Unknown",
            device = Build.MANUFACTURER + " " + Build.MODEL,
            uptimeMinutes = SystemClock.elapsedRealtime() / 60_000L,
            lastCrashAt = prefs.getLong("last_crash_at", 0L),
            lastCrashCategory = prefs.getString("last_crash_category", "").orEmpty()
        )
    }

    fun installCrashRecorder() {
        if (!installed.compareAndSet(false, true)) return
        val previous = Thread.getDefaultUncaughtExceptionHandler()
        Thread.setDefaultUncaughtExceptionHandler { thread, throwable ->
            val category = throwable.javaClass.simpleName.take(64)
            prefs.edit().putLong("last_crash_at", System.currentTimeMillis())
                .putString("last_crash_category", category).apply()
            previous?.uncaughtException(thread, throwable)
        }
    }

    companion object {
        private val installed = AtomicBoolean(false)
    }
}
