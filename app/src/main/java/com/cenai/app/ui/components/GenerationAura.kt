package com.cenai.app.ui.components

import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.cenai.app.LocalCenAiPalette
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.sin

/**
 * CenAI's live-generation aura: a soft, continuously morphing orb built from
 * several shades of the active accent hue. It is deliberately independent of
 * provider/network code and runs only while the latest assistant message streams.
 */
@Composable
internal fun GenerationAura(modifier: Modifier = Modifier) {
    val palette = LocalCenAiPalette.current
    val motion = rememberInfiniteTransition(label = "cenai-generation-aura")
    val phase by motion.animateFloat(
        initialValue = 0f,
        targetValue = 1f,
        animationSpec = infiniteRepeatable(tween(3_200, easing = LinearEasing), RepeatMode.Restart),
        label = "aura-flow"
    )
    val pulse by motion.animateFloat(
        initialValue = 0.88f,
        targetValue = 1.08f,
        animationSpec = infiniteRepeatable(tween(1_050, easing = FastOutSlowInEasing), RepeatMode.Reverse),
        label = "aura-breathe"
    )
    val orbit by motion.animateFloat(
        initialValue = 0f,
        targetValue = 360f,
        animationSpec = infiniteRepeatable(tween(5_600, easing = LinearEasing), RepeatMode.Restart),
        label = "aura-orbit"
    )

    val accent = palette.accent
    val light = lerp(accent, Color.White, 0.42f)
    val deep = lerp(accent, Color.Black, 0.28f)

    Row(
        modifier = modifier,
        verticalAlignment = Alignment.CenterVertically
    ) {
        Box(
            modifier = Modifier.size(76.dp),
            contentAlignment = Alignment.Center
        ) {
            Canvas(Modifier.size(76.dp)) {
                val center = Offset(size.width / 2f, size.height / 2f)
                val radius = size.minDimension / 2f
                val angle = phase * (2f * PI.toFloat())
                val lobeA = Offset(
                    center.x + cos(angle) * radius * 0.15f,
                    center.y + sin(angle * 1.25f) * radius * 0.13f
                )
                val lobeB = Offset(
                    center.x + cos(angle + 2.1f) * radius * 0.16f,
                    center.y + sin(angle * 0.82f + 1.4f) * radius * 0.15f
                )

                // Diffuse outer glow and two overlapping, moving color fields.
                drawCircle(
                    color = accent.copy(alpha = 0.12f),
                    radius = radius * 0.62f * pulse,
                    center = center
                )
                drawCircle(
                    brush = Brush.radialGradient(
                        colors = listOf(light.copy(alpha = 0.96f), accent.copy(alpha = 0.76f), deep.copy(alpha = 0.12f), Color.Transparent),
                        center = lobeA,
                        radius = radius * 1.34f
                    ),
                    radius = radius * 0.76f * pulse,
                    center = lobeA
                )
                drawCircle(
                    brush = Brush.radialGradient(
                        colors = listOf(accent.copy(alpha = 0.76f), deep.copy(alpha = 0.44f), Color.Transparent),
                        center = lobeB,
                        radius = radius * 1.14f
                    ),
                    radius = radius * 0.66f * (2f - pulse),
                    center = lobeB
                )

                // A very fine partial orbit gives the orb a sense of movement
                // without introducing another hue or a distracting spinner.
                drawArc(
                    color = light.copy(alpha = 0.42f),
                    startAngle = orbit + 205f,
                    sweepAngle = 112f,
                    useCenter = false,
                    topLeft = Offset(radius * 0.13f, radius * 0.13f),
                    size = androidx.compose.ui.geometry.Size(radius * 1.74f, radius * 1.74f),
                    style = Stroke(width = 1.dp.toPx())
                )
            }
        }
        Text(
            text = "CenAI is generating",
            modifier = Modifier.padding(start = 12.dp),
            color = palette.muted,
            fontSize = 12.sp,
            fontWeight = FontWeight.Medium,
            letterSpacing = 0.2.sp
        )
    }
}
