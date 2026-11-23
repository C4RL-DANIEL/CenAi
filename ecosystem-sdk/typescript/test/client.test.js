import test from "node:test";
import assert from "node:assert/strict";
import { CenAiEcosystemClient } from "../src/client.js";

const TOKEN = "eyJhbGciOiJIUzI1NiJ9.test.signature";
const responseFor = (requestId, value, status = 200) => ({
  ok: status >= 200 && status < 300,
  status,
  async json() {
    const bytes = new TextEncoder().encode(JSON.stringify(value));
    let binary = "";
    for (const byte of bytes) binary += String.fromCharCode(byte);
    return { protocolVersion: 1, requestId, accepted: true, payloadBase64: btoa(binary), errorCode: null };
  },
});

function makeClient(fetchImpl) {
  return new CenAiEcosystemClient({
    supabaseUrl: "https://example.supabase.co/",
    publishableKey: "sb_publishable_test",
    appId: "com.example.notes",
    userId: "550e8400-e29b-41d4-a716-446655440011",
    getAccessToken: async () => TOKEN,
    fetchImpl,
  });
}

test("writes context using the versioned authenticated envelope and decodes response", async () => {
  let captured;
  const client = makeClient(async (url, options) => {
    captured = { url, options, envelope: JSON.parse(options.body) };
    return responseFor(captured.envelope.requestId, { saved: true, key: "draft" });
  });
  const result = await client.writeContext("draft", { text: "hello" });
  assert.deepEqual(result, { saved: true, key: "draft" });
  assert.equal(captured.url, "https://example.supabase.co/functions/v1/cenai-ecosystem");
  assert.equal(captured.options.headers.Authorization, `Bekkºwµç