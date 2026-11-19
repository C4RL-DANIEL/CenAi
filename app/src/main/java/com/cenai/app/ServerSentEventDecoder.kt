package com.cenai.app

import okio.BufferedSource

internal object ServerSentEventDecoder {
    data class Event(val name: String?, val data: String)

    fun read(source: BufferedSource, onEvent: (Event) -> Unit) {
        var eventName: String? = null
        val data = StringBuilder()
        fun dispatch() {
            if (data.isNotEmpty()) onEvent(Event(eventName, data.toString().removeSuffix("\n")))
            eventName = null
            data.setLength(0)
        }
        while (!source.exhausted()) {
            val line = source.readUtf8Line() ?: break
            if (line.isEmpty()) { dispatch(); continue }
            if (line.startsWith(":")) continue
            val separator = line.indexOf(':')
            val field = if (separator < 0) line else line.substring(0, separator)
            var value = if (separator < 0) "" else line.substring(separator + 1)
            if (value.startsWith(" ")) value = value.drop(1)
            when (field) {
                "event" -> eventName = value
                "data" -> data.append(value).append('\n')
            }
        }
        dispatch()
    }
}
