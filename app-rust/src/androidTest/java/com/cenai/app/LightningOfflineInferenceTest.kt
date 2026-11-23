package com.cenai.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class LightningOfflineInferenceTest {
    @Test
    fun bundledModelInstallsVerifiesAndGeneratesLocally() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        assertTrue("APK must contain the pinned Lightning model.", LightningRuntimeBridge.installBundledModel(context))
        assertTrue("Lightning must verify its private model copy.", LightningRuntimeBridge.isReady(context))
        val response = LightningRuntimeBridge.generate(context, "Reply with one short greeting.")
        assertTrue("Offline Lightning inference returned an empty response.", response.isNotBlank())
        assertTrue("Offline Lightning response exceeded its memory-safety bound.", response.length <= LightningRuntimeBridge.MAX_RESPONSE_CHARS)
        // Exercise a second request in the same process, then recreate the
        // engine after release to catch stale native handles and lifecycle leaks.
        val second = LightningRuntimeBridge.generate(context, "Count from one to three.")
        assertTrue("Repeated offline inference returned an empty response.", second.isNotBlank())
        assertTrue("Repeated offline response exceeded its memory-safety bound.", second.length <= LightningRuntimeBridge.MAX_RESPONSE_CHARS)
        LightningRuntimeBridge.release()
        LightningRuntimeBridge.release()
        val afterRelease = LightningRuntimeBridge.generate(context, "Reply with the word ready.")
        assertTrue("Lightning could not recreate its engine after release.", afterRelease.isNotBlank())
        LightningRuntimeBridge.release()
    }
}
