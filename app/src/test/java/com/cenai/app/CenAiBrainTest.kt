package com.cenai.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class CenAiBrainTest {
    @Test
    fun contextComposerCombinesOnlyProvidedSharedContext() {
        val result = BrainContextComposer.compose(
            systemPrompt = "You are CenAI.",
            approvedMemory = "Prefers concise answers.",
            localKnowledgeContext = "A saved note about the project.",
            toolGuide = "Use only approved tools."
        )

        assertEquals(1, result.size)
        assertEquals("system", result.single().role)
        assertTrue(result.single().content.contains("You are CenAI."))
        assertTrue(result.single().content.contains("User-approved persistent memory"))
        assertTrue(result.single().content.contains("A saved note about the project."))
        assertTrue(result.single().content.contains("Use only approved tools."))
    }

    @Test
    fun contextComposerDoesNotCreateAnEmptySystemMessage() {
        assertTrue(BrainContextComposer.compose("", "", "", null).isEmpty())
    }

    @Test
    fun capabilityRegistryReflectsRuntimeReadiness() {
        val capabilities = BrainCapabilityRegistry().snapshot(
            BrainRuntimeState(
                providerConfigured = false,
                localModelInstalled = true,
                localModelEnabled = true,
                localKnowledgeEnabled = true,
                agentPlanningEnabled = false
            )
        ).associateBy { it.id }

        assertEquals(BrainCapabilityState.READY, capabilities.getValue("model_router").state)
        assertEquals(BrainCapabilityState.READY, capabilities.getValue("local_inference").state)
        assertEquals(BrainCapabilityState.READY, capabilities.getValue("conversation_memory").state)
        assertEquals(BrainCapabilityState.DISABLED, capabilities.getValue("agent_tools").state)
        assertEquals(BrainCapabilityState.READY, capabilities.getValue("guardian").state)
    }

    @Test
    fun unconfiguredModelRouteReportsSetupRequired() {
        val capabilities = BrainCapabilityRegistry().snapshot(
            BrainRuntimeState(
                providerConfigured = false,
                localModelInstalled = false,
                localModelEnabled = false,
                localKnowledgeEnabled = false,
                agentPlanningEnabled = false
            )
        ).associateBy { it.id }

        assertEquals(BrainCapabilityState.NEEDS_SETUP, capabilities.getValue("model_router").state)
        assertEquals(BrainCapabilityState.NEEDS_SETUP, capabilities.getValue("local_inference").state)
    }
}
