#!/usr/bin/env bash
set -euo pipefail

APK="app-rust/build/outputs/apk/debug/app-rust-debug.apk"
PACKAGE="com.cenai.app.rustdebug"
ACTIVITY="com.cenai.app.CenAiNativeActivity"

adb install "$APK"
adb logcat -c
adb shell am start -n "$PACKAGE/$ACTIVITY"
app_pid=""
for attempt in $(seq 1 45); do
  app_pid="$(adb shell pidof "$PACKAGE" | tr -d '\r' || true)"
  if [[ -n "$app_pid" ]]; then
    echo "CenAI NativeActivity process is running (pid $app_pid)."
    break
  fi
  sleep 1
done
if [[ -z "$app_pid" ]]; then
  adb logcat -d -t 1200
  echo "CenAI NativeActivity did not remain running." >&2
  exit 1
fi
sleep 5
if adb logcat -d -t 1600 | grep -E 'FATAL EXCEPTION.*com\.cenai\.app\.rustdebug|Process: com\.cenai\.app\.rustdebug|Fatal signal [0-9]+.*cenai_native'; then
  adb logcat -d -t 1600
  echo "CenAI NativeActivity emitted a fatal startup error." >&2
  exit 1
fi
adb shell dumpsys activity activities | grep -F "$PACKAGE/$ACTIVITY" >/dev/null
echo "CenAI NativeActivity launch and foreground smoke checks passed."
