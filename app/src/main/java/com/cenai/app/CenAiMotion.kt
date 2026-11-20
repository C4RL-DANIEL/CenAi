package com.cenai.app

import android.content.Context
import android.provider.Settings

/**
 * Shared motion timings for the CenAI visual system.
 * Keep movement short, restrained and consistent across the interface.
 */
object CenAiMotion {
    const val tapMs = 120
    const val controlMs = 180
    const val contentMs = 240
    const val screenMs = 300
    const val sheetMs = 250
    const val ambientMs = 700
    const val completionHoldMs = 1_200

    /** Respect the user's Android animation scale without disabling functional transitions. */
    fun systemMotionEnabled(context: Context): Boolean = runCatching {
        Settings.Global.getFloat(
            context.contentResolver,
            Settings.Global.ANIMATOR_DURATION_SCALE,
            1f
        ) > 0f && Settings.Global.getFloat(
            context.contentResolver,
            Settings.Global.TRANSITION_ANIMATION_SCALE,
            1f
        ) > 0f
    }.getOrDefault(true)
}
