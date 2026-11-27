import { isBreached, validatePassword } from "./policy.ts";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

Deno.test("password policy enforces length, UTF-8 size and printable characters", () => {
  assert(validatePassword("short") === "Use at least 12 characters.", "minimum length must be enforced");
  assert(validatePassword("é".repeat(37)) === "Use a password no longer than 72 UTF-8 bytes.", "UTF-8 byte ceiling must be enforced");
  assert(validatePassword("valid-password\n123") === "Choose a password containing printable characters.", "control characters must be rejected");
  assert(validatePassword("aaaaaaaaaaaa") === "Avoid passwords made from one repeated character.", "repeated-character passwords must be rejected");
  assert(validatePassword("correct horse battery staple") === null, "a strong passphrase should pass local policy");
});

Deno.test("breach lookup uses only the five-character SHA-1 prefix and detects a known breach", async () => {
  let requestedUrl = "";
  let padding = "";
  const breached = await isBreached("password", async (input, init) => {
    requestedUrl = String(input);
    padding = new Headers(init?.headers).get("Add-Padding") || "";
    return new Response("1E4C9B93F3F0682250B6CF8331B7EE68FD8:10\n", { status: 200 });
  });
  assert(breached, "known breached password should be detected");
  assert(requestedUrl === "https://api.pwnedpasswords.com/range/5BAA6", "only the five-character hash prefix may be sent");
  assert(padding === "true", "HIBP response padding must be requested");
  assert(!requestedUrl.includes("password") && !requestedUrl.includes("5BAA61E4"), "password and full hash must not leave the function");
});

Deno.test("breach lookup allows hashes absent from the returned range", async () => {
  const breached = await isBreached("a-different-long-passphrase", async () => new Response("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA:2\n", { status: 200 }));
  assert(!breached, "unlisted suffix should not be treated as breached");
});

Deno.test("breach lookup fails closed when the provider is unavailable", async () => {
  let failed = false;
  try {
    await isBreached("a-different-long-passphrase", async () => new Response("unavailable", { status: 503 }));
  } catch (error) {
    failed = error instanceof Error && error.message === "BREACH_CHECK_UNAVAILABLE";
  }
  assert(failed, "provider failure must be surfaced so the caller can fail closed");
});
