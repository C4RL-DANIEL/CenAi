# CenAI Pure Rust Migration — Phase Gates

This document tracks implementation and verification. A phase is complete only when its code is integrated, its tests pass on the current rewrite branch, and its stated acceptance criteria are satisfied.

## Eight phases

1. **Lightning offline AI** — fixed bundled Qwen3-0.6B INT8 model, verified private installation, LiteRT-LM CPU runtime, Rust-owned policy/invocation, offline chat integration and emulator/device inference tests.
2. **Pure Rust application** — migrate remaining app/business logic and remove obsolete Kotlin application implementations while retaining only strictly necessary Android platform bindings.
3. **AI brain and autonomous agents** — planning, tool execution, scoped memory, approval gates, secure sandbox and failure recovery.
4. **Ecosystem Engine** — private per-user app contexts, capability registration, scoped grants, revocation and cross-app integration.
5. **Android services** — foreground execution, persistent scheduling, reboot recovery, lifecycle and battery-aware operation within Android's actual limits.
6. **Voice and multimodal** — voice input/output, custom synthesis, image/document understanding and multimodal workflows.
7. **Integration and device testing** — feature parity, security, offline operation, resource use, long-running tests and Redmi Pad 2 Pro validation.
8. **Production release** — stable signing, reproducible build, APK signature/checksum verification, installation and release publication.

## Phase 1 — Lightning offline AI

### Fixed upstream artifact

- Model: `litert-community/Qwen3-0.6B`
- Revision: `e6687177e00897731e7716267eebd4eba0dde7c2`
- File: `Qwen3-0.6B.litertlm`
- Size: 613,722,264 bytes
- SHA-256: `c5f39b4191926786c3e6668b62af4d6eb15248dfc7a5348252a9b406f33f2108`
- Runtime adapter: LiteRT-LM Android 0.18.1
- Backend: CPU
- License: Apache-2.0

### Implemented on `rewrite/rust-native-core`

- Rust model contract pins the exact model ID, revision, asset path, size, digest, runtime and license.
- Rust prompt validation and model metadata tests added.
- Android build downloads the exact immutable upstream revision, verifies SHA-256 and byte size, and caches the artifact rather than committing a 614 MB binary to Git.
- APK Gradle configuration packages `.litertlm` uncompressed and includes the LiteRT-LM runtime dependency.
- Android runtime adapter installs the bundled model into app-private storage atomically, verifies the copied bytes, initializes the CPU engine, and performs local text generation without a network client.
- Instrumentation test invokes the bundled runtime and asserts non-empty local generation.
- APK CI checks that the model asset and license are present in the packaged APK.

### Not yet verified / still required

- The API 35 emulator workflow completed successfully on commit `0eb72f63b8a58031c06d1093da087590dad7a493`: the APK asset checks verified the exact 613,722,264-byte model and SHA-256, the NativeActivity launch check passed, and the workflow's network-disabled `connectedDebugAndroidTest` step exited successfully.
- The current Android adapter changes include atomic replacement of the verified model file and cleanup of partial user-selected file imports.
- Re-run that complete Android workflow on the current branch head; the current head still needs a clean build and instrumentation pass.
- Rust chat orchestration routes the no-account offline path, default cloud-chat failures, and configured provider failures to the Android LiteRT-LM adapter on a blocking worker; up to eight recent user/assistant turns are forwarded within a bounded prompt size.
- The dedicated Android workflow no longer requests Google's retired SDK `tools` package; it installs platform-tools and pins the required platform/build-tools/NDK packages directly.
- Verify generation on the user's Redmi Pad 2 Pro, including memory use, first-run copy time, cold start, repeat prompts and thermal behavior.
- The current model is about 585 MiB; the final APK will be correspondingly large. Verify installation/storage headroom on target hardware.

Do not label Phase 1 complete until the runtime is wired into the Rust conversation flow and both emulator and target-device inference have been verified.

## Phase 2 — Pure Rust application migration

### Current migration audit

The production entry point in `app-rust` is Android `NativeActivity`; the rendered screens, conversation state, networking, encrypted workspace, permissions, ecosystem registry, agent plans and diagnostics are implemented in Rust. The remaining Java/Kotlin files in `app-rust` are intended to be narrow Android platform/runtime adapters.

The original `:app` module is intentionally retained during parity work and is not packaged by the Rust-native APK task. It must not be deleted until each capability below has a verified Rust replacement.

