package com.cenai.app

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.onChildren
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onNodeWithTag
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

    private fun selectSettingsPage(page: String) {
        // The settings content is vertically scrollable and the page selector is horizontally scrollable.
        // Bring the requested chip into view across both scroll containers before clicking it.
        val tab = compose.onNodeWithTag("settings-page-tabs").onChildren().filterToOne(hasText(page))
        tab.performScrollTo()
        tab.assertIsDisplayed()
        tab.performClick()
    }

    @Test
    fun mainScreenLaunchesAndShowsComposer() {
        compose.onNodeWithText("Message CenAI…").assertIsDisplayed()
    }

    @Test
    fun settingsExposeCoreVoiceAiGuardianAgentAndUpdateSystems() {
        compose.onNodeWithText("Settings").assertIsDisplayed().performClick()
        compose.waitForIdle()
        compose.onNodeWithText("CenAI settings").assertIsDisplayed()

        selectSettingsPage("Appearance")
        val styleField = compose.onNodeWithText("Aurora · Living gradient")
        styleField.performScrollTo()
        styleField.performClick()
        compose.onNodeWithText("Ember · Warm spectrum").assertIsDisplayed().performClick()
        compose.onNodeWithText("Ember · Warm spectrum").assertIsDisplayed()
        compose.onNodeWithText("Ember · Warm spectrum").performClick()
        compose.onNodeWithText("Aurora · Living gradient").assertIsDisplayed().performClick()
        compose.onNodeWithText("Aurora · Living gradient").assertIsDisplayed()

        selectSettingsPage("AI & models")
        listOf(
            "PROVIDER",
            "CENAI CLOUD ACCOUNT · PUBLIC BETA"
        ).forEach { label ->
            val node = compose.onNodeWithText(label, substring = true)
            node.performScrollTo()
            node.assertIsDisplayed()
        }

        // Agent planning and scheduled tasks moved to their own settings page.
        selectSettingsPage("Agents")
        listOf("AGENT CONTROL CENTER", "AGENT WORKSPACE").forEach { label ->
            val node = compose.onNodeWithText(label, substring = true, useUnmergedTree = true)
            node.performScrollTo()
            node.assertIsDisplayed()
        }

        selectSettingsPage("Device")
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

        selectSettingsPage("Updates")
        val softwareUpdate = compose.onNodeWithText("SOFTWARE UPDATE", substring = true)
        softwareUpdate.performScrollTo()
        softwareUpdate.assertIsDisplayed()
    }
}
