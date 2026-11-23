import test from "node:test";
import assert from "node:assert/strict";
import { parseRequest, redact, shouldDeletePreviousSnapshot } from "../api/sandbox.js";

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
