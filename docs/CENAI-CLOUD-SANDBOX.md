# CenAI Cloud Sandbox

## Runtime architecture

- The Android/Rust client submits commands to the branch-scoped Vercel `POST /api/sandbox` function. The Cloudflare Worker also contains a same-origin proxy route for web clients when that Worker revision is deployed.
- The Worker forwards the authenticated request to the Vercel serverless function at `/api/sandbox`.
- The server verifies the Supabase bearer token, verifies CenAI registration eligibility, and claims a per-account quota slot before provisioning any compute.
- Each account's workspace is restored from its own Vercel Sandbox snapshot. First use starts from the pinned Rust-enabled base snapshot configured by `CENAI_SANDBOX_BASE_SNAPSHOT_ID`.
- After a command completes, the microVM is snapshotted and the new snapshot ID is stored through account-scoped Supabase RPCs. The previous snapshot is deleted after the new reference is saved.
- The database stores only the account ID, snapshot ID, and minimal run metadata; it does not store command output or workspace file contents.

## Runtime limits and security

- One vCPU and 2 GB RAM per execution; 55-second VM timeout, with a 60-second Vercel function ceiling.
- Outbound network is limited to GitHub, Rust package registries, npm/Python registries, and package mirrors. It is not unrestricted internet access.
- Commands are selected from a fixed allowlist. Arguments are bounded, path traversal is rejected, and working directories must remain below `/vercel/sandbox/projects/<projectId>`.
- Environment variables are disabled. Credential-like arguments and sensitive output lines are rejected or redacted.
- Account identity is derived from the verified Supabase session on the server; client-provided owner IDs are not trusted.
- Supabase RPCs enforce registration eligibility, per-account/global concurrency limits, run quotas, and owner-only snapshot references.
- User-initiated execution is exposed in Settings. It is not a hidden background process and does not grant Android device-control permissions.

## API request

```json
{
  "projectId": "default",
  "command": "cargo",
  "arguments": ["--version"],
  "workingDirectory": "."
}
```

The API does not accept environment variables, arbitrary network host lists, or absolute working directories. The Rust client performs the same account-scope and request validation before sending the request; the server independently repeats the security checks.

## Snapshot lifecycle

Per-account snapshots are saved for 30 days after a successful run. The initial Rust toolchain snapshot is a separately pinned base image referenced by `CENAI_SANDBOX_BASE_SNAPSHOT_ID`; refresh that environment variable whenever the base image is rebuilt or its snapshot is nearing expiration. Never delete a user's current snapshot until the replacement has been durably saved.
