package com.cenai.app

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * CenAI's own input surface. It replaces platform-default outlined fields with
 * a quiet, compact label + inset editor treatment while preserving Compose input semantics.
 */
@Composable
internal fun CenAiTextField(
    value: String,
    onValueChange: (String) -> Unit,
    modifier: Modifier = Modifier,
    label: (@Composable () -> Unit)? = null,
    placeholder: (@Composable () -> Unit)? = null,
    trailingIcon: (@Composable () -> Unit)? = null,
    singleLine: Boolean = false,
    minLines: Int = 1,
    readOnly: Boolean = false,
    enabled: Boolean = true,
    visualTransformation: VisualTransformation = VisualTransformation.None
) {
    val palette = LocalCenAiPalette.current
    val interaction = remember { MutableInteractionSource() }
    val focused by interaction.collectIsFocusedAsState()
    val borderColor by animateColorAsState(
        targetValue = if (focused) palette.text else palette.border,
        animationSpec = tween(CenAiMotion.controlMs),
        label = "cenai-input-border"
    )
    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        if (label != null) {
            Box(Modifier.padding(start = 2.dp)) { label() }
        }
        Surface(
            modifier = Modifier.fillMaxWidth(),
            color = palette.raised.copy(alpha = if (enabled) 0.72f else 0.38f),
            shape = RoundedCornerShape(12.dp),
            border = BorderStroke(if (focused) 1.5.dp else 1.dp, borderColor)
        ) {
            Row(
                modifier = Modifier.padding(horizontal = 13.dp, vertical = 12.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp)
            ) {
                BasicTextField(
                    value = value,
                    onValueChange = onValueChange,
                    modifier = Modifier.weight(1f),
                    enabled = enabled,
                    readOnly = readOnly,
                    singleLine = singleLine,
                    minLines = if (singleLine) 1 else minLines.coerceAtLeast(1),
                    maxLines = if (singleLine) 1 else Int.MAX_VALUE,
                    visualTransformation = visualTransformation,
                    interactionSource = interaction,
                    textStyle = TextStyle(
                        color = palette.text,
                        fontSize = 14.sp,
                        lineHeight = 21.sp
                    ),
                    cursorBrush = SolidColor(palette.text),
                    decorationBox = { innerTextField ->
                        Box {
                            if (value.isEmpty() && placeholder != null) placeholder()
                            innerTextField()
                        }
                    }
                )
                trailingIcon?.invoke()
            }
        }
    }
}
