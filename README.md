# CenAI

CenAI is an Android-first, multi-provider AI chat client with a native Kotlin/Jetpack Compose interface and a Rust-powered core.

## Project status

Early development bootstrap. The initial milestone is a working Android shell, provider configuration, streaming chat architecture, secure local credential storage, and CI-generated APK artifacts.

## Architecture

- **Android app:** Kotlin, Jetpack Compose, Android Gradle Plugin.
- **Core:** Rust library exposed to Android through JNI (UniFFI can be introduced as the API stabilizes).
- **Security:** Keep provider credentials on-device and protect them with Android Keystore-backed encryption.
- **Providers:** Integrate through documented provider APIs. A consumer subscription does not automatically include API access.
- **CI:** GitHub Actions builds a debug APK and uploads it as a downloadable workflow artifact.

## Build

Open the project in Android Studio or run the Gradle wrapper on a compatible machine:

```bash
./gradlew assembleDebug
```

The APK is produced under `app/build/outputs/apk/debug/`.

## Security

Never commit API keys, OAuth tokens, signing keys, or local configuration containing secrets. Use local encrypted storage for credentials.
