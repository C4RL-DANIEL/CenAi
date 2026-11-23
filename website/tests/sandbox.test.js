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
