package com.cenai.app

import android.net.Uri
import android.provider.OpenableColumns
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.ui.window.Dialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

@Composable
fun YourWorkDialog(vault: SecureKeyStore, ownerId: String, onDismiss: () -> Unit) {
    val context = LocalContext.current
    val store = remember(ownerId) { ProjectWorkspaceStore(context, vault, ownerId) }
    val backupClient = remember(ownerId) { WorkspaceBackupClient(vault, ownerId) }
    val scope = rememberCoroutineScope()
    var projects by remember { mutableStateOf(runCatching { store.list(includeArchived = true) }.getOrDefault(emptyList())) }
    var selectedId by remember { mutableStateOf<String?>(null) }
    var showCreate by remember { mutableStateOf(false) }
    var showDelete by remember { mutableStateOf(false) }
    var projectName by remember { mutableStateOf("") }
    var projectDescription by remember { mutableStateOf("") }
    var noteName by remember { mutableStateOf("") }
    var noteBody by remember { mutableStateOf("") }
    var preview by remember { mutableStateOf("") }
    var status by remember { mutableStateOf("") }
    val selected = projects.firstOrNull { it.id == selectedId }
    var pendingExportBytes by remember { mutableStateOf<ByteArray?>(null) }
    var exportStatus by remember { mutableStateOf("") }
    var backupPassphrase by remember { mutableStateOf("") }
    var backupBusy by remember { mutableStateOf(false) }
    var backupStatus by remember { mutableStateOf("") }
    val exportLauncher = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/octet-stream")) { uri: Uri? ->
        val bytes = pendingExportBytes
        if (uri != null && bytes != null) {
            runCatching {
                context.contentResolver.openOutputStream(uri)?.use { output -> output.write(bytes) }
                    ?: error("Android could not create the export file.")
            }.onSuccess { exportStatus = "File exported successfully." }
                .onFailure { exportStatus = it.message ?: "Could not export file." }
        }
        pendingExportBytes = null
    }
    fun refresh() {
        projects = runCatching { store.list(includeArchived = true) }
            .onFailure { status = it.message ?: "Could not load projects." }
            .getOrDefault(emptyList())
        if (selectedId != null && projects.none { it.id == selectedId }) selectedId = null
    }

    val importLauncher = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri: Uri? ->
        val projectId = selectedId
        if (uri != null && projectId != null) {
            runCatching {
                val bytes = context.contentResolver.openInputStream(uri)?.use { input ->
                    val output = java.io.ByteArrayOutputStream()
                    val buffer = ByteArray(16 * 1024)
                    var total = 0
                    while (true) {
                        val count = input.read(buffer)
                        if (count < 0) break
                        total += count
                        require(total <= 8 * 1024 * 1024) { "Files larger than 8 MiB are not supported yet." }
                        output.write(buffer, 0, count)
                    }
                    output.toByteArray()
                } ?: error("Android could not open this file.")
                val mime = context.contentResolver.getType(uri).orEmpty().ifBlank { "application/octet-stream" }
                val name = context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
                    if (cursor.moveToFirst()) cursor.getString(0) else null
                } ?: "imported-file"
                store.saveArtifact(projectId, name, mime, bytes)
            }.onSuccess {
                status = "File imported and encrypted in this project."
                refresh()
            }.onFailure { status = it.message ?: "Could not import file." }
        }
    }

    Dialog(onDismissRequest = onDismiss) {
        Card(
            modifier = Modifier.fillMaxWidth().widthIn(max = 760.dp).heightIn(max = 760.dp),
            colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
            shape = MaterialTheme.shapes.extraLarge
        ) {
            Column(
                Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(20.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp)
            ) {
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                    Column(Modifier.weight(1f)) {
                        Text("Your Work", style = MaterialTheme.typography.headlineSmall)
                        Text("Projects and private files, encrypted on this device", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    TextButton(onClick = onDismiss) { Text("Close") }
                }
                if (selected == null) {
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                        Text("Projects", style = MaterialTheme.typography.titleMedium)
                        Button(onClick = { projectName = ""; projectDescription = ""; showCreate = true }) { Text("New project") }
                    }
                    if (projects.isEmpty()) Text("No projects yet. Create one to organize notes and working files.", color = MaterialTheme.colorScheme.onSurfaceVariant)
                    projects.forEach { project ->
                        Card(
                            modifier = Modifier.fillMaxWidth().clickable { selectedId = project.id; preview = ""; noteBody = "" },
                            colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow)
                        ) {
                            Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                                    Text(project.name, style = MaterialTheme.typography.titleMedium)
                                    Text(if (project.archived) "Archived" else "Active", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.primary)
                                }
                                if (project.description.isNotBlank()) Text(project.description, style = MaterialTheme.typography.bodySmall)
                                Text(project.artifacts.size.toString() + " files · Updated " + java.text.DateFormat.getDateTimeInstance(java.text.DateFormat.SHORT, java.text.DateFormat.SHORT).format(java.util.Date(project.updatedAt)), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            }
                        }
                    }
                } else {
                    TextButton(onClick = { selectedId = null; preview = ""; status = "" }) { Text("← All projects") }
                    Text(selected.name, style = MaterialTheme.typography.titleLarge)
                    if (selected.description.isNotBlank()) Text(selected.description, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        OutlinedButton(onClick = {
                            runCatching { store.setArchived(selected.id, !selected.archived) }
                                .onSuccess { status = if (it.archived) "Project archived." else "Project restored." }
                                .onFailure { status = it.message ?: "Could not update project." }
                            refresh()
                        }) { Text(if (selected.archived) "Restore" else "Archive") }
                        OutlinedButton(onClick = { showDelete = true }) { Text("Delete project") }
                    }
                    Text("Project files", style = MaterialTheme.typography.titleMedium)
                    OutlinedButton(
                        enabled = !selected.archived,
                        onClick = { importLauncher.launch(arrayOf("*/*")) },
                        modifier = Modifier.fillMaxWidth()
                    ) { Text("Import file · 8 MiB maximum") }
                    if (selected.artifacts.isEmpty()) Text("No files saved in this project yet.", color = MaterialTheme.colorScheme.onSurfaceVariant)
                    selected.artifacts.forEach { artifact ->
                        Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow)) {
                            Column(Modifier.fillMaxWidth().padding(12.dp), verticalArrangement = Arrangement.spacedBy(5.dp)) {
                                Text(artifact.name, style = MaterialTheme.typography.titleSmall)
                                Text(artifact.mimeType + " · " + artifact.byteCount + " bytes · SHA-256 verified on open", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                    TextButton(onClick = {
                                        runCatching { store.readArtifact(selected.id, artifact.id) }
                                            .onSuccess { bytes ->
                                                preview = if (artifact.mimeType.startsWith("text/") || artifact.mimeType.contains("json") || artifact.mimeType.contains("xml")) {
                                                    bytes.toString(Charsets.UTF_8).take(24_000)
                                                } else "This file is stored securely. Preview for " + artifact.mimeType + " is not available yet."
                                                status = "File opened and integrity verified."
                                            }
                                            .onFailure { status = it.message ?: "Could not open file." }
                                    }) { Text("Open") }
                                    TextButton(onClick = {
                                        runCatching { store.readArtifact(selected.id, artifact.id) }
                                            .onSuccess { bytes -> pendingExportBytes = bytes; exportLauncher.launch(artifact.name) }
                                            .onFailure { status = it.message ?: "Could not prepare file export." }
                                    }) { Text("Export") }
                                    TextButton(onClick = {
                                        runCatching { store.deleteArtifact(selected.id, artifact.id) }
                                            .onSuccess { status = "File deleted."; refresh() }
                                            .onFailure { status = it.message ?: "Could not delete file." }
                                    }) { Text("Delete") }
                                }
                            }
                        }
                    }
                    Text("Save a note", style = MaterialTheme.typography.titleMedium)
                    OutlinedTextField(noteName, { noteName = it.take(160) }, label = { Text("File name") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                    OutlinedTextField(noteBody, { noteBody = it.take(500_000) }, label = { Text("Note content") }, minLines = 4, modifier = Modifier.fillMaxWidth())
                    Button(
                        enabled = !selected.archived && noteName.isNotBlank() && noteBody.isNotBlank(),
                        onClick = {
                            runCatching {
                                store.saveArtifact(selected.id, noteName.trim(), "text/plain; charset=utf-8", noteBody.toByteArray(Charsets.UTF_8))
                            }.onSuccess {
                                noteName = ""; noteBody = ""; status = "Note saved in encrypted project storage."; refresh()
                            }.onFailure { status = it.message ?: "Could not save note." }
                        },
                        modifier = Modifier.fillMaxWidth()
                    ) { Text("Save note securely") }
                    if (preview.isNotBlank()) {
                        Text("File preview", style = MaterialTheme.typography.titleMedium)
                        Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow)) {
                            Text(preview, Modifier.fillMaxWidth().padding(12.dp), style = MaterialTheme.typography.bodySmall)
                        }
                    }
                }
                if (status.isNotBlank()) Text(status, color = MaterialTheme.colorScheme.primary, style = MaterialTheme.typography.bodySmall)
                if (exportStatus.isNotBlank()) Text(exportStatus, color = MaterialTheme.colorScheme.primary, style = MaterialTheme.typography.bodySmall)
            }
        }
    }

    if (showCreate) {
        AlertDialog(
            onDismissRequest = { showCreate = false },
            title = { Text("Create project") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedTextField(projectName, { projectName = it.take(100) }, label = { Text("Project name") }, singleLine = true)
                    OutlinedTextField(projectDescription, { projectDescription = it.take(2_000) }, label = { Text("Description") }, minLines = 2)
                }
            },
            confirmButton = {
                Button(enabled = projectName.isNotBlank(), onClick = {
                    runCatching { store.create(projectName, projectDescription) }
                        .onSuccess { created -> refresh(); selectedId = created.id; showCreate = false; status = "Project created." }
                        .onFailure { status = it.message ?: "Could not create project." }
                }) { Text("Create") }
            },
            dismissButton = { TextButton(onClick = { showCreate = false }) { Text("Cancel") } }
        )
    }

    if (showDelete && selected != null) {
        AlertDialog(
            onDismissRequest = { showDelete = false },
            title = { Text("Delete " + selected.name + "?") },
            text = { Text("This permanently deletes the project and its encrypted files from this device.") },
            confirmButton = {
                Button(onClick = {
                    runCatching { store.delete(selected.id) }
                        .onSuccess { selectedId = null; preview = ""; status = "Project deleted."; refresh() }
                        .onFailure { status = it.message ?: "Could not delete project." }
                    showDelete = false
                }) { Text("Delete") }
            },
            dismissButton = { TextButton(onClick = { showDelete = false }) { Text("Cancel") } }
        )
    }
}
