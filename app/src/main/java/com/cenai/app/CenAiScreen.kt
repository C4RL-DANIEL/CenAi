package com.cenai.app

import android.content.Intent
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.fadeIn
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
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
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.History
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Stop
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.util.UUID

@Composable
fun CenAiScreen(vault: SecureKeyStore) {
    var selectedStyle by remember { mutableStateOf(vault.get("ui_style", "DEFAULT")) }
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
    val repo = remember { ChatRepository(vault) }
    val store = remember { ChatStore(context) }
    val updater = remember { UpdateManager(context) }
    val scope = rememberCoroutineScope()
    val clipboard = LocalClipboardManager.current
    var provider by remember { mutableStateOf(Provider.parse(vault.get("provider", "OPENAI"))) }
    var model by remember { mutableStateOf(vault.get("model", Provider.parse(vault.get("provider", "OPENAI")).model)) }
    var base by remember { mutableStateOf(vault.get("base_url", provider.baseUrl)) }
    var providerKeys by remember { mutableStateOf(Provider.entries.associateWith { vault.apiKey(it) }) }
    var multiAgent by remember { mutableStateOf(vault.get("multi_agent", "false").toBoolean()) }
    var agentPlanning by remember { mutableStateOf(vault.get("agent_planning", "false").toBoolean()) }
    var approvalFirst by remember { mutableStateOf(vault.get("approval_first", "true").toBoolean()) }
    var googleOAuthClientId by remember { mutableStateOf(vault.get("google_oauth_client_id")) }
    var googleCloudProjectId by remember { mutableStateOf(vault.get("google_cloud_project_id")) }
    var dailyLimit by remember { mutableStateOf(vault.get("daily_request_limit", "0")) }
    var systemPrompt by remember { mutableStateOf(vault.get("system_prompt", "")) }
    var customHeaders by remember { mutableStateOf(vault.get("custom_headers", "{}")) }
    var customParams by remember { mutableStateOf(vault.get("custom_params", "{}")) }
    var maxTokens by remember { mutableStateOf(vault.get("max_tokens", "4096")) }
    var draft by remember { mutableStateOf("") }
    var settings by remember { mutableStateOf(false) }
    var historyOpen by remember { mutableStateOf(false) }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf("") }
    var info by remember { mutableStateOf("") }
    var testBusy by remember { mutableStateOf(false) }
    var conversations by remember { mutableStateOf(store.load()) }
    var currentId by remember { mutableStateOf(conversations.firstOrNull()?.id ?: UUID.randomUUID().toString()) }
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
        val targets = if (multiAgent) Provider.entries.filter { !providerKeys[it].isNullOrBlank() || (it == Provider.GEMINI && vault.getSecret("gemini_oauth_access_token").isNotBlank()) } else listOf(provider)
        if (targets.isEmpty()) {
            error = "Add at least one provider API key to use multi-agent mode."
            return
        }
        busy = true
        error = ""
        persist(requestMessages)
        activeJob = scope.launch {
            try {
                for (target in targets) {
                    val targetModel = if (target == provider) model else vault.get("model_${target.name}", target.model)
                    val targetBase = if (target == provider) base else vault.get("base_url_${target.name}", target.baseUrl)
                    val heading = if (multiAgent) "${target.label} response\n\n" else ""
                    val runtime = AgentRuntime()
                    val toolGuide = if (agentPlanning) {
                        "You may use these safe local tools when useful: " + runtime.availableTools().joinToString { it.name + " — " + it.description } +
                        ". To call one, output exactly [[CENAI_TOOL_CALL]] followed by a JSON object with name and arguments, then [[/CENAI_TOOL_CALL]]. Example: [[CENAI_TOOL_CALL]]{\"name\":\"current_time\",\"arguments\":{}}[[/CENAI_TOOL_CALL]]. Only call listed tools. Do not claim that unsupported shell, arbitrary code, device-control, file-write or web actions were performed. Explain tool results to the user."
                    } else null
                    val systemMessages = listOfNotNull(systemPrompt.takeIf { it.isNotBlank() }, toolGuide)
                        .takeIf { it.isNotEmpty() }?.joinToString("\n\n")?.let { listOf(ChatMessage("system", it)) }.orEmpty()
                    var agentHistory = requestMessages.toList()
                    var displayText = heading
                    persist(messages + ChatMessage("assistant", displayText))
                    var finished = false
                    for (step in 0..3) {
                        val response = repo.stream(target, targetModel, targetBase, systemMessages + agentHistory) { delta ->
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
                        val execution = runtime.execute(toolCall.first, toolCall.second)
                        displayText += visible + "\n\n" + if (execution.completed) "Used tool: " + execution.toolName else "Tool unavailable: " + execution.toolName
                        val current = messages.toMutableList()
                        if (current.lastOrNull()?.role == "assistant") current[current.lastIndex] = ChatMessage("assistant", displayText)
                        persist(current)
                        if (execution.requiresApproval) {
                            displayText += "\n\nWaiting for your approval."
                            persist(messages.dropLast(1) + ChatMessage("assistant", displayText))
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
                }
            } catch (e: Exception) {
                if (e !is kotlinx.coroutines.CancellationException) error = e.message ?: "Request failed."
                if (messages.lastOrNull()?.let { it.role == "assistant" && it.content.isBlank() } == true) {
                    persist(messages.dropLast(1))
                } else persist()
            } finally {
                busy = false
                activeJob = null
            }
        }
    }

    val attachmentPicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) scope.launch {
            val content = withContext(Dispatchers.IO) {
                runCatching { context.contentResolver.openInputStream(uri)?.bufferedReader()?.use { it.readText().take(250_000) } }.getOrNull()
            }
            if (content.isNullOrBlank()) error = "This file could not be read as text. Try a TXT, MD, CSV or JSON file."
            else draft += "\n\n[Attached document]\n$content"
        }
    }

    LaunchedEffect(messages.size, messages.lastOrNull()?.content?.length) {
        if (messages.isNotEmpty()) listState.animateScrollToItem(messages.lastIndex)
    }

    Scaffold(
        containerColor = palette.background,
        topBar = {
            TopAppBar(
                title = {
                    Column {
                        Text("CenAI", fontWeight = FontWeight.Bold, color = palette.text, fontSize = 20.sp)
                        Text(provider.label + "  ·  " + model, color = palette.muted, fontSize = 11.sp, maxLines = 1)
                    }
                },
                navigationIcon = {
                    IconButton(onClick = { historyOpen = true }) { Icon(Icons.Default.History, "Chat history", tint = palette.text) }
                },
                actions = {
                    IconButton(onClick = { newChat() }) { Icon(Icons.Default.Add, "New chat", tint = palette.accent) }
                    IconButton(onClick = { settings = true }) { Icon(Icons.Default.Settings, "Settings", tint = palette.text) }
                },
                colors = TopAppBarDefaults.topAppBarColors(containerColor = palette.background)
            )
        },
        bottomBar = {
            Surface(color = palette.background, tonalElevation = 4.dp) {
                Column(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 10.dp)) {
                    AnimatedVisibility(visible = error.isNotBlank()) {
                        Text(error, color = Color(0xFFFF938B), fontSize = 12.sp, modifier = Modifier.padding(8.dp))
                    }
                    Surface(color = palette.surface, shape = RoundedCornerShape(26.dp), modifier = Modifier.fillMaxWidth()) {
                        Column(Modifier.padding(12.dp)) {
                            BasicTextField(
                                value = draft, onValueChange = { draft = it },
                                modifier = Modifier.fillMaxWidth().heightIn(min = 46.dp, max = 150.dp).padding(8.dp),
                                textStyle = TextStyle(color = palette.text, fontSize = 15.sp),
                                decorationBox = { inner ->
                                    Box {
                                        if (draft.isEmpty()) Text("Message CenAI…", color = palette.muted)
                                        inner()
                                    }
                                }
                            )
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                IconButton(onClick = { attachmentPicker.launch(arrayOf("text/*", "application/json")) }) {
                                    Icon(Icons.Default.AttachFile, "Attach text document", tint = palette.muted)
                                }
                                TextButton(onClick = { settings = true }) { Text("Configure", color = palette.accent, fontSize = 12.sp) }
                                Spacer(Modifier.weight(1f))
                                AnimatedContent(targetState = busy, label = "send-stop") { sending ->
                                    if (sending) {
                                        FilledIconButton(onClick = { activeJob?.cancel(); busy = false },
                                            colors = IconButtonDefaults.filledIconButtonColors(containerColor = Color(0xFF4B3030))) {
                                            Icon(Icons.Default.Stop, "Stop response")
                                        }
                                    } else {
                                        FilledIconButton(
                                            enabled = draft.isNotBlank(),
                                            onClick = {
                                                val prompt = runCatching { NativeCore.normalizePrompt(draft) }.getOrDefault(draft.trim())
                                                if (prompt.isNotBlank()) {
                                                    val request = messages + ChatMessage("user", prompt)
                                                    draft = ""
                                                    startRequest(request)
                                                }
                                            },
                                            colors = IconButtonDefaults.filledIconButtonColors(containerColor = palette.accent, contentColor = palette.background)
                                        ) { Icon(Icons.Default.ArrowUpward, "Send") }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    ) { padding ->
        Column(Modifier.fillMaxSize().padding(padding)) {
            if (messages.isEmpty()) {
                Column(Modifier.fillMaxSize().padding(24.dp), verticalArrangement = Arrangement.Center) {
                    Text("Your AI workspace.", color = palette.muted, fontSize = 13.sp)
                    Spacer(Modifier.height(8.dp))
                    Text("What would you like to\ncreate today?", color = palette.text, fontSize = 31.sp,
                        lineHeight = 37.sp, fontWeight = FontWeight.SemiBold)
                    Spacer(Modifier.height(24.dp))
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        SuggestionChip("Explore an idea") { draft = "Help me explore an idea: " }
                        SuggestionChip("Write something") { draft = "Help me write: " }
                    }
                    SuggestionChip("Explain a topic") { draft = "Explain this topic clearly: " }
                }
            } else {
                LazyColumn(
                    state = listState, modifier = Modifier.fillMaxSize().padding(horizontal = 14.dp),
                    contentPadding = PaddingValues(top = 16.dp, bottom = 18.dp),
                    verticalArrangement = Arrangement.spacedBy(18.dp)
                ) {
                    itemsIndexed(messages, key = { index, _ -> "$currentId-$index" }) { index, message ->
                        MessageBubble(
                            message = message, index = index, last = index == messages.lastIndex,
                            busy = busy, onCopy = { clipboard.setText(AnnotatedString(message.content)); info = "Copied to clipboard." },
                            onCopyCode = { code -> clipboard.setText(AnnotatedString(code)); info = "Code copied to clipboard." },
                            onEdit = { editIndex = index; editText = message.content },
                            onRegenerate = {
                                val before = messages.takeWhileIndexed { i, _ -> i < index }
                                startRequest(before)
                            }
                        )
                    }
                    if (busy && messages.lastOrNull()?.content.isNullOrBlank()) {
                        item { Text("CenAI is thinking…", color = palette.accent, fontSize = 12.sp) }
                    }
                }
            }
        }
    }

    if (historyOpen) {
        ModalBottomSheet(onDismissRequest = { historyOpen = false }, containerColor = palette.surface) {
            Row(Modifier.fillMaxWidth().padding(horizontal = 20.dp), verticalAlignment = Alignment.CenterVertically) {
                Text("Your conversations", color = palette.text, fontSize = 19.sp, fontWeight = FontWeight.SemiBold)
                Spacer(Modifier.weight(1f))
                TextButton(onClick = { newChat(); historyOpen = false }) { Text("New chat", color = palette.accent) }
            }
            if (conversations.isEmpty()) Text("Your saved chats will appear here.", color = palette.muted, modifier = Modifier.padding(24.dp))
            conversations.forEach { chat ->
                Row(Modifier.fillMaxWidth().clickable {
                    activeJob?.cancel(); busy = false
                    currentId = chat.id; messages = chat.messages.toList(); historyOpen = false
                }.padding(horizontal = 20.dp, vertical = 14.dp), verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text(chat.title, color = palette.text, maxLines = 1)
                        Text("${chat.messages.size} messages", color = palette.muted, fontSize = 11.sp)
                    }
                    IconButton(onClick = {
                        conversations = conversations.filterNot { it.id == chat.id }.toMutableList()
                        store.save(conversations)
                        if (currentId == chat.id) newChat()
                    }) { Icon(Icons.Default.Delete, "Delete conversation", tint = palette.muted) }
                }
            }
            Spacer(Modifier.height(24.dp))
        }
    }

    if (settings) {
        var expanded by remember { mutableStateOf(false) }
        var themeExpanded by remember { mutableStateOf(false) }
        var uiStyle by remember { mutableStateOf(vault.get("ui_style", "DEFAULT")) }
        AlertDialog(
            onDismissRequest = { settings = false },
            title = {
                Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
                    Text("CenAI settings", color = palette.text, fontWeight = FontWeight.Bold)
                    Text("Providers · Agents · Security · Updates", color = palette.muted, fontSize = 11.sp)
                }
            },
            text = {
                Column(Modifier.fillMaxWidth().heightIn(max = 540.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(9.dp)) {
                    Text("APPEARANCE", color = palette.accent, fontSize = 10.sp, fontWeight = FontWeight.Bold)
                    ExposedDropdownMenuBox(expanded = themeExpanded, onExpandedChange = { themeExpanded = !themeExpanded }) {
                        OutlinedTextField(
                            value = when (uiStyle) {
                                "MATERIAL" -> "Android · Material 3"
                                "IOS" -> "iOS-inspired"
                                "FIGMA" -> "Figma design"
                                else -> "CenAI Default"
                            },
                            onValueChange = {}, readOnly = true, label = { Text("Interface style") },
                            trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(themeExpanded) },
                            modifier = Modifier.menuAnchor().fillMaxWidth()
                        )
                        ExposedDropdownMenu(expanded = themeExpanded, onDismissRequest = { themeExpanded = false }) {
                            listOf("DEFAULT" to "CenAI Default", "MATERIAL" to "Android · Material 3", "IOS" to "iOS-inspired", "FIGMA" to "Figma design").forEach { (key, label) ->
                                DropdownMenuItem(text = { Text(label) }, onClick = { uiStyle = key; themeExpanded = false })
                            }
                        }
                    }
                    Text(when (uiStyle) {
                        "MATERIAL" -> "Material You colors follow your wallpaper on Android 12 and newer."
                        "IOS" -> "Rounded cards, softer surfaces and iOS-style spacing."
                        "FIGMA" -> "Figma-inspired violet and indigo design tokens. Connect a Figma file to import its exact assets."
                        else -> "The original CenAI dark workspace."
                    }, color = palette.muted, fontSize = 10.sp)
                    HorizontalDivider(color = palette.raised)
                    Text("PROVIDER", color = palette.accent, fontSize = 10.sp, fontWeight = FontWeight.Bold)
                    ExposedDropdownMenuBox(expanded = expanded, onExpandedChange = { expanded = !expanded }) {
                        OutlinedTextField(value = provider.label, onValueChange = {}, readOnly = true,
                            label = { Text("AI provider") }, trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(expanded) },
                            modifier = Modifier.menuAnchor().fillMaxWidth())
                        ExposedDropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
                            Provider.entries.forEach { p ->
                                DropdownMenuItem(text = { Text(p.label) }, onClick = {
                                    provider = p; model = vault.get("model_${p.name}", p.model); base = vault.get("base_url_${p.name}", p.baseUrl); expanded = false
                                })
                            }
                        }
                    }
                    OutlinedTextField(value = model, onValueChange = { model = it }, label = { Text("Model ID") }, singleLine = true)
                    if (provider == Provider.CUSTOM) OutlinedTextField(value = base, onValueChange = { base = it },
                        label = { Text("HTTPS API base URL") }, singleLine = true)
                    Provider.entries.forEach { p ->
                        OutlinedTextField(value = providerKeys[p].orEmpty(), onValueChange = { value -> providerKeys = providerKeys.toMutableMap().apply { put(p, value) } }, label = { Text("${p.label} API key") }, singleLine = true)
                    }
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
                    Text("AGENT WORKSPACE", color = palette.accent, fontSize = 10.sp, fontWeight = FontWeight.Bold)
                    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                        Column(Modifier.weight(1f)) {
                            Text("Agent planning mode", color = palette.text)
                            Text("Break complex requests into steps and track what needs doing", color = palette.muted, fontSize = 10.sp)
                        }
                        Switch(checked = agentPlanning, onCheckedChange = { agentPlanning = it })
                    }
                    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                        Column(Modifier.weight(1f)) {
                            Text("Approval-first actions", color = palette.text)
                            Text("Require your confirmation before external or destructive actions", color = palette.muted, fontSize = 10.sp)
                        }
                        Switch(checked = approvalFirst, onCheckedChange = { approvalFirst = it }, enabled = agentPlanning)
                    }
                    Text("Planning mode currently creates structured plans in the AI response. Direct device control, background execution and external tool actions are not enabled in this build.", color = palette.muted, fontSize = 10.sp)
                    HorizontalDivider(color = palette.raised)
                    Text("ACCOUNT CONNECTIONS", color = palette.accent, fontSize = 10.sp, fontWeight = FontWeight.Bold)
                    Surface(color = palette.raised, shape = RoundedCornerShape(12.dp), modifier = Modifier.fillMaxWidth()) {
                        Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            Text("Google Gemini account", color = palette.text, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
                            Text(if (vault.getSecret("gemini_oauth_access_token").isNotBlank()) "Google account connected" else "Not connected", color = if (vault.getSecret("gemini_oauth_access_token").isNotBlank()) palette.accent else palette.muted, fontSize = 11.sp)
                            OutlinedTextField(value = googleOAuthClientId, onValueChange = { googleOAuthClientId = it.trim() }, label = { Text("Google OAuth Client ID") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                            OutlinedTextField(value = googleCloudProjectId, onValueChange = { googleCloudProjectId = it.trim() }, label = { Text("Google Cloud Project ID") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                            Text("Create an OAuth client in Google Cloud for this Android app, enable the Generative Language API, configure the consent screen and add the redirect URI: com.cenai.app:/oauth2redirect. Google may require package/SHA-1 verification and app review. Gemini consumer subscriptions do not automatically include API access.", color = palette.muted, fontSize = 10.sp, lineHeight = 15.sp)
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                Button(onClick = {
                                    vault.put("google_oauth_client_id", googleOAuthClientId)
                vault.put("google_cloud_project_id", googleCloudProjectId)
                                    vault.put("google_cloud_project_id", googleCloudProjectId)
                                    runCatching { context.startActivity(GoogleGeminiOAuth(vault).authorizationIntent(googleOAuthClientId)) }
                                        .onFailure { info = it.message ?: "Could not start Google sign-in." }
                                }, enabled = googleOAuthClientId.endsWith(".apps.googleusercontent.com"), modifier = Modifier.weight(1f)) { Text("Connect Google") }
                                OutlinedButton(onClick = { GoogleGeminiOAuth(vault).disconnect(); info = "Google account disconnected." }) { Text("Disconnect") }
                            }
                        }
                    }
                    OutlinedTextField(value = dailyLimit, onValueChange = { dailyLimit = it.filter(Char::isDigit).take(4) }, label = { Text("Daily message limit (0 = unlimited)") }, singleLine = true)
                    OutlinedTextField(value = maxTokens, onValueChange = { maxTokens = it.filter(Char::isDigit).take(6) },
                        label = { Text("Maximum output tokens") }, singleLine = true)
                    OutlinedTextField(value = systemPrompt, onValueChange = { systemPrompt = it },
                        label = { Text("System instructions (optional)") }, minLines = 2)
                    Text("Keys are encrypted on this device using Android Keystore. Each provider has its own key.", color = palette.muted, fontSize = 10.sp)
                    if (info.isNotBlank()) Text(info, color = palette.accent, fontSize = 11.sp)
                    OutlinedButton(enabled = !testBusy, onClick = {
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
                    Text("SOFTWARE UPDATE", color = palette.accent, fontSize = 10.sp, fontWeight = FontWeight.Bold)
                    OutlinedButton(onClick = {
                        updateDialog = true; updateBusy = true; updateStatus = "Checking GitHub Releases…"
                        scope.launch {
                            try {
                                updateInfo = updater.check()
                                updateStatus = if (updateInfo == null) "No public update has been published yet." else "Version ${updateInfo!!.version} is available."
                            } catch (e: Exception) { updateStatus = e.message ?: "Could not check for updates." }
                            finally { updateBusy = false }
                        }
                    }, modifier = Modifier.fillMaxWidth()) { Text("Check for software updates") }
                    Text("Current version 0.2.0 · Updates download and install inside CenAI. Android will ask you to approve installation.", color = palette.muted, fontSize = 10.sp)
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
                vault.put("approval_first", approvalFirst.toString())
                vault.put("google_oauth_client_id", googleOAuthClientId)
                vault.put("google_cloud_project_id", googleCloudProjectId)
                vault.put("daily_request_limit", dailyLimit.ifBlank { "0" })
                vault.put("system_prompt", systemPrompt); vault.put("custom_headers", customHeaders); vault.put("custom_params", customParams)
                vault.put("max_tokens", maxTokens.ifBlank { "4096" })
                settings = false
            }) { Text("Save settings", color = palette.accent) } },
            dismissButton = { TextButton(onClick = { settings = false }) { Text("Close") } },
            containerColor = palette.surface,
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

@Composable
private fun SuggestionChip(label: String, onClick: () -> Unit) {
    val palette = LocalCenAiPalette.current
    Surface(onClick = onClick, color = palette.surface, shape = RoundedCornerShape(16.dp), border = androidx.compose.foundation.BorderStroke(1.dp, palette.raised)) {
        Text(label, color = palette.text, fontSize = 12.sp, modifier = Modifier.padding(horizontal = 13.dp, vertical = 10.dp))
    }
}

@Composable
private fun MessageBubble(
    message: ChatMessage, index: Int, last: Boolean, busy: Boolean,
    onCopy: () -> Unit, onCopyCode: (String) -> Unit, onEdit: () -> Unit, onRegenerate: () -> Unit
) {
    val palette = LocalCenAiPalette.current
    val user = message.role == "user"
    AnimatedVisibility(
        visible = true,
        enter = fadeIn(tween(280)) + slideInVertically(tween(280)) { it / 10 }
    ) {
    Column(Modifier.fillMaxWidth(), horizontalAlignment = if (user) Alignment.End else Alignment.Start) {
        Text(if (user) "YOU" else "CENAI", color = palette.accent, fontSize = 10.sp, fontWeight = FontWeight.Bold)
        Surface(
            color = if (user) palette.raised else palette.surface,
            shape = RoundedCornerShape(if (user) 20.dp else 6.dp, 20.dp, 20.dp, 20.dp),
            modifier = Modifier.fillMaxWidth(if (user) 0.92f else 1f).animateContentSize(spring(stiffness = Spring.StiffnessMediumLow))
        ) {
            Column(Modifier.fillMaxWidth().padding(15.dp)) {
                if (message.content.isBlank()) Text(" ", color = palette.text)
                else RichMessageContent(message.content, onCopyCode)
            }
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            TextButton(onClick = onCopy) { Icon(Icons.Default.ContentCopy, null, Modifier.size(13.dp), tint = palette.muted); Spacer(Modifier.width(4.dp)); Text("Copy", color = palette.muted, fontSize = 10.sp) }
            if (user && !busy) TextButton(onClick = onEdit) { Text("Edit", color = palette.muted, fontSize = 10.sp) }
            if (!user && last && !busy) TextButton(onClick = onRegenerate) { Icon(Icons.Default.Refresh, null, Modifier.size(13.dp), tint = palette.muted); Spacer(Modifier.width(4.dp)); Text("Regenerate", color = palette.muted, fontSize = 10.sp) }
        }
    }
    }
}

private inline fun <T> List<T>.takeWhileIndexed(predicate: (Int, T) -> Boolean): List<T> {
    val result = ArrayList<T>()
    forEachIndexed { index, item -> if (predicate(index, item)) result.add(item) }
    return result
}
