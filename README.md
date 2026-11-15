# CenAI

CenAI is an Android-first multi-provider AI workspace built with Kotlin, Jetpack Compose and a Rust JNI core.

## Current features

- Streaming chat for OpenAI-compatible APIs, Claude Messages and Gemini.
- Per-provider API keys encrypted with Android Keystore.
- Custom OpenAI-compatible HTTPS endpoints and optional JSON headers.
- Saved conversations, searchable chat history, new chat, delete, edit/resend, copy, regenerate and share/export.
- Text-document attachments (TXT, Markdown, CSV and JSON) plus selectable-text PDF extraction (up to 300 pages / 250,000 characters), and on-device OCR for image attachments using ML Kit. Password-protected and scanned/image-only PDFs are not supported yet (attach the page images separately for OCR). Android speech-recognition voice input is also available.
- Configurable speech engine controls: input/output language tags, speech rate, pitch and an in-app voice test; assistant responses can be read aloud with Android Text-to-Speech.
- Optional local knowledge retrieval across saved conversations and attached text documents. Matching excerpts are searched on-device and included as context for the selected provider.
- Offline Brain powered by LiteRT-LM: import a compatible `.litertlm` model into app-private storage and run text chat locally on the CPU without a network connection. The model is not bundled in the APK; users must obtain it under its model licence.
- Save or share conversations; save uses Android's Storage Access Framework so the user chooses the destination.
- Optional user-managed persistent agent memory, encrypted with Android Keystore and editable or clearable in Settings.
- Quick prompt starters for brainstorming, summaries, translation, writing and code review.
- Local usage insights (request count and approximate input-token estimate) with configurable daily limits.
- Privacy-first app monitor with local request success/failure/latency counters and feature activity counts for app starts, voice, document attachments, provider tests and update checks. It does not store prompts, responses or credentials.
- Guardian local diagnostics: available/total RAM, app heap, storage, battery, network, uptime, Android/device version and last crash category/timestamp. No stack traces or prompt content are collected.
- Optional cloud analytics and Guardian reporting are explicitly opt-in per account. Usage telemetry contains only provider/model, token estimates, latency and status; Guardian reports contain device-health aggregates only.
- System instructions and maximum output-token setting.
- Connection test in provider settings.
- In-app update checker and APK downloader using GitHub Releases.
- Animated Compose UI with responsive message transitions and animated composer controls.
- Four switchable UI themes in Settings: CenAI Default, Android Material 3 (including wallpaper-derived Material You colors on Android 12+), iOS-inspired, and Figma-inspired.
- Markdown-aware message rendering with visually separated fenced code blocks, language labels, horizontal code scrolling and per-block copy.
- Agent planning mode with a bounded tool-call loop, per-tool enable/disable controls and nine allow-listed local utilities (date/time, calculator, word/text statistics, JSON formatting, UUID and Base64). This is not unrestricted background or device execution.
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

Then push a version tag such as `v0.2.0`. The workflow builds a signed release APK and attaches it to the GitHub Release. Future app updates can be installed from Settings → Check for software updates.

## Agent tools

Agent planning mode can request an allow-listed set of local tools: current date/time, arithmetic, word counting, text statistics, JSON formatting, UUID generation and Base64 encoding/decoding. Each tool can be enabled or disabled in Settings. The model requests tools through a strict structured marker; the app validates the name against its registry and executes only registered handlers. Agent turns are bounded to four tool/response cycles. This is not arbitrary shell execution, unrestricted code execution, background device control or unrestricted file/network access. Additional tools must be added individually with scoped permissions, explicit approval for sensitive/destructive actions, execution timeouts and audit history.

## Google Gemini OAuth

Settings includes a native Google OAuth authorization-code flow with PKCE for Gemini API access. Configure a Google Cloud project, enable the Generative Language API, complete the OAuth consent screen, register an appropriate native Android OAuth client, and enter its client ID and Cloud Project ID in CenAI Settings. The app requests the `generative-language.retriever` and `cloud-platform` scopes, validates the OAuth state, stores tokens encrypted with Android Keystore and refreshes access tokens when possible. The configured redirect URI is `com.cenai.app:/oauth2redirect`; verify that this redirect and Android package/signing identity are accepted by the OAuth client configuration before distributing the app. OAuth access depends on Google's project, consent-screen and API eligibility policies; it does not bypass quotas, billing, app review or account restrictions.

Other providers are not automatically account-connected. Only implement their OAuth integrations when the provider documents and permits a third-party native-app flow for the specific API product. A consumer subscription does not imply API access.

## Security and provider access

Provider API keys stay on-device and are encrypted using Android Keystore. Use official provider API endpoints and API credentials. A consumer subscription does not automatically include API access. Never commit API keys, OAuth tokens, signing keys or private configuration.


## UI design references

The project includes a Figma design file with editable Material 3 and iOS/iPadOS component instances and CenAI chat/settings screen compositions: https://www.figma.com/design/izsd2XugSdMRQtDidtc3iI/CenAI-Android-App-Design-System. The Figma-inspired in-app theme is a token-based fallback palette; importing a user's private Figma assets directly into Android still requires a target file and selected assets.
