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
        vault.putUserSecret("account-alpha", "supabase_access_token", "alpha-session-token")
        vault.putUserSecret("account-beta", "supabase_access_token", "beta-session-token")
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
        assertEquals("alpha-session-token", vault.getUserSecret("account-alpha", "supabase_access_token"))
        assertEquals("beta-session-token", vault.getUserSecret("account-beta", "supabase_access_token"))
        assertEquals("", vault.getUserSecret("guest", "supabase_access_token"))
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
        vault.putUserSecret("account-alpha", "supabase_access_token", "")
        vault.putUserSecret("account-beta", "supabase_access_token", "")
        vault.saveApiKey("", Provider.OPENAI, "account-alpha")
        vault.saveApiKey("", Provider.OPENAI, "account-beta")
    }

    @Test
    fun legacyGlobalDataMigratesOnlyToItsMatchingSignedInAccount() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val global = context.getSharedPreferences("cenai_private", android.content.Context.MODE_PRIVATE)
        val account = context.getSharedPreferences("cenai_private_legacy-owner", android.content.Context.MODE_PRIVATE)
        val other = context.getSharedPreferences("cenai_private_other-owner", android.content.Context.MODE_PRIVATE)
        global.edit().clear().apply()
        account.edit().clear().apply()
        other.edit().clear().apply()

        val vault = SecureKeyStore(context)
        vault.put("supabase_user_id", "legacy-owner")
        vault.put("provider", "OPENAI")
        vault.put("custom_headers", """{"x-legacy":"private"}""")
        vault.putSecret("agent_memory_legacy-owner", "Legacy private memory")
        vault.putSecret("api_key", "legacy-api-key")
        vault.migrateLegacyAccountData("legacy-owner")

        assertEquals("OPENAI", vault.getUser("legacy-owner", "provider"))
        assertEquals("""{"x-legacy":"private"}""", vault.getUserSecret("legacy-owner", "custom_headers"))
        assertEquals("Legacy private memory", vault.getUserSecret("legacy-owner", "agent_memory"))
        assertEquals("legacy-api-key", vault.apiKey(Provider.OPENAI, "legacy-owner"))
        assertEquals("", vault.apiKey(Provider.OPENAI, "guest"))

        vault.migrateLegacyAccountData("other-owner")
        assertEquals("", vault.getUserSecret("other-owner", "agent_memory"))
        assertEquals("", vault.apiKey(Provider.OPENAI, "other-owner"))

        global.edit().clear().apply()
        account.edit().clear().apply()
        other.edit().clear().apply()
    }


    @Test
    fun legacyConversationHistoryMigratesEncryptedToMatchingAccount() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val vault = SecureKeyStore(context)
        val global = context.getSharedPreferences("cenai_private", android.content.Context.MODE_PRIVATE)
        val legacyHistory = context.getSharedPreferences("cenai_history", android.content.Context.MODE_PRIVATE)
        val scopedHistory = context.getSharedPreferences("cenai_history_legacy-owner", android.content.Context.MODE_PRIVATE)
        global.edit().clear().apply()
        legacyHistory.edit().clear().apply()
        scopedHistory.edit().clear().apply()
        vault.put("supabase_user_id", "legacy-owner")
        legacyHistory.edit().putString(
            "conversations",
            """[{"id":"legacy-chat","title":"Legacy chat","updatedAt":1,"messages":[{"role":"user","content":"Old private history"}]}]"""
        ).apply()

        val migrated = ChatStore(context, "legacy-owner").load()
        assertEquals(listOf("legacy-chat"), migrated.map { it.id })
        val encrypted = scopedHistory.getString("conversations", "").orEmpty()
        assertTrue(encrypted.isNotBlank() && !encrypted.trimStart().startsWith("["))
        assertTrue(legacyHistory.getString("conversations", null) == null)

        global.edit().clear().apply()
        legacyHistory.edit().clear().apply()
        scopedHistory.edit().clear().apply()
    }

}
