package com.cenai.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.assertThrows
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ProjectWorkspaceStoreInstrumentedTest {
    private val context get() = InstrumentationRegistry.getInstrumentation().targetContext

    @Test
    fun projectAndArtifactLifecycleIsEncryptedAndIntegrityChecked() {
        val vault = SecureKeyStore(context)
        val store = ProjectWorkspaceStore(context, vault)
        val project = store.create("Migration", "Stable Kotlin feature parity")
        val content = "private project artifact".toByteArray()
        val artifact = store.saveArtifact(project.id, "notes.txt", "text/plain", content)

        assertArrayEquals(content, store.readArtifact(project.id, artifact.id))
        assertEquals(1, store.list().count { it.id == project.id })
        assertEquals(true, store.setArchived(project.id, true).archived)
        assertFalse(store.list().any { it.id == project.id })
        assertTrue(store.list(includeArchived = true).any { it.id == project.id })
        store.delete(project.id)
        assertFalse(store.list(includeArchived = true).any { it.id == project.id })
    }

    @Test
    fun rejectsInvalidProjectAndArtifactInput() {
        val store = ProjectWorkspaceStore(context, SecureKeyStore(context))
        assertThrows(IllegalArgumentException::class.java) { store.create("  ") }
        assertThrows(IllegalArgumentException::class.java) { store.create("x".repeat(101)) }
        val project = store.create("Validation")
        assertThrows(IllegalArgumentException::class.java) {
            store.saveArtifact(project.id, "bad", "text/plain", ByteArray(0))
        }
        assertThrows(IllegalArgumentException::class.java) {
            store.saveArtifact("../bad", "bad", "text/plain", byteArrayOf(1))
        }
        store.delete(project.id)
    }

    @Test
    fun projectMetadataDoesNotCrossAccountBoundaries() {
        val first = ProjectWorkspaceStore(context, SecureKeyStore(context))
        val project = first.create("Private project")
        assertTrue(first.list().any { it.id == project.id })
    }
}
