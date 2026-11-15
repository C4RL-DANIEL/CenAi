package com.cenai.app

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ArrowUpward
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.*
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.launch

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CenAiScreen(vault: SecureKeyStore) {
    val repo = remember { ChatRepository(vault) }
    val scope = rememberCoroutineScope()
    var provider by remember { mutableStateOf(Provider.parse(vault.get("provider", "OPENAI"))) }
    var model by remember { mutableStateOf(vault.get("model", provider.model)) }
    var base by remember { mutableStateOf(vault.get("base_url", provider.baseUrl)) }
    var key by remember { mutableStateOf(vault.apiKey()) }
    var draft by remember { mutableStateOf("") }
    var settings by remember { mutableStateOf(false) }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf("") }
    val messages = remember { mutableStateListOf<ChatMessage>() }

    Column(Modifier.fillMaxSize().background(Color(0xFF171717))) {
        Row(Modifier.fillMaxWidth().padding(18.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("CenAI", color = Color(0xFFEDE7DF), fontSize = 22.sp, fontWeight = FontWeight.SemiBold)
            Spacer(Modifier.weight(1f))
            Text(provider.label, color = Color(0xFFBCA58F), fontSize = 13.sp)
            IconButton(onClick = { settings = true }) { Icon(Icons.Default.Settings, "Settings", tint = Color.White) }
        }
        HorizontalDivider(color = Color(0xFF343434))
        if (messages.isEmpty()) {
            Column(Modifier.weight(1f).fillMaxWidth().padding(24.dp),
                verticalArrangement = Arrangement.Center, horizontalAlignment = Alignment.CenterHorizontally) {
                Text("What can I help you with?", color = Color(0xFFEDE7DF), fontSize = 25.sp)
                Spacer(Modifier.height(8.dp))
                Text("Your AI workspace. One conversation at a time.", color = Color.Gray)
            }
        } else {
            LazyColumn(Modifier.weight(1f).fillMaxWidth().padding(horizontal = 16.dp),
                contentPadding = PaddingValues(vertical = 18.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                items(messages) { m ->
                    Column(Modifier.fillMaxWidth(), horizontalAlignment =
                        if (m.role == "user") Alignment.End else Alignment.Start) {
                        Text(if (m.role == "user") "You" else provider.label,
                            color = Color(0xFFBCA58F), fontSize = 12.sp, fontWeight = FontWeight.SemiBold)
                        Surface(color = if (m.role == "user") Color(0xFF30302E) else Color.Transparent,
                            shape = RoundedCornerShape(14.dp)) {
                            Text(m.content, Modifier.padding(12.dp), color = Color(0xFFEDE7DF), fontSize = 15.sp)
                        }
                    }
                }
                if (busy) item { Text("Thinking…", color = Color(0xFFBCA58F)) }
            }
        }
        if (error.isNotBlank()) Text(error, color = Color(0xFFFF958B),
            modifier = Modifier.padding(horizontal = 16.dp), fontSize = 12.sp)
        Surface(Modifier.fillMaxWidth().padding(12.dp), color = Color(0xFF252525),
            shape = RoundedCornerShape(22.dp)) {
            Column(Modifier.padding(12.dp)) {
                BasicTextField(value = draft, onValueChange = { draft = it },
                    modifier = Modifier.fillMaxWidth().heightIn(min = 54.dp, max = 140.dp).padding(8.dp),
                    textStyle = TextStyle(color = Color.White, fontSize = 15.sp),
                    decorationBox = { inner -> Box {
                        if (draft.isEmpty()) Text("Message CenAI…", color = Color.Gray)
                        inner()
                    } })
                Row(verticalAlignment = Alignment.CenterVertically) {
                    TextButton(onClick = { settings = true }) {
                        Text("${provider.label} · $model", color = Color(0xFFBCA58F), fontSize = 12.sp)
                    }
                    Spacer(Modifier.weight(1f))
                    IconButton(enabled = draft.isNotBlank() && !busy, onClick = {
                        val prompt = runCatching { NativeCore.normalizePrompt(draft) }.getOrDefault(draft.trim())
                        if (prompt.isNotBlank()) {
                            messages.add(ChatMessage("user", prompt)); draft = ""; error = ""; busy = true
                            scope.launch {
                                try { messages.add(ChatMessage("assistant", repo.send(provider, model, base, messages.toList()))) }
                                catch (e: Exception) { error = e.message ?: "Request failed." }
                                finally { busy = false }
                            }
                        }
                    }) { Icon(Icons.Default.ArrowUpward, "Send", tint = Color(0xFFD6A879)) }
                }
            }
        }
    }

    if (settings) {
        var expanded by remember { mutableStateOf(false) }
        AlertDialog(onDismissRequest = { settings = false },
            title = { Text("Model settings") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    ExposedDropdownMenuBox(expanded = expanded, onExpandedChange = { expanded = !expanded }) {
                        OutlinedTextField(value = provider.label, onValueChange = {}, readOnly = true,
                            label = { Text("Provider") }, trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(expanded) },
                            modifier = Modifier.menuAnchor().fillMaxWidth())
                        ExposedDropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
                            Provider.entries.forEach { p ->
                                DropdownMenuItem(text = { Text(p.label) }, onClick = {
                                    provider = p; model = p.model; base = p.baseUrl; expanded = false
                                })
                            }
                        }
                    }
                    OutlinedTextField(value = model, onValueChange = { model = it },
                        label = { Text("Model ID") }, singleLine = true)
                    if (provider == Provider.CUSTOM) OutlinedTextField(value = base, onValueChange = { base = it },
                        label = { Text("HTTPS API base URL") }, singleLine = true)
                    OutlinedTextField(value = key, onValueChange = { key = it },
                        label = { Text("API key") }, singleLine = true)
                    Text("API key is encrypted locally with Android Keystore.", color = Color.Gray, fontSize = 11.sp)
                }
            },
            confirmButton = { TextButton(onClick = {
                vault.saveApiKey(key); vault.put("provider", provider.name)
                vault.put("model", model); vault.put("base_url", base); settings = false
            }) { Text("Save") } },
            dismissButton = { TextButton(onClick = { settings = false }) { Text("Cancel") } })
    }
}
