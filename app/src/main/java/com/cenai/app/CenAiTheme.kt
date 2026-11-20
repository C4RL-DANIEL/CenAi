package com.cenai.app

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.unit.dp

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
    val danger: Color = Color(0xFFD64B4B),
    val gradient: List<Color> = listOf(Color(0xFF65D6FF), Color(0xFF8B7CFF), Color(0xFFFF78C8))
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
 * CenAI's visual system pairs quiet adaptive surfaces with an animated Aurora spectrum.
 * Style changes affect gradient identity while existing light/dark system behavior remains intact.
 */
@Composable
fun CenAiTheme(style: String, content: @Composable () -> Unit) {
    val dark = isSystemInDarkTheme()
    val gradient = when (style.uppercase()) {
        "EMBER" -> listOf(Color(0xFFFFA45B), Color(0xFFFF5F8F), Color(0xFFB47CFF))
        "STUDIO" -> listOf(Color(0xFF66C8E8), Color(0xFF7E8BFF), Color(0xFFB58BFF))
        else -> listOf(Color(0xFF4DE1D2), Color(0xFF6E8BFF), Color(0xFFD27BFF))
    }
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
            danger = Color(0xFFFF6B6B),
            gradient = gradient
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
            danger = Color(0xFFC83D3D),
            gradient = gradient
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
    val typography = Typography(
        displayLarge = TextStyle(fontFamily = FontFamily.SansSerif, fontWeight = FontWeight.Light, letterSpacing = (-1.2).sp),
        headlineLarge = TextStyle(fontFamily = FontFamily.SansSerif, fontWeight = FontWeight.SemiBold, letterSpacing = (-0.6).sp),
        headlineMedium = TextStyle(fontFamily = FontFamily.SansSerif, fontWeight = FontWeight.SemiBold, letterSpacing = (-0.35).sp),
        titleLarge = TextStyle(fontFamily = FontFamily.SansSerif, fontWeight = FontWeight.SemiBold, letterSpacing = (-0.2).sp),
        titleMedium = TextStyle(fontFamily = FontFamily.SansSerif, fontWeight = FontWeight.Medium, letterSpacing = 0.sp),
        bodyLarge = TextStyle(fontFamily = FontFamily.SansSerif, fontWeight = FontWeight.Normal, letterSpacing = 0.05.sp, lineHeight = 25.sp),
        bodyMedium = TextStyle(fontFamily = FontFamily.SansSerif, fontWeight = FontWeight.Normal, letterSpacing = 0.05.sp, lineHeight = 22.sp),
        labelLarge = TextStyle(fontFamily = FontFamily.SansSerif, fontWeight = FontWeight.SemiBold, letterSpacing = 0.2.sp)
    )
    val shapes = Shapes(
        extraSmall = RoundedCornerShape(8.dp),
        small = RoundedCornerShape(12.dp),
        medium = RoundedCornerShape(16.dp),
        large = RoundedCornerShape(22.dp),
        extraLarge = RoundedCornerShape(28.dp)
    )
    CompositionLocalProvider(LocalCenAiPalette provides palette) {
        MaterialTheme(colorScheme = scheme, typography = typography, shapes = shapes, content = content)
    }
}
