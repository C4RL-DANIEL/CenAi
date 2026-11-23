package com.cenai.app

import android.content.Context
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import java.util.concurrent.TimeUnit

/**
 * Best-effort OS-managed worker. Rust executes only plans with explicit
 * background consent, and only cognitive steps with no tools or device access.
 */
class BackgroundTaskWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result {
        return try {
            val result = nativeRunBackgroundTasks(applicationContext)
            if (result >= 0) Result.success() else Result.retry()
        } catch (_: UnsatisfiedLinkError) {
            Result.retry()
        } catch (_: SecurityException) {
            Result.failure()
        }
    }

    companion object {
        private const val UNIQUE_PERIODIC = "cenai_background_cognitive_periodic"
        private const val UNIQUE_IMMEDIATE = "cenai_background_cognitive_immediate"

        init {
            System.loadLibrary("cenai_native")
        }

        @JvmStatic
        external fun nativeRunBackgroundTasks(context: Context): Int
    }
}

object BackgroundTaskBridge {
    @JvmStatic
    fun schedule(context: Context) {
        val constraints = Constraints.Builder()
            .setRequiredNetworkType(NetworkType.CONNECTED)
            .build()
        val manager = WorkManager.getInstance(context.applicationContext)
        val periodic = PeriodicWorkRequestBuilder<BackgroundTaskWorker>(15, TimeUnit.MINUTES)
            .setConstraints(constraints)
            .build()
        manager.enqueueUniquePeriodicWork(
            "cenai_background_cognitive_periodic",
            ExistingPeriodicWorkPolicy.KEEP,
            periodic
        )
        val immediate = OneTimeWorkRequestBuilder<BackgroundTaskWorker>()
            .setConstraints(constraints)
            .build()
        manager.enqueueUniqueWork(
            "cenai_background_cognitive_immediate",
            ExistingWorkPolicy.KEEP,
            immediate
        )
    }

    @JvmStatic
    fun cancel(context: Context) {
        val manager = WorkManager.getInstance(context.applicationContext)
        manager.cancelUniqueWork("cenai_background_cognitive_periodic")
        manager.cancelUniqueWork("cenai_background_cognitive_immediate")
    }
}
