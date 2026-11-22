# CenAI Rust Migration Status

This is the handoff record for the ongoing migration. It is not a completion claim.

## Rust core implemented

- `agent.rs`: tool catalog and JSON schemas, allowlist/approval policy, structured tool-call parsing, assistant-turn classification, bounded agent-step policy, continuation prompts, tool-guide generation, and safe local tool execution.
- `auth.rs`: auth payload/session validation, email OTP validation, Google OAuth callback/PKCE/token parsing, access-token freshness policy, and constant-time state verification.
- `context.rs`: bounded assembly of system instructions, account memory, local retrieval and tool guidance.
- `knowledge.rs`: local retrieval and ranking over only the active account's supplied conversation context.
- `providers.rs`: canonical provider registry/defaults, credential-source and model/endpoint resolution policies, request payloads, endpoint/header policy, SSE parsing, cloud response/error parsing, image marker handling, multi-agent provider selection and synthesis prompt construction.
- `storage.rs`: conversation title generation, schema validation, sanitation, sorting and normalization.
- `monitor.rs`: privacy-safe feature event mapping, success-rate/latency aggregation, and failure-category extraction.
- `updates.rs`: release-version ordering including prerelease and build metadata.
- `usage.rs`: account-scoped daily quota reset, limit enforcement and counter progression.
- `telemetry.rs`: consent, usage and Guardian payload construction with bounded metrics/error categories.
- `lib.rs`: JNI bridge. All Kotlin `NativeCore.external fun` declarations have matching Rust JNI exports; the bridge was statically checked for missing/orphan symbols.

## Lightning local model

The existing CenAI Device settings already identified `litert-community/Qwen3-0.6B` INT8 as the intended LiteRT-LM model. The Android workflow now downloads the pinned `Qwen3-0.6B.litertlm` artifact from revision `e6687177e00897731e7716267eebd4eba0dde7c2`, verifies its SHA-256 and exact byte size, caches it, and places it at `app/src/main/assets/models/lightning.litertlm` before Gradle builds the APK.

- Model file size: 613,722,264 bytes
- SHA-256: `c5f39b4191926786c3e6668b62af4d6eb15248dfc7a5348252a9b406f33f2108`
- License: Apache-2.0; license text and attribution notice are included in APK assets.
- Runtime: LiteRT-LM Android 0.18.1, CPU backend. The asset is copied into app-private storage and is not uploaded.

The model is bundled at build time rather than committed as a 614 MB binary to Git. Release CI refuses to publish if the verified model asset is absent.

## Android platform adapters still in Kotlin

Kotlin remains for Android-only responsibilities and UI/presentation: Compose rendering and some screen state, Activity/lifecycle/permissions, Android Keystore, OkHttp transport, WorkManager/foreground notifications, Android intents, Google OAuth browser/token transport, and the LiteRT-LM Android runtime wrapper. The model inference runtime itself is currently LiteRT-LM, not a Rust-native inference implementation.

Conversation history is encrypted at rest with Android Keystore and namespaced by account. Provider credentials, OAuth tokens, prompts, custom headers/parameters, agent memory, scheduled task prompts/results and account preferences are account-scoped. A guarded one-time migration moves legacy global settings/credentials/history only when the stored Supabase identity matches the active account; guest mode and different accounts do not inherit legacy data.

## Background execution limits

Scheduled agent work uses WorkManager persistence and a foreground notification while executing. CenAI requests notification permission on Android 13+ so active work can be visible. This improves execution visibility/resilience but does not bypass Android Doze, force-stop, OEM battery policies, foreground-service time limits or reboot restrictions. Uninterrupted 24/7 execution cannot be guaranteed.

## Remaining work before completion

1. Finish migrating remaining app/business logic and determine whether the Compose presentation layer is also to be replaced by a Rust-native renderer. The repository still contains 27 Kotlin source files; this is not yet a complete Pure Rust rewrite.
2. Complete offline model runtime validation on an Android emulator and the target Redmi Pad 2 Pro, including startup, streaming, prompt limits, memory pressure and repeated inference.
3. Complete feature-by-feature integration and regression tests; current CI smoke tests do not prove every feature.
4. Configure a stable release signing keystore using GitHub Actions secrets `CENAI_KEYSTORE_BASE64`, `CENAI_KEYSTORE_PASSWORD`, `CENAI_KEY_ALIAS` and `CENAI_KEY_PASSWORD`. Never put signing secrets in source control or chat.
5. Produce a signed release APK, verify its signature and SHA-256, and publish that exact artifact.

Never claim a queued/in-progress run passed. Never call an unsigned debug APK the finished app.
