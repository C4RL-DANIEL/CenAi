import "jsr:@supabase/functions-js/edge-runtime.d.ts";
import { createClient } from "npm:@supabase/supabase-js@2.57.4";

const url = Deno.env.get("SUPABASE_URL")!;
const anonKey = Deno.env.get("SUPABASE_ANON_KEY")!;
const serviceKey = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!;
const authDb = createClient(url, anonKey, { auth: { persistSession: false, autoRefreshToken: false } });
const adminDb = createClient(url, serviceKey, { auth: { persistSession: false, autoRefreshToken: false } });
const headers = {
  "Content-Type": "application/json", "Cache-Control": "no-store", "X-Content-Type-Options": "nosniff",
  "Access-Control-Allow-Origin": "*", "Access-Control-Allow-Headers": "authorization, apikey, content-type, x-client-info",
  "Access-Control-Allow-Methods": "POST, OPTIONS"
};
const reply = (status: number, body: unknown) => new Response(JSON.stringify(body), { status, headers });
async function registrationAllowed(user: any) {
  const { data: policy, error } = await adminDb.from("registration_policy").select("enforced_from").eq("id", 1).maybeSingle();
  if (error) throw error;
  if (!policy?.enforced_from) throw new Error("Discord registration policy is not configured");
  const createdAt = Date.parse(String(user.created_at ?? ""));
  const enforcedFrom = Date.parse(String(policy.enforced_from));
  const legacy = Number.isFinite(createdAt) && Number.isFinite(enforcedFrom) && createdAt < enforcedFrom;
  const discord = user.app_metadata?.cenai_registration_verified === true && user.app_metadata?.cenai_registration_method === "discord";
  return legacy || discord;
}


