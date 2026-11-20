package com.cenai.app

import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class CenAiIdentityIsolationTest {
    @Test
    fun differentAccountsReceiveDifferentStorageNamespaces() {
        val alice = BrainContextScope("account:alice_00000001", "chat")
        val bob = BrainContextScope("account:bob__00000002", "chat")

        assertNotEquals(alice.storageNamespace, bob.storageNamespace)
        assertNotEquals(
            ScopedKeyNames.preferenceKey(alice, "agent_memory"),
            ScopedKeyNames.preferenceKey(bob, "agent_memory")
        )
    }

    @Test
    fun featuresWithinOneAccountHaveIndependentNamespaces() {
        val chat = BrainContextScope("account:alice_00000001", "chat")
        val automation = BrainContextScope("account:alice_00000001", "automation")
        val documents = BrainContextScope("account:alice_00000001", "documents")

        assertNotEquals(chat.storageNamespace, automation.storageNamespace)
        assertNotEquals(chat.storageNamespace, documents.storageNamespace)
        assertNotEquals(automation.storageNamespace, documents.storageNamespace)
    }

    @Test
    fun storageKeysDoNotExposeRawAccountOrFeatureNames() {
        val scope = BrainContextScope("account:alice_00000001", "chat")
        val key = ScopedKeyNames.preferenceKey(scope, "agent_memory")

        assertFalse(key.contains("alice"))
        assertTrue(key.endsWith("agent_memory"))
    }

    @Test(expected = IllegalArgumentException::class)
    fun rejectsUnownedOrMalformedContextScope() {
        BrainContextScope("shared", "chat")
    }

    @Test(expected = IllegalArgumentException::class)
    fun rejectsInvalidFeatureNamespace() {
        BrainContextScope("account:alice_00000001", "../other-user")
    }
}
