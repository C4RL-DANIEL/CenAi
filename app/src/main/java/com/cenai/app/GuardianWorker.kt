package com.cenai.app

import android.content.Context
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import java.util.concurrent.TimeUnit

/**
 * Best-effort periodic Guardian sampler. Android controls execution timing;
 * this is not an always-on service and never uploads device information.
 */
class GuardianWorker(
    appContext: Context,
    params: WorkerParameters
) : CoroutineWorker(appContext, params) {
    override suspend fun doWork(): Result = runCatching {
        val snapshot = GuardianMonitor(applicationContext).snapshot()
        val prefs = applicationContext.getSharedPreferences("cenai_guardian_history", Context.MODE_PRIVATE)
        val line = listOf(
            System.currentTimeMillis().toString(),
            snapshot.availableRamMb.toString(),
            snapshot.totalRamMb.toString(),
            snapshot.freeStorageMb.toString(),
            snapshot.batteryPercent.toString(),
            snapshot.network.replace("|", " "),
            snapshot.lastCrashCategory.replace("|", " ")
        ).joinToString("|")
        synchronized(historyLock) {
            val history = prefs.getString("samples", "").orEmpty()
                .lineSequence().filter { it.isNotBlank() }.toMutableList()
            history += line
            prefs.edit().putString("samples", history.takeLast(MAX_SAMPLES).joinToString("\n")).apply()
        }
    }.fold({ Result.success() }, { Result.retry() })

    companion object {
        private const val WORK_NAME = "cenai_guardian_periodic"
        private const val MAX_SAMPLES = 96
        private val historyLock = Any()

        fun schedule(context: Context) {
            val request = PeriodicWorkRequestBuilder<GuardianWorker>(15, TimeUnit.MINUTES)
                .setInitialDelay(15, TimeUnit.MINUTES)
                .build()
            WorkManager.getInstance(context.applicationContext).enqueueUniquePeriodicWork(
                WORK_NAME,
                ExistingPeriodicWorkPolicy.KEEP,
                request
            )
        }
    }
}
