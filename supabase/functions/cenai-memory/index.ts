import "jsr:@supabase/functions-js/edge-runtime.d.ts";
import { createClient } from "npm:@supabase/supabase-js@2.57.4";

const url = Deno.env.get("SUPABASE_URL")!;
const anonKey = Deno.env.get("SUPABASE_ANON_KEY")!;
const serviceKey = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!;
const authDb = createClient(url, anonKey, { auth: { persistSession: false, autoRefreshToken: false } });
const adminDb = createClient(url, serviceKey, { auth: { persistSession: false, autoRefreshToken: false } });
const cors = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Headers": "authorization, apikey, content-type, x-client-info",
  "Access-Control-Allow-Methods": "POST, OPTIONS",
  "Content-Type": "application/json",
  "Cache-Control": "no-store",
};
const respond = (status: number, body: unknown) =>
  new Response(JSON.stringify(body), { status, headers: cors });
const fail = (status: number, message: string) => respond(status, { error: message });

function containsCredentialMaterial(value: string) {
  return /-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----/i.test(value)
    || /\bBearer\s+[A-Za-z0-9._~+/-]{20,}/i.test(value)
    || /\b(?:sk-[A-Za-z0-9_-]{20,}|sb_secret_[A-Za-z0-9_-]{20,})\b/.test(value)
    || /\beyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\b/.test(value)
    || /\b(password|passphrase|api[ _-]?key|access[ _-]?token|refresh[ _-]?token)\s*[:=]/i.test(value);
}

Deno.serve(async (req: Request) => {
  if (req.method === "OPTIONS") return new Response("ok", { headers: cors });
  if (req.method !== "POST") return fail(405, "Method not allowed");
  try {
    const authorization = req.headers.get("Authorization") ?? "";
    const token = authorization.replace(/^Bearer\s+/i, "");
    if (!token) return fail(401, "Sign in required");
    const { data: { user }, error: authError } = await authDb.auth.getUser(token);
    if (authError || !user || user.is_anonymous) return fail(401, "Invalid or expired session");

    const raw = await req.text();
    if (raw.length > 32_000) return fail(413, "Memory request is too large");
    let body: Record<string, unknown>;
    try {
      const parsed: unknown = JSON.parse(raw || "{}");
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return fail(400, "Request body must be a JSON object");
      body = parsed as Record<string, unknown>;
    } catch {
      return fail(400, "Request body must contain valid JSON");
    }
    const action = String(body.action ?? "");

    if (action === "memory_status") {
      const { data, error } = await adminDb.from("user_memory_settings")
        .select("enabled").eq("user_id", user.id).maybeSingle();
      if (error) throw error;
      return respond(200, { enabled: data?.enabled === true });
    }

    if (action === "memory_set_enabled") {
      if (typeof body.enabled !== "boolean") return fail(400, "A boolean memory preference is required");
      const { error } = await adminDb.from("user_memory_settings").upsert({
        user_id: user.id,
        enabled: body.enabled,
        updated_at: new Date().toISOString(),
      }, { onConflict: "user_id" });
      if (error) throw error;
      return respond(200, { enabled: body.enabled });
    }

    if (action === "memory_list") {
      const { data: setting, error: settingError } = await adminDb.from("user_memory_settings")
        .select("enabled").eq("user_id", user.id).maybeSingle();
      if (settingError) throw settingError;
      if (setting?.enabled !== true) return fail(403, "Personal memory is disabled for this account");
      const { data, error } = await adminDb.from("user_personal_memories")
        .select("id,category,content,source,created_at,updated_at")
        .eq("user_id", user.id).order("updated_at", { ascending: false }).limit(100);
      if (error) throw error;
      return respond(200, { memories: data ?? [] });
    }

    if (action === "memory_save") {
      const { data: setting, error: settingError } = await adminDb.from("user_memory_settings")
        .select("enabled").eq("user_id", user.id).maybeSingle();
      if (settingError) throw settingError;
      if (setting?.enabled !== true) return fail(403, "Enable personal memory before saving memories");
      const category = String(body.category ?? "");
      const content = String(body.content ?? "").trim();
      const source = String(body.source ?? "user");
      if (!["preference", "profile", "project", "workflow", "fact"].includes(category)
        || !["user", "imported"].includes(source)
        || content.length < 1 || content.length > 8000) {
        return fail(400, "Invalid memory category, source, or content length");
      }
      if (containsCredentialMaterial(content)) {
        return fail(400, "Do not save passwords, API keys, access tokens, private keys, or other credentials as personal memory");
      }
      const { count, error: countError } = await adminDb.from("user_personal_memories")
        .select("id", { count: "exact", head: true }).eq("user_id", user.id);
      if (countError) throw countError;
      if ((count ?? 0) >= 100) return fail(409, "Personal memory is full. Delete an existing memory first");
      const { data, error } = await adminDb.from("user_personal_memories")
        .insert({ user_id: user.id, category, content, source })
        .select("id,category,content,source,created_at,updated_at").single();
      if (error) throw error;
      return respond(201, { memory: data });
    }

    if (action === "memory_delete") {
      const memoryId = String(body.memoryId ?? "");
      if (!/^[0-9a-f-]{36}$/i.test(memoryId)) return fail(400, "Invalid memory identifier");
      const { error } = await adminDb.from("user_personal_memories")
        .delete().eq("id", memoryId).eq("user_id", user.id);
      if (error) throw error;
      return respond(200, { deleted: true });
    }

    return fail(400, "Unknown memory action");
  } catch (error) {
    console.error("personal-memory request failed", error);
    return fail(500, "CenAI personal-memory request failed");
  }
});
