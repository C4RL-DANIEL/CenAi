# CenAI

> **Branch distinction:** `main` documents and ships the existing Kotlin/Compose application. The active `rewrite/rust-native-core` branch is a separate, pure-Rust Android replacement. Its UI and application logic must remain Rust-owned; the legacy app is retained only as rollback/reference until parity and physical-device verification pass. See [the Rust-native rewrite plan](docs/RUST-NATIVE-REWRITE.md) for the binding architecture, local-first data rules, identity/provider authorization boundary and private Ecosystem Engine design.

CenAI is an Android-first multi-provider AI workspace built with Kotlin, Jetpack Compose and a Rust JNI core.

## Current features

- Streaming chat for OpenAI-compatible APIs, Claude Messages and Gemini.
- Per-provider API keys encrypted with Android Keystore.
- Custom OpenAI-compatible HTTPS endpoints and optional JSON headers.
- Saved conversations, searchable chat history, new chat, delete, edit/resend, copy, regenerate and share/export.
- Text-document attachments (TXT, Markdown, CSV and JSON), selectable-text PDF extraction (up to 300 pages / 250,000 characters), on-device OCR, and native multimodal image payloads for vision-capable OpenAI, Claude, Gemini, DeepSeek V4.1 Flash (`deepseek-flash`) and compatible custom models. Images are explicitly selected by the user and capped at 4 MB per attachment; Legacy DeepSeek text-only model IDs are migrated to `deepseek-flash` for image requests. Password-protected PDFs are not supported; scanned PDFs fall back to on-device OCR with a 50-page limit. Android speech-recognition voice input is also available.
- Configurable speech engine controls: input/output language tags, speech rate, pitch and an in-app voice test; assistant responses can be read aloud with Android Text-to-Speech.
- Optional local knowledge retrieval across saved conversations and attached text documents. Matching excerpts are searched on-device and included as context for the selected provider.
- Offline Brain powered by LiteRT-LM: import a compatible `.litertlm` model into app-private storage and run text chat locally on the CPU without a network connection. The model is not bundled in the APK; users must obtain it under its model licence.
- Save or share conversations; save uses Android's Storage Access Framework so the user chooses the destination.
- Optional user-managed persistent agent memory, encrypted with Android Keystore and editable or clearable in Settings.
- Quick prompt starters for brainstorming, summaries, translation, writing and code review.
- Local usage insights (request count and approximate input-token estimate) with configurable daily limits.
- Privacy-first app monitor with local request success/failure/latency counters and feature activity counts for app starts, voice, document attachments, provider tests and update checks. It does not store prompts, responses or credentials.
- Guardian local diagnostics: available/total RAM, app heap, storage, battery, network, uptime, Android/device version and last crash category/timestamp. A WorkManager task samples a small on-device health history periodically (Android schedules it on a best-effort basis, not as an always-on service). No stack traces or prompt content are collected.
- Optional cloud analytics and Guardian reporting are explicitly opt-in per account. Usage telemetry contains only provider/model, token estimates, latency and status; Guardian reports contain device-health aggregates only.
- System instructions and maximum output-token setting.
- Connection test in provider settings.
- In-app update checker and APK downloader using GitHub Releases.
- Animated Compose UI with responsive message transitions and animated composer controls.
- Four switchable UI themes in Settings: CenAI Default, Android Material 3 (including wallpaper-derived Material You colors on Android 12+), iOS-inspired, and Figma-inspired.
- Markdown-aware message rendering with visually separated fenced code blocks, language labels, horizontal code scrolling and per-block copy.
- Agent planning mode with a bounded tool-call loop, per-tool enable/disable controls, nine local utilities, and approval-gated Android actions (open URL, open dialer without calling, share sheet and app settings). Sensitive actions require a separate approval tap and hand off to Android intents; CenAI cannot silently call or send messages. One-off scheduled network tasks run through WorkManager with a bounded four-turn safe-tool loop, encrypted local prompts/results, task status/history, cancellation controls and optional private completion notifications; Android may defer work due to battery and connectivity policy.
- Expanded settings for agent planning, approval preferences and provider account/OAuth capability guidance.

## Build

GitHub Actions builds the ARM64 Rust JNI library and a debug APK on pushes to `main`. Download the `CenAI-debug-apk` artifact from the Actions run.

## In-app software updates

The app checks the latest GitHub Release and downloads its APK directly inside CenAI. Android then opens the system package installer. Android requires the user to approve installation; apps cannot silently install their own updates.

For updates to install over an existing CenAI copy, every release must be signed with the same private signing key. Never commit that key to this repository.

Add these repository Actions secrets before publishing a version tag:

- `CENAI_KEYSTORE_BASE64`: base64-encoded JKS/PKCS12 keystore.
- `CENAI_KEYSTORE_PASSWORD`: keystore password.
- `CENAI_KEY_ALIAS`: signing key alias.
- `CENAI_KEY_PASSWORD`: key password.

Then push a version tag such as `v0.2.0`. The workflow runs JVM unit tests, builds the debug APK, generates SHA-256 checksums, and on version tags builds a signed release APK, verifies its Android signature and attaches both APK and checksum to the GitHub Release. The in-app updater checks the published SHA-256 and also requires the downloaded APK package ID and signing certificate to match the installed CenAI app before opening Android's installer. Future app updates can be installed from Settings → Check for software updates.

## Agent tools

Agent planning mode can request an allow-listed set of local tools: current date/time, arithmetic, word counting, text statistics, JSON formatting, UUID generation and Base64 encoding/decoding. Each tool can be enabled or disabled in Settings. The model requests tools through a strict structured marker; the app validates the name against its registry and executes only registered handlers. Agent turns are bounded to four tool/response cycles. This is not arbitrary shell execution, unrestricted code execution, unattended device control or unrestricted file/network access. Additional tools must be added individually with scoped permissions, explicit approval for sensitive/destructive actions, execution timeouts and audit history.

