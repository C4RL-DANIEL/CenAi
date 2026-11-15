package com.cenai.app

import android.content.Context
import androidx.work.CoroutineWorker
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
        val prefs = applicationContext.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val vault = SecureKeyStore(applicationContext)
        val prompt = vault.getSecret("scheduled_task_prompt_$taskId")
        val provider = Provider.parse(inputData.getString(KEY_PROVIDER))
        val model = inputData.getString(KEY_MODEL).orEmpty().ifBlank { provider.model }
        val base = inputData.getString(KEY_BASE).orEmpty().ifBlank { provider.baseUrl }
        if (taskId.isBlank() || prompt.isBlank()) return Result.failure()
        prefs.edit().putString("status_$taskId", "Running").apply()
        return try {
            val response = ChatRepository(vault).stream(
                provider, model, base,
                listOf(
                    ChatMessage("system", "You are CenAI's scheduled task agent. Complete the user's requested task using only the capabilities available in this request. Never claim to have performed external actions you cannot perform. Be concise and report blockers honestly."),
                    ChatMessage("user", prompt)
                )
            ) { }
            prefs.edit()
                .putString("status_$taskId", "Completed")
                .apply()
            vault.putSecret("scheduled_task_result_$taskId", response.take(16_000))
                .putLong("completed_$taskId", System.currentTimeMillis())
                .apply()
            Result.success()
        } catch (cancelled: kotlinx.coroutines.CancellationException) {
            prefs.edit().putString("status_$taskId", "Cancelled").apply()
            throw cancelled
        } catch (error: Exception) {
            val message = (error.message ?: error.javaClass.simpleName).take(500)
            prefs.edit().putString("status_$taskId", "Failed: $message").apply()
            if (error is IOException && runAttemptCount < 3) Result.retry() else Result.failure()
        }
    }

    companion object {
        const val PREFS = "cenai_agent_tasks"
        const val KEY_ID = "task_id"
        const val KEY_PROMPT = "task_prompt"
        const val KEY_PROVIDER = "task_provider"
        const val KEY_MODEL = "task_model"
        const val KEY_BASE = "task_base"

        fun schedule(
            context: Context,
            prompt: String,
            delayMinutes: Long,
            provider: Provider,
            model: String,
            base: String
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
                .build()
            val constraints = Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build()
            val request = OneTimeWorkRequestBuilder<AgentTaskWorker>()
                .setInputData(data)
                .setConstraints(constraints)
                .setInitialDelay(delayMinutes, TimeUnit.MINUTES)
                .addTag("cenai-agent-task")
                .build()
            context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit()
                .putString("status_$id", "Scheduled")
                .apply()
            SecureKeyStore(context.applicationContext).putSecret("scheduled_task_prompt_$id", prompt)
                .putLong("scheduled_$id", System.currentTimeMillis() + TimeUnit.MINUTES.toMillis(delayMinutes))
                .apply()
            WorkManager.getInstance(context.applicationContext).enqueueUniqueWork(
                "cenai-agent-$id", ExistingWorkPolicy.KEEP, request
            )
            return id
        }

        fun cancel(context: Context, taskId: String) {
            WorkManager.getInstance(context.applicationContext).cancelUniqueWork("cenai-agent-$taskId")
            context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
                .edit().putString("status_$taskId", "Cancelled").apply()
            SecureKeyStore(context.applicationContext).putSecret("scheduled_task_prompt_$taskId", "")
        }
    }
}
