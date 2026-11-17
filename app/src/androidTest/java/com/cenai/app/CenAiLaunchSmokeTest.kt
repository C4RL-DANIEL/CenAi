package com.cenai.app

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
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
        compose.onNodeWithText("Message CenAI…").assertIsDisplayed()
    }

    @Test
    fun settingsExposeCoreVoiceAiGuardianAgentAndUpdateSystems() {
        compose.onNodeWithContentDescription("Settings").performClick()
        compose.onNodeWithText("CenAI settings").assertIsDisplayed()

        compose.onNodeWithText("AI & models").performClick()
        listOf(
            "PROVIDER",
            "AGENT WORKSPACE",
            "CENAI CLOUD ACCOUNT · PUBLIC BETA"
        ).forEach { label ->
            val node = compose.onNodeWithText(label, substring = true)
            node.performScrollTo()
            node.assertIsDisplayed()
        }

        compose.onNodeWithText("Device").performClick()
        listOf(
            "VOICE ENGINE",
            "OFFLINE BRAIN · ON-DEVICE AI",
            "LOCAL KNOWLEDGE",
            "GUARDIAN · DEVICE HEALTH",
            "APP MONITOR"
        ).forEach { label ->
            val node = compose.onNodeWithText(label, substring = true)
            node.performScrollTo()
            node.assertIsDisplayed()
        }

        compose.onNodeWithText("Updates").performClick()
        val softwareUpdate = compose.onNodeWithText("SOFTWARE UPDATE", substring = true)
        softwareUpdate.performScrollTo()
        softwareUpdate.assertIsDisplayed()
    }
}
