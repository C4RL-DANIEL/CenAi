# CenAI Rust-native rewrite

## Non-negotiable migration rule

The replacement application UI, state, networking orchestration and feature logic are Rust-owned. Android's NativeActivity is only the lifecycle host. No Compose UI or WebView is used by `:app-rust`. The existing Kotlin application remains in `:app` as a rollback/reference until feature parity, device testing and release verification are complete.

## Current architecture

- `native/`: Rust application runtime and shared service/domain code.
- `app-rust/`: Android NativeActivity packaging module; packages `libcenai_native.so`.
- `winit 0.30`: Android native window, lifecycle and input.
- `wgpu 24`: GPU rendering and animated ambient shader.
- `egui 0.31`: Rust-owned custom screen composition, controls, text input and Markdown/code display.
- `native/src/android/vault.rs`: AES-GCM secrets encrypted with an Android Keystore-managed key; the encrypted records live in the app-private directory.
- `native/src/core/network.rs`: Supabase auth, refresh, control-plane, billing, telemetry, CenAI Cloud, provider streaming and email OTP.
- `native/src/core/workspace.rs`: per-account workspace metadata, conversation snapshots and path-safe artifacts.
- `native/src/core/brain.rs`: Central Brain plan generation; every returned plan is validated before any action.
- `native/src/core/mcp.rs`: MCP manifest validation and security review; discovery is not permission.
- `native/src/core/guardian.rs`: bounded, sanitized diagnostics queue; upload is disabled until consent is granted.
- `native/src/ecosystem/manifest.rs`: trusted first-party registration and per-user capability grants.

## Implemented in this branch

- Four primary destinations: Conversation, Your Work, Connections and Settings.
- Responsive centered content, animated GPU ambient gradient, custom dark surfaces, custom composer and button states.
- Conversation message model, Markdown headings/lists/code fences, local per-account conversation autosave.
- Existing-account Supabase sign-in and sign-out; no in-app registration flow. Email OTP verification is wired to the existing CenAI worker endpoint.
- Supabase token refresh and account-scoped API requests.
- CenAI Cloud chat endpoint integration.
- OpenAI-compatible and DeepSeek BYOK streaming, with API keys encrypted through Android Keystore.
- Per-account project creation, archived-project state, artifact metadata and local artifact storage.
- Central Brain plan-first flow, agent roles, dependency validation and stop semantics.
- MCP HTTPS/protocol/publisher/license/capability checks and explicit approval for sensitive reads and external writes.
- Guardian diagnostic redaction, deduplication, bounded buffering and consent gating.
- Private ecosystem manifest and per-user authorization contract.
- Rust workspace formatting, unit-test and Clippy CI; Android arm64 debug APK packaging workflow.

## Still required before release

- Verify latest Rust host tests and Clippy on the current branch head.
- Verify Android arm64 cross-compilation, Gradle packaging, APK install and NativeActivity startup.
- Test the actual Android Keystore path, login/refresh/logout, email OTP and provider requests on device.
- Add the remaining provider adapters and model catalog, MFA, subscription status/checkout UI, billing lifecycle and entitlement enforcement.
- Finish project artifact UI, workspace backup/sync and cross-device restore.
- Implement real MCP registry discovery, remote MCP transport, sandbox execution and isolated coding-agent workflows.
- Implement voice input/output, multimodal attachment picker and processing, computer-use approval flow.
- Connect Guardian to real lifecycle/performance/network events and user-consented telemetry.
- Complete Ecosystem Engine service authentication, server-side registration, revocation, compatibility and audit integration.
- Run API 35+ emulator and REDMI Pad 2 Pro physical-device checks; record animation smoothness, lifecycle recovery, battery, memory, network failure and accessibility results.
- Produce release APK, SHA-256 checksum, commit identifier, test report and known-limitations report.

## Security and release rules

- Never place Supabase service-role secrets or provider API keys in the APK.
- Use only the Supabase publishable key in the client; enforce account and capability authorization server-side.
- Credentials are encrypted with Android Keystore; app-private data remains account-scoped.
- Do not transmit prompts, conversation text, filenames, file content or credentials in Guardian telemetry.
- Sensitive reads, external writes, purchases, account changes and destructive operations require explicit approval.
- Never execute an AI-generated plan just because it parsed successfully.
- Do not remove the old Kotlin app until Rust parity and release tests pass.
- Do not describe the app as release-ready or provide an APK link unless the exact artifact exists and the install/runtime tests have passed.
