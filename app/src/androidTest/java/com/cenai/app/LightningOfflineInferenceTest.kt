package com.cenai.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class LightningOfflineInferenceTest {
    @Test
    fun bundledLightningModelLoadsAndGeneratesWithoutNetwork() = runBlocking {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val runtime = LocalModelRuntime(context)
        try {
            println("CenAI offline inference: installing the bundled Lightning model")
            assertTrue("The APK must contain the bundled Lightning model.", runtime.installBundledModelIfMissing())
            assertEquals(613_722_264L, runtime.modelSizeBytes())
            println("CenAI offline inference: model installed; initializing CPU engine and generating")

            val response = withTimeout(600_000L) {
                runtime.stream(
                    listOf(
                        ChatMessage("system", "You are CenAI, a concise private on-device assistant. Reply briefly."),
                        ChatMessage("user", "Reply with one short greeting.")
                    )
                ) { /* The smoke test validates local generation; no network callback is installed. */ }
            }
            println("CenAI offline inference: generation completed with ${response.length} characters")
            assertTrue("Offline Lightning inference returned an empty response.", response.isNotBlank())
        } finally {
            runtime.release()
        }
    }
}
