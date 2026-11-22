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

    /** Evaluates bounded arithmetic in the Rust core; errors are prefixed with ERROR:. */
    external fun evaluateArithmetic(expression: String): String

    /** Executes deterministic safe agent tools in Rust. */
    external fun executeSafeTool(name: String, argumentsJson: String, localTime: String, localDate: String): String

    /** Returns the Rust-owned agent tool catalog and schemas. */
    external fun listAgentTools(enabledToolsJson: String): String

    /** Builds safe foreground/background tool-use instructions in Rust. */
    external fun buildToolGuide(enabledToolsJson: String, planningEnabled: Boolean, allowDeviceActions: Boolean): String

    /** Combines independent provider responses into a bounded synthesis prompt. */
    external fun buildSynthesisPrompt(userRequest: String, answersJson: String): String

    /** Enforces the Rust tool allowlist and approval policy. */
    external fun evaluateToolPolicy(name: String, enabledToolsJson: String): String

    /** Parses structured agent tool calls in the Rust core. */
    external fun parseAgentToolCall(response: String): String

    /** Classifies a model turn as final text or a structured tool request. */
    external fun parseAgentStep(response: String): String

    /** Runs private conversation retrieval in the Rust core. */
    external fun retrieveLocalKnowledge(payload: String, maxResults: Int, maxCharacters: Int): String

    /** Validates and canonicalizes the encrypted conversation store in Rust. */
    external fun normalizeConversations(rawJson: String): String

    /** Builds bounded per-account context for local and remote model requests. */
    external fun buildSystemContext(systemPrompt: String, approvedMemory: String, localKnowledge: String, toolGuide: String): String

    /** Normalizes line endings and removes unsafe control characters from message bodies. */
    external fun normalizeMessageContent(input: String): String

    /** Returns true only for system, user, assistant, or tool messages. */
    external fun isValidChatRole(role: String): Boolean

    /** Builds an OpenAI-compatible text chat payload in the Rust core. */
    external fun buildOpenAiChatPayload(model: String, maxTokens: Int, messagesJson: String): String

    /** Builds provider-specific text and vision payloads in the Rust core. */
    external fun buildProviderPayload(provider: String, model: String, maxTokens: Int, messagesJson: String, customParamsJson: String): String

    /** Parses provider-specific server-sent event payloads in Rust. */
    external fun parseProviderDelta(provider: String, raw: String): String

    /** Extracts a bounded error message from a provider response body. */
    external fun parseProviderError(raw: String): String

    /** Extracts the assistant message from the CenAI Cloud response envelope. */
    external fun parseCloudResponse(raw: String): String

    /** Validates and builds Supabase auth request payloads in Rust. */
    external fun buildAuthPayload(action: String, email: String, password: String, refreshToken: String): String

    /** Validates and decodes an authenticated Supabase session in Rust. */
    external fun parseAuthSession(rawJson: String): String

    /** Returns OK or the validation error for a six-digit email OTP. */
    external fun validateEmailOtp(code: String): String

    /** Validates the Google OAuth client ID and callback state in Rust. */
    external fun validateGoogleOAuthClientId(clientId: String): String
    external fun validateGoogleOAuthCallback(scheme: String, path: String, returnedState: String, expectedState: String, error: String, code: String): String
    /** Parses OAuth token responses and computes expiry timestamps in Rust. */
    external fun parseGoogleOAuthToken(rawJson: String, nowMs: Long): String
    /** Generates PKCE verifier, state and SHA-256 challenge in Rust. */
    external fun generateOAuthPkce(): String

    /** Replaces embedded image bytes with a retrieval-safe marker. */
    external fun stripImageMarkers(content: String): String

    /** Detects valid embedded image payloads in a message. */
    external fun containsImageMarker(content: String): Boolean

    /** Builds a safe provider endpoint and header set in Rust. */
    external fun buildProviderRequestConfig(provider: String, baseUrlOrModel: String, apiKey: String, oauth: Boolean, projectId: String, customHeadersJson: String): String

    /** Selects the provider set for single-provider or multi-agent mode. */
    external fun selectProviderTargets(primaryProvider: String, multiAgent: Boolean, localModelEnabled: Boolean, configuredProvidersJson: String): String

    /** Approximate 1-token-per-4-characters hint; never use for billing or hard limits. */
    external fun estimatePromptTokens(input: String): Int

    /** Checks Unicode code-point length against a caller-supplied character limit. */
    external fun isPromptWithinLimit(input: String, maxChars: Int): Boolean

    /** Owns privacy-safe feature event mapping and monitor aggregation. */
    external fun monitorEventKey(event: String): String
    external fun monitorSuccessRate(succeeded: Int, failed: Int): Int
    external fun monitorAverageLatency(totalMs: Long, completed: Int): Long
    external fun monitorFailureCategory(message: String, className: String): String
}
