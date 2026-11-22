import "jsr:@supabase/functions-js/edge-runtime.d.ts";
import { createClient } from "npm:@supabase/supabase-js@2.57.4";

const url = Deno.env.get("SUPABASE_URL")!;
const anonKey = Deno.env.get("SUPABASE_ANON_KEY")!;
const serviceKey = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!;
const authDb = createClient(url, anonKey, { auth: { persistSession: false, autoRefreshToken: false } });
const db = createClient(url, serviceKey, { auth: { persistSession: false, autoRefreshToken: false } });
const headers = { "Access-Control-Allow-Origin": "*", "Access-Control-Allow-Headers": "authorization, apikey, content-type, x-client-info", "Access-Control-Allow-Methods": "POST, OPTIONS", "Content-Type": "application/json", "Cache-Control": "no-store" };
const respond = (status: number, body: unknown) => new Response(JSON.stringify(body), { status, headers });
const fail = (status: number, error: string) => respond(status, { error });
const validId = (value: unknown, max = 128): value is string => typeof value === "string" && value.length > 0 && value.length <= max && /^[a-zA-Z0-9][a-zA-Z0-9._-]*$/.test(value);
const validList = (value: unknown, maxItems = 64): value is string[] => Array.isArray(value) && value.length > 0 && value.length <= maxItems && value.every(item => validId(item, 128)) && new Set(value).size === value.length;
const validScopes = (value: unknown): value is string[] => Array.isArray(value) && value.length > 0 && value.length <= 64
  && value.every(item => typeof item === "string" && item.length > 0 && item.length <= 128 && /^[A-Za-z0-9][A-Za-z0-9:._-]*$/.test(item))
  && new Set(value).size === value.length;

const MAX_ECOSYSTEM_PAYLOAD_BYTES = 64 * 1024;
const MAX_CONTEXT_VALUE_BYTES = 60 * 1024;
const validRequestId = (value: unknown): value is string =>
  typeof value === "string" && value.length >= 32 && value.length <= 128 && /^[A-Za-z0-9._:-]+$/.test(value);
const validContextKey = (value: unknown): value is string =>
  typeof value === "string" && value.length >= 1 && value.length <= 128 && /^[A-Za-z0-9][A-Za-z0-9._:-]*$/.test(value);
function decodeBase64Payload(value: unknown): Uint8Array | null {
  if (typeof value !== "string" || value.length > Math.ceil(MAX_ECOSYSTEM_PAYLOAD_BYTES / 3) * 4 + 4
    || !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(value)) return null;
  try {
    const binary = atob(value);
    if (binary.length > MAX_ECOSYSTEM_PAYLOAD_BYTES) return null;
    return Uint8Array.from(binary, character => character.charCodeAt(0));
  } catch { return null; }
}
function encodeBase64Payload(bytes: Uint8Array): string {
  let binary = "";
  for (let offset = 0; offset < bytes.length; offset += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000));
  }
  return btoa(binary);
}
async function audit(actorUserId: string, appId: string, action: string, details: Record<string, unknown> = {}) {
  const { error } = await db.from("ecosystem_audit_logs").insert({ actor_user_id: actorUserId, app_id: appId, action, details });
  if (error) throw error;
}

