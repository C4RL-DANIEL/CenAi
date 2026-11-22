package com.cenai.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class AccountIsolationInstrumentedTest {
    @Test
    fun conversationHistoryIsIsolatedBetweenAccountsAndGuestMode() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val first = ChatStore(context, "account-alpha")
        val second = ChatStore(context, "account-beta")
        val guest = ChatStore(context, "guest")

        first.save(emptyList())
        second.save(emptyList())
        guest.save(emptyList())

        first.save(listOf(Conversation(
            id = "alpha-chat",
            title = "Alpha private chat",
            messages = mutableListOf(ChatMessage("user", "Alpha context only"))
        )))
        second.save(listOf(Conversation(
            id = "beta-chat",
            title = "Beta private chat",
            messages = mutableListOf(ChatMessage("user", "Beta context only"))
        )))

        val storedAlpha = context.getSharedPreferences("cenai_history_account-alpha", android.content.Context.MODE_PRIVATE).getString("conversations", "").orEmpty()
        assertTrue(storedAlpha.isNotBlank() && !storedAlpha.trimStart().startsWith("["))
        assertEquals(listOf("alpha-chat"), first.load().map { it.id })
        assertEquals(listOf("beta-chat"), second.load().map { it.id })
        assertTrue(guest.load().isEmpty())

        first.save(emptyList())
        second.save(emptyList())
        guest.save(emptyList())
    }

    @Test
    fun encryptedProviderKeysAndAgentMemoryAreScopedToTheirAccount() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val vault = SecureKeyStore(context)

        vault.putUserSecret("account-alpha", "agent_memory", "Alpha memory")
        vault.putUserSecret("account-beta", "agent_memory", "Beta memory")
        vault.putUserSecret("account-alpha", "custom_headers", """{"x-account":"alpha"}""")
        vault.putUserSecret("account-beta", "custom_headers", """{"x-account":"beta"}""")
        vault.putUser("account-alpha", "usage_count", "5")
        vault.putUser("account-beta", "usage_count", "2")
        vault.putUser("account-alpha", "cenai_email_verified", "true")
        vault.putUser("account-beta", "cenai_email_verified", "false")
        vault.saveApiKey("alpha-provider-key", Provider.OPENAI, "account-alpha")
        vault.saveApiKey("beta-provider-key", Provider.OPENAI, "account-beta")

        assertEquals("Alpha memory", vault.getUserSecret("account-alpha", "agent_memory"))
        assertEquals("Beta memory", vault.getUserSecret("account-beta", "agent_memory"))
        assertEquals("""{"x-account":"alpha"}""", vault.getUserSecret("account-alpha", "custom_headers"))
        assertEquals("""{"x-account":"beta"}""", vault.getUserSecret("account-beta", "custom_headers"))
        assertEquals("5", vault.getUser("account-alpha", "usage_count"))
        assertEquals("2", vault.getUser("account-beta", "usage_count"))
        assertEquals("true", vault.getUser("account-alpha", "cenai_email_verified"))
        assertEquals("false", vault.getUser("account-beta", "cenai_email_verified"))
        assertEquals("", vault.getUserSecret("guest", "agent_memory"))
        assertEquals("alpha-provider-key", vault.apiKey(Provider.OPENAI, "account-alpha"))
        assertEquals("beta-provider-key", vault.apiKey(Provider.OPENAI, "account-beta"))
        assertEquals("", vault.apiKey(Provider.OPENAI, "guest"))

        vault.putUserSecret("account-alpha", "agent_memory", "")
        vault.putUserSecret("account-beta", "agent_memory", "")
        vault.putUserSecret("account-alpha", "custom_headers", "")
        vault.putUserSecret("account-beta", "custom_headers", "")
        vault.saveApiKey("", Provider.OPENAI, "account-alpha")
        vault.saveApiKey("", Provider.OPENAI, "account-beta")
    }
}
