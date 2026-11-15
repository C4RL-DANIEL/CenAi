package com.cenai.app
object NativeCore {
    init { System.loadLibrary("cenai_core") }
    external fun normalizePrompt(input: String): String
}
