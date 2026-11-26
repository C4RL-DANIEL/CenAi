package com.cenai.app

import org.junit.Assert.assertThrows
import org.junit.Test

class CenAiVoiceClientTest {
    @Test
    fun acceptsBoundedMonoPcm16Wav() {
        CenAiVoiceClient.validateWav(validWav())
    }

    @Test
    fun rejectsNonWavAndUnsupportedAudioFormats() {
        assertThrows(IllegalArgumentException::class.java) {
            CenAiVoiceClient.validateWav(ByteArray(64))
        }
        val stereo = validWav().also { it[22] = 2 }
        assertThrows(IllegalArgumentException::class.java) {
            CenAiVoiceClient.validateWav(stereo)
        }
    }

    @Test
    fun rejectsTruncatedAudioChunks() {
        val wav = validWav()
        wav[40] = 0x40
        assertThrows(IllegalArgumentException::class.java) {
            CenAiVoiceClient.validateWav(wav)
        }
    }

    private fun validWav(): ByteArray {
        val bytes = ByteArray(48)
        fun putAscii(offset: Int, value: String) {
            value.toByteArray(Charsets.US_ASCII).copyInto(bytes, offset)
        }
        fun put16(offset: Int, value: Int) {
            bytes[offset] = value.toByte()
            bytes[offset + 1] = (value shr 8).toByte()
        }
        fun put32(offset: Int, value: Int) {
            for (i in 0..3) bytes[offset + i] = (value shr (8 * i)).toByte()
        }
        putAscii(0, "RIFF")
        put32(4, 40)
        putAscii(8, "WAVE")
        putAscii(12, "fmt ")
        put32(16, 16)
        put16(20, 1)
        put16(22, 1)
        put32(24, 24_000)
        put32(28, 48_000)
        put16(32, 2)
        put16(34, 16)
        putAscii(36, "data")
        put32(40, 4)
        put16(44, 0)
        put16(46, 0)
        return bytes
    }
}
