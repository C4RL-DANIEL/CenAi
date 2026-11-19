package com.cenai.app

import com.cenai.app.ui.components.HomeActionCard
import com.cenai.app.ui.components.MessageBubble
import com.cenai.app.ui.components.SuggestionChip
import com.cenai.app.ui.components.ThinkingIndicator

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.graphics.Bitmap
import android.graphics.pdf.PdfRenderer
import android.os.ParcelFileDescriptor
import android.app.Activity
import android.speech.RecognizerIntent
import android.speech.tts.TextToSpeech
import android.provider.OpenableColumns
import com.tom_roush.pdfbox.android.PDFBoxResourceLoader
import com.tom_roush.pdfbox.pdmodel.PDDocument
import com.tom_roush.pdfbox.text.PDFTextStripper
import com.google.mlkit.vision.common.InputImage
import com.google.mlkit.vision.text.TextRecognition
import com.google.mlkit.vision.text.latin.TextRecognizerOptions
import android.net.Uri
import android.util.Base64
import java.util.Locale
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.ContextCompat
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.fadeIn
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.ArrowUpward
import androidx.compose.material.icons.filled.AttachFile
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.History
import androidx.compose.material.icons.filled.Mic
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Share
import androidx.compose.material.icons.filled.Save
import androidx.compose.material.icons.filled.VolumeUp
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Stop
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.blur
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.input.PasswordVisualTransformation
import org.json.JSONObject
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.DialogProperties
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import java.util.UUID
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

private suspend fun extractImageText(context: android.content.Context, uri: Uri): String =
    suspendCancellableCoroutine { continuation ->
        val recognizer = TextRecognition.getClient(TextRecognizerOptions.DEFAULT_OPTIONS)
        val task = runCatching { recognizer.process(InputImage.fromFilePath(context, uri)) }
            .getOrElse {
                recognizer.close()
                continuation.resumeWithException(it)
                return@suspendCancellableCoroutine
            }
        task.addOnSuccessListener { result ->
            recognizer.close()
            if (continuation.isActive) continuation.resume(result.text.take(250_000))
        }.addOnFailureListener { error ->
            recognizer.close()
            if (continuation.isActive) continuation.resumeWithException(error)
        }
        continuation.invokeOnCancellation { recognizer.close() }
    }


private suspend fun recognizeBitmapText(bitmap: Bitmap): String {
    val recognizer = TextRecognition.getClient(TextRecognizerOptions.DEFAULT_OPTIONS)
    return try {
        suspendCancellableCoroutine { continuation ->
            recognizer.process(InputImage.fromBitmap(bitmap, 0))
                .addOnSuccessListener { result ->
                    if (continuation.isActive) continuation.resume(result.text)
                }
                .addOnFailureListener { error ->
                    if (continuation.isActive) continuation.resumeWithException(error)
                }
            continuation.invokeOnCancellation { recognizer.close() }
        }
    } finally {
        recognizer.close()
    }
}

