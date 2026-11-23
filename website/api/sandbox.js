import { isAbsolute, resolve, sep } from "node:path";

const ALLOWED_COMMANDS = new Set([
  "bash", "sh", "cargo", "rustc", "python3", "python", "node", "npm", "npx", "git",
  "go", "make", "cmake", "java", "javac", "gradle", "ls", "cat", "find", "grep",
  "sed", "awk", "pwd", "mkdir", "cp", "mv", "rm", "touch", "tar", "unzip", "zip",
  "which", "head", "tail", "wc", "file"
]);
const NETWORK_ALLOWLIST = [
  "github.com", "api.github.com", "objects.githubusercontent.com", "raw.githubusercontent.com", "codeload.github.com",
  "crates.io", "index.crates.io", "static.crates.io",
  "registry.npmjs.org", "registry.yarnpkg.com", "pypi.org", "files.pythonhosted.org",
  "static.rust-lang.org", "sh.rustup.rs", "archive.ubuntu.com", "security.ubuntu.com", "deb.debian.org",
  "repo.maven.apache.org", "plugins.gradle.org", "services.gradle.org", "downloads.gradle.org", "dl.google.com"
];
const CREDENTIAL_ARGUMENT = /authorization\s*:|bearer\s+|(?:api[_ -]?key|access[_ -]?token|refresh[_ -]?token|password|passphrase|client[_ -]?secret|service[_ -]?role|private[_ -]?key|secret)\s*[:=]|-----BEGIN |\bsk-[A-Za-z0-9_-]{12,}|\bsb_secret_|\beyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\./i;
const MAX_BODY_BYTES = 32 * 1024;
const MAX_OUTPUT_BYTES = 12 * 1024;

export function shouldDeletePreviousSnapshot(previousSnapshotId, nextSnapshotId, sharedBaseSnapshotId) {
  return typeof previousSnapshotId === "string"
    && /^snap_[A-Za-z0-9_-]{8,96}$/.test(previousSnapshotId)
    && previousSnapshotId !== nextSnapshotId
    && previousSnapshotId !== sharedBaseSnapshotId;
}
const SESSION_TIMEOUT_MS = 55_000;
const DEFAULT_BASE_SNAPSHOT_ID = "snap_P9PmDJ8xGmMCwSWiMZMYB0sz7CA4";
export const config = { maxDuration: 60 };

function send(res, status, payload) {
  res.setHeader("Cache-Control", "no-store");
  res.setHeader("X-Content-Type-Options", "nosniff");
  res.setHeader("Access-Control-Allow-Origin", "*");
  res.setHeader("Vary", "Authorization, Origin");
  return res.status(status).json(payload);
}

export function redact(value) {
  return String(value || "").split(/\r?\n/).map((line) => {
    if (/authorization\s*:|bearer\s+|(?:api[_ -]?key|access[_ -]?token|refresh[_ -]?token|password|passphrase|client[_ -]?secret|service[_ -]?role|private[_ -]?key|secret)\s*[:=]|-----BEGIN |\bsk-[A-Za-z0-9_-]{12,}|\bsb_secret_|\beyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\./i.test(line)) {
      return "[redacted sensitive output]";
    }
    return line;
  }).join("\n").slice(0, MAX_OUTPUT_BYTES);
}

export function parseRequest(body) {
  if (!body || typeof body !== "object" || Array.isArray(body)) throw new Error("Invalid request body");
  const command = String(body.command || "");
  if (!ALLOWED_COMMANDS.has(command)) throw new Error("Command is not in the CenAI sandbox allowlist");
  const args = body.arguments ?? [];
  if (!Array.isArray(args) || args.length > 128 || args.some((v) => typeof v !== "string" || v.length > 4096 || v.includes("\0") || CREDENTIAL_ARGUMENT.test(v))) {
    throw new Error("Sandbox arguments exceed the allowed limits");
  }
  const projectId = String(body.projectId || "default");
  if (!/^[A-Za-z0-9_-]{1,80}$/.test(projectId)) throw new Error("Invalid project identifier");
  const requestedCwd = String(body.workingDirectory || ".");
  if (requestedCwd.length > 512 || isAbsolute(requestedCwd) || requestedCwd.includes("\\") || requestedCwd.includes("\0")) {
    throw new Error("Working directory must remain inside the project workspace");
  }
  const projectRoot = `/vercel/sandbox/projects/${projectId}`;
  const cwd = resolve(projectRoot, requestedCwd);
  if (cwd !== projectRoot && !cwd.startsWith(projectRoot + sep)) throw new Error("Working directory must remain inside the project workspace");
  return { command, args, projectId, projectRoot, cwd };
}