## Google Gemini OAuth

Settings contains an experimental Google OAuth authorization-code flow with PKCE for Gemini API access. Google's official Gemini OAuth quickstart currently describes a simplified desktop-app testing flow; this custom in-app redirect has not been verified as a production Android OAuth client. For reliable Android use, configure a Gemini API key. The experimental flow requests the documented `generative-language.retriever` and `cloud-platform` scopes, validates OAuth state, stores returned tokens encrypted with Android Keystore and refreshes access tokens when possible. The configured redirect URI is `com.cenai.app:/oauth2redirect`; verify that this redirect and Android package/signing identity are accepted by the OAuth client configuration before distributing the app. OAuth access depends on Google's project, consent-screen and API eligibility policies; it does not bypass quotas, billing, app review or account restrictions.

Other providers are not automatically account-connected. Only implement their OAuth integrations when the provider documents and permits a third-party native-app flow for the specific API product. A consumer subscription does not imply API access.

## Security and provider access

Provider API keys stay on-device and are encrypted using Android Keystore. Use official provider API endpoints and API credentials. A consumer subscription does not automatically include API access. Never commit API keys, OAuth tokens, signing keys or private configuration.

### Password safety on the Free Supabase plan

Supabase's built-in Have I Been Pwned password protection is a paid-plan Auth setting, so CenAI does not claim that the platform-wide control is enabled. The official website registration and recovery flows instead submit password changes to the authenticated `cenai-password-policy` Edge Function. That function enforces at least 12 characters and at most 72 UTF-8 bytes (Supabase Auth uses bcrypt), rejects a password containing the account email local-part or a single repeated character, and checks the password against Have I Been Pwned's Pwned Passwords range API before changing the account password. The check uses SHA-1 k-anonymity: only the first five hexadecimal characters of the password hash are sent; the plaintext password and full hash are not sent to HIBP or logged. If the breach-check service is unavailable, the password change fails closed and the existing password remains unchanged.

This policy is enforced in CenAI's registration and recovery pages. It is not a Supabase-wide password hook: an administrator or a separate client that calls Supabase Auth's password-update endpoint directly can bypass the website flow. Keep account password changes routed through CenAI's policy endpoint until a platform-wide Auth hook is available.


## UI design references

The project includes a Figma design file with editable Material 3 and iOS/iPadOS component instances and CenAI chat/settings screen compositions: https://www.figma.com/design/izsd2XugSdMRQtDidtc3iI/CenAI-Android-App-Design-System. The Figma-inspired in-app theme is a token-based fallback palette; importing a user's private Figma assets directly into Android still requires a target file and selected assets.

- Multi-provider mode shows independent responses and, when multiple providers respond, requests a consolidated synthesis from the first configured provider.


## Vision and scheduled agents

Image attachments are passed to supported providers as native image blocks (OpenAI-compatible `image_url`, Anthropic image source blocks, or Gemini `inline_data`) rather than relying only on OCR. OCR text is included as an optional reference. Image bytes are kept in the local conversation and sent to the selected cloud provider only when the user sends the chat. Use a vision-capable model; CenAI automatically migrates the old `deepseek-chat` and `deepseek-reasoner` IDs to `deepseek-flash`.

Scheduled tasks are one-shot WorkManager jobs. Their prompts and results are stored in app-private preferences, while WorkManager input contains only task metadata. Jobs require network connectivity and Android may defer them. The app does not claim always-on execution.

## Device-action permissions

The agent registry exposes only narrow Android actions. A device-action tool call produces an approval card; tapping **Approve & run once** launches the corresponding Android screen. Opening the dialer does not place a call, and sharing opens the Android chooser without selecting a recipient or sending content. CenAI does not request Accessibility, SMS, call-log or broad device-control permissions.

## Admin analytics

The authenticated Supabase control plane provides a role-protected analytics dashboard action with time-series, provider/model, latency, status and failure-category aggregates. The local app monitor can export aggregate JSON and be reset. Remote telemetry remains subject to explicit per-account consent and platform feature flags.

## Provider authentication limitations

Gemini's native OAuth flow is the only consumer-facing provider OAuth flow currently implemented. OpenAI and Anthropic offer API-key authentication and selected workload-identity options; DeepSeek documents bearer API-key authentication. These workload-identity methods require a trusted cloud identity provider and are not user-subscription OAuth flows for a standalone Android app. ChatGPT Plus, Claude subscriptions and DeepSeek consumer subscriptions are not API credentials. CenAI does not scrape browser sessions or store consumer-account passwords to imitate provider OAuth. API keys are encrypted at rest with Android Keystore, but a mobile client is not a trusted secret-holding server.


## Production release signing

Before creating a `v*` release tag, configure these repository Actions secrets under **Settings → Secrets and variables → Actions**:

- `CENAI_KEYSTORE_BASE64` — base64-encoded, long-lived production keystore
- `CENAI_KEYSTORE_PASSWORD`
- `CENAI_KEY_ALIAS`
- `CENAI_KEY_PASSWORD`

Keep the keystore and passwords outside Git; never commit them or place them in workflow logs. Use the same keystore for every future release so Android accepts in-app updates. The release workflow intentionally fails closed when signing secrets are missing; it will not publish a debug-signed APK as a production release. The Actions workflow runs JVM tests and an API 35 x86_64 emulator smoke test before publishing build artifacts. A passing emulator run still does not replace physical-device testing on the target Redmi tablet.
