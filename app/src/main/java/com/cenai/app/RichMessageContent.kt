package com.cenai.app

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

private val CodePanel = Color(0xFF101317)
private val CodeBorder = Color(0xFF343A42)
private val CodeAccent = Color(0xFF8CC8B0)

/** Separates Markdown prose from fenced code blocks in assistant and user messages. */
@Composable
fun RichMessageContent(content: String, onCopyCode: (String) -> Unit) {
    val blocks = remember(content) { splitMessageBlocks(content) }
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        blocks.forEach { block ->
            when (block) {
                is MessageBlock.Prose -> if (block.text.isNotBlank()) {
                    Text(markdownText(block.text), color = Color(0xFFF2F0EC), fontSize = 15.sp, lineHeight = 24.sp)
                }
                is MessageBlock.Code -> CodeBlockCard(block.language, block.code, onCopyCode)
            }
        }
    }
}

private sealed interface MessageBlock {
    data class Prose(val text: String) : MessageBlock
    data class Code(val language: String, val code: String) : MessageBlock
}

private fun splitMessageBlocks(source: String): List<MessageBlock> {
    val result = mutableListOf<MessageBlock>()
    val fence = Regex("(?s)\\x60\\x60\\x60([^\\n]*)\\n?(.*?)\\x60\\x60\\x60")
    var cursor = 0
    fence.findAll(source).forEach { match ->
        if (match.range.first > cursor) result += MessageBlock.Prose(source.substring(cursor, match.range.first).trim())
        result += MessageBlock.Code(match.groupValues[1].trim().ifBlank { "code" }, match.groupValues[2].trimEnd())
        cursor = match.range.last + 1
    }
    if (cursor < source.length) result += MessageBlock.Prose(source.substring(cursor).trim())
    if (result.isEmpty()) result += MessageBlock.Prose(source)
    return result
}

@Composable
private fun CodeBlockCard(language: String, code: String, onCopyCode: (String) -> Unit) {
    Surface(
        color = CodePanel,
        shape = RoundedCornerShape(13.dp),
        border = androidx.compose.foundation.BorderStroke(1.dp, CodeBorder),
        modifier = Modifier.fillMaxWidth()
    ) {
        Column {
            Row(
                modifier = Modifier.fillMaxWidth().padding(start = 13.dp, end = 7.dp, top = 5.dp, bottom = 5.dp),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = androidx.compose.ui.Alignment.CenterVertically
            ) {
                Text(language.ifBlank { "code" }.uppercase(), color = CodeAccent, fontSize = 10.sp, fontWeight = FontWeight.Bold)
                TextButton(onClick = { onCopyCode(code) }, contentPadding = PaddingValues(horizontal = 8.dp, vertical = 2.dp)) {
                    Icon(Icons.Default.ContentCopy, contentDescription = null, modifier = Modifier.size(13.dp), tint = Color(0xFFB9C1CA))
                    Spacer(Modifier.width(5.dp))
                    Text("Copy code", color = Color(0xFFB9C1CA), fontSize = 11.sp)
                }
            }
            HorizontalDivider(color = CodeBorder)
            Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(13.dp)) {
                Text(code, color = Color(0xFFE3E8EE), fontFamily = FontFamily.Monospace, fontSize = 12.sp, lineHeight = 19.sp)
            }
        }
    }
}

private fun markdownText(source: String) = buildAnnotatedString {
    val lines = source.lines()
    lines.forEachIndexed { index, raw ->
        val line = raw.trimEnd()
        when {
            line.startsWith("### ") -> withStyle(SpanStyle(fontWeight = FontWeight.Bold, fontSize = 16.sp)) { append(line.removePrefix("### ")) }
            line.startsWith("## ") -> withStyle(SpanStyle(fontWeight = FontWeight.Bold, fontSize = 18.sp)) { append(line.removePrefix("## ")) }
            line.startsWith("# ") -> withStyle(SpanStyle(fontWeight = FontWeight.Bold, fontSize = 20.sp)) { append(line.removePrefix("# ")) }
            line.startsWith("- ") || line.startsWith("* ") -> {
                withStyle(SpanStyle(color = CodeAccent, fontWeight = FontWeight.Bold)) { append("• ") }
                appendInlineMarkdown(line.drop(2))
            }
            else -> appendInlineMarkdown(line)
        }
        if (index != lines.lastIndex) append("\n")
    }
}

private fun androidx.compose.ui.text.AnnotatedString.Builder.appendInlineMarkdown(value: String) {
    val pattern = Regex("(\\x60[^\\x60]+\\x60|\\*\\*[^*]+\\*\\*|\\*[^*]+\\*)")
    var cursor = 0
    pattern.findAll(value).forEach { match ->
        append(value.substring(cursor, match.range.first))
        val token = match.value
        when {
            token.firstOrNull() == 96.toChar() -> withStyle(SpanStyle(fontFamily = FontFamily.Monospace, color = CodeAccent, background = CodePanel)) { append(token.drop(1).dropLast(1)) }
            token.startsWith("**") -> withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append(token.drop(2).dropLast(2)) }
            else -> withStyle(SpanStyle(fontStyle = FontStyle.Italic)) { append(token.drop(1).dropLast(1)) }
        }
        cursor = match.range.last + 1
    }
    append(value.substring(cursor))
}
