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
import androidx.work.ExistingWorkPolicy
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import java.security.MessageDigest
import java.util.concurrent.TimeUnit

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
            ?.takeIf { it.startsWith("cenai_automation_") }
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

object AutomationReminderBridge {
    @JvmStatic
    fun schedule(context: Context, ownerScope: String, taskId: String, delayMillis: Long) {
        require(ownerScope.isNotBlank() && taskId.isNotBlank())
        val name = workName(ownerScope, taskId)
        val request = OneTimeWorkRequestBuilder<AutomationReminderWorker>()
            .setInputData(Data.Builder().putString(AutomationReminderWorker.NOTIFICATION_TAG_KEY, name).build())
            .setInitialDelay(delayMillis.coerceAtLeast(0L), TimeUnit.MILLISECONDS)
            .build()
        WorkManager.getInstance(context.applicationContext).enqueueUniqueWork(
            name,
            ExistingWorkPolicy.REPLACE,
            request
        )
    }

    @JvmStatic
    fun cancel(context: Context, ownerScope: String, taskId: String) {
        if (ownerScope.isBlank() || taskId.isBlank()) return
        val name = workName(ownerScope, taskId)
        WorkManager.getInstance(context.applicationContext).cancelUniqueWork(name)
        val manager = context.applicationContext.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        manager.cancel(name, 0)
    }

    private fun workName(ownerScope: String, taskId: String): String {
        val digest = MessageDigest.getInstance("SHA-256").digest("$ownerScope:$taskId".toByteArray(Charsets.UTF_8))
        val token = digest.joinToString("") { "%02x".format(it.toInt() and 0xff) }
        return "cenai_automation_$token"
    }
}
