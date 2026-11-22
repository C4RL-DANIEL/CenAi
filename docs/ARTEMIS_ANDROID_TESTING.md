# ARTEMIS integration for CenAI Android validation

## Decision

Use [Google ARTEMIS](https://github.com/google/artemis) as an **external Android test and exploration harness** for CenAI. Do not embed ARTEMIS, its Python runtime, MCP server, LangGraph agents, ADB stack or accessibility helper into the CenAI APK.

CenAI remains a Rust-owned Android application. ARTEMIS runs on a trusted development/test host and drives a connected Android device or emulator from outside the app. This keeps the production APK small (apart from its required Lightning model), avoids adding a Python runtime to Android, and preserves the existing Rust/Android platform boundary.

ARTEMIS is Apache-2.0 licensed. Its upstream README describes a Python 3.12+ host application, Android ADB/device automation, a local web console, CLI, Python SDK and MCP server for coding agents. It advertises Flash and Pro execution profiles and reports 99%+ AndroidWorld task completion; that benchmark figure is an upstream project claim, not a CenAI test result or a guarantee for our app.

## Where it fits in the eight-phase plan

- **Phase 1 — Lightning:** ARTEMIS can explore the real app, navigate to offline chat, submit prompts, and collect screenshots/logs. Keep deterministic model integrity, network-disabled inference and output assertions in the existing Gradle/instrumentation CI; ARTEMIS is complementary, not a replacement.
- **Phase 2 — Pure Rust parity:** use it to inspect every screen and flow in the Rust-native APK and compare behavior against the legacy app. Record evidence for login, provider setup, chat, history, settings, attachments, backup/restore, OAuth callbacks and update flows.
- **Phase 3 — Agents:** test approval dialogs and denial paths. Never let the test agent approve a sensitive CenAI action merely to make a test pass; explicitly test that dismissal, denial, stale approval and retry do not execute the action.
- **Phase 4 — Ecosystem:** exercise app registration/grant/revoke UI and verify account isolation with controlled test accounts. Keep backend authorization and replay-protection tests in the API test suite.
- **Phase 5 — Android services:** inspect notifications, reminder deep links, reboot/relaunch recovery and permission-denied behavior. ARTEMIS can observe UI behavior but cannot prove Android will run a process continuously or defeat Doze/OEM restrictions.
- **Phases 6–8:** use it for voice permission/UI checks, multimodal attachment flows, upgrade/install smoke tests and release-candidate exploration. Audio quality, thermal/RAM metrics, cryptographic verification and signing remain covered by specialized tests.

## Recommended architecture

```
CenAI source + GitHub Actions
        |
        +--> Rust unit / security / property tests
        +--> Gradle instrumentation + deterministic emulator checks
        +--> APK/model hash + signature verification
        |
        +--> ARTEMIS test host (outside the APK)
                 |
                 +--> ADB-connected emulator or authorized tablet
                 +--> ARTEMIS CLI / web console / MCP server
                 +--> screenshots, UI hierarchy, Logcat, traces
                 |
                 +--> evidence linked to commit + APK SHA-256
```

### Preferred rollout

1. **Keep GitHub Actions as the build authority.** Continue producing the Rust-native APK and deterministic API 35 emulator results there.
2. **Start with a separate trusted test host.** ARTEMIS requires Python 3.12+, ADB and a connected Android device/emulator. Install it on a Linux/macOS/Windows development or test machine, not inside CenAI and not on the Redmi tablet. Use the upstream setup and inspect the scripts/dependencies before running the one-click installer.
3. **Connect the target device deliberately.** Prefer a local USB-connected test device or CI emulator. Wi-Fi ADB is possible only when explicitly enabled and secured. Do not expose ADB or ARTEMIS's daemon port to the public internet.
4. **Connect through MCP only on the trusted host.** ARTEMIS exposes tools such as `mobile_run_task`, `mobile_manage_task`, `mobile_get_device_state`, `mobile_inspect_trace` and `mobile_diagnose`. A coding agent can request a test, inspect the trace and report evidence. The MCP server is test infrastructure, not a runtime dependency of CenAI.
5. **Use Flash for straightforward exploration** (open a screen, navigate a deterministic flow, capture state). **Use Pro for complex flows** that need a persistent plan, checkpoints, diagnostics and a final report.
6. **Attach evidence to the exact build.** Store commit SHA, APK SHA-256, device model/API level, ARTEMIS trace ID, screenshots and sanitized Logcat with each run. Do not store access tokens, API keys, prompts or personal data in test artifacts.
7. **Promote stable discoveries into deterministic tests.** ARTEMIS exploration finds real selectors and transitions; then implement repeatable Gradle instrumentation/unit tests with explicit waits and assertions. Do not rely on an LLM-driven test as the only release gate.

## Security and privacy constraints

- ARTEMIS's upstream Python SDK documentation says the current daemon administration API does not provide built-in remote authentication. Keep it bound to loopback or behind an authenticated HTTPS reverse proxy; use an SSH tunnel for remote administration. Never publish its port directly.
- Use dedicated test accounts, disposable provider credentials and non-production data. Do not place secrets in natural-language tasks or traces.
- ARTEMIS may install an Accessibility Helper on the test device. Review the permission and visible notification with the device owner before enabling it. The upstream README says the helper reads the screen hierarchy locally and can fall back to UIAutomator2; treat the accessibility permission as a sensitive test-device capability regardless.
- Use a dedicated emulator or test tablet where possible. Do not automate personal apps, notifications, messages, payments or unrelated user data.
- Require human confirmation for any test that could submit, send, delete, purchase, call, share externally or change account security. Prefer mocked/test-only endpoints.
- Keep ARTEMIS's own model/provider credentials on the test host. They are separate from CenAI's in-app provider keys.
- Review upstream changes before upgrading ARTEMIS. Pin a commit or release in the test environment rather than tracking an unreviewed moving branch.

## CenAI test scenarios to add

### Phase 1: bundled Lightning
- Launch the current APK and confirm the local AI screen is reachable.
- Disable network connectivity at the emulator/device level.
- Submit a short prompt and verify a non-empty assistant response.
- Repeat the prompt, navigate away/back, and restart the app; verify the model remains usable.
- Capture first-run model-copy duration, cold-start time, memory pressure and any crash logs where available.
- Keep model SHA-256/size and network-disabled instrumentation assertions as mandatory deterministic CI gates.

### Phase 2: parity matrix
- First launch, onboarding, sign-in/sign-out and account switching.
- Provider setup, Gemini OAuth redirect/cancel/state mismatch/token refresh, and API-key persistence.
- New chat, streaming, edit/resend, regenerate, copy/share, search and delete history.
- Settings, theme, accessibility, language and privacy controls.
- Image/document attachment, PDF/OCR, voice permission and playback.
- Backup/export/import and migration from the legacy app.
- Update check, APK download, package installer handoff and upgrade install.
- Verify no legacy Kotlin application screen is launched by the Rust-native APK.

### Phase 3–5: agent and service safety
- Verify each sensitive action shows the exact operation and requires a fresh one-time approval.
- Deny or dismiss every approval dialog and verify no Android intent or external side effect occurs.
- Attempt stale/replayed approvals and verify rejection.
- Verify reminders open the exact pending item and never execute the action from the notification.
- Test process death/restart, reboot, denied notification permission and battery-restricted behavior.

## Current limitations and non-goals

- ARTEMIS is an Android automation/test tool, not a library that migrates Kotlin code to Rust, not a model runtime, not an Android background-service framework and not a production agent embedded in CenAI.
- It does not replace Rust tests, Android instrumentation, security review, physical-device performance profiling or Google Play/package signing checks.
- The upstream README advertises a 99%+ AndroidWorld result; CenAI must measure its own pass rate and report flaky/blocked cases.
- ARTEMIS needs a host with ADB and a connected device/emulator. It cannot reach the user's tablet from GitHub or this chat unless the user deliberately connects it to a trusted host.
- The upstream Python project requires Python 3.12+, whereas CenAI's Android runtime remains Rust plus narrow Java/Kotlin platform adapters.

## Acceptance criteria for adopting ARTEMIS

- [ ] A pinned, reviewed ARTEMIS revision is installed on a trusted test host.
- [ ] The host can diagnose and identify the intended test device; no ambiguous auto-selection.
- [ ] The daemon is loopback-bound or protected by authenticated HTTPS; no public unauthenticated endpoint.
- [ ] A smoke test launches the CenAI Rust-native APK and captures a trace, screenshot and sanitized logs.
- [ ] The Phase 1 offline Lightning scenario is run with network disabled and its result is correlated to the APK hash.
- [ ] A Phase 2 parity matrix is completed without bypassing approval dialogs.
- [ ] Deterministic tests are added for stable discoveries; ARTEMIS is supplementary exploratory evidence.
- [ ] Test artifacts are scrubbed of secrets and personal data.

## Upstream references

- [Google ARTEMIS repository and setup](https://github.com/google/artemis)
- [ARTEMIS MCP server architecture and tools](https://github.com/google/artemis/tree/main/mcp_server)
- [ARTEMIS remote Python SDK and daemon security note](https://github.com/google/artemis/tree/main/packages/artemis-client)
- [Apache License 2.0](https://github.com/google/artemis/blob/main/LICENSE)
