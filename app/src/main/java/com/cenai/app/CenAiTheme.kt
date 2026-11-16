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
    val background: Color, val surface: Color, val raised: Color,
    val accent: Color, val text: Color, val muted: Color, val glass: Boolean = false
)

val LocalCenAiPalette = staticCompositionLocalOf {
    CenAiPalette(Color(0xFF101114), Color(0xFF1B1D22), Color(0xFF282B32), Color(0xFFB9A1FF), Color(0xFFF4F2FA), Color(0xFF9B9AA8))
}

@Composable
fun CenAiTheme(style: String, content: @Composable () -> Unit) {
    val context = LocalContext.current
    val dark = isSystemInDarkTheme()
    val base = when (style) {
        "MATERIAL" -> CenAiPalette(
            if (dark) Color(0xFF101318) else Color(0xFFF7F7FC),
            if (dark) Color(0xFF191D24) else Color.White,
            if (dark) Color(0xFF272D36) else Color(0xFFE9EAF2),
            if (dark) Color(0xFFD0BCFF) else Color(0xFF6750A4),
            if (dark) Color(0xFFE3E2E9) else Color(0xFF1B1B21),
            if (dark) Color(0xFFBFC3CC) else Color(0xFF62636B)
        )
        "IOS" -> CenAiPalette(
            if (dark) Color(0xFF000000) else Color(0xFFF2F2F7),
            if (dark) Color(0xFF1C1C1E) else Color(0xFFFFFFFF),
            if (dark) Color(0xFF2C2C2E) else Color(0xFFE5E5EA),
            if (dark) Color(0xFF0A84FF) else Color(0xFF007AFF),
            if (dark) Color(0xFFF5F5F7) else Color(0xFF1C1C1E),
            if (dark) Color(0xFFB7C2D4) else Color(0xFF66758B),
            true
        )
        "FIGMA" -> CenAiPalette(
            if (dark) Color(0xFF100D1B) else Color(0xFFF6F3FF),
            if (dark) Color(0xFF1D1730) else Color(0xFFFFFFFF),
            if (dark) Color(0xFF30234B) else Color(0xFFEAE2FF),
            if (dark) Color(0xFFC4A7FF) else Color(0xFF7047EB),
            if (dark) Color(0xFFF7F1FF) else Color(0xFF211536),
            if (dark) Color(0xFFB7A8CC) else Color(0xFF766A88)
        )
        else -> CenAiPalette(
            if (dark) Color(0xFF0B1020) else Color(0xFFF5F7FC),
            if (dark) Color(0xFF121B2D) else Color(0xFFFFFFFF),
            if (dark) Color(0xFF1B2940) else Color(0xFFE8EDF7),
            if (dark) Color(0xFF8DE4DC) else Color(0xFF365FCE),
            if (dark) Color(0xFFF0F5FF) else Color(0xFF18243A),
            if (dark) Color(0xFF9EACC4) else Color(0xFF65738B)
        )
    }
    val colors = when {
        style == "MATERIAL" && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S && dark -> dynamicDarkColorScheme(context)
        style == "MATERIAL" && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S -> dynamicLightColorScheme(context)
        dark -> darkColorScheme(primary = base.accent, background = base.background, surface = base.surface, onBackground = base.text, onSurface = base.text)
        else -> lightColorScheme(primary = base.accent, background = base.background, surface = base.surface, onBackground = base.text, onSurface = base.text)
    }
    val palette = if (style == "MATERIAL" && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
        CenAiPalette(colors.background, colors.surface, colors.surfaceVariant, colors.primary, colors.onSurface, colors.onSurfaceVariant)
    } else base
    CompositionLocalProvider(LocalCenAiPalette provides palette) {
        MaterialTheme(colorScheme = colors, content = content)
    }
}
