package com.cenai.app

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.work.WorkManager
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class BackgroundTaskWorkerTest {
    @Test
    fun guardianSamplingUsesPersistentWorkManagerSchedulingAndCanBeRevoked() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        GuardianWorker.schedule(context)
        val work = WorkManager.getInstance(context)
            .getWorkInfosForUniqueWork("cenai_rust_guardian_periodic").get()
        assertTrue(work.isNotEmpty())
        GuardianWorker.cancel(context)
        val cancelled = WorkManager.getInstance(context)
            .getWorkInfosForUniqueWork("cenai_rust_guardian_periodic").get()
        assertTrue(cancelled.all { it.state == androidx.work.WorkInfo.State.CANCELLED })
    }

    @Test
    fun headlessVaultCanBeOpenedFromApplicationContextWithoutAnActivity() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        // A clean test install has no signed-in session, so Rust must safely no-op.
        assertEquals(0, BackgroundTaskWorker.nativeRunBackgroundTasks(context))
    }

    @Test
    fun automationScheduleRefreshUsesPersistentWorkManagerScheduling() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        AutomationReminderBridge.scheduleRefresh(context)
        val work = WorkManager.getInstance(context)
            .getWorkInfosForUniqueWork("cenai_automation_schedule_refresh").get()
        assertTrue(work.isNotEmpty())
        AutomationReminderBridge.cancelRefresh(context)
    }

    @Test
    fun headlessAutomationRefreshSafelyNoOpsWithoutASignedInSession() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        assertEquals(0, AutomationScheduleRefreshWorker.nativeRefreshAutomationSchedules(context))
    }

    @Test
    fun backgroundWorkUsesPersistentWorkManagerScheduling() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        BackgroundTaskBridge.schedule(context)
        val work = WorkManager.getInstance(context)
            .getWorkInfosForUniqueWork("cenai_background_cognitive_periodic").get()
        assertTrue(work.isNotEmpty())
        BackgroundTaskBridge.cancel(context)
    }
}
