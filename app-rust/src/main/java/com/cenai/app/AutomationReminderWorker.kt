package com.cenai.app

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.work.CoroutineWorker
import androidx.work.ExistingWorkPolicy
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import java.util.concurrent.TimeUnit

/**
 * OS-managed, best-effort reminder only. It never runs the agent or performs an action.
 * Android may defer or suppress this work according to battery and notification policy.
 */
class AutomationReminderWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result {
        val manager = applicationContext.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        val channel = NotificationChannel(CHANNEL_ID, "CenAI automation reminders", NotificationManager.IMPORTANCE_DEFAULT).apply {
            description = "Reminds you to review scheduled CenAI tasks. No task action runs automatically."
        }
        manager.createNotificationChannel(channel)
        val launch = Intent(applicationContext, CenAiNativeActivity::class.java).apply {
            flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP
        }
        val pending = PendingIntent.getActivity(
            applicationContext,
            id.hashCode(),
            launch,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
        val notification = Notification.Builder(applicationContext, CHANNEL_ID)
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setContentTitle("CenAI task ready for review")
            .setContentText(title.take(120))
            .setStyle(Notification.BigTextStyle().bigText("$title\n\nOpen CenAI to review. No action has been executed."))
            .setContentIntent(pending)
            .setAutoCancel(true)
            .build()
        return try {
            manager.notify(taskId.hashCode(), notification)
            Result.success()
        } catch (_: SecurityException) {
            // POST_NOTIFICATIONS may be denied; do not retry forever or bypass the permission.
            Result.success()
        }
    }

    companion object {
        const val CHANNEL_ID = "cenai_automation_reminders"
    }
}

object AutomationReminderBridge {
    @JvmStatic
    fun schedule(context: Context, ownerScope: String, taskId: String, delayMillis: Long) {
        require(ownerScope.isNotBlank() && taskId.isNotBlank())
        val request = OneTimeWorkRequestBuilder<AutomationReminderWorker>()
            .setInitialDelay(delayMillis.coerceAtLeast(0L), TimeUnit.MILLISECONDS)
            .build()
        WorkManager.getInstance(context.applicationContext).enqueueUniqueWork(
            workName(ownerScope, taskId),
            ExistingWorkPolicy.REPLACE,
            request
        )
    }

    @JvmStatic
    fun cancel(context: Context, ownerScope: String, taskId: String) {
        if (ownerScope.isBlank() || taskId.isBlank()) return
        WorkManager.getInstance(context.applicationContext).cancelUniqueWork(workName(ownerScope, taskId))
    }

    private fun workName(ownerScope: String, taskId: String) =
        "cenai_automation_${ownerScope}_${taskId}"
}
