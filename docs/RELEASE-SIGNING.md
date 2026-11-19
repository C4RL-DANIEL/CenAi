# CenAI production Android release signing

The production release workflow signs APKs only from version tags (`v*`). It verifies the APK with Android's `apksigner`, calculates SHA-256, uploads the APK artifact, and publishes a GitHub Release.

## One-time key creation

Create one long-lived release keystore on a trusted machine. Keep the keystore and passwords in a secure offline backup. Do not commit the keystore to Git, attach it to issues, or share it in chat.

```bash
keytool -genkeypair -v -keystore cenai-release.jks -keyalg RSA -keysize 4096 -validity 10000 -alias cenai-release
base64 -w 0 cenai-release.jks > cenai-release.jks.base64
```

On Termux or another system whose `base64` does not support `-w`, use:

```bash
base64 cenai-release.jks | tr -d '\n' > cenai-release.jks.base64
```

## GitHub Actions secrets

In the repository, open **Settings → Secrets and variables → Actions → New repository secret** and add:

| Secret | Value |
|---|---|
| `CENAI_KEYSTORE_BASE64` | Entire one-line contents of `cenai-release.jks.base64` |
| `CENAI_KEYSTORE_PASSWORD` | Keystore password entered during key creation |
| `CENAI_KEY_ALIAS` | `cenai-release` |
| `CENAI_KEY_PASSWORD` | Key password entered during key creation |

Never put any of these values in source files, workflow YAML, commit messages, or logs.

## Build and verify

After all four secrets are configured, create and push a version tag, for example:

```bash
git tag v0.2.0
git push origin v0.2.0
```

The Android CI workflow will build the release APK, verify its signature, generate a SHA-256 checksum, and publish the release. Confirm that the workflow run is green and that the release asset is present before distributing it.

## Key continuity

Every update intended to install over the existing CenAI app must use the same signing key. Losing or replacing this key prevents ordinary in-place updates for existing installations. Back up the original keystore and its passwords securely before publishing the first production release.