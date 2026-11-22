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
        LightningRuntimeBridge.release()
    }
}
