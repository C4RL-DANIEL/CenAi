package com.cenai.app

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color

data class CenAiPalette(
    val background: Color,
    val surface: Color,
    val raised: Color,
    val accent: Color,
    val text: Color,
    val muted: Color,
    val glass: Boolean = false,
    val tertiary: Color = muted,
    val border: Color = raised,
    val positive: Color = Color(0xFF269B68),
    val warning: Color = Color(0xFFC58A2B),
    val danger: Color = Color(0xFFD64B4B)
)

val LocalCenAiPalette = staticCompositionLocalOf {
    CenAiPalette(
        background = Color(0xFFFAFAFA),
        surface = Color.White,
        raised = Color(0xFFF1F1F1),
        accent = Color(0xFF151515),
        text = Color(0xFF151515),
        muted = Color(0xFF626262),
        tertiary = Color(0xFF898989),
        border = Color(0xFFE5E5E5)
    )
}

/**
 * CenAI's visual system is deliberately monochrome: black, white and gray establish
 * hierarchy. Accent colors are reserved for semantic status, never decoration.
 * The style parameter remains accepted so existing preference plumbing is preserved.
 */
@Composable
fun CenAiTheme(style: String, content: @Composable () -> Unit) {
    val dark = isSystemInDarkTheme()
    val palette = if (dark) {
        CenAiPalette(
            background = Color(0xFF090909),
            surface = Color(0xFF111111),
            raised = Color(0xFF1A1A1A),
            accent = Color(0xFFF7F7F7),
            text = Color(0xFFF7F7F7),
            muted = Color(0xFFB5B5B5),
            tertiary = Color(0xFF858585),
            border = Color(0xFF303030),
            positive = Color(0xFF55C995),
            warning = Color(0xFFE7B45A),
            danger = Color(0xFFFF6B6B)
        )
    } else {
        CenAiPalette(
            background = Color(0xFFFAFAFA),
            surface = Color.White,
            raised = Color(0xFFF1F1F1),
            accent = Color(0xFF151515),
            text = Color(0xFF151515),
            muted = Color(0xFF626262),
            tertiary = Color(0xFF898989),
            border = Color(0xFFE5E5E5),
            positive = Color(0xFF188653),
            warning = Color(0xFFAA741F),
            danger = Color(0xFFC83D3D)
        )
    }
    val scheme = if (dark) {
        darkColorScheme(
            primary = palette.text,
            onPrimary = palette.background,
            secondary = palette.muted,
            onSecondary = palette.background,
            background = palette.background,
            onBackground = palette.text,
            surface = palette.surface,
            onSurface = palette.text,
            surfaceVariant = palette.raised,
            onSurfaceVariant = palette.muted,
            outline = palette.border,
            error = palette.danger,
            onError = Color.White
        )
    } else {
        lightColorScheme(
            primary = palette.text,
            onPrimary = Color.White,
            secondary = palette.muted,
            onSecondary = Color.White,
            background = palette.background,
            onBackground = palette.text,
            surface = palette.surface,
            onSurface = palette.text,
            surfaceVariant = palette.raised,
            onSurfaceVariant = palette.muted,
            outline = palette.border,
            error = palette.danger,
            onError = Color.White
        )
    }
    CompositionLocalProvider(LocalCenAiPalette provides palette) {
        MaterialTheme(colorScheme = scheme, content = content)
    }
}
