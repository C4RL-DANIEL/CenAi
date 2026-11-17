package com.cenai.app.ui.components

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.fadeIn
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.BorderStroke
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
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.cenai.app.ChatMessage
import com.cenai.app.LocalCenAiPalette
import com.cenai.app.RichMessageContent
import com.cenai.app.cenAiGlassBorder

/** Stateless home action card shared by workspace entry points. */
@Composable
internal fun HomeActionCard(icon: ImageVector, title: String, subtitle: String, modifier: Modifier = Modifier, onClick: () -> Unit) {
    val palette = LocalCenAiPalette.current
    Surface(onClick = onClick, modifier = modifier.heightIn(min = 108.dp),
        color = if (palette.glass) palette.surface.copy(alpha = 0.58f) else palette.surface,
        shape = RoundedCornerShape(19.dp),
        border = if (palette.glass) cenAiGlassBorder() else BorderStroke(1.dp, palette.accent.copy(alpha = 0.20f)),
        shadowElevation = if (palette.glass) 9.dp else 3.dp) {
        Column(Modifier.fillMaxWidth().padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Surface(color = palette.accent.copy(alpha = 0.13f), shape = RoundedCornerShape(12.dp), modifier = Modifier.size(36.dp)) {
                Box(contentAlignment = Alignment.Center) { Icon(icon, contentDescription = null, tint = palette.accent, modifier = Modifier.size(19.dp)) }
            }
            Text(title, color = palette.text, fontSize = 12.sp, fontWeight = FontWeight.SemiBold)
            Text(subtitle, color = palette.muted, fontSize = 10.sp, lineHeight = 14.sp)
        }
    }
}

/** Prompt shortcut shared by empty-chat and workspace states. */
@Composable
internal fun SuggestionChip(label: String, onClick: () -> Unit) {
    val palette = LocalCenAiPalette.current
    Surface(onClick = onClick, color = if (palette.glass) palette.surface.copy(alpha = 0.62f) else palette.surface,
        shape = RoundedCornerShape(16.dp),
        border = BorderStroke(1.dp, if (palette.glass) Color.White.copy(alpha = 0.76f) else palette.raised),
        shadowElevation = if (palette.glass) 5.dp else 0.dp) {
        Text(label, color = palette.text, fontSize = 12.sp, modifier = Modifier.padding(horizontal = 13.dp, vertical = 10.dp))
    }
}

