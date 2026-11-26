package com.cenai.app

import android.content.Context
import org.json.JSONArray
import org.json.JSONObject
import java.security.MessageDigest
import java.util.UUID

data class CenAiProject(
    val id: String,
    val name: String,
    val description: String,
    val createdAt: Long,
    val updatedAt: Long,
    val archived: Boolean,
    val artifacts: List<CenAiArtifact>
)

data class CenAiArtifact(
    val id: String,
    val name: String,
    val mimeType: String,
    val byteCount: Int,
    val sha256: String,
    val createdAt: Long
)

/**
 * Local-first Projects and Knowledge Vault storage.
 * Project metadata and every artifact are encrypted in the immutable account workspace.
 */
class ProjectWorkspaceStore(context: Context, vault: SecureKeyStore) {
    private val workspace = AccountWorkspaceStore(context.applicationContext, resolveAccountId(context, vault))
    private val lock = Any()

    fun list(includeArchived: Boolean = false): List<CenAiProject> = synchronized(lock) {
        readIndex().filter { includeArchived || !it.archived }.sortedByDescending { it.updatedAt }
    }

    fun create(name: String, description: String = ""): CenAiProject = synchronized(lock) {
        val cleanName = name.trim()
        require(cleanName.isNotEmpty() && cleanName.length <= 100) { "Project name must be 1–100 characters." }
        require(description.length <= 2_000) { "Project description is too long." }
        val now = System.currentTimeMillis()
        val project = CenAiProject(UUID.randomUUID().toString(), cleanName, description.trim(), now, now, false, emptyList())
        writeIndex(readIndex() + project)
        project
    }

    fun update(projectId: String, name: String, description: String): CenAiProject = synchronized(lock) {
        val cleanName = name.trim()
        require(cleanName.isNotEmpty() && cleanName.length <= 100) { "Project name must be 1–100 characters." }
        require(description.length <= 2_000) { "Project description is too long." }
        val projects = readIndex().toMutableList()
        val index = projects.indexOfFirst { it.id == projectId }
        require(index >= 0) { "Project not found." }
        val updated = projects[index].copy(name = cleanName, description = description.trim(), updatedAt = System.currentTimeMillis())
        projects[index] = updated
        writeIndex(projects)
        updated
    }

    fun setArchived(projectId: String, archived: Boolean): CenAiProject = synchronized(lock) {
        val projects = readIndex().toMutableList()
        val index = projects.indexOfFirst { it.id == projectId }
        require(index >= 0) { "Project not found." }
        val updated = projects[index].copy(archived = archived, updatedAt = System.currentTimeMillis())
        projects[index] = updated
        writeIndex(projects)
        updated
    }

    fun delete(projectId: String) = synchronized(lock) {
        val projects = readIndex()
        val project = projects.firstOrNull { it.id == projectId } ?: return@synchronized
        project.artifacts.forEach { artifact ->
            workspace.delete(artifactPath(projectId, artifact.id))
        }
        writeIndex(projects.filterNot { it.id == projectId })
    }

    fun saveArtifact(
        projectId: String,
        name: String,
        mimeType: String,
        bytes: ByteArray
    ): CenAiArtifact = synchronized(lock) {
        require(name.trim().isNotEmpty() && name.length <= 160) { "Artifact name must be 1–160 characters." }
        require(mimeType.length in 1..120 && mimeType.none { it.isISOControl() }) { "Artifact MIME type is invalid." }
        require(bytes.isNotEmpty() && bytes.size <= MAX_ARTIFACT_BYTES) { "Artifacts must be between 1 byte and 8 MiB." }
        val projects = readIndex().toMutableList()
        val index = projects.indexOfFirst { it.id == projectId }
        require(index >= 0) { "Project not found." }
        val project = projects[index]
        require(project.artifacts.size < MAX_ARTIFACTS) { "This project has reached its artifact limit." }
        val artifact = CenAiArtifact(
            UUID.randomUUID().toString(), name.trim(), mimeType, bytes.size,
            sha256(bytes), System.currentTimeMillis()
        )
        val path = artifactPath(projectId, artifact.id)
        workspace.write(path, bytes)
        try {
            projects[index] = project.copy(
                updatedAt = System.currentTimeMillis(),
                artifacts = project.artifacts + artifact
            )
            writeIndex(projects)
        } catch (error: Exception) {
            workspace.delete(path)
            throw error
        }
        artifact
    }

