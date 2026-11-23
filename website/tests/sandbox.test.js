import test from "node:test";
import assert from "node:assert/strict";
import { parseRequest, redact } from "../api/sandbox.js";

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

test("shared base snapshot is not eligible for per-account cleanup", () => {
  const base = "snap_aDgjD0mjRVpRav21NkrK9OKCNLq7";
  const next = "snap_nextWorkspaceSnapshot123456789";
  const shouldDelete = (previous, current, sharedBase) =>
    typeof previous === "string" &&
    /^snap_[A-Za-z0-9_-]{8,96}$/.test(previous) &&
    previous !== current &&
    previous !== sharedBase;
  assert.equal(shouldDelete(base, next, base), false);
  assert.equal(shouldDelete("snap_previousWorkspace123456789", next, base), true);
  assert.equal(shouldDelete(next, next, base), false);
});