| Legacy Kotlin capability | Rust-native replacement / status |
|---|---|
| `CenAiScreen`, theme, rich chat UI | `render/wgpu_renderer.rs`, `render/scene.rs`, `ui/screen.rs`; feature and visual parity still needs device review |
| `ChatRepository`, `CenAiCloud`, provider streaming | `core/network.rs`, `core/providers.rs`; provider adapters and auth paths still require integration tests |
| `ChatStore`, local knowledge, project data | `core/workspace.rs`, `core/context.rs`; encrypted account-scoped persistence exists |
| `LocalModelRuntime` | Fixed Lightning bridge in `app-rust`; actual model inference remains pending emulator/device proof |
| `AgentRuntime` safe local tools | `core/tools.rs`, wired into chat with a four-call cap; sensitive Android actions remain approval-gated and are not auto-executed |
| `AgentTaskWorker`, `GuardianWorker` | Rust automation/Guardian state exists; Android scheduled-work adapter and restart/recovery parity are still outstanding |
| `GuardianMonitor`, `AppMonitor` | `core/guardian.rs` covers privacy-safe diagnostics and system metric parsing; platform sampling and aggregate feature metrics need parity work |
| `DeviceActionManager` | Android JNI/platform adapter is partial; dialer, share sheet and app-settings actions remain to port with approval gates |
| `GoogleGeminiOAuth` | Not yet parity-verified in Rust; preserve PKCE, state validation, secure token storage and refresh semantics before removing Kotlin implementation |
| `UpdateManager` | Not yet parity-verified in Rust; signed digest verification and safe install flow remain outstanding |
| `SecureKeyStore` | Android Keystore-backed Rust vault adapter exists in `native/src/android/vault.rs`; needs device migration/restore tests |
| backup/import and attachments | `core/backup.rs`, `core/multimodal.rs`, Android file-picker bridge; format and end-to-end import/export parity still require tests |

Do not remove `:app` or its source files until the parity audit is complete and migration tests demonstrate that existing user data and account flows remain usable.

## Phase 3 — AI brain and autonomous agents

### Implemented foundations

- Rust task plans enforce unique IDs, bounded steps, valid dependencies and explicit approval states.
- Per-action approvals are bound to the exact operation risk class and step description, consumed by one execution attempt, and invalidated after failure/retry; changing either also requires a new review.
- Rust tool definitions and parsing validate bounded structured tool calls.
- Safe local tools (date/time, arithmetic, text statistics, JSON formatting, UUID and Base64) execute from an app-owned allow-list.
- Chat orchestration runs up to four safe tool calls per request and returns results to the model for a final response.
- Sensitive/destructive tools never execute from model output; they remain blocked until an explicit per-action approval and platform executor exist.
- The four supported Android side effects (open URL, open dialer, share text, open app settings) now require a native review dialog and one-time approval; Rust and Android both validate arguments before dispatch, and dialer uses `ACTION_DIAL` only.
- MCP discovery validates schemas and security metadata but never implicitly approves or executes a newly discovered capability.
- Per-user context and sandbox request policies are implemented in Rust.

### Still required

- Durable execution of approved multi-step task plans, including pause/resume/cancel and restart recovery.
- A real approval UI and one-time capability grant binding for sensitive device/account actions.
- Sandboxed tool execution, resource quotas and failure recovery integrated with the task runner.
- End-to-end tests proving model-generated tool calls cannot bypass allow-lists, owner scopes or approval gates.

## Phase 4 — Ecosystem Engine

### Implemented foundations

- Rust manifest validation pins the protocol version, publisher identity, app identity, declared capabilities and requested scopes.
- The capability registry binds each app ID to its trusted publisher and rejects publisher takeovers.
- Grants are keyed to an authenticated user UUID, limited to manifest-requested scopes, isolated between users and revocable.
- Supabase stores the shared trusted-publisher/app catalog and per-user grants; the `cenai-ecosystem` Edge Function enforces MFA step-up for registration/grant/revoke and checks declared/requested scopes on authorization.
- Rust workspace storage now persists a second account-scoped registry copy through the encrypted workspace adapter, validates it on load, and supports atomic scoped grant/revoke updates.
- The native UI exposes per-app scope review, grant and revoke actions through the account API.
- Rust tests cover trusted registration, grant creation, undeclared-scope rejection, account isolation, persistence and revocation.

### Still required

