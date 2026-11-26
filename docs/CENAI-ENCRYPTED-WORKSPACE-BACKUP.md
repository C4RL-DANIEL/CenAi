# CenAI encrypted workspace backups

The **Your Work → Encrypted cloud backup** controls back up the local Projects workspace.

## What's included

- Project names, descriptions, archive state and timestamps.
- Project artifacts, including their names, MIME types, byte counts and SHA-256 digests.

Chat history, provider credentials, OAuth tokens, MCP bearer credentials, device settings and local Android Keystore keys are not included.

## Encryption and account isolation

- The archive is serialized locally and encrypted on the Android device with AES-256-GCM.
- A 16-byte random salt and 12-byte random nonce are generated for each backup.
- The encryption key is derived from the passphrase using PBKDF2-HMAC-SHA-256 (310,000 iterations).
- The account UUID is authenticated as additional data, so a ciphertext copied to another account path cannot be decrypted as that account's backup.
- The Supabase Storage bucket is private and its row-level policies restrict each object path to the matching authenticated user.
- The server stores the ciphertext envelope; it never receives the passphrase or plaintext project archive.

Use a passphrase of at least 12 characters. CenAI does not store or recover it. A lost passphrase means the backup cannot be decrypted.

## Restore behavior

Restore validates the archive format, project and artifact IDs, per-file size limits, SHA-256 digests and total archive size before updating the encrypted local workspace. It adds project IDs that do not already exist and refuses to overwrite existing projects or artifact paths.

For a restore to a new device, sign in to the same CenAI account and enter the original passphrase. If the account has a verified Supabase MFA factor, the Storage policy may require an AAL2 session before backup access is allowed.

## Limits

- 100 projects per local workspace.
- 100 artifacts per project.
- 8 MiB per artifact.
- 95 MiB per archive (plus the small encryption envelope).
- The cloud bucket allows a 100 MiB object.

Cloud backup requires an authenticated, registered account. Guest workspaces remain local-only.
