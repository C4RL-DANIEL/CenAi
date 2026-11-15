# CenAI

CenAI is an Android-first multi-provider AI workspace built with Kotlin, Jetpack Compose and a Rust JNI core.

## Current features

- Streaming chat for OpenAI-compatible APIs, Claude Messages and Gemini.
- Per-provider API keys encrypted with Android Keystore.
- Custom OpenAI-compatible HTTPS endpoints and optional JSON headers.
- Saved conversations, new chat, history, delete, edit/resend, copy and regenerate.
- Text-document attachments (TXT, Markdown, CSV and JSON).
- System instructions and maximum output-token setting.
- Connection test in provider settings.
- In-app update checker and APK downloader using GitHub Releases.
- Animated Compose UI with responsive message transitions and animated composer controls.
- Markdown-aware message rendering with visually separated fenced code blocks, language labels, horizontal code scrolling and per-block copy.
- Agent planning mode with approval-first guidance; this is planning support, not unrestricted background or device execution.
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

## Security and provider access

Provider API keys stay on-device and are encrypted using Android Keystore. Use official provider API endpoints and API credentials. A consumer subscription does not automatically include API access. Never commit API keys, OAuth tokens, signing keys or private configuration.
