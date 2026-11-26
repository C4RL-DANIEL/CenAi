import test from "node:test";
import assert from "node:assert/strict";
import handler, { parseRequest, redact, shouldDeletePreviousSnapshot } from "../api/sandbox.js";


function mockResponse() {
  return {
    statusCode: 200,
    headers: {},
    payload: null,
    ended: false,
    setHeader(name, value) { this.headers[name] = value; },
    status(code) { this.statusCode = code; return this; },
    json(payload) { this.payload = payload; return this; },
    end() { this.ended = true; return this; }
  };
}

test("Vercel Sandbox SDK exposes the expected Node runtime API", async () => {
  const sdk = await import("@vercel/sandbox");
  assert.equal(typeof sdk.Sandbox?.create, "function");
  assert.equal(typeof sdk.Snapshot?.get, "function");
});

test("sandbox route returns method errors without loading the Node-only SDK", async () => {
  const response = mockResponse();
  await handler({ method: "GET", headers: {} }, response);
  assert.equal(response.statusCode, 405);
  assert.equal(response.payload.error, "Method not allowed");
});

test("sandbox route rejects unauthenticated execution before creating a microVM", async () => {
  const previousUrl = process.env.SUPABASE_URL;
  const previousKey = process.env.SUPABASE_PUBLISHABLE_KEY;
  process.env.SUPABASE_URL = "https://example.supabase.co";
  process.env.SUPABASE_PUBLISHABLE_KEY = "sb_publishable_test_key";
  try {
    const response = mockResponse();
    await handler({ method: "POST", headers: {}, body: { command: "pwd", arguments: [] } }, response);
    assert.equal(response.statusCode, 401);
    assert.equal(response.payload.error, "A valid CenAI session is required");
  } finally {
    if (previousUrl === undefined) delete process.env.SUPABASE_URL;
    else process.env.SUPABASE_URL = previousUrl;
    if (previousKey === undefined) delete process.env.SUPABASE_PUBLISHABLE_KEY;
    else process.env.SUPABASE_PUBLISHABLE_KEY = previousKey;
  }
});

test("accepts bounded commands in a project-scoped working directory", () => {
  const request = parseRequest({
    command: "cargo",
    arguments: ["test", "-p", "cenai-native"],
    projectId: "native_core",
    workingDirectory: "native"
  });
  assert.equal(request.command, "cargo");
  assert.equal(request.cwd, "/vercel/sandbox/projects/native_core/native");
});

test("rejects commands outside the allowlist", () => {
  assert.throws(() => parseRequest({ command: "sudo", arguments: [] }), /allowlist/);
});

test("rejects path traversal and absolute working directories", () => {
  assert.throws(() => parseRequest({ command: "pwd", workingDirectory: "../../etc" }), /inside the project workspace/);
  assert.throws(() => parseRequest({ command: "pwd", workingDirectory: "/tmp" }), /inside the project workspace/);
});

test("rejects credentials embedded in command arguments", () => {
  assert.throws(() => parseRequest({ command: "bash", arguments: ["--api-key=private-value"] }), /arguments/);
  assert.throws(() => parseRequest({ command: "bash", arguments: ["Bearer eyJhbGciOiJIUzI1NiJ9.payload.signature"] }), /arguments/);
});

test("redacts sensitive output before it reaches the app", () => {
  assert.equal(redact("compile ok\nAPI_KEY=secret-value\nBearer eyJabc.def.ghi"), "compile ok\n[redacted sensitive output]\n[redacted sensitive output]");
});

test("shared base snapshot is never deleted by per-account cleanup", () => {
  const base = "snap_aDgjD0mjRVpRav21NkrK9OKCNLq7";
  const next = "snap_nextWorkspaceSnapshot123456789";
  assert.equal(shouldDeletePreviousSnapshot(base, next, base), false);
  assert.equal(shouldDeletePreviousSnapshot("snap_previousWorkspace123456789", next, base), true);
  assert.equal(shouldDeletePreviousSnapshot(next, next, base), false);
  assert.equal(shouldDeletePreviousSnapshot("not-a-snapshot", next, base), false);
});