    fun readArtifact(projectId: String, artifactId: String): ByteArray = synchronized(lock) {
        val project = readIndex().firstOrNull { it.id == projectId } ?: error("Project not found.")
        val artifact = project.artifacts.firstOrNull { it.id == artifactId } ?: error("Artifact not found.")
        val bytes = workspace.read(artifactPath(projectId, artifactId)) ?: error("Artifact data is missing.")
        require(bytes.size == artifact.byteCount && sha256(bytes) == artifact.sha256) {
            "Artifact integrity check failed. The file may be damaged."
        }
        bytes
    }

    fun deleteArtifact(projectId: String, artifactId: String) = synchronized(lock) {
        val projects = readIndex().toMutableList()
        val index = projects.indexOfFirst { it.id == projectId }
        require(index >= 0) { "Project not found." }
        val project = projects[index]
        require(project.artifacts.any { it.id == artifactId }) { "Artifact not found." }
        workspace.delete(artifactPath(projectId, artifactId))
        projects[index] = project.copy(
            updatedAt = System.currentTimeMillis(),
            artifacts = project.artifacts.filterNot { it.id == artifactId }
        )
        writeIndex(projects)
    }

    private fun readIndex(): List<CenAiProject> {
        val raw = workspace.readText(INDEX_PATH) ?: return emptyList()
        val array = JSONArray(raw)
        require(array.length() <= MAX_PROJECTS) { "Project index is too large." }
        return List(array.length()) { index ->
            val item = array.getJSONObject(index)
            val id = item.getString("id").also(::requireUuid)
            val artifactArray = item.optJSONArray("artifacts") ?: JSONArray()
            require(artifactArray.length() <= MAX_ARTIFACTS) { "Project has too many artifacts." }
            val artifacts = List(artifactArray.length()) { artifactIndex ->
                val artifact = artifactArray.getJSONObject(artifactIndex)
                CenAiArtifact(
                    artifact.getString("id").also(::requireUuid),
                    artifact.getString("name").take(160),
                    artifact.getString("mimeType").take(120),
                    artifact.getInt("byteCount"),
                    artifact.getString("sha256").also { require(it.matches(Regex("[a-f0-9]{64}"))) },
                    artifact.getLong("createdAt")
                )
            }
            CenAiProject(
                id, item.getString("name").take(100), item.optString("description").take(2_000),
                item.getLong("createdAt"), item.getLong("updatedAt"), item.optBoolean("archived"), artifacts
            )
        }
    }

    private fun writeIndex(projects: List<CenAiProject>) {
        require(projects.size <= MAX_PROJECTS) { "You have reached the project limit." }
        val array = JSONArray()
        projects.forEach { project ->
            val artifacts = JSONArray()
            project.artifacts.forEach { artifact ->
                artifacts.put(JSONObject().put("id", artifact.id).put("name", artifact.name)
                    .put("mimeType", artifact.mimeType).put("byteCount", artifact.byteCount)
                    .put("sha256", artifact.sha256).put("createdAt", artifact.createdAt))
            }
            array.put(JSONObject().put("id", project.id).put("name", project.name)
                .put("description", project.description).put("createdAt", project.createdAt)
                .put("updatedAt", project.updatedAt).put("archived", project.archived)
                .put("artifacts", artifacts))
        }
        workspace.writeText(INDEX_PATH, array.toString())
    }

    private fun artifactPath(projectId: String, artifactId: String): String {
        requireUuid(projectId); requireUuid(artifactId)
        return "projects/$projectId/artifacts/$artifactId.bin"
    }

    private fun requireUuid(value: String) {
        require(runCatching { UUID.fromString(value).toString() == value.lowercase() }.getOrDefault(false)) {
            "Project or artifact identifier is invalid."
        }
    }

    private fun sha256(bytes: ByteArray): String =
        MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }

    private fun resolveAccountId(context: Context, vault: SecureKeyStore): String {
        val authenticated = vault.get("supabase_user_id").trim().lowercase()
        if (runCatching { UUID.fromString(authenticated).toString() == authenticated }.getOrDefault(false)) return authenticated
        val prefs = context.getSharedPreferences("cenai_local_identity", Context.MODE_PRIVATE)
        val existing = prefs.getString("workspace_id", null)
        if (existing != null && runCatching { UUID.fromString(existing).toString() == existing.lowercase() }.getOrDefault(false)) return existing
        val created = UUID.randomUUID().toString()
        check(prefs.edit().putString("workspace_id", created).commit()) { "Could not create a local workspace identity." }
        return created
    }

    companion object {
        private const val INDEX_PATH = "projects/index.json"
        private const val MAX_PROJECTS = 100
        private const val MAX_ARTIFACTS = 100
        private const val MAX_ARTIFACT_BYTES = 8 * 1024 * 1024
    }
}
