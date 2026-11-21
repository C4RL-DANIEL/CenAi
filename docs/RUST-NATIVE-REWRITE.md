# CenAI Rust-native rewrite

## Decision

CenAI is being migrated to a Rust-owned Android application. Rust owns the app runtime, UI composition, rendering, input, navigation, local persistence, networking adapters, agent execution boundary, and diagnostics. The Android deliverable must not use Jetpack Compose, Kotlin application logic, a WebView shell, or a hidden Kotlin UI with Rust underneath.

The existing Kotlin app remains available during migration as a reference and rollback baseline. It is not the target architecture. Existing Cloudflare/Supabase services, account isolation, billing authority, provider integrations, and user data contracts are preserved and adapted behind Rust interfaces rather than replaced blindly.

## Rendering architecture

- winit provides the Android window, lifecycle, and input event boundary.
- wgpu provides the GPU rendering abstraction, using Vulkan where available and a supported GLES fallback where required by device compatibility.
- A Rust-owned retained scene graph and component system provides layout, typography, controls, transitions, accessibility semantics, and responsive phone/tablet composition.
- GPU work is batched; text and vector assets are cached; animations are time-based and interruptible; frame pacing follows display refresh and Android lifecycle state.
- Reduced-motion, low-memory, and lower-power modes are first-class. No continuous GPU loop while the app is backgrounded.
- The visual language uses animated lighting on selected surfaces, layered depth, precise typography, fluid transitions, and restrained glow. Effects must not compromise legibility, thermals, battery, or touch latency.

## Rust workspace boundaries

- cenai-native: Android entry point, lifecycle, app state, and native runtime.
- render: GPU device/surface, render graph, scene graph, typography, transitions, and adaptive quality.
- ui: four primary destinations (Conversation, Your Work, Connections, Settings) and reusable original components.
- core: typed domain models, task planning, permissions, and orchestration contracts.
- providers: default Cloudflare AI, subscription entitlement, BYOK provider adapters, and streaming.
- workspace: per-user durable files, projects, autosave, checkpoints, and recovery.
- mcp: discovery metadata, manifest validation, scoped execution, and approvals.
- sandbox: isolated remote execution client; never run arbitrary untrusted code in the app process.
- voice: speech input and real-time voice transport adapters with platform permission boundaries.
- guardian: deterministic monitoring, privacy-safe telemetry, and bounded recovery.
- ecosystem: private first-party app identity, capability registry, service-to-service auth, event routing, version compatibility, audit, and revocation.

These boundaries may initially be implemented as modules in a small number of crates. They must not become separate user-facing hubs.

## Migration rules

1. Preserve existing backend contracts and account-scoped data boundaries.
2. Port one vertical slice at a time behind stable Rust traits; do not remove the Kotlin reference implementation until the Rust equivalent passes parity and release checks.
3. No hardcoded credentials, signing secrets, provider keys, or cross-user context in the APK.
4. All sensitive/destructive operations require explicit approval and server-side authorization.
5. The private ecosystem registry is not a public API. Hidden routes are not treated as security; service identity, scoped credentials, authorization, and audit are mandatory.
6. Keep the existing GitHub Actions build as the only user-facing APK build route. Add Rust Android checks and packaging there; do not require local Android builds.

## Release gates

- Rust formatting, linting, unit and integration tests.
- Android arm64 build and APK packaging through GitHub Actions.
- API 35+ emulator tests and physical Redmi Pad 2 Pro tests.
- GPU backend/device-loss fallback, frame pacing, animation interruption, touch responsiveness, and memory/thermal profiling.
- Account isolation, auth, billing entitlement, BYOK, workspace durability, MCP permission checks, sandbox isolation, voice permissions, Guardian consent, and ecosystem revocation tests.
- APK artifact, SHA-256, commit SHA, test report, and explicit known limitations.

## Current status

This commit establishes the Rust workspace and rendering direction only. It does not claim that the Android app has already been ported, that the renderer is complete, or that an APK has been built. Android runtime bootstrap, GPU surface, UI components, feature ports, and CI packaging remain implementation work.
