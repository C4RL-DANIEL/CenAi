package com.cenai.app

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
}
