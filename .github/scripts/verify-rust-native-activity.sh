#!/usr/bin/env bash
set -euo pipefail

APK="app-rust/build/outputs/apk/debug/app-rust-debug.apk"
PACKAGE="com.cenai.app.rustdebug"
ACTIVITY="com.cenai.app.CenAiNativeActivity"

timeout 180s adb wait-for-device || { adb devices -l; echo "Android emulator did not become available." >&2; exit 1; }
booted=""
for attempt in $(seq 1 60); do
  booted="$(timeout 8s adb shell getprop sys.boot_completed 2>/dev/null | tr -d '\\r' || true)"
  if [[ "$booted" == "1" ]]; then break; fi
  sleep 2
done
if [[ "$booted" != "1" ]]; then
  adb devices -l
  timeout 15s adb logcat -d -t 300 || true
  echo "Android emulator did not finish booting." >&2
  exit 1
fi

timeout 60s adb install -r "$APK"
timeout 15s adb logcat -c
timeout 20s adb shell am start -W -n "$PACKAGE/$ACTIVITY"
app_pid=""
for attempt in $(seq 1 45); do
  app_pid="$(timeout 8s adb shell pidof "$PACKAGE" 2>/dev/null | tr -d '\\r' || true)"
  if [[ -n "$app_pid" ]]; then
    echo "CenAI NativeActivity process is running (pid $app_pid)."
    break
  fi
  sleep 1
done
if [[ -z "$app_pid" ]]; then
  timeout 15s adb logcat -d -t 1200 || true
  echo "CenAI NativeActivity did not remain running." >&2
  exit 1
fi
sleep 5
if timeout 15s adb logcat -d -t 1600 | grep -E 'FATAL EXCEPTION.*com\\.cenai\\.app\\.rustdebug|Process: com\\.cenai\\.app\\.rustdebug|Fatal signal [0-9]+.*cenai_native'; then
  timeout 15s adb logcat -d -t 1600 || true
  echo "CenAI NativeActivity emitted a fatal startup error." >&2
  exit 1
fi
timeout 20s adb shell dumpsys activity activities | grep -F "$PACKAGE/$ACTIVITY" >/dev/null
echo "CenAI NativeActivity launch and foreground smoke checks passed."