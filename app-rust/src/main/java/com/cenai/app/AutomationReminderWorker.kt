package com.cenai.app

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.work.CoroutineWorker
import androidx.work.Data
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.ExistingWorkPolicy
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CancellationException

private fun isValidNotificationTag(value: String): Boolean =
    value.matches(Regex("cenai_automation_[0-9a-f]{64}"))

/**
 * OS-managed, best-effort reminder only. It never runs the agent or performs an action.
 * Notification content is generic so private task titles are not exposed on the lock screen.
 */
class AutomationReminderWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result {
        val manager = applicationContext.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL_ID, "CenAI automation reminders", NotificationManager.IMPORTANCE_DEFAULT).apply {
                description = "Reminds you to review scheduled CenAI tasks. No task action runs automatically."
            }
        )
        val notificationTag = inputData.getString(AutomationReminderWorker.NOTIFICATION_TAG_KEY)
            ?.takeIf { isValidNotificationTag(it) }
            ?: return Result.failure()
        val launch = Intent(applicationContext, CenAiNativeActivity::class.java).apply {
            flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP
            data = Uri.parse("cenai://automation-reminder/$notificationTag")
        }
        val pending = PendingIntent.getActivity(
            applicationContext,
            0,
            launch,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
        val notification = Notification.Builder(applicationContext, CHANNEL_ID)
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setContentTitle("CenAI scheduled task")
            .setContentText("A task is ready for your review. Open CenAI to continue.")
            .setStyle(Notification.BigTextStyle().bigText("A scheduled task is ready for your review. Open CenAI to continue. No action has been executed."))
            .setContentIntent(pending)
            .setAutoCancel(true)
            .build()
        return try {
            manager.notify(notificationTag, 0, notification)
            Result.success()
        } catch (_: SecurityException) {
            // POST_NOTIFICATIONS may be denied; do not retry forever or bypass the permission.
            Result.success()
        }
    }

    companion object {
        const val CHANNEL_ID = "cenai_automation_reminders"
        const val NOTIFICATION_TAG_KEY = "notification_tag"
    }
}

class AutomationScheduleRefreshWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result = try {
        if (nativeRefreshAutomationSchedules(applicationContext) >= 0) Result.success() else Result.retry()
    } catch (_: UnsatisfiedLinkError) {
        Result.retry()
    } catch (_: SecurityException) {
        Result.failure()
    } catch (cancelled: CancellationException) {
        throw cancelled
    } catch (error: Exception) {
        android.util.Log.w("CenAI-Automation", "Automation schedule refresh failed; WorkManager will retry.", error)
        Result.retry()
    }

    companion object {
        init { System.loadLibrary("cenai_native") }
        @JvmStatic external fun nativeRefreshAutomationSchedules(context: Context): Int
    }
}

object AutomationReminderBridge {
    private const val UNIQUE_REFRESH = "cenai_automation_schedule_refresh"

    @JvmStatic
    fun scheduleRefresh(context: Context) {
        val request = PeriodicWorkRequestBuilder<AutomationScheduleRefreshWorker>(15, TimeUnit.MINUTES).build()
        WorkManager.getInstance(context.applicationContext).enqueueUniquePeriodicWork(
            UNIQUE_REFRESH, ExistingPeriodicWorkPolicy.KEEP, request
        )
    }

    @JvmStatic
    fun cancelRefresh(context: Context) {
        WorkManager.getInstance(context.applicationContext).cancelUniqueWork(UNIQUE_REFRESH)
    }

    @JvmStatic
    fun schedule(context: Context, notificationTag: String, delayMillis: Long) {
        require(isValidNotificationTag(notificationTag))
        val request = OneTimeWorkRequestBuilder<AutomationReminderWorker>()
            .setInputData(Data.Builder().putString(AutomationReminderWorker.NOTIFICATION_TAG_KEY, notificationTag).build())
            .setInitialDelay(delayMillis.coerceAtLeast(0L), TimeUnit.MILLISECONDS)
            .build()
        WorkManager.getInstance(context.applicationContext).enqueueUniqueWork(
            notificationTag,
            ExistingWorkPolicy.REPLACE,
            request
        )
    }

    @JvmStatic
    fun cancel(context: Context, notificationTag: String) {
        if (!isValidNotificationTag(notificationTag)) return
        WorkManager.getInstance(context.applicationContext).cancelUniqueWork(notificationTag)
        val manager = context.applicationContext.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        manager.cancel(notificationTag, 0)
    }
}
