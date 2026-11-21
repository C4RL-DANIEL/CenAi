# CenAI Rust-native rewrite

## Important product and architecture distinctions

These decisions are binding. Do not collapse them into one generic "AI app" architecture:

1. **Pure-Rust replacement vs. legacy rollback app.** The replacement in `:app-rust` is a Rust-owned application: screens, UI state, rendering, interaction, orchestration and feature logic live in Rust. Android NativeActivity is only the lifecycle/package host. No Kotlin/Compose UI and no WebView are part of the replacement. The old Kotlin app in `:app` is retained separately as a temporary rollback/reference only; its presence does not make the new app hybrid and it must not be used as the implementation shortcut.
2. **Device-first data vs. optional cloud services.** Conversations, prompts, attachments, projects, files, personal memory, settings and provider credentials are local-first and account/device scoped. Do not upload or sync user content by default. Cloud services are limited to functions that need them (identity, entitlements, optional AI routing, explicitly requested sync/backup and consented diagnostics). Send only the minimum context required for the operation. The user must be able to see and control cloud data flows.
3. **CenAI identity vs. provider authorization.** A CenAI login proves identity to CenAI; it does not grant API access to OpenAI, Anthropic, Gemini or DeepSeek. Registration is through the authorized Discord onboarding path only; the app signs in existing eligible accounts and must not expose open self-registration. Each provider connection is a separate authorization: a user-supplied API key, an officially supported OAuth flow, or a CenAI-managed entitlement that the backend verifies. A consumer web login/subscription is not an API key or API entitlement. Never scrape browser sessions, extract cookies, or impersonate consumer sessions to obtain provider access.
4. **Private Ecosystem Engine vs. public integration surface.** The public documentation and public integration contract intentionally reveal only the small, roughly 10% surface needed for approved integrations. The proprietary orchestration, internal service topology and implementation details (the remaining roughly 90%) stay private behind authenticated, encrypted service boundaries. This is not security by obscurity: every request must still be authenticated and authorized server-side, use scoped per-user capabilities, signed manifests, least privilege, audit logging and revocation. Never publish internal secrets, private endpoints, signing material or implementation internals merely to make an integration work.
5. **AI routing vs. user data ownership.** Cloudflare/CenAI AI routing and paid model entitlements may be available as managed routes, while BYOK connections call the provider's official API. Routing a request through a managed model does not transfer ownership of the user's workspace or authorize blanket data collection. Provider/model selection and the exact context sent must remain explicit and permission-aware.

## Non-negotiable migration rule

The replacement application UI, state, networking orchestration and feature logic are Rust-owned. Android's NativeActivity is only the lifecycle host. No Compose UI or WebView is used by `:app-rust`. The existing Kotlin application remains in `:app` as a rollback/reference until feature parity, device testing and release verification are complete.

## Central Brain: shared engine, isolated context

The Central Brain is one shared intelligence service, **not one shared memory pool**.

- Shared across the service: planner/orchestrator implementation, model routing, approved capability catalogue, tool/MCP protocol implementation, policy engine and managed compute infrastructure.
- Isolated by immutable Supabase user UUID: personal memory, conversations, files, projects, workspace indexes, provider credentials, connected accounts, settings, permissions, billing entitlements, diagnostics and automation/task ownership.
- Isolated by feature namespace: feature-specific instructions, workspace data and feature memory. Cross-feature access requires an explicit user-approved grant for that user.
- Isolated by session: temporary task state, conversation context, tool outputs and scratch data are usable only by the exact owning feature/session.
- No cross-user retrieval is ever allowed, even when a user has granted cross-feature access. That grant can never widen access to another account.
- Every Brain request binds to the immutable user UUID returned by Supabase and checks it against the active authenticated session before context is assembled or sent to managed inference.
- Context retrieval must enforce the owner/feature/session predicates at the data-access layer, including database queries, vector indexes, cache keys and remote requests. Filtering after retrieval alone is insufficient.
- Personal memory is included only when the user's memory setting and request-level permission allow it. Model output cannot grant permissions or broaden context.
- Send only the minimum authorized excerpts needed for the task; never ship a whole account history by default. Deletion must also remove indexed/embedded copies and cached derivatives.

The Rust rewrite adds a context authorization gate with tests for cross-user denial, cross-feature grants, per-session isolation, explicit personal-memory access and malformed foreign records. This gate is a client-side defense-in-depth layer; Cloudflare/Supabase services must enforce the same tenant predicates server-side.

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
- Existing-account sign-in against the existing Supabase/CenAI identity system; no open in-app registration. Any account creation must originate from the authorized Discord onboarding path. Email OTP is an existing-account verification step, not a substitute registration path.
- Supabase token refresh and account-scoped API requests.
- CenAI Cloud chat endpoint integration for explicitly routed managed requests; local workspace content remains local unless the user sends selected context.
- OpenAI-compatible and DeepSeek BYOK streaming, with API keys encrypted through Android Keystore. Provider authorization remains separate from CenAI account authentication.
- Per-account project creation, archived-project state, artifact metadata and local artifact storage.
- Central Brain plan-first flow, agent roles, dependency validation and stop semantics.
- MCP HTTPS/protocol/publisher/license/capability checks and explicit approval for sensitive reads and external writes.
- Guardian diagnostic redaction, deduplication, bounded buffering and consent gating.
- Private ecosystem manifest and per-user authorization contract; public docs expose only the approved integration surface while private orchestration stays behind authenticated/encrypted boundaries.
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
- Never treat CenAI sign-in, Discord identity, a consumer AI subscription, or a browser session as proof of API authorization to a model provider.
- Keep private Ecosystem Engine internals private, but never rely on obscurity: enforce server-side authentication, authorization, signed manifests, scoped capabilities, audit and revocation.
- Keep user content and workspace data on-device by default; cloud sync, backup, analytics and diagnostics require explicit user intent/consent.
- Use only the Supabase publishable key in the client; enforce account and capability authorization server-side.
- Credentials are encrypted with Android Keystore; app-private data remains account-scoped.
- Do not transmit prompts, conversation text, filenames, file content or credentials in Guardian telemetry.
- Sensitive reads, external writes, purchases, account changes and destructive operations require explicit approval.
- Never execute an AI-generated plan just because it parsed successfully.
- Do not remove the old Kotlin app until Rust parity and release tests pass.
- Do not describe the app as release-ready or provide an APK link unless the exact artifact exists and the install/runtime tests have passed.