async function supabaseRpc(base, key, token, name, args = {}) {
  const response = await fetch(`${base.replace(/\/$/, "")}/rest/v1/rpc/${name}`, {
    method: "POST",
    headers: { apikey: key, Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
    body: JSON.stringify(args)
  });
  const text = await response.text();
  if (!response.ok) throw new Error(`Supabase ${name} failed (${response.status})`);
  if (!text) return null;
  try { return JSON.parse(text); } catch { return text; }
}

export default async function handler(req, res) {
  if (req.method === "OPTIONS") {
    res.setHeader("Access-Control-Allow-Origin", "*");
    res.setHeader("Access-Control-Allow-Headers", "authorization, content-type, apikey");
    res.setHeader("Access-Control-Allow-Methods", "POST, OPTIONS");
    return res.status(204).end();
  }
  if (req.method !== "POST") {
    res.setHeader("Allow", "POST, OPTIONS");
    return send(res, 405, { error: "Method not allowed" });
  }

  const base = process.env.SUPABASE_URL;
  const publishableKey = process.env.SUPABASE_PUBLISHABLE_KEY;
  if (!base || !publishableKey) return send(res, 503, { error: "Sandbox service is not configured" });

  const authorization = String(req.headers.authorization || "");
  const token = authorization.match(/^Bearer\s+(.+)$/i)?.[1];
  if (!token || token.length > 8192) return send(res, 401, { error: "A valid CenAI session is required" });

  let input;
  try {
    const raw = typeof req.body === "string" ? req.body : JSON.stringify(req.body ?? {});
    if (Buffer.byteLength(raw, "utf8") > MAX_BODY_BYTES) return send(res, 413, { error: "Sandbox request is too large" });
    input = parseRequest(typeof req.body === "string" ? JSON.parse(req.body) : req.body);
  } catch (error) {
    return send(res, 400, { error: error instanceof Error ? error.message : "Invalid sandbox request" });
  }

  let runId = null;
  let sandbox = null;
  let SnapshotApi = null;
  let previousSnapshotId = null;
  let workspaceSaved = false;
  const startedAt = Date.now();
  try {
    const identity = await fetch(`${base.replace(/\/$/, "")}/auth/v1/user`, {
      headers: { apikey: publishableKey, Authorization: `Bearer ${token}` }
    });
    const user = await identity.json().catch(() => ({}));
    if (!identity.ok || !/^[0-9a-f-]{36}$/i.test(String(user.id || ""))) return send(res, 401, { error: "Invalid CenAI session" });

    const registered = await supabaseRpc(base, publishableKey, token, "cenai_registration_allowed");
    if (registered !== true) return send(res, 403, { error: "CenAI account registration must be verified before sandbox access" });

    runId = await supabaseRpc(base, publishableKey, token, "cenai_claim_sandbox_run");
    if (typeof runId !== "string" || !/^[0-9a-f-]{36}$/i.test(runId)) return send(res, 429, { error: "Sandbox usage limit reached. Try again later." });

    previousSnapshotId = await supabaseRpc(base, publishableKey, token, "cenai_get_sandbox_workspace");
    const baseSnapshotId = process.env.CENAI_SANDBOX_BASE_SNAPSHOT_ID || DEFAULT_BASE_SNAPSHOT_ID;
    const restoreSnapshotId = typeof previousSnapshotId === "string" && /^snap_[A-Za-z0-9_-]{8,96}$/.test(previousSnapshotId)
      ? previousSnapshotId
      : baseSnapshotId;
    // Load the Node-only SDK only after method, request and account checks. A
    // module-load failure must not crash the whole function before it can return
    // a clear 401/405 response to unauthenticated probes or browser requests.
    const sandboxSdk = await import("@vercel/sandbox");
    const SandboxApi = sandboxSdk.Sandbox;
    SnapshotApi = sandboxSdk.Snapshot;
    if (!SandboxApi || !SnapshotApi) throw new Error("Vercel Sandbox SDK exports are unavailable");
    sandbox = await SandboxApi.create({
      source: { type: "snapshot", snapshotId: restoreSnapshotId },
      region: "sin1",
      resources: { vcpus: 1 },
      timeout: SESSION_TIMEOUT_MS,
      networkPolicy: { allow: NETWORK_ALLOWLIST }
    });

    const mkdir = await sandbox.runCommand({ cmd: "mkdir", args: ["-p", input.cwd], cwd: "/vercel/sandbox" });
    if (mkdir.exitCode !== 0) throw new Error("Could not initialize the isolated project workspace");

    const result = await sandbox.runCommand({ cmd: input.command, args: input.args, cwd: input.cwd });
    const newSnapshot = await sandbox.snapshot({ expiration: 2_592_000_000 });
    const saved = await supabaseRpc(base, publishableKey, token, "cenai_save_sandbox_workspace", { p_snapshot_id: newSnapshot.snapshotId });
    if (saved !== true) throw new Error("Sandbox workspace snapshot could not be saved to the account");
    workspaceSaved = true;
    sandbox = null; // snapshot() stops the microVM automatically
    // A shared base image is never an account-owned workspace snapshot and must never be deleted here.
    if (shouldDeletePreviousSnapshot(previousSnapshotId, newSnapshot.snapshotId, baseSnapshotId)) {
      try { await (await SnapshotApi.get({ snapshotId: previousSnapshotId })).delete(); } catch { /* Expired snapshots are harmless; retention cleanup is best effort. */ }
    }
    const exitCode = Number.isInteger(result.exitCode) ? result.exitCode : 1;
    const durationMs = Math.min(120_000, Date.now() - startedAt);
    const finished = await supabaseRpc(base, publishableKey, token, "cenai_finish_sandbox_run", {
      p_run_id: runId,
      p_status: exitCode === 0 ? "succeeded" : "failed",
      p_duration_ms: durationMs,
      p_exit_code: exitCode
    });
    if (finished !== true) throw new Error("Sandbox run could not be finalized in the account ledger");

    return send(res, 200, {
      runId,
      projectId: input.projectId,
      status: exitCode === 0 ? "succeeded" : "failed",
      exitCode,
      durationMs,
      stdout: redact(await result.stdout()),
      stderr: redact(await result.stderr())
    });
  } catch (error) {
    if (sandbox && !workspaceSaved) {
      try {
        const recoverySnapshot = await sandbox.snapshot({ expiration: 2_592_000_000 });
        await supabaseRpc(base, publishableKey, token, "cenai_save_sandbox_workspace", { p_snapshot_id: recoverySnapshot.snapshotId });
        sandbox = null;
      } catch {
        try { await sandbox.stop(); } catch { /* Session may already have stopped. */ }
      }
    }
    if (runId) {
      try {
        await supabaseRpc(base, publishableKey, token, "cenai_finish_sandbox_run", {
          p_run_id: runId,
          p_status: "failed",
          p_duration_ms: Math.min(120_000, Date.now() - startedAt),
          p_exit_code: 1
        });
      } catch { /* Preserve the original failure; the database run remains auditable as running. */ }
    }
    console.error("CenAI sandbox execution failed", error instanceof Error ? error.message : "unknown error");
    return send(res, 502, { error: "Isolated sandbox execution failed. No secrets or raw environment values were forwarded." });
  }
}
