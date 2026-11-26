package com.cenai.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.util.UUID

@RunWith(AndroidJUnit4::class)
class AccountWorkspaceStoreInstrumentedTest {
    private val context get() = InstrumentationRegistry.getInstrumentation().targetContext

    @Test
    fun encryptedWorkspaceRoundTripsAndDoesNotWritePlaintext() {
        val account = UUID.randomUUID().toString()
        val store = AccountWorkspaceStore(context, account)
        val secret = "private conversation body " + UUID.randomUUID()
        store.writeText("conversations.json", secret)
        assertEquals(secret, store.readText("conversations.json"))
        val file = File(context.noBackupFilesDir, "cenai_workspaces/$account/conversations.json")
        val raw = file.readBytes().toString(Charsets.ISO_8859_1)
        assertEquals(false, raw.contains(secret))
        assertEquals(true, raw.startsWith("CENAIWS1"))
        store.delete("conversations.json")
    }

    @Test
    fun ciphertextCannotBeReadByAnotherAccount() {
        val first = AccountWorkspaceStore(context, UUID.randomUUID().toString())
        val second = AccountWorkspaceStore(context, UUID.randomUUID().toString())
        first.writeText("projects/one.json", """{"private":true}""")
        assertNull(second.readText("projects/one.json"))
    }

    @Test
    fun authenticatedPathPreventsCiphertextRelocation() {
        val account = UUID.randomUUID().toString()
        val store = AccountWorkspaceStore(context, account)
        store.writeText("projects/one.json", "bound to one path")
        assertThrows(Exception::class.java) { store.readText("projects/two.json") }
        store.delete("projects/one.json")
    }

    @Test
    fun rejectsTraversalAndNonUuidAccounts() {
        assertThrows(IllegalArgumentException::class.java) {
            AccountWorkspaceStore(context, "../other-account")
        }
        val store = AccountWorkspaceStore(context, UUID.randomUUID().toString())
        assertThrows(IllegalArgumentException::class.java) { store.writeText("../escape.txt", "no") }
    }
}
