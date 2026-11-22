#!/usr/bin/env bash
set -euo pipefail

WORKSPACE="${GITHUB_WORKSPACE:-$(pwd)}"
APK="$WORKSPACE/app-rust/build/outputs/apk/debug/app-rust-debug.apk"

if [[ ! -s "$APK" ]]; then
  echo "Rust-native debug APK is missing or empty: $APK" >&2
  exit 1
fi

timeout 60s adb wait-for-device
timeout 10s adb shell service check package | grep -q found
timeout 90s adb install --no-streaming -r "$APK"
timeout 15s adb shell monkey -p com.cenai.app.rustdebug 1
sleep 10
timeout 10s adb shell pidof com.cenai.app.rustdebug

# Disable every network transport before running the instrumentation test.
adb shell svc wifi disable
adb shell svc data disable
sleep 3

cd "$WORKSPACE"
gradle --no-daemon :app-rust:connectedDebugAndroidTest --stacktrace
