package com.cenai.app

import org.junit.Assert.assertThrows
import org.junit.Test

class CenAiSandboxClientTest {
    @Test
    fun acceptsBoundedProjectScopedCommands() {
        CenAiSandboxClient.validateRequest("cargo", listOf("test", "-p", "cenai-native"), "project_1", "native")
        CenAiSandboxClient.validateRequest("pwd", emptyList(), "default", ".")
    }

    @Test
    fun rejectsUnlistedCommandsAndPathTraversal() {
        assertThrows(IllegalArgumentException::class.java) {
            CenAiSandboxClient.validateRequest("sudo", emptyList(), "default", ".")
        }
        assertThrows(IllegalArgumentException::class.java) {
            CenAiSandboxClient.validateRequest("pwd", emptyList(), "default", "../../etc")
        }
        assertThrows(IllegalArgumentException::class.java) {
            CenAiSandboxClient.validateRequest("pwd", emptyList(), "default", "/tmp")
        }
    }

    @Test
    fun rejectsCredentialLikeArgumentsAndOversizedRequests() {
        assertThrows(IllegalArgumentException::class.java) {
            CenAiSandboxClient.validateRequest("bash", listOf("--api-key=secret-value"), "default", ".")
        }
        assertThrows(IllegalArgumentException::class.java) {
            CenAiSandboxClient.validateRequest("bash", listOf("x".repeat(4_097)), "default", ".")
        }
        assertThrows(IllegalArgumentException::class.java) {
            CenAiSandboxClient.validateRequest("bash", emptyList(), "../outside", ".")
        }
    }
}