/** Stateless message renderer. Chat state and actions remain owned by the screen. */
@Composable
internal fun MessageBubble(message: ChatMessage, index: Int, last: Boolean, busy: Boolean, style: String,
    onCopy: () -> Unit, onCopyCode: (String) -> Unit, onSpeak: () -> Unit, onEdit: () -> Unit,
    onRegenerate: () -> Unit, onApproveDeviceAction: (String, String) -> Unit) {
    val palette = LocalCenAiPalette.current
    if (message.role == "tool") { ToolActivityCard(message.content, onApproveDeviceAction); return }
    val user = message.role == "user"
    val ios = style == "IOS"
    val editorial = style == "FIGMA"
    AnimatedVisibility(visible = true, enter = fadeIn(tween(320)) + slideInVertically(tween(320)) { it / 8 } +
        androidx.compose.animation.scaleIn(spring(dampingRatio = 0.82f, stiffness = 380f), initialScale = 0.985f)) {
        Column(Modifier.fillMaxWidth(), horizontalAlignment = if (user) Alignment.End else Alignment.Start) {
            if (!ios) Text(if (user) "YOU" else if (editorial) "CENAI / RESPONSE" else "CENAI", color = palette.accent,
                fontSize = 10.sp, fontWeight = FontWeight.Bold, letterSpacing = if (editorial) 1.sp else 0.sp)
            Surface(color = when { ios && user -> palette.accent; ios && !user -> palette.surface.copy(alpha = 0.62f);
                editorial && !user -> palette.surface; user -> palette.raised; else -> palette.surface },
                shape = when { ios && user -> RoundedCornerShape(22.dp); ios -> RoundedCornerShape(4.dp);
                    editorial -> RoundedCornerShape(topStart = 5.dp, topEnd = 22.dp, bottomEnd = 22.dp, bottomStart = 22.dp);
                    user -> RoundedCornerShape(22.dp, 22.dp, 6.dp, 22.dp); else -> RoundedCornerShape(6.dp, 22.dp, 22.dp, 22.dp) },
                border = when { ios -> BorderStroke(1.dp, Brush.linearGradient(listOf(Color.White.copy(alpha = 0.88f), palette.accent.copy(alpha = 0.20f), Color.White.copy(alpha = 0.32f))));
                    editorial && !user -> BorderStroke(1.dp, palette.raised); else -> null },
                modifier = Modifier.fillMaxWidth(if (user) 0.92f else 1f).animateContentSize(spring(stiffness = Spring.StiffnessMediumLow))) {
                Column(Modifier.fillMaxWidth().padding(if (ios && !user) 4.dp else 15.dp)) {
                    if (message.content.isBlank()) Text(" ", color = palette.text)
                    else RichMessageContent(message.content.replace(Regex("""\[\[CENAI_IMAGE\|[^|]+\|[A-Za-z0-9+/=]+]]"""), "[Image attached for vision analysis]"), onCopyCode)
                }
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                TextButton(onClick = onCopy) { Icon(Icons.Default.ContentCopy, null, Modifier.size(13.dp), tint = palette.muted); Spacer(Modifier.width(4.dp)); Text("Copy", color = palette.muted, fontSize = 10.sp) }
                if (!user) TextButton(onClick = onSpeak) { Icon(Icons.Default.VolumeUp, null, Modifier.size(13.dp), tint = palette.muted); Spacer(Modifier.width(4.dp)); Text("Listen", color = palette.muted, fontSize = 10.sp) }
                if (user && !busy) TextButton(onClick = onEdit) { Text("Edit", color = palette.muted, fontSize = 10.sp) }
                if (!user && last && !busy) TextButton(onClick = onRegenerate) { Icon(Icons.Default.Refresh, null, Modifier.size(13.dp), tint = palette.muted); Spacer(Modifier.width(4.dp)); Text("Regenerate", color = palette.muted, fontSize = 10.sp) }
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
    val icon = when { title == "Completed" -> Icons.Default.CheckCircle; title == "Approval required" -> Icons.Default.Lock; else -> Icons.Default.Info }
    Surface(color = palette.raised.copy(alpha = 0.72f), shape = RoundedCornerShape(16.dp), border = BorderStroke(1.dp, palette.raised),
        modifier = Modifier.fillMaxWidth().animateContentSize(spring(stiffness = Spring.StiffnessMediumLow)).clickable { expanded = !expanded }) {
        Column(Modifier.padding(horizontal = 14.dp, vertical = 11.dp), verticalArrangement = Arrangement.spacedBy(5.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(icon, null, tint = palette.accent, modifier = Modifier.size(17.dp)); Spacer(Modifier.width(9.dp))
                Column(Modifier.weight(1f)) {
                    Text(tool.replace('_', ' ').replaceFirstChar { it.uppercase() }, color = palette.text, fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
                    Text(title, color = palette.muted, fontSize = 10.sp)
                }
                Text(if (expanded) "Hide" else "Details", color = palette.accent, fontSize = 11.sp)
            }
            AnimatedVisibility(expanded, enter = fadeIn(tween(180)) + slideInVertically { -it / 5 }) {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(details.ifBlank { "No additional details." }, color = palette.muted, fontSize = 12.sp, lineHeight = 18.sp)
                    if (title == "Approval required" && actionArgs != null) {
                        Text("This action opens an Android screen. CenAI cannot place calls or send content without your separate action in that app.", color = palette.muted, fontSize = 10.sp)
                        OutlinedButton(onClick = { onApproveDeviceAction(tool, actionArgs) }, modifier = Modifier.fillMaxWidth()) { Text("Approve & run once") }
                    }
                }
            }
        }
    }
}

/** Non-blocking generation indicator. */
@Composable
internal fun ThinkingIndicator() {
    val palette = LocalCenAiPalette.current
    val transition = androidx.compose.animation.core.rememberInfiniteTransition(label = "thinking")
    val alpha = transition.animateFloat(initialValue = 0.25f, targetValue = 1f,
        animationSpec = androidx.compose.animation.core.infiniteRepeatable(animation = tween(650), repeatMode = androidx.compose.animation.core.RepeatMode.Reverse),
        label = "thinking-pulse")
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Text("CenAI is thinking", color = palette.muted, fontSize = 12.sp)
        Text("•••", color = palette.accent, fontSize = 17.sp, modifier = Modifier.alpha(alpha.value))
    }
}
