import "jsr:@supabase/functions-js/edge-runtime.d.ts";
import { createClient } from "npm:@supabase/supabase-js@2.57.4";

const url = Deno.env.get("SUPABASE_URL")!;
const anonKey = Deno.env.get("SUPABASE_ANON_KEY")!;
const serviceKey = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!;
const adminDb = createClient(url, serviceKey, { auth: { persistSession: false, autoRefreshToken: false } });
const authDb = createClient(url, anonKey, { auth: { persistSession: false, autoRefreshToken: false } });
const cors = { "Access-Control-Allow-Origin": "*", "Access-Control-Allow-Headers": "authorization, apikey, content-type, x-client-info", "Access-Control-Allow-Methods": "POST, OPTIONS", "Content-Type": "application/json", "Cache-Control": "no-store" };
const response = (status: number, body: unknown) => new Response(JSON.stringify(body), { status, headers: cors });
const fail = (status: number, message: string) => response(status, { error: message });
async function audit(user: any, action: string, details: Record<string, unknown> = {}) {
  const { error } = await adminDb.from("admin_audit_logs").insert({ actor_user_id: user.id, actor_email: user.email ?? "", action, details });
  if (error) throw error;
}
Deno.serve(async (req: Request) => {
  if (req.method === "OPTIONS") return new Response("ok", { headers: cors });
  if (req.method !== "POST") return fail(405, "Method not allowed");
  try {
    const token = (req.headers.get("Authorization") ?? "").replace(/^Bearer\s+/i, "");
    if (!token) return fail(401, "Sign in required");
    const { data: { user }, error: authError } = await authDb.auth.getUser(token);
    if (authError || !user || user.is_anonymous) return fail(401, "Invalid or expired session");
    const body = await req.json().catch(() => ({}));
    const action = String(body.action ?? "");
    if (action === "apply") {
      const { data: verifiedEmail, error: verifyError } = await adminDb.from("email_verifications").select("email").eq("user_id", user.id).eq("email", String(user.email ?? "").toLowerCase()).maybeSingle();
      if (verifyError) throw verifyError;
      if (!verifiedEmail) return fail(403, "Verify your account email with a one-time code before applying for beta access");
      const { data: flags, error: flagError } = await adminDb.from("admin_feature_flags").select("key,enabled").in("key", ["beta.applications.open","beta.invite_only"]);
      if (flagError) throw flagError;
      if (flags?.find(x => x.key === "beta.applications.open")?.enabled !== true) return fail(403, "Beta applications are currently closed");
      if (flags?.find(x => x.key === "beta.invite_only")?.enabled === true) return fail(403, "Beta applications currently require an administrator-issued invitation. Contact the CenAI team.");
      const display_name = String(body.displayName ?? "").trim();
      const motivation = String(body.motivation ?? "").trim();
      const experience = String(body.experience ?? "new");
      if (display_name.length < 1 || display_name.length > 80 || motivation.length < 20 || motivation.length > 2000 || !["new","some","advanced"].includes(experience)) return fail(400, "Please provide a display name and a 20–2000 character application note.");
      const { data: existing, error: existingError } = await adminDb.from("beta_applications").select("id,status").eq("user_id", user.id).maybeSingle();
      if (existingError) throw existingError;
      if (existing) return response(409, { error: "You already have a beta application. Check its status in your beta dashboard.", application: existing });
      const { data, error } = await adminDb.from("beta_applications").insert({ user_id: user.id, email: user.email ?? "", display_name, motivation, experience }).select("id,display_name,status,created_at").single();
      if (error) throw error;
      return response(201, { application: data });
    }
    if (action === "status") {
      const { data, error } = await adminDb.from("beta_applications").select("id,display_name,status,reviewer_note,created_at,reviewed_at").eq("user_id", user.id).maybeSingle();
      if (error) throw error;
      return response(200, { application: data, isBetaTester: data?.status === "approved" });
    }
    if (action === "feedback") {
      const { data: feedbackFlag, error: feedbackFlagError } = await adminDb.from("admin_feature_flags").select("enabled").eq("key", "beta.feedback.enabled").maybeSingle();
      if (feedbackFlagError) throw feedbackFlagError;
      if (feedbackFlag?.enabled !== true) return fail(403, "Beta feedback is currently disabled");
      const { data: approved, error: approvedError } = await adminDb.from("beta_applications").select("id").eq("user_id", user.id).eq("status", "approved").maybeSingle();
      if (approvedError) throw approvedError;
      if (!approved) return fail(403, "Only approved beta testers can submit feedback");
      const category = String(body.category ?? "other"), title = String(body.title ?? "").trim(), content = String(body.body ?? "").trim();
      if (!["bug","idea","ux","performance","other"].includes(category) || title.length < 4 || title.length > 120 || content.length < 10 || content.length > 5000) return fail(400, "Invalid feedback. Check the title and description lengths.");
      const { data, error } = await adminDb.from("beta_feedback").insert({ user_id: user.id, category, title, body: content, app_version: String(body.appVersion ?? "").slice(0,40) }).select("id,created_at").single();
      if (error) throw error;
      return response(201, { feedback: data });
    }
    const { data: adminRow, error: adminError } = await adminDb.from("admin_users").select("role,is_active").eq("user_id", user.id).maybeSingle();
    if (adminError) throw adminError;
    if (!adminRow?.is_active || !["owner","admin","operator"].includes(adminRow.role)) return fail(403, "Administrator access denied");
    const role = adminRow.role;
    if (action === "models") {
      const { data, error } = await adminDb.from("model_registry").select("id,slug,provider,model_id,display_name,description,tasks,modalities,enabled,public,updated_at").order("provider").order("slug");
      if (error) throw error;
      await audit(user, "models.registry.view", { count: data?.length ?? 0 });
      return response(200, { models: data ?? [] });
    }
    if (action === "model_upsert") {
      if (!["owner","admin"].includes(role)) return fail(403, "Owner or admin role required");
      const slug = String(body.slug ?? "").trim().toLowerCase();
      const provider = String(body.provider ?? "");
      const modelId = String(body.modelId ?? "").trim();
      const displayName = String(body.displayName ?? "").trim();
      const tasks = Array.isArray(body.tasks) ? body.tasks.filter((x: unknown) => typeof x === "string").slice(0, 12) : ["chat"];
      const modalities = Array.isArray(body.modalities) ? body.modalities.filter((x: unknown) => typeof x === "string").slice(0, 8) : ["text"];
      if (!/^[a-z0-9][a-z0-9._-]{1,100}$/.test(slug) || !["openai","anthropic","google","deepseek","custom","local"].includes(provider) || !modelId || modelId.length > 200 || !displayName || displayName.length > 120) return fail(400, "Invalid model registry entry");
      const { data, error } = await adminDb.from("model_registry").upsert({ slug, provider, model_id:modelId, display_name:displayName, description:String(body.description ?? "").slice(0,1000), tasks, modalities, enabled:body.enabled === true, public:body.public === true, updated_by:user.id, updated_at:new Date().toISOString(), created_by:user.id }, { onConflict:"slug" }).select("id,slug,provider,model_id,display_name,enabled,public").single();
      if (error) throw error;
      await audit(user, "models.registry.upsert", { slug, provider, enabled:data.enabled, public:data.public });
      return response(200, { model:data });
    }
    if (action === "usage_analytics") {
      const days = [7,30,90].includes(Number(body.days)) ? Number(body.days) : 30;
      const since = new Date(Date.now() - days * 86400000).toISOString();
      const { data, error } = await adminDb.from("usage_events").select("source,event_type,provider,model_id,input_tokens,output_tokens,latency_ms,status,created_at").gte("created_at",since).order("created_at",{ascending:false}).limit(5000);
      if (error) throw error;
      const rows = data ?? [];
      const sum = (key:string) => rows.reduce((total:any,row:any) => total + Number(row[key] ?? 0), 0);
      const providers: Record<string,number> = {};
      for (const row of rows) providers[row.provider || "unknown"] = (providers[row.provider || "unknown"] || 0) + 1;
      await audit(user, "analytics.usage.view", { days, rows: rows.length });
      return response(200, { days, total:rows.length, succeeded:rows.filter((x:any)=>x.status==="success").length, failed:rows.filter((x:any)=>x.status==="error").length, cancelled:rows.filter((x:any)=>x.status==="cancelled").length, inputTokens:sum("input_tokens"), outputTokens:sum("output_tokens"), averageLatencyMs:rows.length ? Math.round(sum("latency_ms")/rows.length) : 0, providers, truncated:rows.length===5000 });
    }
    if (action === "api_keys") {
      const { data, error } = await adminDb.from("api_keys").select("id,name,key_prefix,scopes,enabled,created_at,last_used_at,revoked_at,owner_user_id").eq("owner_user_id",user.id).order("created_at",{ascending:false}).limit(100);
      if (error) throw error;
      await audit(user, "api.keys.view", { count:data?.length ?? 0 });
      return response(200, { apiKeys:data ?? [] });
    }
    if (action === "api_key_create") {
      if (!["owner","admin"].includes(role)) return fail(403, "Owner or admin role required");
      const name = String(body.name ?? "").trim();
      if (name.length < 1 || name.length > 80) return fail(400, "API key name must be 1–80 characters");
      const { count, error:countError } = await adminDb.from("api_keys").select("id",{count:"exact",head:true}).eq("owner_user_id",user.id).eq("enabled",true);
      if (countError) throw countError;
      if ((count ?? 0) >= 5) return fail(429, "Each administrator may have at most five active API keys");
      const bytes = crypto.getRandomValues(new Uint8Array(32));
      const secret = "cai_live_" + Array.from(bytes).map(x=>x.toString(16).padStart(2,"0")).join("");
      const digest = await crypto.subtle.digest("SHA-256",new TextEncoder().encode(secret));
      const keyHash = Array.from(new Uint8Array(digest)).map(x=>x.toString(16).padStart(2,"0")).join("");
      const { data, error } = await adminDb.from("api_keys").insert({ owner_user_id:user.id, name, key_prefix:secret.slice(0,17), key_hash:keyHash, scopes:["models:read","usage:read"] }).select("id,name,key_prefix,scopes,enabled,created_at").single();
      if (error) throw error;
      await audit(user, "api.key.created", { api_key_id:data.id, name });
      return response(201, { apiKey:data, secret, warning:"Copy this key now. It cannot be retrieved again." });
    }
    if (action === "api_key_revoke") {
      if (!["owner","admin"].includes(role)) return fail(403, "Owner or admin role required");
      const id = String(body.id ?? "");
      if (!/^[0-9a-f-]{36}$/i.test(id)) return fail(400, "Invalid API key identifier");
      const { data, error } = await adminDb.from("api_keys").update({ enabled:false, revoked_at:new Date().toISOString() }).eq("id",id).eq("owner_user_id",user.id).select("id,name,enabled,revoked_at").maybeSingle();
      if (error) throw error;
      if (!data) return fail(404, "API key not found");
      await audit(user, "api.key.revoked", { api_key_id:id });
      return response(200, { apiKey:data });
    }
    if (action === "admin_list") {
      const status = String(body.status ?? "pending");
      if (!["pending","approved","rejected","revoked","all"].includes(status)) return fail(400, "Invalid status filter");
      let q = adminDb.from("beta_applications").select("id,user_id,email,display_name,motivation,experience,status,reviewer_note,created_at,reviewed_at").order("created_at", { ascending: false }).limit(200);
      if (status !== "all") q = q.eq("status", status);
      const { data, error } = await q;
      if (error) throw error;
      await audit(user, "beta.applications.view", { status });
      return response(200, { applications: data ?? [] });
    }
    if (action === "admin_review") {
      if (!["owner","admin"].includes(role)) return fail(403, "Owner or admin role required");
      const id = String(body.id ?? ""), status = String(body.status ?? ""), note = String(body.note ?? "").trim();
      if (!/^[0-9a-f-]{36}$/i.test(id) || !["approved","rejected","revoked"].includes(status) || note.length > 1000) return fail(400, "Invalid review request");
      const { data, error } = await adminDb.from("beta_applications").update({ status, reviewer_id: user.id, reviewer_note: note, reviewed_at: new Date().toISOString(), updated_at: new Date().toISOString() }).eq("id", id).select("id,user_id,status").maybeSingle();
      if (error) throw error;
      if (!data) return fail(404, "Application not found");
      await audit(user, "beta.application." + status, { application_id: id, target_user_id: data.user_id });
      return response(200, { application: data });
    }
    if (action === "admin_feedback") {
      const { data, error } = await adminDb.from("beta_feedback").select("id,user_id,category,title,body,app_version,created_at").order("created_at", { ascending: false }).limit(200);
      if (error) throw error;
      return response(200, { feedback: data ?? [] });
    }
    if (action === "overview" || action === "flags" || action === "audit" || action === "integrations" || action === "update_flag" || action === "releases") {
      if (action === "overview" || action === "flags") {
        const { data: flags, error } = await adminDb.from("admin_feature_flags").select("key,enabled,description,updated_at").order("key");
        if (error) throw error;
        if (action === "flags") return response(200, { flags });
        const integrations = [{ name: "Supabase Auth + Database", connected: true, detail: "Authenticated session and database verified." }, { name: "GitHub Releases", connected: true, detail: "Public release feed available." }, { name: "CenAI Public API", connected: true, detail: "API-key service deployed; public access remains disabled by the api.public.enabled launch flag." }, { name: "Model registry", connected: true, detail: "Server-managed model registry is available." }, { name: "Usage analytics", connected: true, detail: "Privacy-minimized usage event aggregation is available." }, { name: "Guardian device diagnostics", connected: true, detail: "Local device diagnostics are available; cloud ingestion is not enabled." }, { name: "Billing provider", connected: Boolean(Deno.env.get("STRIPE_SECRET_KEY") && Deno.env.get("STRIPE_WEBHOOK_SECRET")), detail: Deno.env.get("STRIPE_SECRET_KEY") && Deno.env.get("STRIPE_WEBHOOK_SECRET") ? "Stripe secrets configured; webhook and checkout validation still required." : "Not connected. Requires a Stripe account and server-side keys." }, { name: "AI provider gateway", connected: false, detail: "Not connected." }];
        await audit(user, "admin.overview.view");
        return response(200, { controlPlane: "authenticated", role, items: [{ name: "Administrator", detail: user.email ?? "", status: role }, { name: "Feature flags", detail: flags.length + " stored flags", status: "database" }, { name: "Privileged actions", detail: "Server authorization and audit enabled", status: "protected" }], integrations, flags });
      }
      if (action === "update_flag") {
        if (!["owner","admin"].includes(role)) return fail(403, "Owner or admin role required");
        const key = String(body.key ?? ""), enabled = body.enabled;
        if (!/^[a-z0-9._-]{2,80}$/.test(key) || typeof enabled !== "boolean") return fail(400, "Invalid feature flag");
        const { data, error } = await adminDb.from("admin_feature_flags").update({ enabled, updated_by: user.id, updated_at: new Date().toISOString() }).eq("key", key).select().maybeSingle();
        if (error) throw error;
        if (!data) return fail(404, "Feature flag not found");
        await audit(user, "feature_flag.updated", { key, enabled });
        return response(200, { ok: true, flag: data });
      }
      if (action === "audit") {
        const { data, error } = await adminDb.from("admin_audit_logs").select("actor_email,action,details,created_at").order("created_at", { ascending: false }).limit(100);
        if (error) throw error;
        return response(200, { events: data ?? [] });
      }
      if (action === "integrations") return response(200, { integrations: [{ name: "Supabase Auth + Postgres", connected: true, detail: "Verified for this request." }, { name: "GitHub Releases", connected: true, detail: "Public release feed available." }, { name: "CenAI Public API", connected: true, detail: "API-key service deployed; public access remains disabled by launch flag." }, { name: "Model registry", connected: true, detail: "Server-managed registry deployed." }, { name: "Usage analytics", connected: true, detail: "Aggregate usage analytics deployed." }, { name: "Guardian", connected: true, detail: "Local Android diagnostics deployed; remote ingestion is off." }, { name: "Billing", connected: Boolean(Deno.env.get("STRIPE_SECRET_KEY") && Deno.env.get("STRIPE_WEBHOOK_SECRET")), detail: Deno.env.get("STRIPE_SECRET_KEY") && Deno.env.get("STRIPE_WEBHOOK_SECRET") ? "Stripe keys configured; production checkout still requires verification." : "Not connected. Requires Stripe credentials." }, { name: "AI Gateway", connected: false, detail: "Not connected." }] });
      if (action === "releases") {
        const r = await fetch("https://api.github.com/repos/C4RL-DANIEL/CenAi/releases?per_page=20", { headers: { "Accept": "application/vnd.github+json", "User-Agent": "CenAI-Control-Plane" } });
        if (!r.ok) return fail(502, "GitHub Releases API unavailable");
        const raw = await r.json();
        return response(200, { releases: raw.filter((x: any) => !x.draft).map((x: any) => ({ name: x.name || x.tag_name, tag: x.tag_name, publishedAt: x.published_at, url: x.html_url })) });
      }
    }
    return fail(400, "Unknown beta/admin action");
  } catch (error) {
    console.error("control-plane request failed", error);
    return fail(500, "CenAI control-plane request failed");
  }
});