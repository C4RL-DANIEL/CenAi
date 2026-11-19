package com.cenai.app.ui.components

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.VolumeUp
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.pointerInteropFilter
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.cenai.app.ChatMessage
import com.cenai.app.LocalCenAiPalette
import com.cenai.app.RichMessageContent

/** Plain, restrained home suggestion. */
@Composable
internal fun HomeActionCard(
    icon: androidx.compose.ui.graphics.vector.ImageVector,
    title: String,
    subtitle: String,
    modifier: Modifier = Modifier,
    onClick: () -> Unit
) {
    val palette = LocalCenAiPalette.current
    Surface(
        onClick = onClick,
        modifier = modifier.heightIn(min = 72.dp),
        color = Color.Transparent,
        shape = RoundedCornerShape(10.dp)
    ) {
        Row(
            Modifier.fillMaxWidth().padding(vertical = 12.dp, horizontal = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp)
        ) {
            Icon(icon, contentDescription = null, tint = palette.muted, modifier = Modifier.size(20.dp))
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(title, color = palette.text, fontSize = 14.sp, fontWeight = FontWeight.Medium)
                Text(subtitle, color = palette.tertiary, fontSize = 12.sp, lineHeight = 16.sp)
            }
        }
    }
}

/** Small outlined prompt chip with no decorative color. */
@Composable
internal fun SuggestionChip(label: String, onClick: () -> Unit) {
    val palette = LocalCenAiPalette.current
    Surface(
        onClick = onClick,
        color = palette.surface,
        shape = RoundedCornerShape(50),
        border = BorderStroke(1.dp, palette.border)
    ) {
        Text(label, color = palette.text, fontSize = 13.sp, modifier = Modifier.padding(horizontal = 14.dp, vertical = 9.dp))
    }
}

