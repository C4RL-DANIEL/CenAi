# CenAI physical-device QA checklist

Target device: REDMI Pad 2 Pro Wi-Fi, ARM64, Android 16 / HyperOS 3. Record the exact build number, app version, APK SHA-256, and test date in the QA report.

## Installation and lifecycle

- [ ] Install the release APK on a clean device; record any Android package-installer warning.
- [ ] Launch CenAI and confirm the first frame appears without a crash, blank screen, or stuck loading indicator.
- [ ] Force-stop and relaunch; verify saved preferences and conversations persist as designed.
- [ ] Reboot the device and confirm CenAI launches normally.
- [ ] Upgrade an existing installation using an APK signed by the same release key; confirm app data is retained.

## UI and adaptive layout

- [ ] Verify Chat, conversation history, Settings, Agents, Account, Device, and Updates navigation.
- [ ] Open Settings; verify the full-screen layout, scroll behavior, all six settings categories, and save/close behavior.
- [ ] Rotate between portrait and landscape; confirm the composer, dialogs, and settings remain usable.
- [ ] Test split-screen / floating-window resizing and tablet navigation.
- [ ] Increase system font/display size and verify controls remain visible and tappable.
- [ ] Verify light/dark theme, TalkBack labels, keyboard focus, and reduced-motion behavior.

## Core workflows

- [ ] Send and receive a short text prompt using a configured provider.
- [ ] Stop a streaming response and verify the draft and conversation remain consistent.
- [ ] Test retry, regenerate, copy, edit/resend, and text-to-speech.
- [ ] Attach an image and verify vision-capable provider handling.
- [ ] Attach a text PDF and a scanned PDF; verify extraction/OCR limits and error handling.
- [ ] Test voice input permission grant and denial.
- [ ] Test agent planning, approval, rejection, and stop controls; confirm no device action runs without the required approval.
- [ ] Test app update check and confirm Android's package installer opens only for a verified compatible APK.

## Reliability and resources

- [ ] Test airplane mode, Wi-Fi loss, and recovery; confirm errors are actionable and drafts are preserved.
- [ ] Test low-storage behavior and deny file access where applicable.
- [ ] Run a 15-minute chat/session soak test; monitor ANRs, crashes, memory pressure, and battery impact.
- [ ] Review Guardian and App Monitor diagnostics; confirm no prompts, responses, API keys, or private content appear in exported telemetry.

## Evidence and release gate

Record pass/fail, reproduction steps, screenshots or logcat excerpts, and severity for every failed item. Redact tokens, email OTPs, prompts, and other private data before sharing logs.

Do not mark physical-device QA complete based on an emulator run. A release candidate is not production-ready until the same-key signed APK installs and the critical workflows pass on the target tablet.