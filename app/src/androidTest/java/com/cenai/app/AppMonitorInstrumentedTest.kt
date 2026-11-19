package com.cenai.app

import android.content.Context
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.io.IOException

@RunWith(AndroidJUnit4::class)
class AppMonitorInstrumentedTest {
    private lateinit var monitor: AppMonitor

    @Before
    fun setUp() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        monitor = AppMonitor(context)
        monitor.reset()
    }

    @After
    fun tearDown() {
        monitor.reset()
    }

    @Test
    fun recordsRequestLifecycleAndComputesSuccessRate() {
        monitor.recordStarted()
        monitor.recordStarted()
        monitor.recordSucceeded(120)
        monitor.recordFailed(80, IOException("private prompt text must not be retained"))

        val snapshot = monitor.snapshot()
        assertEquals(2, snapshot.started)
        assertEquals(1, snapshot.succeeded)
        assertEquals(1, snapshot.failed)
        assertEquals(50, snapshot.successRate)
        assertEquals(100L, snapshot.averageLatencyMs)
        assertEquals("IOException", snapshot.lastFailureCategory)
        assertFalse(snapshot.lastFailureCategory.contains("private prompt"))
    }

    @Test
    fun recordsHttpFailureAsStatusOnly() {
        monitor.recordFailed(25, IOException("Request failed: HTTP 429 secret-token"))

        val snapshot = monitor.snapshot()
        assertEquals("HTTP 429", snapshot.lastFailureCategory)
        assertFalse(snapshot.lastFailureCategory.contains("secret-token"))
    }

    @Test
    fun recordsFeatureEventsAndIgnoresUnknownEvents() {
        monitor.recordEvent("app_start")
        monitor.recordEvent("voice_input")
        monitor.recordEvent("voice_output")
        monitor.recordEvent("document_attached")
        monitor.recordEvent("update_check")
        monitor.recordEvent("provider_test")
        monitor.recordEvent("unknown_sensitive_event")

        val snapshot = monitor.snapshot()
        assertEquals(1, snapshot.appStarts)
        assertEquals(1, snapshot.voiceInputs)
        assertEquals(1, snapshot.voiceOutputs)
        assertEquals(1, snapshot.documentsAttached)
        assertEquals(1, snapshot.updateChecks)
        assertEquals(1, snapshot.providerTests)
    }

    @Test
    fun cancellationDoesNotCountAsSuccessOrFailure() {
        monitor.recordStarted()
        monitor.recordCancelled()

        val snapshot = monitor.snapshot()
        assertEquals(1, snapshot.started)
        assertEquals(1, snapshot.cancelled)
        assertEquals(0, snapshot.succeeded)
        assertEquals(0, snapshot.failed)
        assertEquals(0, snapshot.successRate)
        assertEquals(0L, snapshot.averageLatencyMs)
    }

    @Test
    fun resetClearsAllCountersAndFailureMetadata() {
        monitor.recordStarted()
        monitor.recordSucceeded(500)
        monitor.recordFailed(100, IOException("HTTP 500"))
        monitor.recordEvent("voice_input")
        monitor.reset()

        val snapshot = monitor.snapshot()
        assertEquals(0, snapshot.started)
        assertEquals(0, snapshot.succeeded)
        assertEquals(0, snapshot.failed)
        assertEquals(0, snapshot.cancelled)
        assertEquals(0, snapshot.averageLatencyMs)
        assertEquals("", snapshot.lastFailureCategory)
        assertEquals(0, snapshot.voiceInputs)
        assertTrue(snapshot.successRate == 0)
    }
}
