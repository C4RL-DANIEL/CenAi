package com.cenai.app

import androidx.compose.ui.test.assertExists
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class CenAiLaunchSmokeTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    @Test
    fun mainScreenLaunchesAndShowsComposer() {
        compose.onNodeWithText("Message CenAI…").assertExists()
    }

    @Test
    fun settingsExposeCoreVoiceAiGuardianAgentAndUpdateSystems() {
        compose.onNodeWithContentDescription("Settings").performClick()
        compose.onNodeWithText("CenAI settings").assertExists()
        listOf(
            "PROVIDER",
            "AGENT WORKSPACE",
            "CENAI CLOUD ACCOUNT · PUBLIC BETA",
            "VOICE ENGINE",
            "OFFLINE BRAIN · ON-DEVICE AI",
            "LOCAL KNOWLEDGE",
            "GUARDIAN · DEVICE HEALTH",
            "APP MONITOR",
            "SOFTWARE UPDATE"
        ).forEach { label ->
            compose.onNodeWithText(label, substring = true).assertExists()
        }
    }
}
