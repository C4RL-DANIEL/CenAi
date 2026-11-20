package com.cenai.app

import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/** Animated, brand-owned gradient typography for high-emphasis CenAI labels. */
@Composable
fun CenAiGradientText(
    text: String,
    modifier: Modifier = Modifier,
    fontSize: androidx.compose.ui.unit.TextUnit = 20.sp,
    fontWeight: FontWeight = FontWeight.SemiBold,
    letterSpacing: androidx.compose.ui.unit.TextUnit = 0.sp
) {
    val palette = LocalCenAiPalette.current
    val context = LocalContext.current
    val motionEnabled = remember(context) { CenAiMotion.systemMotionEnabled(context) }
    val entrance by animateFloatAsState(
        targetValue = 1f,
        animationSpec = tween(if (motionEnabled) 560 else 0, easing = androidx.compose.animation.core.FastOutSlowInEasing),
        label = "gradient-type-entrance"
    )
    val transition = rememberInfiniteTransition(label = "cenai-gradient-type")
    val phase by transition.animateFloat(
        initialValue = 0f,
        targetValue = if (motionEnabled) 1f else 0f,
        animationSpec = if (motionEnabled) infiniteRepeatable(
            animation = tween(4200, easing = androidx.compose.animation.core.LinearEasing),
            repeatMode = RepeatMode.Reverse
        ) else tween(0),
        label = "gradient-type-phase"
    )
    val colors = palette.gradient
    val brush = Brush.linearGradient(
        colors = colors,
        start = Offset(phase * 420f, 0f),
        end = Offset(420f + phase * 420f, 100f)
    )
    Text(
        text = text,
        modifier = modifier.graphicsLayer {
            alpha = entrance
            scaleX = 0.975f + 0.025f * entrance
            scaleY = 0.975f + 0.025f * entrance
            translationY = (1f - entrance) * 9f
        },
        style = TextStyle(
            brush = brush,
            fontSize = fontSize,
            fontWeight = fontWeight,
            letterSpacing = letterSpacing
        )
    )
}

/** Brand gradient surface; content remains fully interactive and accessible. */
@Composable
fun CenAiGradientSurface(
    modifier: Modifier = Modifier,
    content: @Composable BoxScope.() -> Unit
) {
    val palette = LocalCenAiPalette.current
    val context = LocalContext.current
    val motionEnabled = remember(context) { CenAiMotion.systemMotionEnabled(context) }
    val transition = rememberInfiniteTransition(label = "cenai-gradient-surface")
    val phase by transition.animateFloat(
        initialValue = 0f,
        targetValue = if (motionEnabled) 1f else 0f,
        animationSpec = if (motionEnabled) infiniteRepeatable(
            animation = tween(6500, easing = androidx.compose.animation.core.LinearEasing),
            repeatMode = RepeatMode.Reverse
        ) else tween(0),
        label = "gradient-surface-phase"
    )
    val colors = palette.gradient
    val brush = Brush.linearGradient(
        colors = colors,
        start = Offset(phase * 500f, 0f),
        end = Offset(500f + phase * 500f, 500f)
    )
    Box(
        modifier = modifier
            .clip(RoundedCornerShape(20.dp))
            .background(brush),
        contentAlignment = Alignment.Center,
        content = content
    )
}

/** A soft gradient halo used for AI activity and selected brand moments. */
@Composable
fun CenAiGradientHalo(
    modifier: Modifier = Modifier,
    content: @Composable BoxScope.() -> Unit
) {
    val palette = LocalCenAiPalette.current
    Box(
        modifier = modifier.background(
            Brush.radialGradient(
                colors = listOf(palette.gradient.first().copy(alpha = 0.26f), Color.Transparent)
            ),
            RoundedCornerShape(50)
        ),
        contentAlignment = Alignment.Center,
        content = content
    )
}
