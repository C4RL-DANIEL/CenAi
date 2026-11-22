#!/usr/bin/env bash
set -euo pipefail

APK="app-rust/build/outputs/apk/debug/app-rust-debug.apk"
PACKAGE="com.cenai.app.rustdebug"

# android-emulator-runner has already waited for sys.boot_completed before
# invoking this script. Keep the smoke test bounded and avoid a second boot wait.
timeout 35s adb install -r "$APK"
timeout 10s adb shell monkey -p "$PACKAGE" 1
sleep 20

app_pid="$(timeout 8s adb shell pidof "$PACKAGE" 2>/dev/null | tr -d '\r' || true)"
if [[ -z "$app_pid" ]]; then
  timeout 10s adb logcat -d -t 1200 || true
  echo "CenAI NativeActivity did not remain running after launch." >&2
  exit 1
fi

if timeout 10s adb logcat -d -t 1600 | grep -E 'FATAL EXCEPTION.*com\.cenai\.app\.rustdebug|Process: com\.cenai\.app\.rustdebug|Fatal signal [0-9]+.*cenai_native'; then
  timeout 10s adb logcat -d -t 1600 || true
  echo "CenAI NativeActivity emitted a fatal startup error." >&2
  exit 1
fi

echo "CenAI NativeActivity launch and process-liveness smoke checks passed (pid $app_pid)."
