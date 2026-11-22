# CenAI Rust Migration Status

This file is the handoff record for the ongoing full Rust migration. It is not a completion claim.

## Rust core currently implemented

- \`agent.rs\`: Rust-owned tool catalog and JSON schemas, allowlist/approval policy, structured tool-call parsing, assistant-turn classification, bounded agent-step policy, tool-continuation prompts, tool-guide generation, and safe local tool execution.
- \`auth.rs\`: signup/sign-in/refresh payload validation, Supabase session decoding, and six-digit email OTP validation.
- \`context.rs\`: bounded assembly of system instructions, account memory, local retrieval and tool guidance.
- \`knowledge.rs\`: local retrieval/ranking over only the active account's supplied conversation context.
- \`providers.rs\`: provider payloads, endpoint/header policy, SSE delta parsing, cloud response parsing, error extraction, image marker parsing/detection, multi-agent provider selection and synthesis prompt construction.
- \`storage.rs\`: conversation title generation, schema validation, sanitation, sorting and normalization.
- \`lib.rs\`: JNI bridge. Every Kotlin \`NativeCore.external fun\` declaration currently has a matching Rust JNI export.

## Android platform adapters still in Kotlin

Kotlin remains for Android-only responsibilities and some work still to migrate: Compose UI and screen state, Activity/lifecycle/permissions, Android Keystore, OkHttp transport, WorkManager/foreground notifications, Android intents, Google OAuth browser/token transport, and the current LiteRT-LM Android runtime wrapper.

Conversation history is encrypted at rest with Android Keystore and namespaced by account. Provider credentials, OAuth tokens, prompts, custom headers/parameters, agent memory, scheduled task prompts/results and account preferences are account-scoped. A guarded one-time migration moves legacy global settings/credentials/history only when the stored Supabase identity matches the active account; guest mode and different accounts do not inherit legacy data.

Scheduled agent work uses a foreground notification while executing and WorkManager persistence. This improves execution visibility/resilience but does not bypass Android Doze, force-stop, OEM battery policies, FGS time limits or reboot restrictions. Uninterrupted 24/7 execution cannot be guaranteed.

## Current blockers to a complete release

1. No bundled local-model weight file is present in this repository tree. The user requirement is a private, hidden, fixed, non-replaceable and non-disableable local engine named Lightning; its exact dedicated duty, model identity, architecture, source and license were not specified. The existing LiteRT-LM runtime still exposes an import/enable UI that conflicts with this requirement and must not be shipped as the finished implementation. Do not invent a model or assign Lightning an unapproved duty.
2. Compose UI and local inference are not yet a full Rust-native implementation. Do not describe the project as a completed Pure Rust rewrite.
3. A stable release signing keystore is not configured. The release workflow expects repository Actions secrets \`CENAI_KEYSTORE_BASE64\`, \`CENAI_KEYSTORE_PASSWORD\`, \`CENAI_KEY_ALIAS\` and \`CENAI_KEY_PASSWORD\`. Never put signing secrets in source control or chat.
4. A debug APK or an APK from an earlier commit is not the requested finished release.

## Verification

Continue using the Android GitHub Actions workflow. Require all of: Rust unit tests, Rust JNI cross-build, Kotlin JVM tests, debug APK assembly, Android emulator instrumentation/smoke tests, and—after signing secrets and model asset are available—a signed release APK plus signature/hash verification.

Never claim a stage passed based only on a queued or in-progress run. Never call an unsigned debug APK the finished app.
