package com.cenai.app

import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext

data class CenAiPalette(
    val background: Color,
    val surface: Color,
    val raised: Color,
    val accent: Color,
    val text: Color,
    val muted: Color
)

val LocalCenAiPalette = staticCompositionLocalOf {
    CenAiPalette(
        Color(0xFF111214), Color(0xFF1C1E21), Color(0xFF26292D),
        Color(0xFFD8B48A), Color(0xFFF2F0EC), Color(0xFF92969D)
    )
}

@Composable
fun CenAiTheme(style: String, content: @Composable () -> Unit) {
    val context = LocalContext.current
    val dark = isSystemInDarkTheme()
    val palette = when (style) {
        "MATERIAL" -> CenAiPalette(
            background = if (dark) Color(0xFF101318) else Color(0xFFF7F7FC),
            surface = if (dark) Color(0xFF191D24) else Color(0xFFFFFFFF),
            raised = if (dark) Color(0xFF272D36) else Color(0xFFE9EAF2),
            accent = if (dark) Color(0xFFD0BCFF) else Color(0xFF6750A4),
            text = if (dark) Color(0xFFE3E2E9) else Color(0xFF1B1B21),
            muted = if (dark) Color(0xFFBFC3CC) else Color(0xFF62636B)
        )
        "IOS" -> CenAiPalette(
            background = if (dark) Color(0xFF000000) else Color(0xFFF2F2F7),
            surface = if (dark) Color(0xFF1C1C1E) else Color(0xFFFFFFFF),
            raised = if (dark) Color(0xFF2C2C2E) else Color(0xFFE5E5EA),
            accent = Color(0xFF0A84FF),
            text = if (dark) Color(0xFFF5F5F7) else Color(0xFF1C1C1E),
            muted = if (dark) Color(0xFF98989D) else Color(0xFF6C6C70)
        )
        "FIGMA" -> CenAiPalette(
            background = if (dark) Color(0xFF11101A) else Color(0xFFF7F5FF),
            surface = if (dark) Color(0xFF201C31) else Color(0xFFFFFFFF),
            raised = if (dark) Color(0xFF302A46) else Color(0xFFEAE5FF),
            accent = Color(0xFFA78BFA),
            text = if (dark) Color(0xFFF4F0FF) else Color(0xFF211A38),
            muted = if (dark) Color(0xFFB7AECF) else Color(0xFF6B6280)
        )
        else -> CenAiPalette(
            Color(0xFF111214), Color(0xFF1C1E21), Color(0xFF26292D),
            Color(0xFFD8B48A), Color(0xFFF2F0EC), Color(0xFF92969D)
        )
    }
    val colors = when {
        style == "MATERIAL" && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S && dark -> dynamicDarkColorScheme(context)
        style == "MATERIAL" && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S -> dynamicLightColorScheme(context)
        dark -> darkColorScheme(
            primary = palette.accent, background = palette.background,
            surface = palette.surface, onBackground = palette.text, onSurface = palette.text
        )
        else -> lightColorScheme(
            primary = palette.accent, background = palette.background,
            surface = palette.surface, onBackground = palette.text, onSurface = palette.text
        )
    }
    CompositionLocalProvider(LocalCenAiPalette provides palette) {
        MaterialTheme(colorScheme = colors, content = content)
    }
}
