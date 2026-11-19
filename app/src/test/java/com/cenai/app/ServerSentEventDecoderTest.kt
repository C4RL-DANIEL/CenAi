package com.cenai.app

import okio.Buffer
import org.junit.Assert.assertEquals
import org.junit.Test

class ServerSentEventDecoderTest {
    @Test fun decodesNamedAndMultilineEvents() {
        val source = Buffer().writeUtf8(
            ": heartbeat\n" +
            "event: response.output_text.delta\n" +
            "data: {\"type\":\"response.output_text.delta\",\n" +
            "data: \"delta\":\"hello\"}\n\n" +
            "data: [DONE]\n\n"
        )
        val events = mutableListOf<ServerSentEventDecoder.Event>()
        ServerSentEventDecoder.read(source, events::add)
        assertEquals(2, events.size)
        assertEquals("response.output_text.delta", events[0].name)
        assertEquals("{\"type\":\"response.output_text.delta\",\n\"delta\":\"hello\"}", events[0].data)
        assertEquals("[DONE]", events[1].data)
    }

    @Test fun dispatchesFinalFrameWithoutTrailingBlankLine() {
        val events = mutableListOf<ServerSentEventDecoder.Event>()
        ServerSentEventDecoder.read(Buffer().writeUtf8("event: message\ndata: last"), events::add)
        assertEquals(listOf(ServerSentEventDecoder.Event("message", "last")), events)
    }
}
