#!/usr/bin/env bash
set -euo pipefail

APK="app-rust/build/outputs/apk/debug/app-rust-debug.apk"
PACKAGE="com.cenai.app.rustdebug"

# Wait for Android's package manager, not just the emulator boot property.
timeout 60s adb wait-for-device
package_ready=false
for _ in $(seq 1 45); do
  if timeout 5s adb shell service check package 2>/dev/null | grep -q found; then
    package_ready=true
    break
  fi
  sleep 2
done
if [[ "$package_ready" != true ]]; then
  timeout 10s adb logcat -d -t 500 || true
  echo "Android package manager did not become ready." >&2
  exit 1
fi

# --no-streaming first pushes the APK before invoking PackageManager. Retry
# transient emulator-side service restarts rather than failing on one broken pipe.
installed=false
for attempt in 1 2 3; do
  if timeout 60s adb install --no-streaming -r "$APK"; then
    installed=true
    break
  fi
  echo "APK installation attempt $attempt failed; reconnecting adb." >&2
  adb reconnect device >/dev/null 2>&1 || true
  timeout 60s adb wait-for-device
  sleep 5
done
if [[ "$installed" != true ]]; then
  timeout 10s adb logcat -d -t 1200 || true
  echo "CenAI APK could not be installed after three attempts." >&2
  exit 1
fi

timeout 15s adb shell monkey -p "$PACKAGE" 1
sleep 20

app_pid="$(timeout 8s adb shell pidof "$PACKAGE" 2>/dev/null | tr -d '\r' || true)"
if [[ -z "$app_pid" ]]; then
  timeout 10s adb logcat -d -t 1200 || true
  echo "CenAI NativeActivity did not remain running after launch." >&2
  exit 1
fi

# Install and run the real on-device Lightning test. This verifies that the
# model is present in the APK, copied into private storage, hash-checked, and
# able to generate a response without network access.
TEST_APK="app-rust/build/outputs/apk/androidTest/debug/app-rust-debug-androidTest.apk"
if [[ ! -s "$TEST_APK" ]]; then
  echo "Lightning instrumentation APK was not produced." >&2
  exit 1
fi
timeout 120s adb install --no-streaming -r "$TEST_APK"
test_output="$(timeout 600s adb shell am instrument -w -r \
  -e package com.cenai.app \
  com.cenai.app.rustdebug.test/androidx.test.runner.AndroidJUnitRunner 2>&1)" || {
    printf '%s\n' "$test_output" >&2
    timeout 10s adb logcat -d -t 1600 || true
    echo "CenAI Android instrumentation tests failed." >&2
    exit 1
  }
printf '%s\n' "$test_output"
if ! grep -Eq 'OK \([0-9]+ tests?\)' <<<"$test_output"; then
  echo "CenAI Android instrumentation runner did not report test success." >&2
  exit 1
fi

if timeout 10s adb logcat -d -t 1600 | grep -E 'FATAL EXCEPTION.*com\\.cenai\\.app\\.rustdebug|Process: com\\.cenai\\.app\\.rustdebug|Fatal signal [0-9]+.*cenai_native'; then
  timeout 10s adb logcat -d -t 1600 || true
  echo "CenAI NativeActivity emitted a fatal startup error." >&2
  exit 1
fi

echo "CenAI NativeActivity launch and process-liveness smoke checks passed (pid $app_pid)."
