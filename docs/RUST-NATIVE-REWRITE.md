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

The Rust rewrite adds a context authorization gate with tests for cross-user denial, cross-feature grants, per-session isolation, explicit personal-memory access and malformed foreign records. It accepts only canonical Supabase UUIDs. This gate is a client-side defense-in-depth layer; Cloudflare/Supabase services must enforce the same tenant predicates server-side. **Important current limitation:** the gate authorizes records supplied to it; it is not yet a durable personal-memory database or retrieval/indexing service. The current planner uses a bounded recent conversation context, not an account-wide memory search.

## Current architecture

- `native/`: Rust application runtime and shared service/domain code.
- `app-rust/`: Android NativeActivity packaging module; packages `libcenai_native.so`.
- `winit 0.30`: Android native window, lifecycle and input.
- `wgpu 24`: GPU rendering and animated ambient shader.
- `egui 0.31`: Rust-owned custom screen composition, controls, text input and Markdown/code display.
- `native/src/android/vault.rs`: AES-GCM secrets encrypted with an Android Keystore-managed key; the encrypted records live in the app-private directory.
- `native/src/core/network.rs`: Supabase auth, serialized session mutation/refresh, account-bound authenticated requests, control-plane, billing, telemetry, CenAI Cloud streaming and provider streaming/email OTP.
- `native/src/core/workspace.rs`: per-account workspace metadata, conversation snapshots and path-safe artifacts. Data is stored in Android app-private storage and partitioned by a hash of the immutable user UUID; the current workspace files are not separately encrypted with a per-account key.
- `native/src/core/brain.rs`: Central Brain plan generation; every returned plan is validated before any action.
- `native/src/core/mcp.rs`: MCP manifest validation and security review; discovery is not permission.
- `native/src/core/guardian.rs`: bounded local diagnostics and a separate consent-gated upload queue; pre-consent events are never uploaded retroactively and revoking consent clears pending uploads.
- `native/src/ecosystem/manifest.rs`: trusted first-party registration and per-user capability grants.

## Implemented in this branch (source-level; not release-verified)

- Four primary Rust-owned destinations: Conversation, Your Work, Connections and Settings, with WGPU/egui rendering, animated GPU background, responsive layouts and account-scoped appearance preferences.
- Supabase existing-account sign-in, token refresh, email OTP, TOTP MFA, registration-policy checks and account-generation guards against stale asynchronous results.
- Managed CenAI Cloud chat client and Workers AI quota reservation/recording source; BYOK adapters currently cover OpenAI-compatible, DeepSeek, Anthropic and Gemini paths. The model catalogue and model-selection experience remain limited.
- Explicit per-account Offline AI preference: when enabled, new chat prompts route directly to the bundled Lightning runtime and skip cloud/provider calls; without an authenticated cloud session, local Lightning is the only chat route. The preference is saved through the authenticated user-settings service.
- Per-account encrypted workspace persistence using the Android Keystore-backed vault, atomic metadata writes, encrypted attachment storage, safe artifact IDs and SHA-256 validation.
- Project creation, archive state, artifact save/open/delete and task-plan persistence in the local workspace.
- Passphrase-protected Argon2id/AES-256-GCM backup export/import, plus authenticated encrypted backup upload/download to the account's private Supabase Storage object. This is backup/restore, not continuous cross-device synchronization.
- MCP HTTPS client with endpoint/DNS safety checks, initialization, remote `tools/list`, schema validation, remote `tools/call`, per-account token storage and explicit approval for sensitive reads or external writes. Server connection is manually configured; there is no public registry discovery service.
- Android AudioRecord microphone capture and AudioTrack playback adapters; push-to-talk recording is transcribed through the user's Gemini API key. Cloud speech synthesis uses Cloudflare Workers AI Deepgram Aura-1, with per-user character and shared daily Neuron reservations. The Cloudflare Worker and Supabase quota functions/migrations are deployed and the protected endpoints pass unauthenticated 401 smoke tests. Actual authenticated audio inference has not been run; `voice.cloud.enabled` remains false and the app disables Read aloud until the service reports ready.
- Attachment import and account-scoped encrypted storage with MIME/signature/size/checksum validation. The current Gemini analyzer supports plain text/Markdown/CSV, WAV transcription, PNG/JPEG/WebP images and PDF. Other document formats, video analysis and broad OCR/office-document extraction are not implemented in this path.
- Guardian local snapshots for CPU, RAM, network, storage and battery; sanitized/deduplicated local diagnostics; opt-in coarse telemetry with a revocation path.
- Ecosystem Engine account-scoped app catalog, manifest registration, capability grants, revocation and authorization backed by Supabase function/migration source and audit events.
- Android debug packaging for ARM64 and x86_64, Rust formatting/tests/Clippy CI, Worker/voice source checks and an emulator NativeActivity smoke-test script.

