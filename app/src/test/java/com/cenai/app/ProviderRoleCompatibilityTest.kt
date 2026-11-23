package com.cenai.app

import org.junit.Assert.assertEquals
import org.junit.Test

class ProviderRoleCompatibilityTest {
    @Test
    fun toolResultsBecomeUserMessagesForChatCompletionsProviders() {
        listOf(Provider.OPENAI, Provider.DEEPSEEK, Provider.CUSTOM, Provider.CLAUDE).forEach { provider ->
            assertEquals("user", providerCompatibleRole(provider, "tool"))
        }
    }

    @Test
    fun ordinaryRolesRemainUnchanged() {
        listOf(Provider.OPENAI, Provider.DEEPSEEK, Provider.CUSTOM, Provider.CLAUDE).forEach { provider ->
            assertEquals("user", providerCompatibleRole(provider, "user"))
            assertEquals("assistant", providerCompatibleRole(provider, "assistant"))
            assertEquals("system", providerCompatibleRole(provider, "system"))
        }
    }
}
