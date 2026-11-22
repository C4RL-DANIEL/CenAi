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

- The new Rust-native Android workflow must complete successfully on the current branch head.
- The LiteRT-LM instrumentation test must pass on the API 35 emulator.
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
- Rust tool definitions and parsing validate bounded structured tool calls.
- Safe local tools (date/time, arithmetic, text statistics, JSON formatting, UUID and Base64) execute from an app-owned allow-list.
- Chat orchestration runs up to four safe tool calls per request and returns results to the model for a final response.
- Sensitive/destructive tools never execute from model output; they remain blocked until an explicit per-action approval and platform executor exist.
- MCP discovery validates schemas and security metadata but never implicitly approves or executes a newly discovered capability.
- Per-user context and sandbox request policies are implemented in Rust.

### Still required

- Durable execution of approved multi-step task plans, including pause/resume/cancel and restart recovery.
- A real approval UI and one-time capability grant binding for sensitive device/account actions.
- Sandboxed tool execution, resource quotas and failure recovery integrated with the task runner.
- End-to-end tests proving model-generated tool calls cannot bypass allow-lists, owner scopes or approval gates.

## Global constraints

- Keep the existing `main` branch unchanged during migration.
- Do not expose Lightning as a configurable provider or upload prompts/model weights.
- Do not silently claim 24/7 execution; Android Doze, force-stop, OEM battery management and service limits still apply.
- Never claim an APK is release-ready until it is signed, signature-verified, installed and smoke-tested.
