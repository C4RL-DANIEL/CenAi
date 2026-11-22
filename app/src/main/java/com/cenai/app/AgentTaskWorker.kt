package com.cenai.app

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import androidx.work.CoroutineWorker
import androidx.work.ForegroundInfo
import androidx.work.Data
import androidx.work.Constraints
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import java.io.IOException
import java.util.UUID
import java.util.concurrent.TimeUnit

/**
 * Executes an explicitly scheduled, user-authored task while the app is in the background.
 * WorkManager enforces network availability and Android battery policy; this is not an always-on agent.
 */
class AgentTaskWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result {
        val taskId = inputData.getString(KEY_ID).orEmpty()
        val ownerId = inputData.getString(KEY_OWNER_ID).orEmpty().ifBlank { "guest" }
        val prefs = applicationContext.getSharedPreferences(prefsName(ownerId), Context.MODE_PRIVATE)
        val vault = SecureKeyStore(applicationContext)
        val prompt = vault.getUserSecret(ownerId, "scheduled_task_prompt_$taskId")
        val provider = Provider.parse(inputData.getString(KEY_PROVIDER))
        val model = inputData.getString(KEY_MODEL).orEmpty().ifBlank { provider.model }
        val base = inputData.getString(KEY_BASE).orEmpty().ifBlank { provider.baseUrl }
        if (taskId.isBlank() || prompt.isBlank()) return Result.failure()
        return try {
            setForeground(foregroundInfo(taskId))
            prefs.edit().putString("status_$taskId", "Running").apply()
            val repository = ChatRepository(vault) { ownerId }
            val runtime = AgentRuntime(enabledTools = setOf(
                "current_time", "current_date", "calculate", "count_words", "text_stats",
                "format_json", "generate_uuid", "base64_encode", "base64_decode"
            ))
            val toolGuide = "You are CenAI's scheduled background agent. Complete the user's task and use these safe local tools when useful: " +
                runtime.availableTools().joinToString { it.name + " — " + it.description } +
                ". Tool calls must use [[CENAI_TOOL_CALL]] followed by JSON {name,arguments} and [[/CENAI_TOOL_CALL]]. " +
                "Background tasks cannot execute Android device actions, send messages, access arbitrary files or run shell commands. Never claim to have performed external actions you cannot perform."
            val history = mutableListOf(ChatMessage("user", prompt))
            var response = ""
            var lastTurn = ""
            var finished = false
            for (step in 0..3) {
                val turn = repository.stream(provider, model, base, listOf(ChatMessage("system", toolGuide)) + history) { }
                lastTurn = turn
                val call = runtime.parseToolCall(turn)
                if (call == null) {
                    response = turn
                    finished = true
                    break
                }
                val execution = runtime.execute(call.first, call.second)
                if (!execution.completed) {
                    response = "The scheduled agent could not complete a requested tool action: " + execution.output
                    finished = true
                    break
                }
                history += ChatMessage("assistant", turn)
                history += ChatMessage("user", "Tool result for " + execution.toolName + ": " + execution.output + "\nContinue the task. Use another safe tool if needed, otherwise provide the final answer.")
            }
            if (!finished) response = "The scheduled agent reached its four-step tool limit. Last model output: " + lastTurn.take(2_000)
            if (response.isBlank()) response = "The scheduled agent returned an empty response."
            prefs.edit().putString("status_$taskId", "Completed")
                .putLong("completed_$taskId", System.currentTimeMillis()).apply()
            vault.putUserSecret(ownerId, "scheduled_task_result_$taskId", response.take(16_000))
            postNotification(taskId, "Completed", response)
            Result.success()
        } catch (cancelled: kotlinx.coroutines.CancellationException) {
            prefs.edit().putString("status_$taskId", "Cancelled").apply()
            throw cancelled
        } catch (error: Exception) {
            val message = (error.message ?: error.javaClass.simpleName).take(500)
            prefs.edit().putString("status_$taskId", "Failed: $message").apply()
            postNotification(taskId, "Failed", message)
            if (error is IOException && runAttemptCount < 3) Result.retry() else Result.failure()
        }
    }

    private fun foregroundInfo(taskId: String): ForegroundInfo {
        val manager = applicationContext.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        if (Build.VERSION.SDK_INT >= 26) {
            manager.createNotificationChannel(
                NotificationChannel(CHANNEL_ID, "CenAI agent tasks", NotificationManager.IMPORTANCE_LOW)
            )
        }
        val openApp = PendingIntent.getActivity(
            applicationContext,
            taskId.hashCode(),
            Intent(applicationContext, MainActivity::class.java)
                .addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
        val notification = NotificationCompat.Builder(applicationContext, CHANNEL_ID)
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setContentTitle("CenAI task running")
            .setContentText("A scheduled task is working in the background.")
            .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
            .setContentIntent(openApp)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .build()
        return if (Build.VERSION.SDK_INT >= 29) {
            ForegroundInfo(taskId.hashCode(), notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
        } else {
            ForegroundInfo(taskId.hashCode(), notification)
        }
    }

    private fun postNotification(taskId: String, status: String, detail: String) {
        if (Build.VERSION.SDK_INT >= 33 && ContextCompat.checkSelfPermission(applicationContext, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) return
        runCatching {
            val manager = applicationContext.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
            if (Build.VERSION.SDK_INT >= 26) manager.createNotificationChannel(NotificationChannel(CHANNEL_ID, "CenAI agent tasks", NotificationManager.IMPORTANCE_DEFAULT))
            val openApp = PendingIntent.getActivity(applicationContext, taskId.hashCode(), Intent(applicationContext, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP), PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
            val notification = NotificationCompat.Builder(applicationContext, CHANNEL_ID)
                .setSmallIcon(android.R.drawable.ic_dialog_info)
                .setContentTitle("CenAI background task " + status)
                .setContentText("Tap to open CenAI and review the private result.")
                .setStyle(NotificationCompat.BigTextStyle().bigText("Your scheduled task finished. Open CenAI to review its private result."))
                .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
                .setContentIntent(openApp).setAutoCancel(true).build()
            NotificationManagerCompat.from(applicationContext).notify(taskId.hashCode(), notification)
        }
    }
    companion object {
        private const val CHANNEL_ID = "cenai_agent_tasks"
        const val PREFS = "cenai_agent_tasks"
        const val KEY_ID = "task_id"
        const val KEY_PROMPT = "task_prompt"
        const val KEY_PROVIDER = "task_provider"
        const val KEY_MODEL = "task_model"
        const val KEY_BASE = "task_base"
        const val KEY_OWNER_ID = "task_owner_id"

        fun prefsName(ownerId: String): String =
            PREFS + "_" + ownerId.filter { it.isLetterOrDigit() || it == '_' || it == '-' }.ifBlank { "guest" }

        fun schedule(
            context: Context,
            prompt: String,
            delayMinutes: Long,
            provider: Provider,
            model: String,
            base: String,
            ownerId: String = "guest"
        ): String {
            require(prompt.isNotBlank()) { "Enter a task before scheduling it." }
            require(prompt.length <= 8_000) { "Scheduled tasks are limited to 8,000 characters." }
            require(delayMinutes in 1..43_200) { "Schedule between 1 minute and 30 days from now." }
            val id = UUID.randomUUID().toString()
            val data = Data.Builder()
                .putString(KEY_ID, id)
                .putString(KEY_PROVIDER, provider.name)
                .putString(KEY_MODEL, model)
                .putString(KEY_BASE, base)
                .putString(KEY_OWNER_ID, ownerId)
                .build()
            val constraints = Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build()
            val request = OneTimeWorkRequestBuilder<AgentTaskWorker>()
                .setInputData(data)
                .setConstraints(constraints)
                .setInitialDelay(delayMinutes, TimeUnit.MINUTES)
                .addTag("cenai-agent-task")
                .build()
            context.getSharedPreferences(prefsName(ownerId), Context.MODE_PRIVATE).edit()
                .putString("status_$id", "Scheduled")
                .putLong("scheduled_$id", System.currentTimeMillis() + TimeUnit.MINUTES.toMillis(delayMinutes))
                .apply()
            SecureKeyStore(context.applicationContext).putUserSecret(ownerId, "scheduled_task_prompt_$id", prompt)
            WorkManager.getInstance(context.applicationContext).enqueueUniqueWork(
                "cenai-agent-$id", ExistingWorkPolicy.KEEP, request
            )
            return id
        }

        fun cancel(context: Context, taskId: String, ownerId: String = "guest") {
            WorkManager.getInstance(context.applicationContext).cancelUniqueWork("cenai-agent-$taskId")
            context.getSharedPreferences(prefsName(ownerId), Context.MODE_PRIVATE)
                .edit().putString("status_$taskId", "Cancelled").apply()
            SecureKeyStore(context.applicationContext).putUserSecret(ownerId, "scheduled_task_prompt_$taskId", "")
        }
    }
}
