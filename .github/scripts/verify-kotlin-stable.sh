#!/usr/bin/env bash
set -euo pipefail

APK="${GITHUB_WORKSPACE:-.}/app/build/outputs/apk/debug/app-debug.apk"
test -s "$APK"
adb install -r "$APK"
adb logcat -c
adb shell am force-stop com.cenai.app
adb shell monkey -p com.cenai.app 1
sleep 30

PID="$(adb shell pidof com.cenai.app | tr -d '\r')"
if [[ -z "$PID" ]]; then
  echo "::error::CenAI process exited during startup."
  adb logcat -d -t 4000
  exit 1
fi

adb shell uiautomator dump /sdcard/cenai-window.xml >/dev/null
adb pull /sdcard/cenai-window.xml "$RUNNER_TEMP/cenai-window.xml" >/dev/null
if ! grep -Eiq 'CenAI|New chat|Message|Ask|Settings|Chat' "$RUNNER_TEMP/cenai-window.xml"; then
  echo "::error::CenAI stayed alive but its accessible UI did not expose expected screen content."
  cat "$RUNNER_TEMP/cenai-window.xml"
  adb logcat -d -t 4000
  exit 1
fi

if adb logcat -d -t 4000 | grep -E 'FATAL EXCEPTION|Fatal signal' | grep -q 'com.cenai.app'; then
  echo "::error::CenAI emitted a fatal crash."
  adb logcat -d -t 4000
  exit 1
fi

echo "Stable Kotlin Compose app stayed alive for 30 seconds and exposed its UI accessibility tree."
