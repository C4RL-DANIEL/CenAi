package com.cenai.app

/**
 * Small, deterministic functions shared with the Rust core.
 *
 * Provider-specific tokenization and billing remain the responsibility of the provider;
 * estimatePromptTokens is only a local UI estimate.
 */
object NativeCore {
    init { System.loadLibrary("cenai_core") }

    external fun normalizePrompt(input: String): String

    /** Approximate 1-token-per-4-characters hint; never use for billing or hard limits. */
    external fun estimatePromptTokens(input: String): Int

    /** Checks Unicode code-point length against a caller-supplied character limit. */
    external fun isPromptWithinLimit(input: String, maxChars: Int): Boolean
}
