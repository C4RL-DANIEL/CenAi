# CenAI Ecosystem Protocol v1

This is the versioned request/response contract for apps that integrate with CenAI. The server derives the account from the caller's Supabase access token; `userScope` is only a consistency check and is never an authority source.

## Authentication and endpoint

Call `POST {SUPABASE_URL}/functions/v1/cenai-ecosystem` with the signed-in user's Supabase access token in the Authorization bearer header. Never embed or send the Supabase service-role key in an app.

An app must already have a registered v1 manifest and an explicit, non-revoked per-user grant for the requested capability. Registration and grant changes retain their existing MFA step-up requirement.

## Request envelope

The JSON envelope uses camelCase field names. `payloadBase64` is Base64 of UTF-8 JSON bytes, with a 64 KiB decoded limit. Request IDs should be cryptographically random tokens of at least 32 characters. Requests expire after five minutes and are consumed once by the server.

```json
{
  "action": "invoke",
  "protocolVersion": 1,
  "requestId": "0123456789abcdef0123456789abcdef",
  "appId": "com.example.notes",
  "userScope": "550e8400-e29b-41d4-a716-446655440011",
  "capability": "context.write",
  "createdAtUnixMs": 1795453200000,
  "payloadBase64": "eyJrZXkiOiJkcmFmdHMvNDIiLCJ2YWx1ZSI6eyJ0aXRsZSI6IkhlbGxvIn19"
}
```

The Edge Function validates the JWT, app registration, declared/requested capability, current user grant, request timestamp and replay nonce. A SQL transaction locks the app/grant rows while it consumes the request ID, so a concurrent revocation cannot race an accepted call.

## v1 capabilities

- `context.read`: payload `{"key":"drafts/42"}`; returns `{key,found,value,updatedAt}`.
- `context.write`: payload `{"key":"drafts/42","value":{"title":"Hello"}}`; returns `{key,saved,updatedAt}`.
- `context.delete`: payload `{"key":"drafts/42"}`; returns `{key,deleted}`.

Context keys are limited to 128 characters. JSON values are limited to 60 KiB. Each app has a private namespace keyed by (user, app, key); one app cannot read another app's context. Every operation requires the matching capability to be declared, requested and granted.

## Example with supabase-js

```ts
const encoder = new TextEncoder();
const payloadBytes = encoder.encode(JSON.stringify({
  key: "drafts/42",
  value: { title: "Hello from my app" }
}));
const payloadBase64 = btoa(Array.from(payloadBytes, byte => String.fromCharCode(byte)).join(""));
const requestId = Array.from(crypto.getRandomValues(new Uint8Array(32)),
  byte => byte.toString(16).padStart(2, "0")).join("");

const { data, error } = await supabase.functions.invoke("cenai-ecosystem", {
  body: {
    action: "invoke",
    protocolVersion: 1,
    requestId,
    appId: "com.example.notes",
    userScope: session.user.id,
    capability: "context.write",
    createdAtUnixMs: Date.now(),
    payloadBase64
  }
});
if (error) throw error;
if (!data.accepted || data.requestId !== requestId) throw new Error("CenAI rejected the request");
const responseBytes = Uint8Array.from(atob(data.payloadBase64), char => char.charCodeAt(0));
const result = JSON.parse(new TextDecoder().decode(responseBytes));
```

## Response envelope

Successful responses echo `protocolVersion` and `requestId`, set `accepted: true`, include a Base64 JSON `payloadBase64`, and set `errorCode: null`. HTTP errors reject the call; create a new request ID for a retry. Never replay a consumed request ID.

The server audit log records app ID and capability only. It does not record context keys, values, prompts or payload bytes.