Deno.serve(async (req: Request) => {
  if (req.method === "OPTIONS") return new Response("ok", { headers });
  if (req.method !== "POST") return fail(405, "Method not allowed");
  try {
    const token = (req.headers.get("Authorization") ?? "").replace(/^Bearer\s+/i, "");
    if (!token) return fail(401, "Sign in required");
    const { data: { user }, error: authError } = await authDb.auth.getUser(token);
    if (authError || !user || user.is_anonymous) return fail(401, "Invalid or expired session");
    const rawBytes = new Uint8Array(await req.arrayBuffer());
    if (rawBytes.byteLength > 100_000) return fail(413, "Ecosystem request is too large");
    let raw: string;
    try { raw = new TextDecoder("utf-8", { fatal: true }).decode(rawBytes); }
    catch { return fail(400, "Request must be valid UTF-8 JSON"); }
    let body: Record<string, unknown>;
    try {
      const parsed: unknown = JSON.parse(raw || "{}");
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return fail(400, "Request must be a JSON object");
      body = parsed as Record<string, unknown>;
    } catch { return fail(400, "Request must contain valid JSON"); }
    const action = String(body.action ?? "");
    // Supabase has already verified this JWT through getUser; use its signed assurance claim.
    const encodedClaims = token.split(".")[1] ?? "";
    const normalizedClaims = encodedClaims.replace(/-/g, "+").replace(/_/g, "/");
    const claims = JSON.parse(atob(normalizedClaims.padEnd(Math.ceil(normalizedClaims.length / 4) * 4, "=")));
    if (["register", "grant", "revoke"].includes(action) && claims.aal !== "aal2") {
      return respond(403, { error: "Complete authenticator verification before changing Ecosystem permissions", code: "mfa_required" });
    }

    if (action === "list") {
      const { data: apps, error: appError } = await db.from("ecosystem_apps").select("app_id,publisher_id,version,protocol_version,description,capabilities,requested_scopes,updated_at").eq("enabled", true).order("app_id").limit(200);
      if (appError) throw appError;
      const { data: grants, error: grantError } = await db.from("ecosystem_user_grants").select("app_id,granted_scopes,revoked,updated_at").eq("user_id", user.id).limit(200);
      if (grantError) throw grantError;
      return respond(200, { apps: apps ?? [], grants: grants ?? [] });
    }

    if (action === "register") {
      const appId = body.appId, publisherId = body.publisherId, version = body.version;
      const protocolVersion = body.protocolVersion;
      const description = typeof body.description === "string" ? body.description.trim() : "";
      const capabilities = body.capabilities, scopes = body.requestedScopes;
      if (!validId(appId, 160) || !validId(publisherId, 128) || typeof version !== "string"
        || !/^[0-9A-Za-z.+-]{1,64}$/.test(version) || protocolVersion !== 1 || description.length > 1000
        || !validList(capabilities) || !validScopes(scopes)) return fail(400, "Invalid app manifest or unsupported ecosystem protocol");
      if (!capabilities.every(capability => scopes.includes(capability))) return fail(400, "Every declared capability must appear in requested scopes");
      const { data: publisher, error: publisherError } = await db.from("ecosystem_trusted_publishers").select("owner_user_id,enabled").eq("publisher_id", publisherId).maybeSingle();
      if (publisherError) throw publisherError;
      const { data: adminRecord, error: adminError } = await db.from("admin_users")
        .select("role,is_active").eq("user_id", user.id).maybeSingle();
      if (adminError) throw adminError;
      const isAdmin = adminRecord?.is_active === true && ["owner", "admin"].includes(String(adminRecord.role ?? ""));
      if (!publisher || publisher.enabled !== true || (publisher.owner_user_id !== user.id && !isAdmin)) return fail(403, "Publisher is not trusted for this account");
      const { data: existing, error: existingError } = await db.from("ecosystem_apps").select("publisher_id").eq("app_id", appId).maybeSingle();
      if (existingError) throw existingError;
      if (existing && existing.publisher_id !== publisherId) return fail(409, "App ID is permanently bound to another publisher");
      const { data, error } = await db.from("ecosystem_apps").upsert({ app_id: appId, publisher_id: publisherId, version, protocol_version: protocolVersion, description, capabilities, requested_scopes: scopes, enabled: true, updated_at: new Date().toISOString() }, { onConflict: "app_id" }).select("app_id,publisher_id,version,protocol_version,description,capabilities,requested_scopes,updated_at").single();
      if (error) throw error;
      await audit(user.id, appId, "manifest.registered", { publisher_id: publisherId });
      return respond(200, { app: data });
    }

    if (action === "grant") {
      const appId = body.appId, scopes = body.scopes;
      if (!validId(appId, 160) || !validScopes(scopes)) return fail(400, "App ID and non-empty capability scopes are required");
      const { data: app, error: appError } = await db.from("ecosystem_apps").select("capabilities,requested_scopes,enabled").eq("app_id", appId).maybeSingle();
      if (appError) throw appError;
      if (!app || app.enabled !== true) return fail(404, "Registered app not found");
      const allowed = new Set<string>([...(app.capabilities ?? []), ...(app.requested_scopes ?? [])]);
      if (scopes.some(scope => !allowed.has(scope))) return fail(403, "Grant exceeds the app's declared capabilities or scopes");
      const { data, error } = await db.from("ecosystem_user_grants").upsert({ user_id: user.id, app_id: appId, granted_scopes: scopes, revoked: false, updated_at: new Date().toISOString() }, { onConflict: "user_id,app_id" }).select("app_id,granted_scopes,revoked,updated_at").single();
      if (error) throw error;
      await audit(user.id, appId, "grant.updated", { scopes });
      return respond(200, { grant: data });
    }

    if (action === "revoke") {
      const appId = body.appId;
      if (!validId(appId, 160)) return fail(400, "Valid app ID is required");
      const { data, error } = await db.from("ecosystem_user_grants").update({ revoked: true, granted_scopes: [], updated_at: new Date().toISOString() }).eq("user_id", user.id).eq("app_id", appId).select("app_id,revoked,updated_at").maybeSingle();
      if (error) throw error;
      await audit(user.id, appId, "grant.revoked");
      return respond(200, { revoked: Boolean(data), grant: data });
    }

    if (action === "invoke") {
      const protocolVersion = body.protocolVersion;
      const requestId = body.requestId;
      const appId = body.appId;
      const userScope = body.userScope;
      const capability = body.capability;
      const createdAtUnixMs = body.createdAtUnixMs;
      if (protocolVersion !== 1 || !validId(appId, 160) || typeof capability !== "string" || !validScopes([capability])
        || !validRequestId(requestId) || typeof userScope !== "string" || userScope !== user.id
        || typeof createdAtUnixMs !== "number" || !Number.isSafeInteger(createdAtUnixMs)) {
        return fail(400, "Invalid ecosystem protocol envelope");
      }
      if (Math.abs(Date.now() - createdAtUnixMs) > 5 * 60 * 1000) {
        return fail(408, "Ecosystem request expired; create a fresh request ID");
      }
      if (!["context.read", "context.write", "context.delete"].includes(capability)) {
        return fail(400, "This ecosystem capability has no registered handler");
      }
      const payloadBytes = decodeBase64Payload(body.payloadBase64);
      if (!payloadBytes) return fail(400, "Ecosystem payload Base64 is invalid or exceeds 64 KB");
      let payload: Record<string, unknown>;
      try {
        const decoded = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(payloadBytes)) as unknown;
        if (!decoded || typeof decoded !== "object" || Array.isArray(decoded)) return fail(400, "Ecosystem payload must be a JSON object");
        payload = decoded as Record<string, unknown>;
      } catch { return fail(400, "Ecosystem payload must contain valid UTF-8 JSON"); }
      const key = payload.key;
      if (!validContextKey(key)) return fail(400, "Context key is invalid");
      if (capability === "context.write") {
        if (!Object.prototype.hasOwnProperty.call(payload, "value")) return fail(400, "Context write requires a value");
        const valueBytes = new TextEncoder().encode(JSON.stringify(payload.value));
        if (valueBytes.length > MAX_CONTEXT_VALUE_BYTES) return fail(413, "Context value exceeds the 60 KB storage limit");
      }

      // The SQL function locks the app/grant rows and consumes the nonce atomically.
      const { data: consumeStatus, error: consumeError } = await db.rpc("consume_ecosystem_request", {
        p_user_id: user.id,
        p_app_id: appId,
        p_capability: capability,
        p_request_id: requestId,
        p_created_at: new Date(createdAtUnixMs).toISOString()
      });
      if (consumeError) throw consumeError;
      if (consumeStatus === "replay") return fail(409, "Ecosystem request replay detected");
      if (consumeStatus !== "accepted") return fail(403, "App capability is not currently authorized");

      let result: Record<string, unknown>;
      if (capability === "context.read") {
        const { data, error } = await db.from("ecosystem_context_store").select("value,updated_at")
          .eq("user_id", user.id).eq("app_id", appId).eq("context_key", key).maybeSingle();
        if (error) throw error;
        result = { key, found: Boolean(data), value: data?.value ?? null, updatedAt: data?.updated_at ?? null };
      } else if (capability === "context.write") {
        const { data, error } = await db.from("ecosystem_context_store").upsert({
          user_id: user.id, app_id: appId, context_key: key, value: payload.value, updated_at: new Date().toISOString()
        }, { onConflict: "user_id,app_id,context_key" }).select("updated_at").single();
        if (error) throw error;
        result = { key, saved: true, updatedAt: data.updated_at };
      } else {
        const { data, error } = await db.from("ecosystem_context_store").delete()
          .eq("user_id", user.id).eq("app_id", appId).eq("context_key", key)
          .select("context_key").maybeSingle();
        if (error) throw error;
        result = { key, deleted: Boolean(data) };
      }
      await audit(user.id, appId, capability, {});
      const responseBytes = new TextEncoder().encode(JSON.stringify(result));
      if (responseBytes.length > MAX_ECOSYSTEM_PAYLOAD_BYTES) return fail(413, "Ecosystem response exceeds 64 KB");
      return respond(200, {
        protocolVersion: 1, requestId, accepted: true,
        payloadBase64: encodeBase64Payload(responseBytes), errorCode: null
      });
    }

    if (action === "authorize") {
      const appId = body.appId, capability = body.capability;
      if (!validId(appId, 160) || !validId(capability, 128)) return fail(400, "Valid app and capability identifiers are required");
      const { data: app, error: appError } = await db.from("ecosystem_apps").select("capabilities,requested_scopes,enabled").eq("app_id", appId).maybeSingle();
      if (appError) throw appError;
      const { data: grant, error: grantError } = await db.from("ecosystem_user_grants").select("granted_scopes,revoked").eq("user_id", user.id).eq("app_id", appId).maybeSingle();
      if (grantError) throw grantError;
      const authorized = app?.enabled === true && grant?.revoked === false && (app.capabilities ?? []).includes(capability) && (app.requested_scopes ?? []).includes(capability) && (grant.granted_scopes ?? []).includes(capability);
      return respond(200, { authorized: Boolean(authorized) });
    }
    return fail(400, "Unknown ecosystem action");
  } catch (error) {
    console.error("ecosystem request failed", error);
    return fail(500, "CenAI ecosystem request failed");
  }
});