## Still required before release

- Complete the model/provider catalogue, per-model configuration and capability metadata; add subscription status, entitlement resolution and enforcement across managed AI, tools and premium features. Existing Stripe/PayPal checkout and webhook source is not proof that live entitlements are configured.
- Validate the explicit Offline AI preference and local-only network boundary on API 35 emulator and the REDMI Pad 2 Pro, including account switch/sign-out behavior and repeated inference.
- Finish project workflows beyond local CRUD: robust artifact type previews, versions, imports/exports, collaboration and recovery UX.
- Implement continuous encrypted cross-device workspace synchronization with conflict resolution, deletion propagation and offline queue/retry. Current upload/download is encrypted backup/restore only.
- Add MCP registry discovery and publisher verification/revocation at the service boundary. Build an actual remote sandbox executor in isolated per-user containers; current `sandbox.rs` validates requests and output policy but does not execute jobs.
- Implement full-duplex real-time voice, interruption/barge-in, streaming speech recognition and synthesis, language/provider selection, and session recovery. Current Android voice input is push-to-talk and Aura-1 TTS is a separate request/response path.
- Expand multimodal processing to office documents, scanned-PDF OCR, audio formats beyond WAV, video frame/audio extraction and provider-capability-aware routing. Unsupported formats must fail visibly rather than be represented as analyzed.
- Implement a permissioned Android computer-use service with user-visible screen capture, narrow action allowlists, per-action approval, emergency stop, audit history and revocation. No accessibility-driven computer-use executor is currently present.
- Connect Guardian to Android lifecycle, crash/ANR, background-task, network and render events with resource-aware sampling and consented reporting. Current metrics are best-effort snapshots, not a 24/7 monitor.
- Complete Ecosystem Engine production hardening: trusted publisher onboarding, signed manifest verification, version compatibility, per-user capability authorization, revocation propagation and audit review. The Supabase function and migration are deployed; authenticated registration/grant/revoke flows still need end-to-end tests.
- Complete authenticated end-to-end verification of the deployed Cloudflare Worker and Supabase Edge Functions/migrations, including email OTP delivery, invite redemption, backup upload/restore, MCP calls, billing webhooks, entitlement gating and storage policies. The Worker deployment and required bindings/secrets are present; Aura-1 is intentionally disabled until a signed-in audio inference test is completed.
- Pass Android emulator install/startup checks, then run the documented physical REDMI Pad 2 Pro / HyperOS 3 test matrix. Emulator success cannot substitute for physical-device validation.
- Produce a same-key signed release APK, checksum, provenance, test evidence and known-limitations report. A debug artifact or successful Gradle package is not a production release.

## Security and release rules

- Never place Supabase service-role secrets or provider API keys in the APK.
- Never treat CenAI sign-in, Discord identity, a consumer AI subscription, or a browser session as proof of API authorization to a model provider.
- Keep private Ecosystem Engine internals private, but never rely on obscurity: enforce server-side authentication, authorization, signed manifests, scoped capabilities, audit and revocation.
- Keep user content and workspace data on-device by default; cloud sync, backup, analytics and diagnostics require explicit user intent/consent.
- Use only the Supabase publishable key in the client; enforce account and capability authorization server-side.
- Provider credentials and session tokens are encrypted with Android Keystore. Workspace data is stored under Android app-private storage and partitioned by account UUID; separate per-account workspace encryption is still outstanding.
- Do not transmit prompts, conversation text, filenames, file content or credentials in Guardian telemetry.
- Sensitive reads, external writes, purchases, account changes and destructive operations require explicit approval.
- Never execute an AI-generated plan just because it parsed successfully.
- Do not remove the old Kotlin app until Rust parity and release tests pass.
- Do not describe the app as release-ready or provide an APK link unless the exact artifact exists and the install/runtime tests have passed.
