package com.cenai.app

import android.app.ActivityManager
import android.content.Context
import android.os.BatteryManager
import android.os.StatFs
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CancellationException

/**
 * Android-scheduled, best-effort local health sampling.
 * WorkManager controls timing; this is not advertised as an always-on service.
 */
class GuardianWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result {
        return try {
            val app = applicationContext
            val activity = app.getSystemService(Context.ACTIVITY_SERVICE) as ActivityManager
            val memory = ActivityManager.MemoryInfo().also(activity::getMemoryInfo)
            val stat = StatFs(app.filesDir.absolutePath)
            val battery = app.getSystemService(Context.BATTERY_SERVICE) as BatteryManager
            val percent = battery.getIntProperty(BatteryManager.BATTERY_PROPERTY_CAPACITY).coerceIn(0, 100)
            val crash = app.getSharedPreferences("cenai_guardian_runtime", Context.MODE_PRIVATE)
            val sample = listOf(
                System.currentTimeMillis().toString(),
                memory.availMem.toString(),
                memory.totalMem.toString(),
                stat.availableBytes.toString(),
                percent.toString(),
                crash.getLong("last_crash_at", 0L).toString(),
                crash.getString("last_crash_category", "").orEmpty().replace("|", "_").take(64)
            ).joinToString("|")
            val history = app.getSharedPreferences("cenai_guardian_history", Context.MODE_PRIVATE)
            synchronized(LOCK) {
                val lines = history.getString("samples", "").orEmpty()
                    .lineSequence().filter(String::isNotBlank).toMutableList()
                lines += sample
                if (!history.edit().putString("samples", lines.takeLast(MAX_SAMPLES).joinToString("\n")).commit()) {
                    return Result.retry()
                }
            }
            Result.success()
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (_: Exception) {
            Result.retry()
        }
    }

    companion object {
        private const val WORK_NAME = "cenai_rust_guardian_periodic"
        private const val MAX_SAMPLES = 96
        private val LOCK = Any()

        @JvmStatic
        fun schedule(context: Context) {
            val request = PeriodicWorkRequestBuilder<GuardianWorker>(15, TimeUnit.MINUTES)
                .setInitialDelay(15, TimeUnit.MINUTES)
                .build()
            WorkManager.getInstance(context.applicationContext).enqueueUniquePeriodicWork(
                WORK_NAME, ExistingPeriodicWorkPolicy.KEEP, request
            )
        }

        @JvmStatic
        fun cancel(context: Context) {
            WorkManager.getInstance(context.applicationContext).cancelUniqueWork(WORK_NAME)
        }
    }
}