Deno.serve(async (req: Request) => {
  if (req.method === "OPTIONS") return new Response("ok", { headers });
  if (req.method !== "POST") return reply(405, { error: "Method not allowed" });
  try {
    const token = (req.headers.get("Authorization") || "").replace(/^Bearer\s+/i, "").trim();
    if (!token) return reply(401, { error: "Sign in required" });
    const { data: { user }, error: authError } = await authDb.auth.getUser(token);
    if (authError || !user || user.is_anonymous) return reply(401, { error: "Valid signed-in account required" });
    const declaredLength = Number(req.headers.get("Content-Length") || 0);
    if (Number.isFinite(declaredLength) && declaredLength > 65536) {
      return reply(413, { error: "Quota request exceeds the 64 KB limit." });
    }
    if (!req.body) return reply(400, { error: "A JSON request body is required." });
    const reader = req.body.getReader();
    const chunks: Uint8Array[] = [];
    let totalBytes = 0;
    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        totalBytes += value.byteLength;
        if (totalBytes > 65536) {
          await reader.cancel("Quota request exceeds the 64 KB limit.");
          return reply(413, { error: "Quota request exceeds the 64 KB limit." });
        }
        chunks.push(value);
      }
    } finally {
      reader.releaseLock();
    }
    const bytes = new Uint8Array(totalBytes);
    let offset = 0;
    for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
    let body: Record<string, unknown>;
    try {
      body = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
    } catch {
      return reply(400, { error: "Quota request must be valid UTF-8 JSON." });
    }
    if (!body || typeof body !== "object" || Array.isArray(body)) {
      return reply(400, { error: "Quota request must be a JSON object." });
    }
    if (!await registrationAllowed(user)) return reply(403, { error: "Complete Discord invitation registration before using CenAI.", code: "REGISTRATION_REQUIRED" });
    const action = String(body.action || "");
    if (["authorize", "record", "voice_authorize", "voice_record", "voice_status"].includes(action)) {
      const { data: hasVerifiedMfa, error: mfaError } = await adminDb.rpc("cenai_user_has_verified_mfa", { p_user_id: user.id });
      if (mfaError) throw mfaError;
      const encodedClaims = token.split(".")[1] ?? "";
      const normalizedClaims = encodedClaims.replace(/-/g, "+").replace(/_/g, "/");
      const claims = JSON.parse(atob(normalizedClaims.padEnd(Math.ceil(normalizedClaims.length / 4) * 4, "=")));
      if (hasVerifiedMfa === true && claims.aal !== "aal2") {
        return reply(403, { error: "Complete authenticator verification before using managed AI services.", code: "MFA_REQUIRED" });
      }
    }
    if (action === "voice_status") {
      const { data: flag, error: flagError } = await adminDb.from("admin_feature_flags").select("enabled").eq("key", "voice.cloud.enabled").maybeSingle();
      if (flagError) throw flagError;
      if (flag?.enabled !== true) return reply(503, { error: "CenAI Voice is not enabled by the administrator yet.", code: "VOICE_CLOUD_DISABLED" });
      const dayStart = new Date(); dayStart.setUTCHours(0, 0, 0, 0);
      const { data: userRows, error: userError } = await adminDb.from("cenai_voice_usage_events").select("characters").eq("owner_user_id", user.id).gte("created_at", dayStart.toISOString());
      if (userError) throw userError;
      const dayKey = dayStart.toISOString().slice(0, 10);
      const { data: globalRow, error: globalError } = await adminDb.from("cenai_ai_daily_budget").select("reserved_neurons").eq("day_utc", dayKey).maybeSingle();
      if (globalError) throw globalError;
      const userUsed = (userRows || []).reduce((sum: number, row: any) => sum + Number(row.characters || 0), 0);
      const globalUsed = Number(globalRow?.reserved_neurons || 0);
      return reply(200, { ok: true, quota: { dailyCharacterLimit: 1000, dailyCharactersUsed: userUsed, voiceNeuronAllocation: 4000, globalNeuronLimit: 8000, globalNeuronsUsed: Number(globalUsed.toFixed(2)), resetsAt: new Date(dayStart.getTime() + 86400000).toISOString() } });
    }
    if (action === "voice_authorize") {
      const { data: flag, error: flagError } = await adminDb.from("admin_feature_flags").select("enabled").eq("key", "voice.cloud.enabled").maybeSingle();
      if (flagError) throw flagError;
      if (flag?.enabled !== true) return reply(503, { error: "CenAI Voice is not enabled by the administrator yet.", code: "VOICE_CLOUD_DISABLED" });
      const characters = Math.floor(Number(body.characters));
      if (!Number.isSafeInteger(characters) || characters < 1 || characters > 1000) return reply(400, { error: "Each voice request is limited to 1,000 characters.", code: "VOICE_REQUEST_LIMIT" });
      const { data: reservation, error: reserveError } = await adminDb.rpc("cenai_reserve_voice_usage", { p_user_id: user.id, p_characters: characters, p_user_daily_character_limit: 1000, p_global_daily_neuron_limit: 4000 });
      if (reserveError) { console.error("CenAI Voice quota reservation failed", reserveError.message); return reply(503, { error: "Could not reserve CenAI Voice quota.", code: "VOICE_QUOTA_UNAVAILABLE" }); }
      const row = Array.isArray(reservation) ? reservation[0] : reservation;
      if (!row?.allowed) return reply(429, { error: row?.reason === "global_daily_neuron_limit" ? "CenAI Voice has reached its 4,000-Neuron daily allocation. Try again after the daily reset." : row?.reason === "shared_global_daily_neuron_limit" ? "CenAI has reached its shared 8,000-Neuron daily AI budget. Try again after the UTC reset." : row?.reason === "user_daily_character_limit" ? "You have reached your daily CenAI Voice allowance." : "CenAI Voice request could not be authorized.", code: row?.reason || "VOICE_QUOTA_BLOCKED", quota: { dailyCharacterLimit: 1000, dailyCharactersUsed: row?.user_characters_used || 0, voiceNeuronAllocation: 4000, globalNeuronLimit: 8000, globalNeuronsUsed: Number(row?.global_neurons_used || 0) } });
      return reply(200, { ok: true, eventId: row.event_id, quota: { dailyCharacterLimit: 1000, dailyCharactersUsed: row.user_characters_used, voiceNeuronAllocation: 4000, globalNeuronLimit: 8000, globalNeuronsUsed: Number(row.global_neurons_used || 0) } });
    }
    if (action === "voice_record") {
      const eventId = Number(body.eventId);
      const status = String(body.status || "");
      if (!Number.isSafeInteger(eventId) || eventId < 1 || !["success", "error"].includes(status)) return reply(400, { error: "Invalid CenAI Voice usage event." });
      const { data, error } = await adminDb.rpc("cenai_record_voice_usage", { p_user_id: user.id, p_event_id: eventId, p_status: status });
      if (error) throw error;
      if (data !== true) return reply(404, { error: "CenAI Voice usage reservation was not found." });
      return reply(200, { ok: true, recorded: true });
    }
    if (action === "authorize") {
      const { data: flag, error: flagError } = await adminDb.from("admin_feature_flags").select("enabled").eq("key", "ai.cloud.enabled").maybeSingle();
      if (flagError) throw flagError;
      if (flag?.enabled !== true) return reply(503, { error: "Cloud AI is disabled by the CenAI administrator.", code: "CLOUD_AI_DISABLED" });
      const modelSlug = String(body.model || "cloudflare-gemma-4-26b-a4b-it").trim();
      const { data: model, error: modelError } = await adminDb.from("model_registry").select("slug,provider,model_id,display_name,config").eq("slug", modelSlug).eq("enabled", true).eq("public", true).maybeSingle();
      if (modelError) throw modelError;
      if (!model || model.config?.runtime !== "cloudflare-workers-ai") return reply(404, { error: "Requested model is not available.", code: "MODEL_UNAVAILABLE" });
      const inputTokens = Math.max(0, Math.min(20000, Math.floor(Number(body.inputTokens) || 0)));
      const maxOutputTokens = Math.max(1, Math.min(1024, Math.floor(Number(body.maxOutputTokens) || 1024)));
      const { data: policy } = await adminDb.from("ai_quota_policies").select("daily_request_limit,monthly_token_limit,enabled").eq("owner_user_id",user.id).maybeSingle();
      const dailyLimit = policy?.daily_request_limit ?? 20;
      const monthlyTokenLimit = policy?.monthly_token_limit ?? 20000;
      const neuronRates: Record<string, { input: number; output: number }> = {
        "@cf/google/gemma-4-26b-a4b-it": { input: 9091 / 1_000_000, output: 27273 / 1_000_000 }
      };
      const rates = neuronRates[model.model_id];
      if (!rates) return reply(503, { error: "No safe Neuron cost estimate is configured for this model.", code: "MODEL_NEURON_PRICE_MISSING" });
      const estimatedNeurons = Math.ceil((inputTokens * rates.input + maxOutputTokens * rates.output) * 1.25 * 100) / 100;
      const { data: reservation, error: reserveError } = await adminDb.rpc("cenai_reserve_ai_request_v2", {
        p_user_id: user.id, p_model_id: model.model_id, p_model_slug: model.slug,
        p_input_tokens: inputTokens, p_max_output_tokens: maxOutputTokens,
        p_estimated_neurons: Math.max(0.01, estimatedNeurons),
        p_default_daily_limit: 20, p_default_monthly_token_limit: 20000,
        p_global_daily_neuron_limit: 8000
      });
      if (reserveError) {
        console.error("Cloud AI quota reservation failed", reserveError.message);
        return reply(503, { error: "Could not reserve AI quota.", code: "QUOTA_SERVICE_UNAVAILABLE" });
      }
      const row = Array.isArray(reservation) ? reservation[0] : reservation;
      if (!row?.allowed) {
        const status = row?.reason === "quota_disabled" ? 403 : row?.reason === "invalid_request" || row?.reason === "invalid_token_budget" ? 400 : 429;
        return reply(status, {
          error: row?.reason === "daily_request_limit" ? "Daily AI request quota exceeded." : row?.reason === "monthly_token_limit" ? "Monthly AI token quota exceeded." : row?.reason === "rate_limit" ? "AI rate limit exceeded. Try again shortly." : row?.reason === "quota_disabled" ? "Cloud AI quota is disabled for this account." : row?.reason === "shared_global_daily_neuron_limit" ? "CenAI has reached its shared daily AI allowance. Try again after the UTC reset." : "AI request could not be authorized.",
          code: row?.reason || "QUOTA_BLOCKED",
          quota: { dailyLimit, monthlyTokenLimit, dailyUsed: row?.daily_used || 0, monthlyTokens: row?.monthly_tokens || 0, globalNeuronLimit: 8000, globalNeuronsUsed: Number(row?.global_neurons_used || 0) }
        });
      }
      return reply(200, { ok: true, eventId: row.event_id, model: model.slug, modelId: model.model_id, displayName: model.display_name, quota: { dailyLimit, monthlyTokenLimit, dailyUsed: row.daily_used, monthlyTokens: row.monthly_tokens, globalNeuronLimit: 8000, globalNeuronsUsed: Number(row.global_neurons_used || 0) } });
    }
    if (action === "record") {
      const eventId = Number(body.eventId);
      const inputTokens = Math.max(0, Math.min(20000, Math.floor(Number(body.inputTokens) || 0)));
      const outputTokens = Math.max(0, Math.min(2048, Math.floor(Number(body.outputTokens) || 0)));
      const latencyMs = Math.max(0, Math.min(600000, Math.floor(Number(body.latencyMs) || 0)));
      const status = ["success", "error", "cancelled", "blocked"].includes(String(body.status)) ? String(body.status) : "success";
      if (!Number.isSafeInteger(eventId) || eventId < 1) return reply(400, { error: "Invalid usage event." });
      const { data, error } = await adminDb.from("usage_events").update({
        input_tokens: inputTokens, output_tokens: outputTokens, latency_ms: latencyMs, status,
        error_code: status === "error" ? String(body.errorCode || "AI_INFERENCE_FAILED").slice(0, 80) : null,
        metadata: { model_slug: String(body.model || "cloudflare-gemma-4-26b-a4b-it"), reservation: false }
      }).eq("id", eventId).eq("owner_user_id", user.id).eq("source", "web").eq("event_type", "chat.completion").select("id").maybeSingle();
      if (error) throw error;
      if (!data) return reply(404, { error: "Usage reservation not found." });
      return reply(200, { ok: true, recorded: true });
    }
    return reply(400, { error: "Unknown cloud AI quota action." });
  } catch (error) {
    console.error("CenAI AI quota service failed", error instanceof Error ? error.message : "unknown error");
    return reply(503, { error: "Cloud AI quota service is temporarily unavailable." });
  }
});