- A versioned app-to-CenAI request/response transport with app authentication, replay protection, bounded payloads, delivery semantics and revocation checks at message time.
- An external sample app and SDK/client contract that can obtain a user-approved app credential and exercise the transport end to end.
- End-to-end tests using two accounts and multiple registered apps, including revocation during active sessions.
- End-to-end tests using two accounts and multiple registered apps, including revocation during active sessions.
- A real external sample app that integrates through the published protocol rather than a UI-only catalog record.

## Phase 5 — Android services

### Implemented foundations

- Rust automation definitions use owner UUIDs, validated UTC cron schedules, persisted cursors, pause/resume and approval-required due occurrences.
- A narrow Android WorkManager adapter schedules durable one-shot reminders for the next occurrence; notifications are generic and contain no task title or private prompt.
- Reminder notifications only open CenAI for review. They never execute the automation or any external action.
- Guardian diagnostics and procfs metric parsers are implemented in Rust and sampled while the native app is active.
- Background task and Guardian policy is privacy-first; scheduled work is not allowed to silently execute external or destructive actions.

### Still required

- A WorkManager worker that wakes the Rust runtime to process a due occurrence and persist its approval request while the app is not running; the current worker is reminder-only.
- A full user-visible approval screen for scheduled actions, including notification deep-link routing to the exact pending task.
- Reboot recovery, rescheduling of recurring schedules when the user does not open the reminder, and retry semantics for permission-denied notifications.
- Lifecycle, memory, battery and network constraints tested on Android 15/16 and Xiaomi HyperOS.
- No implementation can promise uninterrupted 24/7 execution: force-stop, Doze, OEM restrictions and Android background-start rules remain authoritative.

## Phase 6 — Voice and multimodal

### Implemented foundations

- Rust voice session state, PCM validation, audio buffering, WAV encoding and voice activity handling.
- Android AudioRecord/AudioTrack integration for microphone capture and playback.
- Rust attachment metadata validation, content signatures, size limits, integrity hashes and encrypted account-scoped storage.
- Rich-content, document and media request contracts plus the configured CenAI cloud speech synthesis profile.

### Still required

- End-to-end permission, capture, transcription, synthesis and playback tests on physical devices.
- Explicit disclosure and controls for cloud transcription/synthesis; those services are not offline.
- Verified image/document understanding and extraction across supported formats, including malformed and oversized inputs.
- Accessibility, interruption, audio-route and battery/thermal tests on the target tablet.

## Phase 7 — Integration and device testing

### Current evidence

- Rust host CI passed formatting, unit tests and Clippy with 120 tests on commit `73cd9f25749f105bf0019438459e89316177c74f`; the subsequent Android adapter-only changes are being rechecked on the current head.
- Android Rust cross-compilation, NativeActivity launch and network-disabled model instrumentation workflow completed successfully on commit `0eb72f63b8a58031c06d1093da087590dad7a493`; repeat on the latest head before release.
- The pinned model download, APK asset inclusion, exact byte-size and SHA-256 verification have passed in CI.

### Still required

- A clean current-head Android APK build, package-content verification and API 35 emulator launch.
- A passing instrumentation test that loads the packaged model and generates a non-empty response.
- Offline-mode test with network unavailable, plus repeated prompts and app process restart.
- Redmi Pad 2 Pro / HyperOS 3 validation for first-run model installation, RAM pressure, cold-start latency, thermal behavior, permissions and background restrictions.

## Phase 8 — Production release

- The release workflow is configured to require a stable keystore and four GitHub Actions secrets: CENAI_KEYSTORE_BASE64, CENAI_KEYSTORE_PASSWORD, CENAI_KEY_ALIAS and CENAI_KEY_PASSWORD.
- The repository currently does not have those signing secrets configured. Branch CI reports this as a warning; a tagged production release remains blocked until the owner supplies the stable signing key.
- Never generate a new signing key for every release: the same private key must be preserved to sign future updates to the installed application.
- Still required: successful signed build, APK signature verification, checksum verification, clean installation/upgrade testing and publication of the signed APK.
## Global constraints

- Keep the existing `main` branch unchanged during migration.
- Do not expose Lightning as a configurable provider or upload prompts/model weights.
- Do not silently claim 24/7 execution; Android Doze, force-stop, OEM battery management and service limits still apply.
- Never claim an APK is release-ready until it is signed, signature-verified, installed and smoke-tested.