private suspend fun extractScannedPdfText(context: android.content.Context, uri: Uri): String {
    val descriptor = context.contentResolver.openFileDescriptor(uri, "r")
        ?: throw IllegalStateException("Android could not open this PDF for OCR.")
    return descriptor.use { fd ->
        PdfRenderer(fd).use { renderer ->
            require(renderer.pageCount in 1..50) {
                "This scanned PDF has ${renderer.pageCount} pages. OCR is limited to 50 pages per attachment."
            }
            val output = StringBuilder()
            for (index in 0 until renderer.pageCount) {
                renderer.openPage(index).use { page ->
                    val scale = minOf(2f, 2200f / maxOf(page.width, page.height).toFloat())
                    val bitmap = Bitmap.createBitmap(
                        (page.width * scale).toInt().coerceAtLeast(1),
                        (page.height * scale).toInt().coerceAtLeast(1),
                        Bitmap.Config.ARGB_8888
                    )
                    try {
                        page.render(bitmap, null, null, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
                        val text = recognizeBitmapText(bitmap)
                        if (text.isNotBlank()) output.append("\n\n[PDF page ${index + 1}]\n").append(text)
                    } finally {
                        bitmap.recycle()
                    }
                }
                if (output.length >= 250_000) break
            }
            output.toString().take(250_000)
        }
    }
}

@Composable
fun CenAiScreen(vault: SecureKeyStore) {
    var selectedStyle by remember { mutableStateOf(vault.get("ui_style", "STUDIO").let { if (it == "AURORA" || it == "DEFAULT") "STUDIO" else it }) }
    CenAiTheme(style = selectedStyle) {
        CenAiScreenContent(vault) { style ->
            selectedStyle = style
            vault.put("ui_style", style)
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun CenAiScreenContent(vault: SecureKeyStore, onUiStyleChange: (String) -> Unit) {
    val palette = LocalCenAiPalette.current
    val context = LocalContext.current
    val isTabletLayout = LocalConfiguration.current.screenWidthDp >= 600
    var wideDestination by remember { mutableStateOf("Home") }
    var selectedSettingsPage by remember { mutableStateOf("Appearance") }
    var selectedSettingsCategory by remember { mutableStateOf("Appearance") }
    var moreActionsExpanded by remember { mutableStateOf(false) }
    val notificationPermissionLauncher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { }
    val repo = remember { ChatRepository(vault) }
    val store = remember { ChatStore(context) }
    val updater = remember { UpdateManager(context) }
    val localModelRuntime = remember { LocalModelRuntime(context) }
    val guardian = remember { GuardianMonitor(context) }
    val cloud = remember { CenAiCloud(vault) }
    var speechOutputReady by remember { mutableStateOf(false) }
    var speechRate by remember { mutableFloatStateOf(vault.get("speech_rate", "1.0").toFloatOrNull()?.coerceIn(0.5f, 2f) ?: 1f) }
    var speechPitch by remember { mutableFloatStateOf(vault.get("speech_pitch", "1.0").toFloatOrNull()?.coerceIn(0.5f, 2f) ?: 1f) }
    var speechLanguage by remember { mutableStateOf(vault.get("speech_language", "device")) }
    var speechInputLanguage by remember { mutableStateOf(vault.get("speech_input_language", "device")) }
    val textToSpeech = remember { TextToSpeech(context) { status -> speechOutputReady = status == TextToSpeech.SUCCESS } }
    LaunchedEffect(speechOutputReady, speechRate, speechPitch, speechLanguage) {
        if (speechOutputReady) {
            textToSpeech.setSpeechRate(speechRate)
            textToSpeech.setPitch(speechPitch)
            val locale = if (speechLanguage == "device") Locale.getDefault() else Locale.forLanguageTag(speechLanguage)
            if (textToSpeech.setLanguage(locale) < TextToSpeech.LANG_AVAILABLE) textToSpeech.setLanguage(Locale.getDefault())
        }
    }
    DisposableEffect(textToSpeech) { onDispose { textToSpeech.stop(); textToSpeech.shutdown() } }
    val localKnowledge = remember { LocalKnowledgeStore() }
    val appMonitor = remember { AppMonitor(context) }
    var localKnowledgeEnabled by remember { mutableStateOf(vault.get("local_knowledge_enabled", "false").toBoolean()) }
    var localModelEnabled by remember { mutableStateOf(vault.get("local_model_enabled", "false").toBoolean()) }
    var localModelReady by remember { mutableStateOf(localModelRuntime.isModelInstalled()) }
    var guardianSnapshot by remember { mutableStateOf(guardian.snapshot()) }
    var guardianHistoryCount by remember { mutableIntStateOf(context.getSharedPreferences("cenai_guardian_history", android.content.Context.MODE_PRIVATE).getString("samples", "").orEmpty().lineSequence().count { it.isNotBlank() }) }
    var monitorSnapshot by remember { mutableStateOf(appMonitor.snapshot()) }
    val scope = rememberCoroutineScope()
    val clipboard = LocalClipboardManager.current
    var provider by remember { mutableStateOf(Provider.parse(vault.get("provider", "OPENAI"))) }
    var model by remember {
        val savedProvider = Provider.parse(vault.get("provider", "OPENAI"))
        val savedModel = vault.get("model", savedProvider.model)
        mutableStateOf(if (savedProvider == Provider.DEEPSEEK && savedModel in setOf("deepseek-chat", "deepseek-reasoner")) savedProvider.model else savedModel)
    }
    var base by remember { mutableStateOf(vault.get("base_url", provider.baseUrl)) }
    var providerKeys by remember { mutableStateOf(Provider.entries.associateWith { vault.apiKey(it) }) }
    var multiAgent by remember { mutableStateOf(vault.get("multi_agent", "false").toBoolean()) }
    var toolEnabled by remember { mutableStateOf(listOf("current_time", "calculate", "count_words", "format_json", "current_date", "generate_uuid", "text_stats", "base64_encode", "base64_decode", "open_url", "open_dialer", "share_text", "open_app_settings").associateWith { vault.get("tool_enabled_" + it, "true").toBoolean() }) }
    var agentPlanning by remember { mutableStateOf(vault.get("agent_planning", "false").toBoolean()) }
    var googleOAuthClientId by remember { mutableStateOf(vault.get("google_oauth_client_id")) }
    var googleCloudProjectId by remember { mutableStateOf(vault.get("google_cloud_project_id")) }
    var dailyLimit by remember { mutableStateOf(vault.get("daily_request_limit", "0")) }
    var updateChannel by remember { mutableStateOf(vault.get("update_channel", "stable")) }
    var systemPrompt by remember { mutableStateOf(vault.get("system_prompt", "")) }
    var agentMemory by remember { mutableStateOf(vault.getSecret("agent_memory")) }
    var customHeaders by remember { mutableStateOf(vault.get("custom_headers", "{}")) }
    var customParams by remember { mutableStateOf(vault.get("custom_params", "{}")) }
    var maxTokens by remember { mutableStateOf(vault.get("max_tokens", "4096")) }
    var draft by remember { mutableStateOf("") }
    var settings by remember { mutableStateOf(false) }
    var historyOpen by remember { mutableStateOf(false) }
    var historyQuery by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var generationHasStarted by remember { mutableStateOf(false) }
    var generationCompleted by remember { mutableStateOf(false) }
    LaunchedEffect(busy) {
        if (busy) {
            generationHasStarted = true
            generationCompleted = false
        } else if (generationHasStarted) {
            generationHasStarted = false
            generationCompleted = true
            delay(CenAiMotion.completionHoldMs.toLong())
            generationCompleted = false
        }
    }
    var error by remember { mutableStateOf("") }
    var info by remember { mutableStateOf("") }
    var testBusy by remember { mutableStateOf(false) }
    var conversations by remember { mutableStateOf(store.load()) }
    var currentId by remember { mutableStateOf(conversations.firstOrNull()?.id ?: UUID.randomUUID().toString()) }
    var renameDialogOpen by remember { mutableStateOf(false) }
    var renameDraft by remember { mutableStateOf("") }
    var messages by remember { mutableStateOf(conversations.firstOrNull()?.messages?.toList() ?: emptyList()) }
    var activeJob by remember { mutableStateOf<Job?>(null) }
    var editIndex by remember { mutableStateOf<Int?>(null) }
    var editText by remember { mutableStateOf("") }
    var updateInfo by remember { mutableStateOf<UpdateInfo?>(null) }
    var updateDialog by remember { mutableStateOf(false) }
    var updateBusy by remember { mutableStateOf(false) }
    var updateProgress by remember { mutableIntStateOf(0) }
    var updateStatus by remember { mutableStateOf("Check for the latest CenAI release.") }
    val listState = rememberLazyListState()
    val localModelPicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) scope.launch {
            info = "Importing on-device model…"
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    val displayName = context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
                        if (cursor.moveToFirst()) cursor.getString(0) else null
                    } ?: "selected-model.litertlm"
                    localModelRuntime.importModel(uri, displayName)
                }
            }
            if (result.isSuccess) {
                localModelReady = true
                localModelEnabled = true
                vault.put("local_model_enabled", "true")
                info = "Offline model imported (${result.getOrThrow() / 1_048_576L} MB)."
            } else info = result.exceptionOrNull()?.message ?: "Model import failed."
            guardianSnapshot = guardian.snapshot()
        }
    }
    LaunchedEffect(Unit) { appMonitor.recordEvent("app_start"); monitorSnapshot = appMonitor.snapshot() }

    fun persist(next: List<ChatMessage> = messages) {
        messages = next
        val list = conversations.toMutableList()
        val index = list.indexOfFirst { it.id == currentId }
        val firstPrompt = next.firstOrNull { it.role == "user" }?.content.orEmpty()
        val title = firstPrompt.take(42).ifBlank { "New chat" }
        val conversation = Conversation(currentId, title, next.toMutableList(), System.currentTimeMillis())
        if (index >= 0) list[index] = conversation else list.add(0, conversation)
        conversations = list.sortedByDescending { it.updatedAt }.toMutableList()
        store.save(conversations)
    }

    fun newChat() {
        activeJob?.cancel()
        busy = false
        currentId = UUID.randomUUID().toString()
        messages = emptyList()
        draft = ""
        error = ""
    }

    fun startRequest(requestMessages: List<ChatMessage>) {
        if (busy) return
        val today = java.time.LocalDate.now().toString()
        if (vault.get("usage_day") != today) {
            vault.put("usage_day", today)
            vault.put("usage_count", "0")
        }
        val used = vault.get("usage_count", "0").toIntOrNull() ?: 0
        val limit = dailyLimit.toIntOrNull() ?: 0
        if (limit > 0 && used >= limit) {
            error = "Daily message limit reached. Change it in Settings or try again tomorrow."
            return
        }
        vault.put("usage_count", (used + 1).toString())
        val inputEstimate = requestMessages.sumOf { it.content.length } / 4
        val totalEstimate = vault.get("usage_estimated_tokens", "0").toLongOrNull() ?: 0L
        vault.put("usage_estimated_tokens", (totalEstimate + inputEstimate).toString())
        if (localModelEnabled && !localModelReady) { error = "Import a compatible .litertlm model in Settings before enabling Offline AI."; return }
        val targets = if (localModelEnabled) listOf(provider) else if (multiAgent) Provider.entries.filter { !providerKeys[it].isNullOrBlank() || (it == Provider.GEMINI && vault.getSecret("gemini_oauth_access_token").isNotBlank()) || (it == Provider.CENAI_CLOUD && cloud.isSignedIn()) } else listOf(provider)
        if (targets.isEmpty()) {
            error = "Add at least one provider API key to use multi-agent mode."
            return
        }
        busy = true
        error = ""
        persist(requestMessages)
        val queryForKnowledge = requestMessages.lastOrNull { it.role == "user" }?.content.orEmpty()
            .replace(Regex("""\[\[CENAI_IMAGE\|[^|]+\|[A-Za-z0-9+/=]+]]"""), "[Image attached]")
        appMonitor.recordStarted()
        monitorSnapshot = appMonitor.snapshot()
        val requestStartedAt = System.currentTimeMillis()
        activeJob = scope.launch {
            try {
                val localKnowledgeContext = if (localKnowledgeEnabled) withContext(Dispatchers.Default) {
                    localKnowledge.retrieve(queryForKnowledge, conversations, requestMessages)
                } else ""
                val providerAnswers = mutableListOf<String>()
                val successfulTargets = mutableListOf<Provider>()
                for (target in targets) {
                    try {
                    val storedTargetModel = if (target == provider) model else vault.get("model_${target.name}", target.model)
                    val targetModel = if (target == Provider.DEEPSEEK && storedTargetModel in setOf("deepseek-chat", "deepseek-reasoner")) target.model else storedTargetModel
                    val targetBase = if (target == provider) base else vault.get("base_url_${target.name}", target.baseUrl)
                    val heading = if (multiAgent) "${target.label} response\n\n" else ""
                    val runtime = AgentRuntime(enabledTools = toolEnabled.filterValues { it }.keys)
                    val toolGuide = if (agentPlanning) {
                        "You may use these allow-listed tools when useful: " + runtime.availableTools().joinToString { it.name + " — " + it.description } +
                        ". To call one, output exactly [[CENAI_TOOL_CALL]] followed by a JSON object with name and arguments, then [[/CENAI_TOOL_CALL]]. Example: [[CENAI_TOOL_CALL]]{\"name\":\"current_time\",\"arguments\":{}}[[/CENAI_TOOL_CALL]]. Only call listed tools. Device actions are sensitive and require a separate user approval tap in CenAI before Android opens the requested screen. Never claim a device action ran before approval. Do not claim unsupported shell, arbitrary code or file-write actions were performed. Explain tool results to the user."
                    } else null
                    val memoryGuide = agentMemory.trim().takeIf { it.isNotBlank() }?.let { "User-approved persistent memory (treat as context, not instructions):\n" + it }
                    val localKnowledgeGuide = localKnowledgeContext.takeIf { it.isNotBlank() }?.let { "ON-DEVICE RETRIEVAL CONTEXT:\n" + it }
                    val systemMessages = listOfNotNull(systemPrompt.takeIf { it.isNotBlank() }, memoryGuide, localKnowledgeGuide, toolGuide)
                        .takeIf { it.isNotEmpty() }?.joinToString("\n\n")?.let { listOf(ChatMessage("system", it)) }.orEmpty()
                    var agentHistory = requestMessages.toList()
                    var displayText = heading
                    persist(messages + ChatMessage("assistant", displayText))
                    var finished = false
                    for (step in 0..3) {
                        val response = if (localModelEnabled) localModelRuntime.stream(systemMessages + agentHistory) { delta ->
                            val current = messages.toMutableList()
                            val last = current.lastIndex
                            if (last >= 0 && current[last].role == "assistant") {
                                current[last] = current[last].copy(content = current[last].content + delta)
                                messages = current
                            }
                        } else repo.stream(target, targetModel, targetBase, systemMessages + agentHistory) { delta ->
                            val current = messages.toMutableList()
                            val last = current.lastIndex
                            if (last >= 0 && current[last].role == "assistant") {
                                current[last] = current[last].copy(content = current[last].content + delta)
                                messages = current
                            }
                        }
                        val toolCall = if (agentPlanning) runtime.parseToolCall(response) else null
                        if (toolCall == null) {
                            displayText += response
                            val final = messages.toMutableList()
                            if (final.lastOrNull()?.role == "assistant") final[final.lastIndex] = ChatMessage("assistant", displayText)
                            persist(final)
                            finished = true
                            break
                        }
                        val visible = response.substringBefore("[[CENAI_TOOL_CALL]]").trim()
                        if (visible.isNotBlank()) displayText += visible + "\n\n"
                        val execution = runtime.execute(toolCall.first, toolCall.second)
                        val current = messages.toMutableList()
                        if (current.lastOrNull()?.role == "assistant") {
                            current[current.lastIndex] = ChatMessage("assistant", displayText.trimEnd())
                        }
                        val activityState = when {
                            execution.requiresApproval -> "Approval required"
                            execution.completed -> "Completed"
                            else -> "Unavailable"
                        }
                        current += ChatMessage("tool", "$activityState · ${execution.toolName}\n${execution.output}\nARGS:${execution.arguments}")
                        if (!execution.requiresApproval) current += ChatMessage("assistant", "")
                        persist(current)
                        if (execution.requiresApproval) {
                            finished = true
                            break
                        }
                        agentHistory = agentHistory + ChatMessage("assistant", response) +
                            ChatMessage("user", "Tool result for " + execution.toolName + ": " + execution.output + "\nContinue the task. If another safe tool is needed, request it using the same tool-call format.")
                    }
                    if (!finished) {
                        val current = messages.toMutableList()
                        if (current.lastOrNull()?.role == "assistant") current[current.lastIndex] = ChatMessage("assistant", displayText + "\n\nAgent step limit reached. Ask me to continue if needed.")
                        persist(current)
                    }
                    providerAnswers += "${target.label}:\n" + displayText.trim().take(8_000)
                    successfulTargets += target
                    } catch (cancelled: kotlinx.coroutines.CancellationException) {
                        throw cancelled
                    } catch (failure: Exception) {
                        if (!multiAgent) throw failure
                        val failureSummary = "${target.label}: provider request failed — " + (failure.message ?: failure.javaClass.simpleName)
                        providerAnswers += failureSummary
                        persist(messages + ChatMessage("assistant", failureSummary))
                    }
                }
                if (multiAgent && successfulTargets.size > 1 && !localModelEnabled) {
                    val synthesisProvider = successfulTargets.first()
                    val synthesisModel = if (synthesisProvider == provider) model else vault.get("model_${synthesisProvider.name}", synthesisProvider.model)
                    val synthesisBase = if (synthesisProvider == provider) base else vault.get("base_url_${synthesisProvider.name}", synthesisProvider.baseUrl)
                    val synthesisPrompt = "USER REQUEST:\n" + queryForKnowledge.take(8_000) +
                        "\n\nCompare these independent AI responses to that request. Produce one clear, accurate, balanced answer. Resolve contradictions where evidence permits, preserve useful disagreements and uncertainty, remove repetition, and do not invent facts.\n\n" +
                        providerAnswers.joinToString("\n\n---\n\n").take(32_000)
                    val synthesisHeader = "Cross-provider synthesis\n\n"
                    persist(messages + ChatMessage("assistant", synthesisHeader))
                    val synthesisText = StringBuilder()
                    runCatching {
                        repo.stream(
                            synthesisProvider,
                            synthesisModel,
                            synthesisBase,
                            listOf(
                                ChatMessage("system", "You are CenAI's synthesis coordinator. Be precise, transparent about disagreements, and concise."),
                                ChatMessage("user", synthesisPrompt)
                            )
                        ) { delta ->
                            synthesisText.append(delta)
                            val current = messages.toMutableList()
                            if (current.lastOrNull()?.role == "assistant") {
                                current[current.lastIndex] = ChatMessage("assistant", synthesisHeader + synthesisText)
                                messages = current
                            }
                        }
                    }.onSuccess { result ->
                        val current = messages.toMutableList()
                        if (current.lastOrNull()?.role == "assistant") current[current.lastIndex] = ChatMessage("assistant", synthesisHeader + result)
                        persist(current)
                    }.onFailure { failure ->
                        val current = messages.toMutableList()
                        if (current.lastOrNull()?.role == "assistant") current[current.lastIndex] = ChatMessage("assistant", synthesisHeader + "I collected the provider responses, but the synthesis request failed: " + (failure.message ?: "unknown error"))
                        persist(current)
                    }
                }
                val completedLatency = System.currentTimeMillis() - requestStartedAt
                appMonitor.recordSucceeded(completedLatency)
                if (cloud.isSignedIn() && vault.get("cloud_usage_analytics_consent", "false").toBoolean()) {
                    runCatching {
                        val outputEstimate = messages.lastOrNull { it.role == "assistant" }?.content?.length?.div(4) ?: 0
                        cloud.recordUsageEvent(if (localModelEnabled) "local" else provider.name.lowercase(), if (localModelEnabled) "on-device" else model, inputEstimate, outputEstimate, completedLatency, "success")
                    }
                }
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) {
                    appMonitor.recordCancelled()
                } else {
                    val failedLatency = System.currentTimeMillis() - requestStartedAt
                    appMonitor.recordFailed(failedLatency, e)
                    if (cloud.isSignedIn() && vault.get("cloud_usage_analytics_consent", "false").toBoolean()) {
                        runCatching { cloud.recordUsageEvent(if (localModelEnabled) "local" else provider.name.lowercase(), if (localModelEnabled) "on-device" else model, inputEstimate, 0, failedLatency, "error", e.javaClass.simpleName) }
                    }
                    error = e.message ?: "Request failed."
                }
                if (messages.lastOrNull()?.let { it.role == "assistant" && it.content.isBlank() } == true) {
                    persist(messages.dropLast(1))
                } else persist()
            } finally {
                monitorSnapshot = appMonitor.snapshot()
                busy = false
                activeJob = null
            }
        }
    }

    val speechLauncher = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
        if (result.resultCode == Activity.RESULT_OK) {
            val spoken = result.data?.getStringArrayListExtra(RecognizerIntent.EXTRA_RESULTS)?.firstOrNull().orEmpty()
            if (spoken.isNotBlank()) { draft = listOf(draft, spoken).filter { it.isNotBlank() }.joinToString(" "); appMonitor.recordEvent("voice_input"); monitorSnapshot = appMonitor.snapshot() }
        }
    }

    val saveConversationLauncher = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("text/markdown")) { uri ->
        if (uri != null) scope.launch {
            val export = messages.joinToString("\n\n") { message ->
                val speaker = when (message.role) { "user" -> "You"; "assistant" -> "CenAI"; "tool" -> "Agent activity"; else -> message.role }
                "## $speaker\n\n${message.content}"
            }
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    context.contentResolver.openOutputStream(uri)?.bufferedWriter()?.use { it.write("# CenAI conversation\n\n$export\n") }
                        ?: error("Could not open the selected file for writing.")
                }
            }
            info = if (result.isSuccess) "Conversation saved to your selected location." else result.exceptionOrNull()?.message ?: "Could not save conversation."
        }
    }

    val attachmentPicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) scope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    val mime = context.contentResolver.getType(uri).orEmpty().lowercase()
                    if (mime.startsWith("image/")) {
                        val bytes = context.contentResolver.openInputStream(uri)?.use { it.readBytes() }
                            ?: throw IllegalStateException("Android could not read this image.")
                        require(bytes.size in 1..4_000_000) { "Images must be 4 MB or smaller. Choose a smaller image." }
                        val safeMime = mime.takeIf { it in setOf("image/jpeg", "image/png", "image/webp", "image/gif") }
                            ?: throw IllegalStateException("This image format is not supported for vision requests.")
                        val encoded = Base64.encodeToString(bytes, Base64.NO_WRAP)
                        val ocr = runCatching { extractImageText(context, uri) }.getOrDefault("")
                        if (localModelEnabled) {
                            "[Image attached; offline model is text-only]" + if (ocr.isNotBlank()) "\n\n[On-device OCR reference]\n" + ocr else ""
                        } else {
                            "[[CENAI_IMAGE|$safeMime|$encoded]]" + if (ocr.isNotBlank()) "\n\n[On-device OCR reference]\n" + ocr else ""
                        }
                    } else if (mime == "application/pdf" || uri.toString().lowercase().endsWith(".pdf")) {
                        PDFBoxResourceLoader.init(context.applicationContext)
                        val selectableText = context.contentResolver.openInputStream(uri)?.use { input ->
                            PDDocument.load(input).use { document ->
                                require(!document.isEncrypted) { "Password-protected PDFs are not supported yet." }
                                require(document.numberOfPages in 1..300) { "PDFs must contain between 1 and 300 pages." }
                                PDFTextStripper().apply { endPage = minOf(document.numberOfPages, 300) }
                                    .getText(document).take(250_000)
                            }
                        } ?: throw IllegalStateException("Android could not open this PDF.")
                        if (selectableText.isNotBlank()) selectableText else extractScannedPdfText(context, uri)
                    } else {
                        context.contentResolver.openInputStream(uri)?.bufferedReader()?.use { it.readText().take(250_000) }
                            ?: throw IllegalStateException("Android could not open this document.")
                    }
                }
            }
            val content = result.getOrNull()
            if (content.isNullOrBlank()) {
                error = result.exceptionOrNull()?.message
                    ?: "No selectable text was found. Scanned/image-only PDFs are not OCR-processed yet; attach the page as an image to use on-device OCR."
            } else {
                val detectedMime = context.contentResolver.getType(uri).orEmpty().lowercase()
                val label = when { detectedMime.startsWith("image/") -> "image OCR"; detectedMime == "application/pdf" || uri.toString().lowercase().endsWith(".pdf") -> "PDF document"; else -> "text document" }
                draft += "\n\n[Attached $label]\n$content"
                appMonitor.recordEvent("document_attached")
                monitorSnapshot = appMonitor.snapshot()
            }
        }
    }

    LaunchedEffect(messages.size, messages.lastOrNull()?.content?.length) {
        if (messages.isNotEmpty()) listState.animateScrollToItem(messages.lastIndex)
    }

    val composerFocusRequester = remember { androidx.compose.ui.focus.FocusRequester() }
    val keyboardController = androidx.compose.ui.platform.LocalSoftwareKeyboardController.current
    var attachmentTrayOpen by remember { mutableStateOf(false) }

    Box(Modifier.fillMaxSize().background(palette.background)) {
        Row(Modifier.fillMaxSize()) {
            if (isTabletLayout) {
                NavigationRail(
                    modifier = Modifier.width(72.dp).fillMaxHeight(),
                    containerColor = palette.surface,
                    contentColor = palette.text,
                    windowInsets = WindowInsets(0, 0, 0, 0)
                ) {
                    NavigationRailItem(
                        selected = wideDestination == "Home",
                        onClick = { wideDestination = "Home"; historyOpen = false; settings = false; newChat() },
                        icon = { Icon(Icons.Default.Add, contentDescription = "Home") },
                        label = { Text("Home") },
                        alwaysShowLabel = true
                    )
                    NavigationRailItem(
                        selected = wideDestination == "History",
                        onClick = { wideDestination = "History"; settings = false; historyOpen = true },
                        icon = { Icon(Icons.Default.History, contentDescription = "History") },
                        label = { Text("History") },
                        alwaysShowLabel = true
                    )
                    NavigationRailItem(
                        selected = wideDestination == "Agents",
                        onClick = { wideDestination = "Agents"; historyOpen = false; selectedSettingsPage = "Agents"; selectedSettingsCategory = "Agents"; settings = true },
                        icon = { Icon(Icons.Default.Info, contentDescription = "Agents") },
                        label = { Text("Agents") },
                        alwaysShowLabel = true
                    )
                    NavigationRailItem(
                        selected = wideDestination == "Settings",
                        onClick = { wideDestination = "Settings"; historyOpen = false; selectedSettingsPage = "Appearance"; selectedSettingsCategory = "Appearance"; settings = true },
                        icon = { Icon(Icons.Default.Settings, contentDescription = "Settings") },
                        label = { Text("Settings") },
                        alwaysShowLabel = true
                    )
                }
            }
            Scaffold(
                modifier = Modifier.weight(1f).fillMaxHeight(),
                containerColor = palette.background,
                topBar = {
                    TopAppBar(
                        modifier = Modifier.height(56.dp),
                        title = {
                            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                                Text("CenAI", color = palette.text, fontSize = 20.sp, fontWeight = FontWeight.Medium)
                                AssistChip(
                                    onClick = { selectedSettingsPage = "AI & models"; selectedSettingsCategory = "AI & models"; settings = true },
                                    label = { Text(model, maxLines = 1, fontSize = 11.sp) },
                                    colors = AssistChipDefaults.assistChipColors(containerColor = palette.raised, labelColor = palette.muted),
                                    border = null
                                )
                            }
                        },
                        actions = {
                            Box {
                                IconButton(onClick = { moreActionsExpanded = true }) {
                                    Icon(Icons.Default.MoreVert, "Conversation actions", tint = palette.text)
                                }
                                DropdownMenu(expanded = moreActionsExpanded, onDismissRequest = { moreActionsExpanded = false }) {
                                    DropdownMenuItem(
                                        text = { Text("Rename conversation") },
                                        enabled = messages.isNotEmpty(),
                                        onClick = {
                                            renameDraft = conversations.firstOrNull { it.id == currentId }?.title ?: messages.firstOrNull { it.role == "user" }?.content?.take(42).orEmpty()
                                            renameDialogOpen = true
                                            moreActionsExpanded = false
                                        }
                                    )
                                    DropdownMenuItem(
                                        text = { Text("Share conversation") },
                                        enabled = messages.isNotEmpty(),
                                        onClick = {
                                            moreActionsExpanded = false
                                            val body = messages.joinToString("\n\n") { (if (it.role == "user") "You" else "CenAI") + ":\n" + it.content }
                                            val send = Intent(Intent.ACTION_SEND).apply { type = "text/plain"; putExtra(Intent.EXTRA_TEXT, body) }
                                            context.startActivity(Intent.createChooser(send, "Share conversation"))
                                        }
                                    )
                                    DropdownMenuItem(
                                        text = { Text("Export as Markdown") },
                                        enabled = messages.isNotEmpty(),
                                        onClick = { moreActionsExpanded = false; saveConversationLauncher.launch("CenAI-conversation.md") }
                                    )
                                    DropdownMenuItem(
                                        text = { Text("Delete conversation", color = palette.danger) },
                                        enabled = messages.isNotEmpty(),
                                        onClick = {
                                            moreActionsExpanded = false
                                            conversations = conversations.filterNot { it.id == currentId }.toMutableList()
                                            store.save(conversations)
                                            newChat()
                                        }
                                    )
                                }
                            }
                        },
                        colors = TopAppBarDefaults.topAppBarColors(
                            containerColor = palette.background,
                            titleContentColor = palette.text
                        )
                    )
                },
                bottomBar = {
                    Column(
                        Modifier.fillMaxWidth().background(palette.background)
                    ) {
                        AnimatedVisibility(visible = error.isNotBlank(), enter = fadeIn(tween(160)), exit = androidx.compose.animation.fadeOut(tween(120))) {
                            Text(
                                error,
                                color = palette.danger,
                                fontSize = 12.sp,
                                modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 6.dp)
                            )
                        }
                        Surface(
                            color = palette.surface,
                            shape = RoundedCornerShape(12.dp),
                            border = androidx.compose.foundation.BorderStroke(1.dp, palette.border),
                            modifier = Modifier.fillMaxWidth().padding(horizontal = if (isTabletLayout) 24.dp else 12.dp, vertical = 8.dp)
                        ) {
                            Column(Modifier.padding(horizontal = 10.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                                BasicTextField(
                                    value = draft,
                                    onValueChange = { draft = it },
                                    modifier = Modifier.fillMaxWidth().heightIn(min = 40.dp, max = 120.dp).padding(horizontal = 4.dp, vertical = 7.dp)
                                        .then(Modifier.focusRequester(composerFocusRequester)),
                                    textStyle = TextStyle(color = palette.text, fontSize = 16.sp, lineHeight = 24.sp),
                                    decorationBox = { inner ->
                                        Box {
                                            if (draft.isEmpty()) Text("Message CenAI…", color = palette.tertiary, fontSize = 16.sp)
                                            inner()
                                        }
                                    }
                                )
                                Row(verticalAlignment = Alignment.CenterVertically) {
                                    IconButton(onClick = { attachmentTrayOpen = true }, modifier = Modifier.size(40.dp)) {
                                        Icon(Icons.Default.AttachFile, "Attachments", tint = palette.muted)
                                    }
                                    Box {
                                        DropdownMenu(expanded = attachmentTrayOpen, onDismissRequest = { attachmentTrayOpen = false }) {
                                            DropdownMenuItem(
                                                text = { Text("Image") },
                                                onClick = { attachmentTrayOpen = false; attachmentPicker.launch(arrayOf("image/*")) }
                                            )
                                            DropdownMenuItem(
                                                text = { Text("File or document") },
                                                onClick = { attachmentTrayOpen = false; attachmentPicker.launch(arrayOf("text/*", "application/json", "application/pdf", "image/*")) }
                                            )
                                        }
                                    }
                                    IconButton(onClick = {
                                        val speechIntent = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).apply {
                                            putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
                                            putExtra(RecognizerIntent.EXTRA_LANGUAGE, if (speechInputLanguage == "device") Locale.getDefault().toLanguageTag() else speechInputLanguage)
                                            putExtra(RecognizerIntent.EXTRA_PROMPT, "Speak your message to CenAI")
                                        }
                                        runCatching { speechLauncher.launch(speechIntent) }
                                            .onFailure { error = "Speech recognition is not available on this device." }
                                    }, modifier = Modifier.size(40.dp)) {
                                        Icon(Icons.Default.Mic, "Voice input", tint = palette.muted)
                                    }
                                    Spacer(Modifier.weight(1f))
                                    AnimatedContent(
                                        targetState = when {
                                            busy -> "stop"
                                            generationCompleted -> "done"
                                            else -> "send"
                                        },
                                        label = "send-stop-complete-morph"
                                    ) { action ->
                                        when (action) {
                                            "stop" -> FilledIconButton(
                                                onClick = { activeJob?.cancel(); busy = false },
                                                colors = IconButtonDefaults.filledIconButtonColors(containerColor = palette.danger, contentColor = Color.White),
                                                shape = RoundedCornerShape(10.dp),
                                                modifier = Modifier.size(44.dp).animateContentSize(spring(dampingRatio = 0.82f, stiffness = 520f))
                                            ) { Icon(Icons.Default.Stop, "Stop response") }
                                            "done" -> FilledIconButton(
                                                onClick = { },
                                                colors = IconButtonDefaults.filledIconButtonColors(containerColor = palette.positive, contentColor = Color.White),
                                                shape = RoundedCornerShape(12.dp),
                                                modifier = Modifier.size(44.dp)
                                            ) { Icon(Icons.Default.CheckCircle, "Response complete") }
                                            else -> FilledIconButton(
                                                enabled = draft.isNotBlank(),
                                                onClick = {
                                                    val prompt = runCatching { NativeCore.normalizePrompt(draft) }.getOrDefault(draft.trim())
                                                    if (prompt.isNotBlank()) {
                                                        val request = messages + ChatMessage("user", prompt)
                                                        draft = ""
                                                        startRequest(request)
                                                    }
                                                },
                                                colors = IconButtonDefaults.filledIconButtonColors(containerColor = palette.text, contentColor = palette.background),
                                                shape = CircleShape,
                                                modifier = Modifier.size(44.dp)
                                            ) { Icon(Icons.Default.ArrowUpward, "Send") }
                                        }
                                    }
                                }
                            }
                        }
                        if (!isTabletLayout) {
                            NavigationBar(
                                containerColor = palette.surface,
                                contentColor = palette.text,
                                tonalElevation = 0.dp,
                                windowInsets = WindowInsets.navigationBars
                            ) {
                                NavigationBarItem(
                                    selected = wideDestination == "Home",
                                    onClick = { wideDestination = "Home"; historyOpen = false; settings = false; newChat() },
                                    icon = { Icon(Icons.Default.Add, "Home") },
                                    label = { Text("Home") },
                                    alwaysShowLabel = true
                                )
                                NavigationBarItem(
                                    selected = wideDestination == "History",
                                    onClick = { wideDestination = "History"; settings = false; historyOpen = true },
                                    icon = { Icon(Icons.Default.History, "History") },
                                    label = { Text("History") },
                                    alwaysShowLabel = true
                                )
                                NavigationBarItem(
                                    selected = wideDestination == "Agents",
                                    onClick = { wideDestination = "Agents"; historyOpen = false; selectedSettingsPage = "Agents"; settings = true },
                                    icon = { Icon(Icons.Default.Info, "Agents") },
                                    label = { Text("Agents") },
                                    alwaysShowLabel = true
                                )
                                NavigationBarItem(
                                    selected = wideDestination == "Settings",
                                    onClick = { wideDestination = "Settings"; historyOpen = false; selectedSettingsPage = "Appearance"; settings = true },
                                    icon = { Icon(Icons.Default.Settings, "Settings") },
                                    label = { Text("Settings") },
                                    alwaysShowLabel = true
                                )
                            }
                        }
                    }
                }
            ) { padding ->
                if (messages.isEmpty()) {
                    Box(Modifier.fillMaxSize().padding(padding), contentAlignment = Alignment.TopCenter) {
                        Column(
                            Modifier.fillMaxWidth().widthIn(max = 720.dp).verticalScroll(rememberScrollState())
                                .padding(horizontal = 24.dp, vertical = 24.dp),
                            verticalArrangement = Arrangement.spacedBy(24.dp)
                        ) {
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                                    val hour = java.time.LocalTime.now().hour
                                    val greeting = when {
                                        hour < 12 -> "Good morning"
                                        hour < 18 -> "Good afternoon"
                                        else -> "Good evening"
                                    }
                                    Text(greeting, color = palette.text, fontSize = 28.sp, lineHeight = 34.sp, fontWeight = FontWeight.Medium)
                                    Text("What would you like to work on?", color = palette.muted, fontSize = 14.sp)
                                }
                                Surface(
                                    onClick = { selectedSettingsPage = "Account"; selectedSettingsCategory = "Account"; settings = true },
                                    shape = CircleShape,
                                    color = palette.raised,
                                    modifier = Modifier.size(44.dp)
                                ) {
                                    Box(contentAlignment = Alignment.Center) {
                                        Text("C", color = palette.text, fontSize = 18.sp, fontWeight = FontWeight.Medium)
                                    }
                                }
                            }
                            Button(
                                onClick = {
                                    newChat()
                                    composerFocusRequester.requestFocus()
                                    keyboardController?.show()
                                },
                                modifier = Modifier.fillMaxWidth().height(48.dp),
                                shape = RoundedCornerShape(10.dp),
                                colors = ButtonDefaults.buttonColors(containerColor = palette.text, contentColor = palette.background)
                            ) {
                                Icon(Icons.Default.Add, null, modifier = Modifier.size(18.dp))
                                Spacer(Modifier.width(8.dp))
                                Text("New chat", fontSize = 15.sp, fontWeight = FontWeight.Medium)
                            }
                            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                                Text("Recent", color = palette.text, fontSize = 14.sp, fontWeight = FontWeight.Medium)
                                if (conversations.isEmpty()) {
                                    Text("Your conversations will appear here.", color = palette.tertiary, fontSize = 13.sp)
                                } else {
                                    conversations.take(3).forEach { chat ->
                                        Row(
                                            Modifier.fillMaxWidth().clickable {
                                                activeJob?.cancel()
                                                busy = false
                                                currentId = chat.id
                                                messages = chat.messages.toList()
                                                wideDestination = "Home"
                                            }.padding(vertical = 12.dp),
                                            verticalAlignment = Alignment.CenterVertically
                                        ) {
                                            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                                                Text(chat.title, color = palette.text, fontSize = 15.sp, maxLines = 1)
                                                Text(chat.messages.lastOrNull()?.content.orEmpty().replace(Regex("\\s+"), " ").take(72),
                                                    color = palette.tertiary, fontSize = 12.sp, maxLines = 1)
                                            }
                                            val age = (System.currentTimeMillis() - chat.updatedAt).coerceAtLeast(0L)
                                            val timeLabel = when {
                                                age < 60_000L -> "now"
                                                age < 3_600_000L -> "${age / 60_000L}m"
                                                age < 86_400_000L -> "${age / 3_600_000L}h"
                                                else -> "${age / 86_400_000L}d"
                                            }
                                            Text(timeLabel, color = palette.tertiary, fontSize = 12.sp)
                                        }
                                        HorizontalDivider(color = palette.border)
                                    }
                                }
                            }
                            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                                Text("Try asking", color = palette.text, fontSize = 14.sp, fontWeight = FontWeight.Medium)
                                Row(
                                    Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()),
                                    horizontalArrangement = Arrangement.spacedBy(8.dp)
                                ) {
                                    SuggestionChip("Explain a concept") { draft = "Explain a concept with a simple example: "; composerFocusRequester.requestFocus(); keyboardController?.show() }
                                    SuggestionChip("Help me write") { draft = "Help me write: "; composerFocusRequester.requestFocus(); keyboardController?.show() }
                                    SuggestionChip("Review my code") { draft = "Review this code: "; composerFocusRequester.requestFocus(); keyboardController?.show() }
                                    SuggestionChip("Plan a project") { draft = "Help me plan a project: "; composerFocusRequester.requestFocus(); keyboardController?.show() }
                                }
                            }
                            Text(
                                if (localModelEnabled && localModelReady) "Offline model · ${provider.label} · ${model}"
                                else "${provider.label} · ${model}",
                                color = palette.tertiary,
                                fontSize = 12.sp
                            )
                        }
                    }
                } else {
                    Box(Modifier.fillMaxSize().padding(padding), contentAlignment = Alignment.TopCenter) {
                        LazyColumn(
                            state = listState,
                            modifier = Modifier.fillMaxWidth().widthIn(max = 720.dp).padding(horizontal = 16.dp),
                            contentPadding = PaddingValues(top = 16.dp, bottom = 24.dp),
                            verticalArrangement = Arrangement.spacedBy(16.dp)
                        ) {
                            itemsIndexed(messages, key = { index, _ -> "$currentId-$index" }) { index, message ->
                                MessageBubble(
                                    message = message,
                                    index = index,
                                    last = index == messages.lastIndex,
                                    busy = busy,
                                    style = vault.get("ui_style", "STUDIO"),
                                    onApproveDeviceAction = { name, rawArgs ->
                                        runCatching { DeviceActionManager(context).execute(name, JSONObject(rawArgs)) }
                                            .onSuccess { result ->
                                                info = result
                                                val updated = messages.map { item ->
                                                    if (item.role == "tool" && item.content.contains("\nARGS:$rawArgs")) {
                                                        ChatMessage("tool", item.content.replaceFirst("Approval required", "Completed").substringBefore("\nARGS:") + "\n" + result)
                                                    } else item
                                                }
                                                persist(updated)
                                            }
                                            .onFailure { error = it.message ?: "Android action could not be opened." }
                                    },
                                    onCopy = {
                                        clipboard.setText(AnnotatedString(message.content.replace(Regex("""\[\[CENAI_IMAGE\|[^|]+\|[A-Za-z0-9+/=]+]]"""), "[Image attached]")))
                                        info = "Copied to clipboard."
                                    },
                                    onCopyCode = { code -> clipboard.setText(AnnotatedString(code)); info = "Code copied to clipboard." },
                                    onSpeak = {
                                        if (speechOutputReady) {
                                            appMonitor.recordEvent("voice_output")
                                            monitorSnapshot = appMonitor.snapshot()
                                            textToSpeech.speak(message.content, TextToSpeech.QUEUE_FLUSH, null, "cenai-$currentId-$index")
                                        } else info = "Speech output is not ready yet."
                                    },
                                    onEdit = {
                                        editIndex = index
                                        editText = message.content.replace(Regex("""\[\[CENAI_IMAGE\|[^|]+\|[A-Za-z0-9+/=]+]]"""), "[Image attached]")
                                    },
                                    onRegenerate = { startRequest(messages.take(index)) }
                                )
                            }
                            if (busy && messages.lastOrNull()?.content.isNullOrBlank()) {
                                item { ThinkingIndicator() }
                            }
                        }
                    }
                }
            }
        }
    }

    if (renameDialogOpen) {
        AlertDialog(
            onDismissRequest = { renameDialogOpen = false },
            title = { Text("Rename conversation") },
            text = {
                OutlinedTextField(
                    value = renameDraft,
                    onValueChange = { renameDraft = it.take(80) },
                    singleLine = true,
                    label = { Text("Conversation title") }
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    val clean = renameDraft.trim()
                    if (clean.isNotEmpty()) {
                        conversations.firstOrNull { it.id == currentId }?.title = clean
                        conversations = conversations.toMutableList()
                        store.save(conversations)
                    }
                    renameDialogOpen = false
                }) { Text("Save") }
            },
            dismissButton = { TextButton(onClick = { renameDialogOpen = false }) { Text("Cancel") } }
        )
    }

    if (historyOpen) {
        ModalBottomSheet(onDismissRequest = { historyOpen = false }, containerColor = palette.surface) {
            Column(Modifier.fillMaxHeight(0.92f).verticalScroll(rememberScrollState())) {
            Row(Modifier.fillMaxWidth().padding(horizontal = 20.dp), verticalAlignment = Alignment.CenterVertically) {
                Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    Text("Your conversations", color = palette.text, fontSize = 19.sp, fontWeight = FontWeight.SemiBold)
                    Text("${conversations.size} saved chats · Search your past work", color = palette.muted, fontSize = 11.sp)
                }
                Spacer(Modifier.weight(1f))
                TextButton(onClick = { newChat(); historyOpen = false }) { Text("New chat", color = palette.accent) }
            }
            OutlinedTextField(value = historyQuery, onValueChange = { historyQuery = it }, label = { Text("Search conversations") }, singleLine = true, modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 8.dp).animateContentSize(tween(CenAiMotion.contentMs)))
            val visibleConversations = conversations.filter { chat ->
                chat.title.contains(historyQuery, true) || chat.messages.any { it.content.contains(historyQuery, true) }
            }
            if (visibleConversations.isEmpty()) {
                Text(
                    if (historyQuery.isBlank()) "Your saved chats will appear here." else "No matching conversations.",
                    color = palette.tertiary,
                    modifier = Modifier.padding(24.dp)
                )
            } else {
                val now = System.currentTimeMillis()
                val groups = listOf(
                    "Today" to visibleConversations.filter { now - it.updatedAt < 86_400_000L },
                    "Yesterday" to visibleConversations.filter { now - it.updatedAt in 86_400_000L..172_800_000L },
                    "Earlier" to visibleConversations.filter { now - it.updatedAt > 172_800_000L }
                )
                groups.forEach { (heading, chats) ->
                    if (chats.isNotEmpty()) {
                        Text(
                            heading.uppercase(),
                            color = palette.tertiary,
                            fontSize = 11.sp,
                            fontWeight = FontWeight.Medium,
                            modifier = Modifier.padding(start = 20.dp, top = 14.dp, bottom = 4.dp)
                        )
                        chats.forEach { chat ->
                            Row(
                                Modifier.fillMaxWidth()
                                    .background(if (currentId == chat.id) palette.raised else Color.Transparent)
                                    .clickable {
                                        activeJob?.cancel()
                                        busy = false
                                        currentId = chat.id
                                        messages = chat.messages.toList()
                                        historyOpen = false
                                        wideDestination = "Home"
                                    }
                                    .padding(horizontal = 20.dp, vertical = 12.dp),
                                verticalAlignment = Alignment.CenterVertically
                            ) {
                                Box(
                                    Modifier.width(2.dp).height(42.dp)
                                        .background(if (currentId == chat.id) palette.text else Color.Transparent)
                                )
                                Spacer(Modifier.width(12.dp))
                                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                                    Text(chat.title, color = palette.text, fontSize = 15.sp, maxLines = 1)
                                    Text(
                                        chat.messages.lastOrNull { it.role == "assistant" }?.content
                                            ?.replace(Regex("""\s+"""), " ")?.take(92).orEmpty(),
                                        color = palette.tertiary,
                                        fontSize = 12.sp,
                                        maxLines = 1
                                    )
                                    Text(
                                        java.text.DateFormat.getDateTimeInstance(java.text.DateFormat.SHORT, java.text.DateFormat.SHORT)
                                            .format(java.util.Date(chat.updatedAt)),
                                        color = palette.tertiary,
                                        fontSize = 11.sp
                                    )
                                }
                                IconButton(onClick = {
                                    conversations = conversations.filterNot { it.id == chat.id }.toMutableList()
                                    store.save(conversations)
                                    if (currentId == chat.id) newChat()
                                }) {
                                    Icon(Icons.Default.Delete, "Delete conversation", tint = palette.tertiary)
                                }
                            }
                            HorizontalDivider(color = palette.border)
                        }
                    }
                }
            }
            Spacer(Modifier.height(24.dp))
            }
        }
    }

    if (settings) {
        var cloudEmail by remember { mutableStateOf(cloud.accountEmail()) }
        var cloudPassword by remember { mutableStateOf("") }
        var cloudStatus by remember { mutableStateOf("") }
        var adminAnalytics by remember { mutableStateOf("") }
        var usageAnalyticsConsent by remember { mutableStateOf(vault.get("cloud_usage_analytics_consent", "false").toBoolean()) }
        var guardianCloudConsent by remember { mutableStateOf(vault.get("guardian_cloud_consent", "false").toBoolean()) }
        var cloudBusy by remember { mutableStateOf(false) }
        var paypalBusy by remember { mutableStateOf(false) }
        var paypalStatus by remember { mutableStateOf("Load plans to check current PayPal availability.") }
        var paypalBillingEnabled by remember { mutableStateOf(false) }
        var paypalPlans by remember { mutableStateOf(emptyList<JSONObject>()) }
        var paypalSubscriptions by remember { mutableStateOf(emptyList<JSONObject>()) }
        var cloudSignedIn by remember { mutableStateOf(cloud.isSignedIn()) }
        var otpCode by remember { mutableStateOf("") }
        var emailVerified by remember { mutableStateOf(cloud.isEmailVerified()) }
        var betaDisplayName by remember { mutableStateOf(vault.get("beta_display_name", "")) }
        var betaMotivation by remember { mutableStateOf("") }
        var betaApplicationStatus by remember { mutableStateOf("") }
        var expanded by remember { mutableStateOf(false) }
        var themeExpanded by remember { mutableStateOf(false) }
        var backgroundTaskPrompt by remember { mutableStateOf("") }
        var backgroundTaskDelay by remember { mutableStateOf("15") }
        var backgroundTaskStatus by remember { mutableStateOf("") }
        var uiStyle by remember { mutableStateOf(vault.get("ui_style", "STUDIO").let { if (it == "AURORA" || it == "DEFAULT") "STUDIO" else it }) }
        AlertDialog(
            modifier = Modifier.fillMaxSize().padding(8.dp).widthIn(max = 1120.dp),
            properties = DialogProperties(usePlatformDefaultWidth = false),
            onDismissRequest = { settings = false },
            containerColor = if (palette.glass) palette.surface.copy(alpha = 0.94f) else palette.surface,
            tonalElevation = if (palette.glass) 18.dp else 6.dp,
            title = {
                Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
                    Text("CenAI settings", color = palette.text, fontWeight = FontWeight.Bold)
                    Text("Personalize your workspace, models, agents and device", color = palette.muted, fontSize = 11.sp)
                }
            },
            text = {
                Row(
                    Modifier.fillMaxWidth().heightIn(max = (LocalConfiguration.current.screenHeightDp - 160).coerceAtLeast(320).dp),
                    horizontalArrangement = Arrangement.spacedBy(18.dp)
                ) {
                    val settingsCategories = listOf(
                        "Appearance" to "Appearance",
                        "Chat" to "Device",
                        "AI & models" to "AI & models",
                        "Providers" to "AI & models",
                        "Account" to "Account",
                        "Voice" to "Device",
                        "Notifications" to "Agents",
                        "Privacy & permissions" to "Device",
                        "Agents" to "Agents",
                        "Data & storage" to "Device",
                        "About & diagnostics" to "Device"
                    )
                    if (isTabletLayout) {
                        Column(
                            Modifier.width(232.dp).fillMaxHeight().verticalScroll(rememberScrollState()),
                            verticalArrangement = Arrangement.spacedBy(3.dp)
                        ) {
                            settingsCategories.forEach { (category, target) ->
                                Surface(
                                    onClick = {
                                        selectedSettingsCategory = category
                                        selectedSettingsPage = target
                                    },
                                    color = animateColorAsState(if (selectedSettingsCategory == category) palette.raised else Color.Transparent, animationSpec = tween(CenAiMotion.controlMs), label = "settings-selection").value,
                                    shape = RoundedCornerShape(10.dp),
                                    modifier = Modifier.fillMaxWidth()
                                ) {
                                    Text(
                                        category,
                                        color = if (selectedSettingsCategory == category) palette.text else palette.muted,
                                        fontSize = 14.sp,
                                        fontWeight = if (selectedSettingsCategory == category) FontWeight.Medium else FontWeight.Normal,
                                        modifier = Modifier.padding(horizontal = 12.dp, vertical = 12.dp)
                                    )
                                }
                            }
                        }
                        VerticalDivider(color = palette.border, modifier = Modifier.fillMaxHeight())
                    }
                    Column(
                        Modifier.weight(1f).fillMaxHeight().verticalScroll(rememberScrollState()),
                        verticalArrangement = Arrangement.spacedBy(12.dp)
                    ) {
                        if (!isTabletLayout) {
                            Row(
                                Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).testTag("settings-page-tabs"),
                                horizontalArrangement = Arrangement.spacedBy(7.dp)
                            ) {
                                settingsCategories.forEach { (category, target) ->
                                    FilterChip(
                                        selected = selectedSettingsCategory == category,
                                        onClick = {
                                            selectedSettingsCategory = category
                                            selectedSettingsPage = target
                                        },
                                        label = { Text(category, maxLines = 1) }
                                    )
                                }
                            }
                        }
                        Spacer(Modifier.height(4.dp))
                    if (selectedSettingsPage == "Agents") {
                        Surface(
                            color = palette.accent.copy(alpha = 0.09f),
                            shape = RoundedCornerShape(18.dp),
                            border = androidx.compose.foundation.BorderStroke(1.dp, palette.accent.copy(alpha = 0.22f)),
                            modifier = Modifier.fillMaxWidth()
                        ) {
                            Column(Modifier.padding(15.dp), verticalArrangement = Arrangement.spacedBy(5.dp)) {
                                Text("AGENT CONTROL CENTER", color = palette.accent, fontSize = 10.sp, fontWeight = FontWeight.Bold, letterSpacing = 1.1.sp)
                                Text("Choose what CenAI can do", color = palette.text, fontSize = 18.sp, fontWeight = FontWeight.SemiBold)
                                Text("Configure planning, approved device actions, local tools and scheduled tasks. Sensitive actions still require your approval.", color = palette.muted, fontSize = 11.sp, lineHeight = 16.sp)
                            }
                        }
                    }
                    if (selectedSettingsPage == "Appearance") {
Text("APPEARANCE", color = palette.accent, fontSize = 10.sp, fontWeight = FontWeight.Bold)
                    ExposedDropdownMenuBox(expanded = themeExpanded, onExpandedChange = { themeExpanded = !themeExpanded }) {
                        OutlinedTextField(
                            value = when (uiStyle) {
                                "MATERIAL" -> "Android · Material 3"
                                "IOS" -> "iOS · Liquid Glass"
                                "FIGMA" -> "CenAI Studio · Editorial"
                                else -> "CenAI Default"
                            },
                            onValueChange = {}, readOnly = true, label = { Text("Interface style") },
                            trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(themeExpanded) },
                            modifier = Modifier.menuAnchor().fillMaxWidth()
                        )
                        ExposedDropdownMenu(expanded = themeExpanded, onDismissRequest = { themeExpanded = false }) {
                            listOf("STUDIO" to "CenAI Monochrome · Studio").forEach { (key, label) ->
                                DropdownMenuItem(text = { Text(label) }, onClick = {
                                    uiStyle = key
                                    themeExpanded = false
                                    onUiStyleChange(key)
                                    vault.put("ui_style", key)
                                })
                            }
                        }
                    }
                    Text(
                        "CenAI uses a black-and-white foundation with gray hierarchy. Color is reserved for real status, warnings and errors.",
                        color = palette.muted,
                        fontSize = 12.sp,
                        lineHeight = 18.sp
                    )
                    HorizontalDivider(color = palette.raised)
                    
                    }
                    if (selectedSettingsPage == "AI & models") {
Text("PROVIDER", color = palette.accent, fontSize = 10.sp, fontWeight = FontWeight.Bold)
                    ExposedDropdownMenuBox(expanded = expanded, onExpandedChange = { expanded = !expanded }) {
                        OutlinedTextField(value = provider.label, onValueChange = {}, readOnly = true,
                            label = { Text("AI provider") }, trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(expanded) },
                            modifier = Modifier.menuAnchor().fillMaxWidth())
                        ExposedDropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
                            Provider.entries.forEach { p ->
                                DropdownMenuItem(text = { Text(p.label) }, onClick = {
                                    provider = p
                                    val savedModel = vault.get("model_${p.name}", p.model)
                                    model = if (p == Provider.DEEPSEEK && savedModel in setOf("deepseek-chat", "deepseek-reasoner")) p.model else savedModel
                                    base = vault.get("base_url_${p.name}", p.baseUrl)
                                    expanded = false
                                })
                            }
                        }
                    }
                    OutlinedTextField(value = model, onValueChange = { model = it }, label = { Text("Model ID") }, singleLine = true)
                    if (provider == Provider.CUSTOM) OutlinedTextField(value = base, onValueChange = { base = it },
                        label = { Text("HTTPS API base URL") }, singleLine = true)
                    Provider.entries.filter { it != Provider.CENAI_CLOUD }.forEach { p ->
                        OutlinedTextField(value = providerKeys[p].orEmpty(), onValueChange = { value -> providerKeys = providerKeys.toMutableMap().apply { put(p, value) } }, label = { Text("${p.label} API key") }, singleLine = true)
                    }
                    if (provider == Provider.CENAI_CLOUD) Text(if (cloud.isSignedIn()) "Uses your signed-in CenAI account. Daily and monthly quotas are enforced by the Cloud AI gateway; no external API key is needed." else "Sign in to your CenAI account in the Cloud Account section below to use this model.", color = palette.muted, fontSize = 11.sp)
                    if (provider == Provider.CUSTOM || provider == Provider.CLAUDE)
                        OutlinedTextField(value = customHeaders, onValueChange = { customHeaders = it },
                            label = { Text("Extra headers (JSON)") }, minLines = 2)
                    if (provider == Provider.CUSTOM)
                        OutlinedTextField(value = customParams, onValueChange = { customParams = it },
                            label = { Text("Extra request parameters (JSON)") }, minLines = 2)
                    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                        Column(Modifier.weight(1f)) { Text("Multi-agent responses", color = palette.text); Text("Ask every configured provider", color = palette.muted, fontSize = 10.sp) }
                        Switch(checked = multiAgent, onCheckedChange = { multiAgent = it })
                    }
                    HorizontalDivider(color = palette.raised)
                    
                    }
                    if (selectedSettingsPage == "Agents") {
Text("AGENT WORKSPACE", color = palette.accent, fontSize = 10.sp, fontWeight = FontWeight.Bold)
                    Text("Scheduled background task", color = palette.text, fontWeight = FontWeight.SemiBold)
                    Text("Schedule a task to run once in the background. Android may delay execution for battery and network conditions. The task uses the selected provider and its saved credentials.", color = palette.muted, fontSize = 10.sp)
                    OutlinedTextField(
                        value = backgroundTaskPrompt,
                        onValueChange = { backgroundTaskPrompt = it.take(8_000) },
                        label = { Text("Task instructions") },
                        placeholder = { Text("e.g. Summarize my saved project notes…") },
                        minLines = 2,
                        modifier = Modifier.fillMaxWidth()
                    )
                    OutlinedTextField(
                        value = backgroundTaskDelay,
                        onValueChange = { backgroundTaskDelay = it.filter(Char::isDigit).take(5) },
                        label = { Text("Run after minutes (1–43200)") },
                        singleLine = true,
                        modifier = Modifier.fillMaxWidth()
                    )
                    OutlinedButton(
                        enabled = backgroundTaskPrompt.isNotBlank() &&
                            (backgroundTaskDelay.toLongOrNull() ?: 0L) in 1L..43_200L &&
                            (provider == Provider.CUSTOM || !providerKeys[provider].isNullOrBlank() ||
                                (provider == Provider.GEMINI && (vault.getSecret("gemini_oauth_access_token").isNotBlank() || vault.getSecret("gemini_oauth_refresh_token").isNotBlank()))),
                        onClick = {
                            if (Build.VERSION.SDK_INT >= 33 && ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
                                notificationPermissionLauncher.launch(Manifest.permission.POST_NOTIFICATIONS)
                            }
                            runCatching {
                                val id = AgentTaskWorker.schedule(context, backgroundTaskPrompt.trim(), backgroundTaskDelay.toLong(), provider, model, base)
                                backgroundTaskStatus = "Task queued · ID " + id.take(8)
                                backgroundTaskPrompt = ""
                            }.onFailure { backgroundTaskStatus = it.message ?: "Could not schedule task." }
                        },
                        modifier = Modifier.fillMaxWidth()
                    ) { Text("Schedule background task") }
                    if (backgroundTaskStatus.isNotBlank()) Text(backgroundTaskStatus, color = palette.accent, fontSize = 11.sp)
                    val taskPrefs = context.getSharedPreferences(AgentTaskWorker.PREFS, android.content.Context.MODE_PRIVATE).all
                    val taskEntries = taskPrefs.filterKeys { it.startsWith("status_") }.entries.sortedByDescending { entry ->
                        taskPrefs["completed_" + entry.key.removePrefix("status_")] as? Long ?: (taskPrefs["scheduled_" + entry.key.removePrefix("status_")] as? Long ?: 0L)
                    }.take(5)
                    if (taskEntries.isNotEmpty()) {
                        
                    }
                    if (selectedSettingsPage == "Agents") {
Text("RECENT BACKGROUND TASKS", color = palette.accent, fontSize = 10.sp, fontWeight = FontWeight.Bold)
                        taskEntries.forEach { entry ->
                            val id = entry.key.removePrefix("status_")
                            val taskName = vault.getSecret("scheduled_task_prompt_$id").take(90).ifBlank { "Scheduled task" }
                            Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                                Text(taskName, color = palette.text, fontSize = 11.sp)
                                Text((entry.value as? String).orEmpty(), color = palette.muted, fontSize = 10.sp)
                                vault.getSecret("scheduled_task_result_$id").takeIf { it.isNotBlank() }?.let { result ->
                                    Text("Result: " + result.take(700), color = palette.muted, fontSize = 10.sp)
                                }
                                if ((entry.value as? String) == "Scheduled" || (entry.value as? String) == "Running") {
                                    TextButton(onClick = { AgentTaskWorker.cancel(context, id); backgroundTaskStatus = "Task cancelled." }) { Text("Cancel task", color = Color(0xFFFF938B)) }
                                }
                            }
                        }
                    }
                    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                        Column(Modifier.weight(1f)) {
                            Text("Agent planning mode", color = palette.text)
                            Text("Break complex requests into steps and track what needs doing", color = palette.muted, fontSize = 10.sp)
                        }
                        Switch(checked = agentPlanning, onCheckedChange = { agentPlanning = it })
                    }
                    Text("Sensitive Android actions always require your explicit approval. This safety check cannot be disabled.", color = palette.muted, fontSize = 10.sp)
                    Text("The agent can run allow-listed local tools, schedule one-off network tasks, and request narrowly scoped Android actions. Device actions require an explicit approval tap and are handed to Android; CenAI cannot silently place calls or send messages.", color = palette.muted, fontSize = 10.sp)
                    HorizontalDivider(color = palette.raised)
                    
                    }
                    if (selectedSettingsPage == "Agents") {
Text("LOCAL TOOLS & PLUGINS", color = palette.accent, fontSize = 10.sp, fontWeight = FontWeight.Bold)
                    Text("Enable only the tools you want the agent to access. These run locally and do not execute shell commands or arbitrary code.", color = palette.muted, fontSize = 10.sp)
                    listOf(
                        "current_time" to "Current time",
                        "current_date" to "Current date",
                        "calculate" to "Safe calculator",
                        "count_words" to "Word counter",
                        "text_stats" to "Text statistics",
                        "format_json" to "JSON formatter",
                        "generate_uuid" to "UUID generator",
                        "base64_encode" to "Base64 encoder",
                        "base64_decode" to "Base64 decoder",
                        "open_url" to "Open web link (approval required)",
                        "open_dialer" to "Open dialer (approval required)",
                        "share_text" to "Open share sheet (approval required)",
                        "open_app_settings" to "Open app settings (approval required)"
                    ).forEach { (tool, label) ->
                        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                            Text(label, color = palette.text, modifier = Modifier.weight(1f), fontSize = 13.sp)
                            Switch(checked = toolEnabled[tool] == true, onCheckedChange = { enabled ->
                                toolEnabled = toolEnabled.toMutableMap().apply { put(tool, enabled) }
                                vault.put("tool_enabled_" + tool, enabled.toString())
                            })
                        }
                    }
                    HorizontalDivider(color = palette.raised)
                    
                    }
                    if (selectedSettingsPage == "AI & models") {
Text("ACCOUNT CONNECTIONS", color = palette.accent, fontSize = 10.sp, fontWeight = FontWeight.Bold)
                    Surface(color = palette.raised, shape = RoundedCornerShape(12.dp), modifier = Modifier.fillMaxWidth()) {
                        Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            Text("Google Gemini account", color = palette.text, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
                            Text(if (vault.getSecret("gemini_oauth_access_token").isNotBlank()) "Google account connected" else "Not connected", color = if (vault.getSecret("gemini_oauth_access_token").isNotBlank()) palette.accent else palette.muted, fontSize = 11.sp)
                            OutlinedTextField(value = googleOAuthClientId, onValueChange = { googleOAuthClientId = it.trim() }, label = { Text("Google OAuth Client ID (experimental)") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                            OutlinedTextField(value = googleCloudProjectId, onValueChange = { googleCloudProjectId = it.trim() }, label = { Text("Google Cloud Project ID") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                            Text("Google’s official Gemini OAuth quickstart documents a simplified desktop-app testing flow; this in-app PKCE redirect integration is experimental and has not been verified as a production Android OAuth client. For reliable mobile access, use a Gemini API key. Enable the Generative Language API and configure Google OAuth consent/test users if you experiment with this flow. Gemini consumer subscriptions do not automatically include API access. OpenAI and Anthropic support API keys and certain workload-identity options; DeepSeek documents bearer API-key authentication. Consumer ChatGPT, Claude and DeepSeek subscriptions are not third-party API credentials. API keys are encrypted at rest on this device, but direct mobile API calls are not equivalent to keeping secrets on a trusted server.", color = palette.muted, fontSize = 10.sp, lineHeight = 15.sp)
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                Button(onClick = {
                                    vault.put("google_oauth_client_id", googleOAuthClientId)
                                    vault.put("google_cloud_project_id", googleCloudProjectId)
                                    runCatching { context.startActivity(GoogleGeminiOAuth(vault).authorizationIntent(googleOAuthClientId)) }
                                        .onFailure { info = it.message ?: "Could not start Google sign-in." }
                                }, enabled = googleOAuthClientId.endsWith(".apps.googleusercontent.com"), modifier = Modifier.weight(1f)) { Text("Connect Google") }
                                OutlinedButton(onClick = { GoogleGeminiOAuth(vault).disconnect(); info = "Google account disconnected." }) { Text("Disconnect") }
                            }
                        }
                    }
                    HorizontalDivider(color = palette.raised)
                    
                    }
                    if (selectedSettingsPage == "AI & models") {
Text("CENAI CLOUD ACCOUNT · PUBLIC BETA", color = palette.accent, fontSize = 10.sp, fontWeight = FontWeight.Bold)
                    Surface(color = palette.raised, shape = RoundedCornerShape(12.dp), modifier = Modifier.fillMaxWidth()) {
                        Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            Text("CenAI account", color = palette.text, fontWeight = FontWeight.SemiBold)
                            Text(if (cloudSignedIn) "Signed in as ${cloud.accountEmail()}" else "Sign in to apply for public beta access and sync your tester status.", color = palette.muted, fontSize = 11.sp)
                            OutlinedTextField(value = cloudEmail, onValueChange = { cloudEmail = it }, label = { Text("Email address") }, singleLine = true, modifier = Modifier.fillMaxWidth(), enabled = !cloudSignedIn)
                            if (!cloudSignedIn) OutlinedTextField(value = cloudPassword, onValueChange = { cloudPassword = it }, label = { Text("Password · at least 8 characters") }, singleLine = true, visualTransformation = PasswordVisualTransformation(), modifier = Modifier.fillMaxWidth())
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                if (!cloudSignedIn) {
                                    Button(enabled = !cloudBusy, onClick = { cloudBusy = true; cloudStatus = "Creating account…"; scope.launch { try { val result = cloud.signUp(cloudEmail, cloudPassword); cloudStatus = result.message; cloudSignedIn = result.signedIn; if (cloudSignedIn) cloudPassword = "" } catch (e: Exception) { cloudStatus = e.message ?: "Sign-up failed." } finally { cloudBusy = false } } }, modifier = Modifier.weight(1f)) { Text("Create account") }
                                    OutlinedButton(enabled = !cloudBusy, onClick = { cloudBusy = true; cloudStatus = "Signing in…"; scope.launch { try { cloudStatus = cloud.signIn(cloudEmail, cloudPassword).message; cloudSignedIn = true; cloudPassword = ""; val status = cloud.call("status"); betaApplicationStatus = status.optJSONObject("application")?.optString("status") ?: "No application yet" } catch (e: Exception) { cloudStatus = e.message ?: "Sign-in failed." } finally { cloudBusy = false } } }, modifier = Modifier.weight(1f)) { Text("Sign in") }
                                } else {
                                    OutlinedButton(enabled = !cloudBusy, onClick = { cloudBusy = true; scope.launch { try { cloud.call("status").optJSONObject("application")?.let { betaApplicationStatus = it.optString("status") } ?: run { betaApplicationStatus = "No application yet" }; cloudStatus = "Beta status refreshed." } catch (e: Exception) { cloudStatus = e.message ?: "Could not refresh status." } finally { cloudBusy = false } } }, modifier = Modifier.weight(1f)) { Text("Refresh status") }
                                    TextButton(onClick = { cloud.signOut(); cloudSignedIn = false; betaApplicationStatus = ""; cloudPassword = ""; cloudStatus = "Signed out of CenAI." }) { Text("Sign out") }
                                }
                            }
                            if (cloudSignedIn) {
                                HorizontalDivider(color = palette.raised)
                                
                    }
                    if (selectedSettingsPage == "Account") {
Text("EMAIL SECURITY", color = palette.accent, fontSize = 10.sp, fontWeight = FontWeight.Bold)
                                Text(if (emailVerified) "Email verified for this account." else "Verify your account email with a one-time code before applying for beta access.", color = palette.muted, fontSize = 11.sp)
                                if (!emailVerified) {
                                    OutlinedButton(enabled = !cloudBusy, onClick = { cloudBusy = true; scope.launch { try { cloudStatus = cloud.sendEmailOtp() } catch (e: Exception) { cloudStatus = e.message ?: "Could not send verification code." } finally { cloudBusy = false } } }, modifier = Modifier.fillMaxWidth()) { Text("Send email verification code") }
                                    OutlinedTextField(value = otpCode, onValueChange = { otpCode = it.filter(Char::isDigit).take(6) }, label = { Text("Six-digit email code") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                                    Button(enabled = !cloudBusy && otpCode.length == 6, onClick = { cloudBusy = true; scope.launch { try { cloudStatus = cloud.verifyEmailOtp(otpCode); emailVerified = true; otpCode = "" } catch (e: Exception) { cloudStatus = e.message ?: "Verification failed." } finally { cloudBusy = false } } }, modifier = Modifier.fillMaxWidth()) { Text("Verify email") }
                                }
                                HorizontalDivider(color = palette.raised)
                                
                    }
                    if (selectedSettingsPage == "Account") {
Text("PUBLIC BETA APPLICATION", color = palette.accent, fontSize = 10.sp, fontWeight = FontWeight.Bold)
                                OutlinedTextField(value = betaDisplayName, onValueChange = { betaDisplayName = it.take(80) }, label = { Text("Display name") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                                OutlinedTextField(value = betaMotivation, onValueChange = { betaMotivation = it.take(2000) }, label = { Text("Why do you want to test CenAI?") }, minLines = 3, modifier = Modifier.fillMaxWidth())
                                Text(if (emailVerified) "Your application must contain at least 20 characters. An administrator reviews access requests." else "Verify your email above before submitting a beta application.", color = palette.muted, fontSize = 10.sp)
                                if (betaApplicationStatus.isNotBlank()) Text("Application status: $betaApplicationStatus", color = palette.accent, fontSize = 12.sp)
                                Button(enabled = !cloudBusy && emailVerified && betaMotivation.trim().length >= 20 && betaDisplayName.isNotBlank(), onClick = { cloudBusy = true; cloudStatus = "Submitting beta application…"; scope.launch { try { cloud.call("apply", JSONObject().put("displayName", betaDisplayName.trim()).put("motivation", betaMotivation.trim()).put("experience", "new")); betaApplicationStatus = "pending"; betaMotivation = ""; vault.put("beta_display_name", betaDisplayName); cloudStatus = "Your beta application has been submitted." } catch (e: Exception) { cloudStatus = e.message ?: "Application submission failed." } finally { cloudBusy = false } } }, modifier = Modifier.fillMaxWidth()) { Text(if (cloudBusy) "Please wait…" else "Apply for beta access") }
                            }
                            if (cloudSignedIn) {
                                HorizontalDivider(color = palette.raised)
                                
                    }
                    if (selectedSettingsPage == "Account") {
Text("CENAI PLANS · PAYPAL", color = palette.accent, fontSize = 10.sp, fontWeight = FontWeight.Bold)
                                Text("View and manage PayPal subscriptions here. Checkout opens PayPal's secure approval page; CenAI never handles your PayPal password or card details.", color = palette.muted, fontSize = 10.sp)
                                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                    OutlinedButton(enabled = !paypalBusy, onClick = {
                                        paypalBusy = true
                                        paypalStatus = "Loading PayPal plans…"
                                        scope.launch {
                                            try {
                                                val result = cloud.paypalBilling("list_public_plans")
                                                paypalBillingEnabled = result.optBoolean("billingEnabled")
                                                val rows = result.optJSONArray("plans")
                                                paypalPlans = if (rows == null) emptyList() else (0 until rows.length()).map { rows.getJSONObject(it) }
                                                paypalStatus = if (paypalBillingEnabled) "PayPal checkout is enabled. Review each recurring amount before approving." else "PayPal checkout is currently locked while live subscription lifecycle checks are completed. You can still view plans."
                                            } catch (e: Exception) { paypalStatus = e.message ?: "Could not load PayPal plans." }
                                            finally { paypalBusy = false }
                                        }
                                    }, modifier = Modifier.weight(1f)) { Text(if (paypalBusy) "Loading…" else "Load plans") }
                                    OutlinedButton(enabled = !paypalBusy, onClick = {
                                        paypalBusy = true
                                        scope.launch {
                                            try {
                                                val result = cloud.paypalBilling("status")
                                                val rows = result.optJSONArray("subscriptions")
                                                paypalSubscriptions = if (rows == null) emptyList() else (0 until rows.length()).map { rows.getJSONObject(it) }
                                                paypalStatus = if (paypalSubscriptions.isEmpty()) "No PayPal subscriptions are linked to this account." else "Your PayPal subscriptions are up to date."
                                            } catch (e: Exception) { paypalStatus = e.message ?: "Could not load subscription status." }
                                            finally { paypalBusy = false }
                                        }
                                    }, modifier = Modifier.weight(1f)) { Text("My subscriptions") }
                                }
                                OutlinedButton(
                                    enabled = !paypalBusy && cloudSignedIn,
                                    onClick = {
                                        paypalBusy = true
                                        paypalStatus = "Checking the latest PayPal checkout records…"
                                        scope.launch {
                                            try {
                                                val result = cloud.paypalBilling("diagnose_recent_checkouts")
                                                val rows = result.optJSONArray("diagnostics")
                                                paypalStatus = if (rows == null || rows.length() == 0) {
                                                    "No recent checkout attempts were found for this account."
                                                } else {
                                                    (0 until rows.length()).joinToString("\n\n") { index ->
                                                        val row = rows.getJSONObject(index)
                                                        val id = row.optString("subscriptionId").takeLast(8)
                                                        val failure = row.optString("error")
                                                        if (failure.isNotBlank()) {
                                                            "Checkout …$id · ${row.optString("recordedStatus")}\nPayPal lookup: $failure"
                                                        } else {
                                                            "Checkout …$id · ${row.optString("paypalStatus")}\nPlan: ${row.optString("planAmount")} ${row.optString("planCurrency")} · ${row.optString("planStatus")}\nFailed payments: ${row.optInt("failedPaymentsCount")}\nReason: ${row.optString("lastFailedPaymentReasonCode").ifBlank { "Not reported" }}\nRetry: ${row.optString("nextPaymentRetryTime").ifBlank { "Not scheduled" }}\n${row.optString("statusChangeNote")}".trim()
                                                        }
                                                    }
                                                }
                                            } catch (e: Exception) {
                                                paypalStatus = e.message ?: "Could not diagnose PayPal checkout."
                                            } finally {
                                                paypalBusy = false
                                            }
                                        }
                                    },
                                    modifier = Modifier.fillMaxWidth()
                                ) { Text(if (paypalBusy) "Checking…" else "Diagnose latest checkout") }
                                Text(paypalStatus, color = if (paypalBillingEnabled) palette.accent else palette.muted, fontSize = 11.sp)
                                paypalPlans.forEach { plan ->
                                    Surface(color = palette.raised.copy(alpha = if (vault.get("ui_style", "AURORA") == "IOS") 0.62f else 1f), shape = RoundedCornerShape(14.dp), border = androidx.compose.foundation.BorderStroke(1.dp, palette.muted.copy(alpha = 0.18f)), modifier = Modifier.fillMaxWidth()) {
                                        Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(5.dp)) {
                                            Text(plan.optString("display_name", "CenAI plan"), color = palette.text, fontWeight = FontWeight.SemiBold)
                                            Text("${plan.optString("currency", "PHP")} ${plan.optString("amount")} · every ${plan.optInt("interval_count", 1)} ${plan.optString("interval_unit", "MONTH").lowercase()}", color = palette.text, fontSize = 13.sp)
                                            if (plan.optBoolean("is_test")) Text("Test-labelled plan — verify the PayPal environment and recurring charge before enabling checkout.", color = Color(0xFFB97832), fontSize = 10.sp)
                                            Button(enabled = paypalBillingEnabled && !paypalBusy && emailVerified, onClick = {
                                                paypalBusy = true
                                                paypalStatus = "Preparing secure PayPal approval…"
                                                scope.launch {
                                                    try {
                                                        val result = cloud.paypalBilling("create_subscription", JSONObject().put("planKey", plan.optString("plan_key")))
                                                        val approvalUrl = result.optString("approvalUrl")
                                                        val approval = Uri.parse(approvalUrl)
                                                        if (approval.scheme != "https" || !(approval.host == "paypal.com" || approval.host?.endsWith(".paypal.com") == true)) throw IllegalStateException("PayPal returned an unexpected approval destination.")
                                                        context.startActivity(Intent(Intent.ACTION_VIEW, approval))
                                                        paypalStatus = "PayPal approval opened. Return here and refresh subscription status after approval."
                                                    } catch (e: Exception) {
                                                        runCatching { cloud.paypalBilling("cancel_open_checkouts") }
                                                        paypalStatus = (e.message ?: "Could not start PayPal checkout.") + " Any unfinished checkout was sent for cancellation."
                                                    }
                                                    finally { paypalBusy = false }
                                                }
                                            }, modifier = Modifier.fillMaxWidth()) { Text(if (paypalBillingEnabled) "Continue to PayPal" else "Checkout locked pending verification") }
                                        }
                                    }
                                }
                                paypalSubscriptions.forEach { subscription ->
                                    Surface(color = palette.raised.copy(alpha = 0.52f), shape = RoundedCornerShape(12.dp), modifier = Modifier.fillMaxWidth()) {
                                        Column(Modifier.padding(12.dp)) {
                                            Text(subscription.optString("plan_key") + " · " + subscription.optString("status"), color = palette.text, fontWeight = FontWeight.SemiBold)
                                            Text("Subscription " + subscription.optString("provider_subscription_id"), color = palette.muted, fontSize = 10.sp)
                                            if (subscription.optString("status") in setOf("ACTIVE", "APPROVAL_PENDING")) {
                                                TextButton(enabled = !paypalBusy, onClick = {
                                                    paypalBusy = true
                                                    scope.launch {
                                                        try {
                                                            cloud.paypalBilling("cancel", JSONObject().put("subscriptionId", subscription.optString("provider_subscription_id")))
                                                            paypalStatus = "Cancellation requested. Refresh status to confirm."
                                                            val refreshed = cloud.paypalBilling("status").optJSONArray("subscriptions")
                                                            paypalSubscriptions = if (refreshed == null) emptyList() else (0 until refreshed.length()).map { refreshed.getJSONObject(it) }
                                                        } catch (e: Exception) { paypalStatus = e.message ?: "Could not cancel subscription." }
                                                        finally { paypalBusy = false }
                                                    }
                                                }) { Text("Cancel subscription") }
                                            }
                                        }
                                    }
                                }
                            }
                            if (cloudSignedIn) {
                                HorizontalDivider(color = palette.raised)
                                
                    }
                    if (selectedSettingsPage == "Account") {
Text("ADMINISTRATION & ANALYTICS", color = palette.accent, fontSize = 10.sp, fontWeight = FontWeight.Bold)
                                Text("Administrator and owner roles can load privacy-minimized aggregate usage analytics. Ordinary accounts receive an access-denied response.", color = palette.muted, fontSize = 10.sp)
                                OutlinedButton(enabled = !cloudBusy, onClick = {
                                    cloudBusy = true
                                    adminAnalytics = "Loading administrator analytics…"
                                    scope.launch {
                                        try {
                                            val data = cloud.call("analytics_dashboard", JSONObject().put("days", 30))
                                            val statuses = data.optJSONObject("statuses") ?: JSONObject()
                                            val health = data.optJSONObject("guardian") ?: JSONObject()
                                            val providers = data.optJSONArray("providers") ?: org.json.JSONArray()
                                            val models = data.optJSONArray("models") ?: org.json.JSONArray()
                                            val failures = data.optJSONArray("failureCategories") ?: org.json.JSONArray()
                                            val daily = data.optJSONArray("daily") ?: org.json.JSONArray()
                                            val providerLines = (0 until providers.length()).map { index ->
                                                val item = providers.getJSONObject(index)
                                                item.optString("name") + ": " + item.optInt("requests") + " requests"
                                            }
                                            val modelLines = (0 until minOf(models.length(), 5)).map { index ->
                                                val item = models.getJSONObject(index)
                                                item.optString("name") + ": " + item.optInt("requests")
                                            }
                                            val failureLines = (0 until minOf(failures.length(), 5)).map { index ->
                                                val item = failures.getJSONObject(index)
                                                item.optString("category") + ": " + item.optInt("count")
                                            }
                                            val dailyLines = (maxOf(0, daily.length() - 7) until daily.length()).map { index ->
                                                val item = daily.getJSONObject(index)
                                                item.optString("date") + ": " + item.optInt("requests") + " requests"
                                            }
                                            adminAnalytics = "Last 30 days · " + data.optInt("total") + " events" + (if (data.optBoolean("truncated")) " (limited to 5,000)" else "") + "\n" +
                                                "Success " + statuses.optInt("success") + " · Errors " + statuses.optInt("error") + " · Cancelled " + statuses.optInt("cancelled") + "\n" +
                                                "Tokens in/out " + data.optLong("inputTokens") + " / " + data.optLong("outputTokens") + "\n" +
                                                "Average latency " + data.optLong("averageLatencyMs") + " ms · P95 " + data.optLong("p95LatencyMs") + " ms\n" +
                                                "Providers: " + providerLines.joinToString(" · ").ifBlank { "No provider data yet." } + "\n" +
                                                "Models: " + modelLines.joinToString(" · ").ifBlank { "No model data yet." } + "\n" +
                                                "Top failures: " + failureLines.joinToString(" · ").ifBlank { "None recorded." } + "\n" +
                                                "Daily: " + dailyLines.joinToString(" · ").ifBlank { "No daily events." } + "\n" +
                                                "Guardian reports " + health.optInt("reports") + " · Avg battery " + health.opt("averageBatteryPercent") + "% · Avg RAM " + health.opt("averageAvailableRamMb") + " MB"
                                        } catch (e: Exception) {
                                            adminAnalytics = e.message ?: "Administrator analytics could not be loaded."
                                        } finally { cloudBusy = false }
                                    }
                                }, modifier = Modifier.fillMaxWidth()) { Text("Load admin analytics · 30 days") }
                                if (adminAnalytics.isNotBlank()) Text(adminAnalytics, color = palette.muted, fontSize = 11.sp, lineHeight = 16.sp)
                            }
                            if (cloudSignedIn) {
                                HorizontalDivider(color = palette.raised)
                                
                    }
                    if (selectedSettingsPage == "Device" && selectedSettingsCategory == "Privacy & permissions") {
Text("PRIVACY & OPTIONAL CLOUD DIAGNOSTICS", color = palette.accent, fontSize = 10.sp, fontWeight = FontWeight.Bold)
                                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                                    Column(Modifier.weight(1f)) {
                                        Text("Share aggregate usage analytics", color = palette.text, fontSize = 12.sp)
                                        Text("Sends provider/model, token estimates, response time and success/error only. Never sends prompts or answers.", color = palette.muted, fontSize = 10.sp)
                                    }
                                    Switch(checked = usageAnalyticsConsent, onCheckedChange = { usageAnalyticsConsent = it })
                                }
                                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                                    Column(Modifier.weight(1f)) {
                                        Text("Share Guardian health reports", color = palette.text, fontSize = 12.sp)
                                        Text("Sends RAM, storage, battery, Android version, network type and crash category only.", color = palette.muted, fontSize = 10.sp)
                                    }
                                    Switch(checked = guardianCloudConsent, onCheckedChange = { guardianCloudConsent = it })
                                }
                                OutlinedButton(enabled = !cloudBusy, onClick = {
                                    cloudBusy = true
                                    scope.launch {
                                        try {
                                            cloud.setTelemetryConsent(usageAnalyticsConsent, guardianCloudConsent)
                                            vault.put("cloud_usage_analytics_consent", usageAnalyticsConsent.toString())
                                            vault.put("guardian_cloud_consent", guardianCloudConsent.toString())
                                            cloudStatus = "Privacy choices saved. Guardian uploads also require the platform owner to enable the Guardian ingestion flag."
                                        } catch (e: Exception) { cloudStatus = e.message ?: "Could not save privacy choices." }
                                        finally { cloudBusy = false }
                                    }
                                }, modifier = Modifier.fillMaxWidth()) { Text("Save privacy choices") }
                            }
                            if (cloudStatus.isNotBlank()) Text(cloudStatus, color = if (cloudStatus.contains("failed", true) || cloudStatus.contains("error", true)) MaterialTheme.colorScheme.error else palette.muted, fontSize = 11.sp)
                        }
                    }
                    Surface(color = palette.raised, shape = RoundedCornerShape(12.dp), modifier = Modifier.fillMaxWidth()) {
                        Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                            
                    }
                    if (selectedSettingsPage == "Device" && selectedSettingsCategory == "Chat") {
Text("USAGE", color = palette.accent, fontSize = 10.sp, fontWeight = FontWeight.Bold)
                            Text("Today: " + vault.get("usage_count", "0") + " requests", color = palette.text, fontWeight = FontWeight.SemiBold)
                            Text("Estimated input tokens: " + vault.get("usage_estimated_tokens", "0"), color = palette.muted, fontSize = 11.sp)
                            Text("Token count is a local character-based estimate, not provider billing data.", color = palette.muted, fontSize = 10.sp)
                        }
                    }
                    
                    }
                    if (selectedSettingsPage == "Device" && selectedSettingsCategory == "Voice") {
Text("VOICE ENGINE", color = palette.accent, fontSize = 10.sp, fontWeight = FontWeight.Bold)
                    Surface(color = palette.raised, shape = RoundedCornerShape(12.dp), modifier = Modifier.fillMaxWidth()) {
                        Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(7.dp)) {
                            Text("Speech output · Android Text-to-Speech", color = palette.text, fontWeight = FontWeight.SemiBold)
                            Text(if (speechOutputReady) "Voice engine ready" else "Voice engine initializing or unavailable", color = palette.muted, fontSize = 11.sp)
                            OutlinedTextField(value = speechLanguage, onValueChange = { speechLanguage = it.take(40) }, label = { Text("Output language tag (device or en-US)") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                            Text("Speech speed · ${String.format(Locale.US, "%.1f", speechRate)}×", color = palette.text, fontSize = 12.sp)
                            Slider(value = speechRate, onValueChange = { speechRate = it }, valueRange = 0.5f..2f)
                            Text("Voice pitch · ${String.format(Locale.US, "%.1f", speechPitch)}×", color = palette.text, fontSize = 12.sp)
                            Slider(value = speechPitch, onValueChange = { speechPitch = it }, valueRange = 0.5f..2f)
                            OutlinedTextField(value = speechInputLanguage, onValueChange = { speechInputLanguage = it.take(40) }, label = { Text("Input language tag (device or en-US)") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                            Text("Voice input uses Android speech recognition. Voice output uses the speech engine installed on this device. Set either language to 'device' to follow Android language settings.", color = palette.muted, fontSize = 10.sp)
                            OutlinedButton(enabled = speechOutputReady, onClick = { appMonitor.recordEvent("voice_output"); monitorSnapshot = appMonitor.snapshot(); textToSpeech.speak("CenAI voice engine is ready.", TextToSpeech.QUEUE_FLUSH, null, "cenai-voice-test") }, modifier = Modifier.fillMaxWidth()) { Text("Test voice output") }
                        }
                    }
                    
                    }
                    if (selectedSettingsPage == "Device" && selectedSettingsCategory == "Data & storage") {
Text("OFFLINE BRAIN · ON-DEVICE AI", color = palette.accent, fontSize = 10.sp, fontWeight = FontWeight.Bold)
                    Surface(color = palette.raised, shape = RoundedCornerShape(12.dp), modifier = Modifier.fillMaxWidth()) {
                        Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            Text("Private local language model", color = palette.text, fontWeight = FontWeight.SemiBold)
                            Text(if (localModelReady) "Model installed · ${localModelRuntime.modelSizeBytes() / 1_048_576L} MB" else "No local model installed", color = if (localModelReady) palette.accent else palette.muted, fontSize = 11.sp)
                            Text("Import a compatible .litertlm model once. After import, chats can run fully offline on this device using LiteRT-LM CPU inference. Model files stay in private app storage and are not included in the APK.", color = palette.muted, fontSize = 10.sp, lineHeight = 15.sp)
                            OutlinedButton(onClick = { localModelPicker.launch(arrayOf("application/octet-stream", "*/*")) }, modifier = Modifier.fillMaxWidth()) {
                                Text(if (localModelReady) "Replace local model" else "Import .litertlm model")
                            }
                            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                                Column(Modifier.weight(1f)) {
                                    Text("Use Offline AI for chats", color = palette.text)
                                    Text("Overrides cloud providers while enabled", color = palette.muted, fontSize = 10.sp)
                                }
                                Switch(checked = localModelEnabled, enabled = localModelReady, onCheckedChange = {
                                    localModelEnabled = it
                                    vault.put("local_model_enabled", it.toString())
                                })
                            }
                            Text("Recommended model: Qwen3 0.6B INT8 · about 614 MB · Apache 2.0. Download it, then import the .litertlm file above. CPU inference is used for broad device compatibility.", color = palette.muted, fontSize = 10.sp)
                            OutlinedButton(
                                onClick = {
                                    val modelUri = Uri.parse("https://huggingface.co/litert-community/Qwen3-0.6B/resolve/main/Qwen3-0.6B.litertlm")
                                    try { context.startActivity(Intent(Intent.ACTION_VIEW, modelUri)) }
                                    catch (e: Exception) { info = e.message ?: "Could not open model download." }
                                },
                                modifier = Modifier.fillMaxWidth()
                            ) { Text("Download recommended 614 MB model") }
                        }
                    }
                    
                    }
                    if (selectedSettingsPage == "Device" && selectedSettingsCategory == "Data & storage") {
Text("LOCAL KNOWLEDGE", color = palette.accent, fontSize = 10.sp, fontWeight = FontWeight.Bold)
                    Surface(color = palette.raised, shape = RoundedCornerShape(12.dp), modifier = Modifier.fillMaxWidth()) {
                        Row(Modifier.fillMaxWidth().padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
                            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                                Text("Search this device's saved data", color = palette.text, fontWeight = FontWeight.SemiBold)
                                Text("Retrieves relevant excerpts from saved chats and attached text documents locally, then supplies them as context to your selected AI provider. No separate on-device language model is included.", color = palette.muted, fontSize = 10.sp)
                            }
                            Switch(checked = localKnowledgeEnabled, onCheckedChange = { localKnowledgeEnabled = it })
                        }
                    }
                    
                    }
                    if (selectedSettingsPage == "Device" && selectedSettingsCategory == "About & diagnostics") {
Text("GUARDIAN · DEVICE HEALTH", color = palette.accent, fontSize = 10.sp, fontWeight = FontWeight.Bold)
                    Surface(color = palette.raised, shape = RoundedCornerShape(12.dp), modifier = Modifier.fillMaxWidth()) {
                        Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                            Text("Local device diagnostics", color = palette.text, fontWeight = FontWeight.SemiBold)
                            Text("RAM available ${guardianSnapshot.availableRamMb} / ${guardianSnapshot.totalRamMb} MB · App heap ${guardianSnapshot.appHeapUsedMb} / ${guardianSnapshot.appHeapMaxMb} MB", color = palette.muted, fontSize = 10.sp)
                            Text("Storage free ${guardianSnapshot.freeStorageMb} / ${guardianSnapshot.totalStorageMb} MB · Battery ${guardianSnapshot.batteryPercent}% ${if (guardianSnapshot.charging) "· Charging" else ""}", color = palette.muted, fontSize = 10.sp)
                            Text("${guardianSnapshot.device} · Android ${guardianSnapshot.androidVersion} · ${guardianSnapshot.network} · Uptime ${guardianSnapshot.uptimeMinutes} min", color = palette.muted, fontSize = 10.sp)
                            Text("Last crash: ${if (guardianSnapshot.lastCrashAt == 0L) "None recorded" else guardianSnapshot.lastCrashCategory + " · " + java.text.DateFormat.getDateTimeInstance().format(java.util.Date(guardianSnapshot.lastCrashAt))}", color = palette.muted, fontSize = 10.sp)
                            Text("Background Guardian: $guardianHistoryCount health samples · scheduled every 15 minutes when Android permits", color = palette.muted, fontSize = 10.sp)
                            Text("Diagnostics stay on-device by default. If you explicitly enable Guardian sharing in your CenAI account privacy settings, only these summary fields are sent; no stack traces, prompts or personal content.", color = palette.muted, fontSize = 10.sp)
                            if (cloud.isSignedIn() && vault.get("guardian_cloud_consent", "false").toBoolean()) {
                                OutlinedButton(enabled = !cloudBusy, onClick = {
                                    cloudBusy = true
                                    scope.launch {
                                        try {
                                            guardianSnapshot = guardian.snapshot()
                                            cloud.sendGuardianReport("0.2.0", guardianSnapshot.androidVersion, guardianSnapshot.batteryPercent, guardianSnapshot.availableRamMb, guardianSnapshot.freeStorageMb, guardianSnapshot.network, guardianSnapshot.lastCrashCategory)
                                            cloudStatus = "Guardian health report sent."
                                        } catch (e: Exception) { cloudStatus = e.message ?: "Guardian report was not sent." }
                                        finally { cloudBusy = false }
                                    }
                                }, modifier = Modifier.fillMaxWidth()) { Text("Send Guardian health report") }
                            }
                            TextButton(onClick = { guardianSnapshot = guardian.snapshot(); monitorSnapshot = appMonitor.snapshot(); guardianHistoryCount = context.getSharedPreferences("cenai_guardian_history", android.content.Context.MODE_PRIVATE).getString("samples", "").orEmpty().lineSequence().count { it.isNotBlank() } }) { Text("Run health check") }
                        }
                    }
                    
                    }
                    if (selectedSettingsPage == "Device" && selectedSettingsCategory == "About & diagnostics") {
Text("APP MONITOR", color = palette.accent, fontSize = 10.sp, fontWeight = FontWeight.Bold)
                    Surface(color = palette.raised, shape = RoundedCornerShape(12.dp), modifier = Modifier.fillMaxWidth()) {
                        Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                            Text("Local runtime health", color = palette.text, fontWeight = FontWeight.SemiBold)
                            Text("Started ${monitorSnapshot.started} · Successful ${monitorSnapshot.succeeded} · Failed ${monitorSnapshot.failed} · Cancelled ${monitorSnapshot.cancelled}", color = palette.muted, fontSize = 11.sp)
                            Text("Success rate ${monitorSnapshot.successRate}% · Average response ${monitorSnapshot.averageLatencyMs} ms", color = palette.muted, fontSize = 11.sp)
                            Text("Feature activity · App starts ${monitorSnapshot.appStarts} · Voice in/out ${monitorSnapshot.voiceInputs}/${monitorSnapshot.voiceOutputs} · Documents ${monitorSnapshot.documentsAttached} · Update checks ${monitorSnapshot.updateChecks} · Provider tests ${monitorSnapshot.providerTests}", color = palette.muted, fontSize = 10.sp)
                            Text("Last failure category: ${monitorSnapshot.lastFailureCategory.ifBlank { "None recorded" }}", color = palette.muted, fontSize = 10.sp)
                            Text("Only aggregate counters and error categories are stored. Prompts, responses and API keys are never recorded.", color = palette.muted, fontSize = 10.sp)
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
                                TextButton(onClick = { monitorSnapshot = appMonitor.snapshot() }) { Text("Refresh monitor") }
                                TextButton(onClick = {
                                    val snapshot = appMonitor.snapshot()
                                    val report = JSONObject()
                                        .put("generatedAt", System.currentTimeMillis())
                                        .put("started", snapshot.started)
                                        .put("succeeded", snapshot.succeeded)
                                        .put("failed", snapshot.failed)
                                        .put("cancelled", snapshot.cancelled)
                                        .put("successRatePercent", snapshot.successRate)
                                        .put("averageLatencyMs", snapshot.averageLatencyMs)
                                        .put("lastFailureCategory", snapshot.lastFailureCategory)
                                        .put("appStarts", snapshot.appStarts)
                                        .put("voiceInputs", snapshot.voiceInputs)
                                        .put("voiceOutputs", snapshot.voiceOutputs)
                                        .put("documentsAttached", snapshot.documentsAttached)
                                        .put("updateChecks", snapshot.updateChecks)
                                        .put("providerTests", snapshot.providerTests)
                                    val send = Intent(Intent.ACTION_SEND).setType("application/json")
                                        .putExtra(Intent.EXTRA_SUBJECT, "CenAI local analytics")
                                        .putExtra(Intent.EXTRA_TEXT, report.toString(2))
                                    context.startActivity(Intent.createChooser(send, "Export local analytics"))
                                }) { Text("Export analytics") }
                            }
                            TextButton(onClick = {
                                appMonitor.reset()
                                monitorSnapshot = appMonitor.snapshot()
                            }) { Text("Reset local analytics", color = Color(0xFFFF938B)) }
                        }
                    }
                    }
                    if (selectedSettingsPage == "Device" && selectedSettingsCategory == "Chat") {
                        Text("CHAT PREFERENCES", color = palette.accent, fontSize = 10.sp, fontWeight = FontWeight.Bold)
                    OutlinedTextField(value = dailyLimit, onValueChange = { dailyLimit = it.filter(Char::isDigit).take(4) }, label = { Text("Daily message limit (0 = unlimited)") }, singleLine = true)
                    OutlinedTextField(value = maxTokens, onValueChange = { maxTokens = it.filter(Char::isDigit).take(6) },
                        label = { Text("Maximum output tokens") }, singleLine = true)
                    OutlinedTextField(value = systemPrompt, onValueChange = { systemPrompt = it },
                        label = { Text("System instructions (optional)") }, minLines = 2)
                    OutlinedTextField(value = agentMemory, onValueChange = { if (it.length <= 12000) agentMemory = it },
                        label = { Text("Persistent agent memory (optional)") }, minLines = 3)
                    Text("Only details you enter here are retained. Memory is encrypted on this device and can be edited or cleared at any time.", color = palette.muted, fontSize = 10.sp)


                    }
                    if (selectedSettingsPage == "AI & models" && selectedSettingsCategory == "Providers") {
                        Text("PROVIDER CONNECTION", color = palette.accent, fontSize = 10.sp, fontWeight = FontWeight.Bold)
                        Text("Keys are encrypted on this device using Android Keystore. Each provider has its own key.", color = palette.muted, fontSize = 10.sp)
                        if (info.isNotBlank()) Text(info, color = palette.muted, fontSize = 11.sp)
                    OutlinedButton(enabled = !testBusy, onClick = {
                        appMonitor.recordEvent("provider_test"); monitorSnapshot = appMonitor.snapshot()
                        vault.saveApiKey(providerKeys[provider].orEmpty(), provider)
                        testBusy = true; info = "Testing connection…"
                        scope.launch {
                            try {
                                val result = repo.stream(provider, model, base, listOf(ChatMessage("user", "Reply with OK."))) { }
                                info = "Connection successful: " + result.take(80)
                            } catch (e: Exception) { info = e.message ?: "Connection failed." }
                            finally { testBusy = false }
                        }
                    }, modifier = Modifier.fillMaxWidth()) { Text(if (testBusy) "Testing…" else "Test connection") }
                    HorizontalDivider(color = palette.raised)
                    }
                    if (selectedSettingsPage == "Updates" || selectedSettingsCategory == "About & diagnostics") {
Text("SOFTWARE UPDATE", color = palette.accent, fontSize = 10.sp, fontWeight = FontWeight.Bold)
                    Text("Release channel", color = palette.text, fontWeight = FontWeight.SemiBold)
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        FilterChip(selected = updateChannel == "stable", onClick = { updateChannel = "stable" }, label = { Text("Stable") })
                        FilterChip(selected = updateChannel == "beta", onClick = { updateChannel = "beta" }, label = { Text("Public beta") })
                    }
                    Text(if (updateChannel == "beta") "Beta channel includes prereleases. Expect unfinished features and possible bugs." else "Stable channel receives production releases only.", color = palette.muted, fontSize = 10.sp)
                    OutlinedButton(onClick = {
                        appMonitor.recordEvent("update_check"); monitorSnapshot = appMonitor.snapshot()
                        updateDialog = true; updateBusy = true; updateStatus = "Checking GitHub Releases…"
                        scope.launch {
                            try {
                                updateInfo = updater.check(includePrerelease = updateChannel == "beta")
                                updateStatus = if (updateInfo == null) "No public update has been published yet." else "Version ${updateInfo!!.version} is available."
                            } catch (e: Exception) { updateStatus = e.message ?: "Could not check for updates." }
                            finally { updateBusy = false }
                        }
                    }, modifier = Modifier.fillMaxWidth()) { Text("Check for software updates") }
                    Text("Current version 0.2.0 · Updates download and install inside CenAI. Android will ask you to approve installation.", color = palette.muted, fontSize = 10.sp)
                }

                    }
                        }
                    }
            },
            confirmButton = { TextButton(onClick = {
                Provider.entries.forEach { p -> vault.saveApiKey(providerKeys[p].orEmpty(), p) }
                onUiStyleChange(uiStyle)
                vault.put("provider", provider.name)
                vault.put("model", model); vault.put("model_${provider.name}", model)
                vault.put("base_url", base); vault.put("base_url_${provider.name}", base)
                vault.put("multi_agent", multiAgent.toString())
                vault.put("agent_planning", agentPlanning.toString())
                vault.put("google_oauth_client_id", googleOAuthClientId)
                vault.put("google_cloud_project_id", googleCloudProjectId)
                vault.put("daily_request_limit", dailyLimit.ifBlank { "0" })
                vault.put("speech_rate", speechRate.toString())
                vault.put("speech_pitch", speechPitch.toString())
                vault.put("speech_language", speechLanguage.ifBlank { "device" })
                vault.put("speech_input_language", speechInputLanguage.ifBlank { "device" })
                vault.put("local_knowledge_enabled", localKnowledgeEnabled.toString())
                vault.put("local_model_enabled", localModelEnabled.toString())
                vault.put("update_channel", updateChannel)
                vault.put("system_prompt", systemPrompt); vault.putSecret("agent_memory", agentMemory.trim()); vault.put("custom_headers", customHeaders); vault.put("custom_params", customParams)
                vault.put("max_tokens", maxTokens.ifBlank { "4096" })
                settings = false
            }) { Text("Save settings", color = palette.accent) } },
            dismissButton = { TextButton(onClick = { settings = false }) { Text("Close") } },
            titleContentColor = palette.text,
            textContentColor = palette.text,
            shape = RoundedCornerShape(24.dp)
        )
    }

    if (editIndex != null) {
        AlertDialog(onDismissRequest = { editIndex = null }, title = { Text("Edit message") },
            text = { OutlinedTextField(value = editText, onValueChange = { editText = it }, minLines = 3) },
            confirmButton = { TextButton(onClick = {
                val index = editIndex!!
                val changed = messages.take(index) + ChatMessage("user", editText) 
                editIndex = null
                startRequest(changed)
            }) { Text("Save and resend") } },
            dismissButton = { TextButton(onClick = { editIndex = null }) { Text("Cancel") } })
    }

    if (updateDialog) {
        AlertDialog(onDismissRequest = { if (!updateBusy) updateDialog = false },
            title = { Text("CenAI software update") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Text(updateStatus)
                    updateInfo?.let { Text("Version ${it.version}", color = palette.accent, fontWeight = FontWeight.Bold) }
                    if (updateBusy && updateProgress > 0) LinearProgressIndicator(progress = updateProgress / 100f, modifier = Modifier.fillMaxWidth())
                    if (!updateInfo?.notes.isNullOrBlank()) Text(updateInfo?.notes.orEmpty().take(800), color = palette.muted, fontSize = 12.sp)
                }
            },
            confirmButton = {
                if (updateInfo != null) TextButton(enabled = !updateBusy, onClick = {
                    updateBusy = true; updateStatus = "Downloading update…"
                    scope.launch {
                        try {
                            updater.downloadAndInstall(updateInfo!!) { updateProgress = it }
                            updateStatus = "Opening Android installer…"
                        } catch (e: Exception) { updateStatus = e.message ?: "Update installation failed." }
                        finally { updateBusy = false }
                    }
                }) { Text("Download and install", color = palette.accent) }
                else TextButton(onClick = { updateDialog = false }) { Text("Done") }
            },
            dismissButton = { if (updateInfo != null) TextButton(onClick = { updateDialog = false }) { Text("Later") } }
        )
    }

}


private inline fun <T> List<T>.takeWhileIndexed(predicate: (Int, T) -> Boolean): List<T> {
    val result = ArrayList<T>()
    forEachIndexed { index, item -> if (predicate(index, item)) result.add(item) }
    return result
}