/** Message renderer: plain assistant text, compact user bubble, contextual actions. */
@Composable
internal fun MessageBubble(
    message: ChatMessage,
    index: Int,
    last: Boolean,
    busy: Boolean,
    style: String,
    onCopy: () -> Unit,
    onCopyCode: (String) -> Unit,
    onSpeak: () -> Unit,
    onEdit: () -> Unit,
    onRegenerate: () -> Unit,
    onApproveDeviceAction: (String, String) -> Unit
) {
    val palette = LocalCenAiPalette.current
    if (message.role == "tool") {
        ToolActivityCard(message.content, onApproveDeviceAction)
        return
    }
    val user = message.role == "user"
    val generating = !user && last && busy
    var actionsVisible by remember(message.role, index) { mutableStateOf(false) }

    AnimatedVisibility(
        visible = true,
        enter = fadeIn(tween(220)) + slideInVertically(tween(220)) { it / 12 } + scaleIn(
            animationSpec = spring(dampingRatio = 0.92f, stiffness = 500f),
            initialScale = 0.985f
        )
    ) {
        Column(
            Modifier.fillMaxWidth().pointerInput(message.role, index) {
                detectTapGestures(onLongPress = { actionsVisible = !actionsVisible })
            },
            horizontalAlignment = if (user) Alignment.End else Alignment.Start
        ) {
            if (generating && message.content.isBlank()) {
                GenerationAura(Modifier.padding(start = 2.dp, top = 4.dp, bottom = 8.dp))
            }
            if (message.content.isNotBlank()) {
                if (user) {
                    Surface(
                        color = palette.text,
                        contentColor = palette.background,
                        shape = RoundedCornerShape(topStart = 12.dp, topEnd = 12.dp, bottomEnd = 4.dp, bottomStart = 12.dp),
                        modifier = Modifier.fillMaxWidth(0.78f).animateContentSize(
                            spring(dampingRatio = 0.9f, stiffness = Spring.StiffnessMediumLow)
                        )
                    ) {
                        Box(Modifier.padding(horizontal = 14.dp, vertical = 11.dp)) {
                            RichMessageContent(
                                message.content.replace(Regex("""\[\[CENAI_IMAGE\|[^|]+\|[A-Za-z0-9+/=]+]]"""), "[Image attached for vision analysis]"),
                                onCopyCode
                            )
                        }
                    }
                } else {
                    Column(
                        Modifier.fillMaxWidth().widthIn(max = 640.dp).animateContentSize(
                            spring(dampingRatio = 0.92f, stiffness = Spring.StiffnessMediumLow)
                        ),
                        verticalArrangement = Arrangement.spacedBy(6.dp)
                    ) {
                        RichMessageContent(
                            message.content.replace(Regex("""\[\[CENAI_IMAGE\|[^|]+\|[A-Za-z0-9+/=]+]]"""), "[Image attached for vision analysis]"),
                            onCopyCode
                        )
                    }
                }
                AnimatedVisibility(
                    visible = actionsVisible || (last && !busy && !user),
                    enter = fadeIn(tween(160)),
                    exit = fadeOut(tween(120))
                ) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(2.dp)
                    ) {
                        IconButton(onClick = onCopy, modifier = Modifier.size(34.dp)) {
                            Icon(Icons.Default.ContentCopy, "Copy", tint = palette.tertiary, modifier = Modifier.size(16.dp))
                        }
                        if (!user) {
                            IconButton(onClick = onSpeak, modifier = Modifier.size(34.dp)) {
                                Icon(Icons.Default.VolumeUp, "Read aloud", tint = palette.tertiary, modifier = Modifier.size(17.dp))
                            }
                            if (last && !busy) {
                                IconButton(onClick = onRegenerate, modifier = Modifier.size(34.dp)) {
                                    Icon(Icons.Default.Refresh, "Regenerate", tint = palette.tertiary, modifier = Modifier.size(17.dp))
                                }
                            }
                        } else if (!busy) {
                            TextButton(onClick = onEdit, contentPadding = PaddingValues(horizontal = 8.dp, vertical = 0.dp)) {
                                Text("Edit", color = palette.tertiary, fontSize = 12.sp)
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun ToolActivityCard(content: String, onApproveDeviceAction: (String, String) -> Unit) {
    val palette = LocalCenAiPalette.current
    var expanded by remember(content) { mutableStateOf(false) }
    val title = content.substringBefore('\n').substringBefore(" · ")
    val tool = content.substringBefore('\n').substringAfter(" · ", "Agent tool")
    val details = content.substringAfter('\n', "").substringBefore("\nARGS:")
    val actionArgs = content.substringAfter("\nARGS:", "").takeIf { it.isNotBlank() }
    val isApproval = title == "Approval required"
    val isDone = title == "Completed"
    val statusColor = when {
        isApproval -> palette.warning
        isDone -> palette.positive
        else -> palette.tertiary
    }
    Surface(
        color = palette.raised,
        shape = RoundedCornerShape(12.dp),
        border = BorderStroke(1.dp, palette.border),
        modifier = Modifier.fillMaxWidth().animateContentSize(tween(200)).clickable { expanded = !expanded }
    ) {
        Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(7.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(
                    if (isDone) Icons.Default.CheckCircle else if (isApproval) Icons.Default.Lock else Icons.Default.Info,
                    contentDescription = null,
                    tint = statusColor,
                    modifier = Modifier.size(18.dp)
                )
                Spacer(Modifier.width(9.dp))
                Column(Modifier.weight(1f)) {
                    Text(tool.replace('_', ' ').replaceFirstChar { it.uppercase() }, color = palette.text, fontSize = 14.sp, fontWeight = FontWeight.Medium)
                    Text(title, color = statusColor, fontSize = 12.sp)
                }
                Text(if (expanded) "Hide" else "Details", color = palette.tertiary, fontSize = 12.sp)
            }
            AnimatedVisibility(expanded, enter = fadeIn(tween(160)) + slideInVertically(tween(180)) { -it / 6 }) {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(details.ifBlank { "No additional details." }, color = palette.muted, fontSize = 13.sp, lineHeight = 19.sp)
                    if (isApproval && actionArgs != null) {
                        Text(
                            "This action opens an Android screen. CenAI cannot place calls or send content without your separate action in that app.",
                            color = palette.muted,
                            fontSize = 12.sp,
                            lineHeight = 17.sp
                        )
                        OutlinedButton(onClick = { onApproveDeviceAction(tool, actionArgs) }, modifier = Modifier.fillMaxWidth()) {
                            Text("Approve & run once")
                        }
                    }
                }
            }
        }
    }
}

/** Small generation state; intentionally quiet and only animated while active. */
@Composable
internal fun ThinkingIndicator() {
    val palette = LocalCenAiPalette.current
    val transition = androidx.compose.animation.core.rememberInfiniteTransition(label = "thinking")
    val alpha = transition.animateFloat(
        initialValue = 0.35f,
        targetValue = 1f,
        animationSpec = androidx.compose.animation.core.infiniteRepeatable(
            animation = tween(700),
            repeatMode = androidx.compose.animation.core.RepeatMode.Reverse
        ),
        label = "thinking-pulse"
    )
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Text("Generating", color = palette.tertiary, fontSize = 12.sp)
        Text("•••", color = palette.text, fontSize = 16.sp, modifier = Modifier.alpha(alpha.value))
    }
}
