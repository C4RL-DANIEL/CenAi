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
async function sha256Hex(value: string) {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(value));
  return Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, "0")).join("");
}
function createInviteToken() {
  const bytes = crypto.getRandomValues(new Uint8Array(32));
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}
async function audit(user: any, action: string, details: Record<string, unknown> = {}) {
  const { error } = await adminDb.from("admin_audit_logs").insert({ actor_user_id: user.id, actor_email: user.email ?? "", action, details });
  if (error) throw error;
}
function tokenAal(token: string) {
  try {
    const part = token.split(".")[1] || "";
    const normalized = part.replace(/-/g, "+").replace(/_/g, "/").padEnd(Math.ceil(part.length / 4) * 4, "=");
    return String(JSON.parse(atob(normalized)).aal || "aal1");
  } catch { return "aal1"; }
}
async function hasEmailMfa(userId: string) {
  const { data, error } = await adminDb.from("admin_email_mfa").select("verified_until").eq("user_id", userId).maybeSingle();
  if (error) throw error;
  return Boolean(data?.verified_until && Date.parse(String(data.verified_until)) > Date.now());
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
      const { data: verified, error: verifiedError } = await adminDb.from("email_verifications").select("email").eq("user_id", user.id).eq("email", String(user.email ?? "").toLowerCase()).maybeSingle();
      if (verifiedError) throw verifiedError;
      return response(200, { application: data, isBetaTester: data?.status === "approved", emailVerified: Boolean(verified) });
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
    if (action === "registration_redeem") {
      const inviteToken = String(body.inviteToken ?? "");
      if (!/^[A-Za-z0-9_-]{40,64}$/.test(inviteToken)) return fail(400, "Invalid registration invitation");
      const providers = Array.isArray(user.app_metadata?.providers) ? user.app_metadata.providers : [];
      const identities = Array.isArray(user.identities) ? user.identities : [];
      const isDiscord = user.app_metadata?.provider === "discord" || providers.includes("discord") || identities.some((identity: any) => identity?.provider === "discord");
      if (!isDiscord) return fail(403, "Continue with Discord to redeem this invitation");
      const tokenHash = await sha256Hex(inviteToken);
      const { data: redeemed, error: redeemError } = await adminDb.rpc("cenai_redeem_registration_invite", { p_token_hash: tokenHash, p_user_id: user.id });
      if (redeemError) throw redeemError;
      if (redeemed !== true) return fail(403, "This invitation is expired, revoked, invalid, or has already been used");
      const appMetadata = { ...(user.app_metadata ?? {}), cenai_registration_verified: true, cenai_registration_method: "discord" };
      const { error: metadataError } = await adminDb.auth.admin.updateUserById(user.id, { app_metadata: appMetadata });
      if (metadataError) throw metadataError;
      await audit(user, "registration.invite.redeemed", { registration_method: "discord" });
      return response(200, { registered: true, provider: "discord", email: user.email ?? null });
    }
    if (action === "admin_preflight") {
      const { data: preflightAdmin, error: preflightError } = await adminDb.from("admin_users").select("role,is_active").eq("user_id", user.id).maybeSingle();
      if (preflightError) throw preflightError;
      if (!preflightAdmin?.is_active || !["owner","admin","operator"].includes(preflightAdmin.role)) return fail(403, "Administrator access denied");
      return response(200, { authorized:true, mfaRequired:tokenAal(token) !== "aal2" && !await hasEmailMfa(user.id) });
    }
    const { data: adminRow, error: adminError } = await adminDb.from("admin_users").select("role,is_active").eq("user_id", user.id).maybeSingle();
    if (adminError) throw adminError;
    if (!adminRow?.is_active || !["owner","admin","operator"].includes(adminRow.role)) return fail(403, "Administrator access denied");
    if (tokenAal(token) !== "aal2" && !await hasEmailMfa(user.id)) return fail(403, "MFA_REQUIRED: Verify the email code sent to your administrator address.");
    const role = adminRow.role;
    if (action === "members_list") {
      const page = Math.max(1, Math.min(100, Number(body.page) || 1));
      const perPage = Math.max(10, Math.min(100, Number(body.perPage) || 100));
      const { data: authPage, error: usersError } = await adminDb.auth.admin.listUsers({ page, perPage });
      if (usersError) throw usersError;
      const users = authPage?.users ?? [];
      const ids = users.map((entry:any) => String(entry.id)).filter((id:string) => /^[0-9a-f-]{36}$/i.test(id));
      if (!ids.length) return response(200, { members: [], page, perPage, total: 0 });
      const [subsResult, betaResult, adminsResult, plansResult, paymentsResult] = await Promise.all([
        adminDb.from("billing_subscriptions").select("user_id,provider,provider_subscription_id,plan_key,status,current_period_end,cancel_at_period_end,updated_at").in("user_id", ids).order("updated_at", { ascending:false }),
        adminDb.from("beta_applications").select("user_id,status").in("user_id", ids),
        adminDb.from("admin_users").select("user_id,role,is_active").in("user_id", ids),
        adminDb.from("billing_plan_config").select("plan_key,display_name,amount,currency,interval_unit,interval_count"),
        adminDb.from("billing_one_time_payments").select("user_id,provider_order_id,provider_capture_id,plan_key,amount,currency,status,paid_at,access_expires_at").in("user_id", ids).order("created_at", { ascending:false })
      ]);
      if (subsResult.error) throw subsResult.error;
      if (betaResult.error) throw betaResult.error;
      if (adminsResult.error) throw adminsResult.error;
      if (plansResult.error) throw plansResult.error;
      if (paymentsResult.error) throw paymentsResult.error;
      const byUser = (rows:any[]) => new Map(rows.map(row => [String(row.user_id), row]));
      const betaByUser = byUser(betaResult.data ?? []);
      const adminByUser = byUser(adminsResult.data ?? []);
      const planByKey = new Map((plansResult.data ?? []).map((row:any) => [String(row.plan_key), row]));
      const subsByUser = new Map<string, any[]>();
      for (const sub of subsResult.data ?? []) {
        const key = String(sub.user_id);
        const rows = subsByUser.get(key) ?? [];
        rows.push(sub); subsByUser.set(key, rows);
      }
      const members = users.map((entry:any) => {
        const id = String(entry.id), admin = adminByUser.get(id), subscriptions = subsByUser.get(id) ?? [];
        return {
          id, email: String(entry.email ?? ""), displayName: String(entry.user_metadata?.display_name ?? entry.user_metadata?.full_name ?? ""),
          createdAt: entry.created_at ?? null, lastSignInAt: entry.last_sign_in_at ?? null,
          emailConfirmedAt: entry.email_confirmed_at ?? null, bannedUntil: (entry as any).banned_until ?? null,
          betaStatus: betaByUser.get(id)?.status ?? "none",
          adminRole: admin?.is_active ? admin.role : null,
          subscriptions: subscriptions.map((sub:any) => {
            const plan:any = planByKey.get(String(sub.plan_key));
            return { provider:sub.provider, subscriptionId:sub.provider_subscription_id, planKey:sub.plan_key,
              planName:plan?.display_name ?? sub.plan_key, amount:plan?.amount ?? null, currency:plan?.currency ?? null,
              status:sub.status, currentPeriodEnd:sub.current_period_end, cancelAtPeriodEnd:sub.cancel_at_period_end };
          }),
          payments: (paymentsResult.data ?? []).filter((payment:any) => String(payment.user_id) === id).map((payment:any) => ({
            orderId:payment.provider_order_id, captureId:payment.provider_capture_id, planKey:payment.plan_key,
            amount:payment.amount, currency:payment.currency, status:payment.status,
            paidAt:payment.paid_at, accessExpiresAt:payment.access_expires_at,
          }))
        };
      });
      await audit(user, "members.list.view", { page, per_page:perPage, returned:members.length });
      return response(200, { members, page, perPage, total:members.length, note:"Auth users are paginated by Supabase. Membership is derived from verified billing_subscriptions records." });
    }
    if (action === "member_update") {
      if (!["owner","admin"].includes(role)) return fail(403, "Owner or admin role required");
      const userId = String(body.userId ?? "");
      const displayName = String(body.displayName ?? "").trim();
      if (!/^[0-9a-f-]{36}$/i.test(userId) || displayName.length < 1 || displayName.length > 80) return fail(400, "Provide a valid member ID and a display name between 1 and 80 characters.");
      const { data: target, error: targetError } = await adminDb.auth.admin.getUserById(userId);
      if (targetError) throw targetError;
      if (!target?.user) return fail(404, "Member account not found.");
      const { data, error } = await adminDb.auth.admin.updateUserById(userId, {
        user_metadata: { ...(target.user.user_metadata ?? {}), display_name: displayName }
      });
      if (error) throw error;
      await audit(user, "member.profile.updated", { target_user_id:userId });
      return response(200, { member:{ id:data.user.id, email:data.user.email, displayName:String(data.user.user_metadata?.display_name ?? displayName) } });
    }
    if (action === "member_suspend" || action === "member_restore") {
      if (!["owner","admin"].includes(role)) return fail(403, "Owner or admin role required");
      const userId = String(body.userId ?? "");
      if (!/^[0-9a-f-]{36}$/i.test(userId)) return fail(400, "Invalid member ID.");
      const { data: target, error: targetError } = await adminDb.auth.admin.getUserById(userId);
      if (targetError) throw targetError;
      if (!target?.user) return fail(404, "Member account not found.");
      const { data: targetAdmin, error: targetAdminError } = await adminDb.from("admin_users").select("role,is_active").eq("user_id",userId).maybeSingle();
      if (targetAdminError) throw targetAdminError;
      if (targetAdmin?.is_active) return fail(409, "Administrator accounts cannot be suspended from member management.");
      const banDuration = action === "member_suspend" ? "876000h" : "none";
      const { data, error } = await adminDb.auth.admin.updateUserById(userId, { ban_duration:banDuration });
      if (error) throw error;
      await audit(user, action === "member_suspend" ? "member.account.suspended" : "member.account.restored", { target_user_id:userId });
      return response(200, { member:{ id:data.user.id, email:data.user.email, bannedUntil:(data.user as any).banned_until ?? null } });
    }
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
    if (action === "analytics_dashboard") {
      if (!["owner","admin"].includes(role)) return fail(403, "Owner or admin role required");
      const days = [7,30,90].includes(Number(body.days)) ? Number(body.days) : 30;
      const since = new Date(Date.now() - days * 86400000).toISOString();
      const { data, error } = await adminDb.from("usage_events").select("source,event_type,provider,model_id,input_tokens,output_tokens,latency_ms,status,error_code,created_at").gte("created_at", since).order("created_at", { ascending: false }).limit(5000);
      if (error) throw error;
      const rows = data ?? [];
      const providers: Record<string, any> = {}, models: Record<string, any> = {}, daily: Record<string, any> = {}, statuses: Record<string, number> = {}, failures: Record<string, number> = {};
      const add = (bucket: Record<string, any>, key: string, row: any) => {
        const value = bucket[key] ??= { requests:0, inputTokens:0, outputTokens:0, latencyTotal:0, succeeded:0, failed:0 };
        value.requests++; value.inputTokens += Number(row.input_tokens || 0); value.outputTokens += Number(row.output_tokens || 0); value.latencyTotal += Number(row.latency_ms || 0);
        if (row.status === "success") value.succeeded++; if (row.status === "error") value.failed++;
      };
      for (const row of rows) {
        add(providers, row.provider || "unknown", row); add(models, row.model_id || "unknown", row); add(daily, String(row.created_at || "").slice(0,10), row);
        statuses[row.status || "unknown"] = (statuses[row.status || "unknown"] || 0) + 1;
        if (row.status === "error") { const key = String(row.error_code || "unknown").slice(0,80); failures[key] = (failures[key] || 0) + 1; }
      }
      const summarize = (bucket: Record<string, any>) => Object.entries(bucket).map(([name, v]: [string, any]) => ({ name, requests:v.requests, inputTokens:v.inputTokens, outputTokens:v.outputTokens, averageLatencyMs:v.requests ? Math.round(v.latencyTotal/v.requests) : 0, succeeded:v.succeeded, failed:v.failed })).sort((a,b)=>b.requests-a.requests);
      const latencies = rows.map((r:any)=>Number(r.latency_ms || 0)).filter((n:number)=>Number.isFinite(n)&&n>=0).sort((a:number,b:number)=>a-b);
      const { data: guardianRows, error: guardianError } = await adminDb.from("guardian_reports").select("battery_percent,available_ram_mb,free_storage_mb,network_type,crash_category,created_at").gte("created_at", since).order("created_at", { ascending:false }).limit(5000);
      if (guardianError) throw guardianError;
      const health = guardianRows ?? [];
      const averageHealth = (key:string) => { const values = health.map((row:any)=>Number((row as any)[key])).filter((n:number)=>Number.isFinite(n)&&n>=0); return values.length ? Math.round(values.reduce((a:number,b:number)=>a+b,0)/values.length) : null; };
      const healthCounts = (key:string) => { const counts:Record<string,number> = {}; for (const row of health) { const name=String((row as any)[key]||"unknown").slice(0,64); counts[name]=(counts[name]||0)+1; } return Object.entries(counts).map(([name,count])=>({name,count})).sort((a,b)=>b.count-a.count).slice(0,12); };
      await audit(user, "analytics.dashboard.view", { days, rows: rows.length, guardianRows:health.length });
      return response(200, { days, total:rows.length, truncated:rows.length===5000, statuses, inputTokens:rows.reduce((sum:number,row:any)=>sum+Number(row.input_tokens||0),0), outputTokens:rows.reduce((sum:number,row:any)=>sum+Number(row.output_tokens||0),0), providers:summarize(providers), models:summarize(models), daily:Object.entries(daily).sort(([a],[b])=>a.localeCompare(b)).map(([date,v]:[string,any])=>({date,requests:v.requests,inputTokens:v.inputTokens,outputTokens:v.outputTokens,averageLatencyMs:v.requests?Math.round(v.latencyTotal/v.requests):0,succeeded:v.succeeded,failed:v.failed})), failureCategories:Object.entries(failures).map(([category,count])=>({category,count})).sort((a,b)=>Number(b.count)-Number(a.count)).slice(0,20), averageLatencyMs:latencies.length?Math.round(latencies.reduce((a:number,b:number)=>a+b,0)/latencies.length):0, p95LatencyMs:latencies.length?latencies[Math.min(latencies.length-1,Math.ceil(latencies.length*0.95)-1)]:0, guardian:{reports:health.length,averageBatteryPercent:averageHealth("battery_percent"),averageAvailableRamMb:averageHealth("available_ram_mb"),averageFreeStorageMb:averageHealth("free_storage_mb"),networks:healthCounts("network_type"),crashCategories:healthCounts("crash_category")} });
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
    if (action === "registration_invite_create") {
      if (!["owner", "admin"].includes(role)) return fail(403, "Owner or admin role required");
      const label = String(body.label ?? "").trim().slice(0, 120);
      const hours = Math.max(1, Math.min(168, Math.floor(Number(body.expiresInHours) || 24)));
      const token = createInviteToken();
      const tokenHash = await sha256Hex(token);
      const expiresAt = new Date(Date.now() + hours * 60 * 60 * 1000).toISOString();
      const { data, error } = await adminDb.from("registration_invites").insert({
        token_hash: tokenHash, created_by: user.id, label, expires_at: expiresAt
      }).select("id,label,expires_at,created_at").single();
      if (error) throw error;
      await audit(user, "registration.invite.created", { invite_id: data.id, expires_at: expiresAt, expires_in_hours: hours });
      return response(201, { invite: data, token });
    }
    if (action === "registration_invite_list") {
      if (!["owner", "admin"].includes(role)) return fail(403, "Owner or admin role required");
      const { data, error } = await adminDb.from("registration_invites")
        .select("id,label,expires_at,created_at,redeemed_at,revoked_at,redeemed_by")
        .order("created_at", { ascending: false }).limit(100);
      if (error) throw error;
      return response(200, { invites: data ?? [] });
    }
    if (action === "registration_invite_revoke") {
      if (!["owner", "admin"].includes(role)) return fail(403, "Owner or admin role required");
      const id = String(body.id ?? "");
      if (!/^[0-9a-f-]{36}$/i.test(id)) return fail(400, "Invalid invitation identifier");
      const { data, error } = await adminDb.from("registration_invites")
        .update({ revoked_at: new Date().toISOString() })
        .eq("id", id).is("redeemed_at", null).is("revoked_at", null)
        .select("id,revoked_at").maybeSingle();
      if (error) throw error;
      if (!data) return fail(409, "Invitation is already used, expired, or revoked");
      await audit(user, "registration.invite.revoked", { invite_id: id });
      return response(200, { invite: data });
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
    if (action === "site_pages") {
      const { data, error } = await adminDb.from("site_pages").select("slug,title,description,content,published,updated_at").order("slug");
      if (error) throw error;
      await audit(user, "site.pages.view", { count: data?.length ?? 0 });
      return response(200, { pages: data ?? [] });
    }
    if (action === "site_page_save") {
      if (!["owner","admin"].includes(role)) return fail(403, "Owner or admin role required");
      const slug = String(body.slug ?? "").trim().toLowerCase();
      const title = String(body.title ?? "").trim();
      const description = String(body.description ?? "").trim();
      const content = body.content && typeof body.content === "object" && !Array.isArray(body.content) ? body.content : {};
      const published = body.published === true;
      if (!/^[a-z0-9][a-z0-9-]{0,79}$/.test(slug) || title.length < 2 || title.length > 120 || description.length > 500 || JSON.stringify(content).length > 20000) return fail(400, "Invalid page. Check slug, title, description, and content size.");
      const { data, error } = await adminDb.from("site_pages").upsert({ slug, title, description, content, published, updated_by:user.id, updated_at:new Date().toISOString() }, { onConflict:"slug" }).select("slug,title,description,content,published,updated_at").single();
      if (error) throw error;
      await audit(user, "site.page.saved", { slug, published });
      return response(200, { page:data });
    }
    if (action === "quota_list") {
      const { data, error } = await adminDb.from("ai_quota_policies").select("owner_user_id,daily_request_limit,monthly_token_limit,enabled,updated_at").order("updated_at",{ascending:false}).limit(200);
      if (error) throw error;
      await audit(user, "ai.quota.view", { count:data?.length ?? 0 });
      return response(200, { policies:data ?? [], defaults:{ dailyRequestLimit:100, monthlyTokenLimit:500000, enforcement:"CenAI Public API requests only" } });
    }
    if (action === "quota_update") {
      if (!["owner","admin"].includes(role)) return fail(403, "Owner or admin role required");
      const ownerUserId = String(body.ownerUserId ?? "");
      const daily = Number(body.dailyRequestLimit), monthly = Number(body.monthlyTokenLimit), enabled = body.enabled !== false;
      if (!/^[0-9a-f-]{36}$/i.test(ownerUserId) || !Number.isInteger(daily) || daily < 1 || daily > 1000000 || !Number.isSafeInteger(monthly) || monthly < 1000 || monthly > 100000000000) return fail(400, "Invalid quota policy values.");
      const { data, error } = await adminDb.from("ai_quota_policies").upsert({ owner_user_id:ownerUserId, daily_request_limit:daily, monthly_token_limit:monthly, enabled, updated_by:user.id, updated_at:new Date().toISOString() }, { onConflict:"owner_user_id" }).select("owner_user_id,daily_request_limit,monthly_token_limit,enabled,updated_at").single();
      if (error) throw error;
      await audit(user, "ai.quota.updated", { owner_user_id:ownerUserId, daily_request_limit:daily, monthly_token_limit:monthly, enabled });
      return response(200, { policy:data });
    }
    if (action === "overview" || action === "flags" || action === "audit" || action === "integrations" || action === "update_flag" || action === "releases") {
      if (action === "overview" || action === "flags") {
        const { data: flags, error } = await adminDb.from("admin_feature_flags").select("key,enabled,description,updated_at").order("key");
        if (error) throw error;
        if (action === "flags") return response(200, { flags });
        const { data: paypalPlan, error: paypalPlanError } = await adminDb.from("billing_plan_config").select("paypal_plan_id,enabled").eq("plan_key","live_test").maybeSingle();
        if (paypalPlanError) throw paypalPlanError;
        const { data: billingFlag, error: billingFlagError } = await adminDb.from("admin_feature_flags").select("enabled").eq("key","billing.enabled").maybeSingle();
        if (billingFlagError) throw billingFlagError;
        const { count: webhookCount, error: webhookCountError } = await adminDb.from("billing_webhook_events").select("event_id",{count:"exact",head:true}).eq("provider","paypal");
        if (webhookCountError) throw webhookCountError;
        const { count: subscriptionCount, error: subscriptionCountError } = await adminDb.from("billing_subscriptions").select("provider_subscription_id",{count:"exact",head:true}).eq("provider","paypal");
        if (subscriptionCountError) throw subscriptionCountError;
        const integrations = [{ name: "Supabase Auth + Database", connected: true, detail: "Authenticated session and database verified." }, { name: "GitHub Releases", connected: true, detail: "Public release feed available." }, { name: "CenAI Public API", connected: true, detail: "API-key service deployed; public access remains disabled by the api.public.enabled launch flag." }, { name: "Model registry", connected: true, detail: "Server-managed model registry is available." }, { name: "Usage analytics", connected: true, detail: "Privacy-minimized usage event aggregation is available." }, { name: "Guardian device diagnostics", connected: true, detail: "Local diagnostics are available; cloud ingestion is opt-in and currently disabled." }, { name: "PayPal Live", connected: Boolean(paypalPlan?.paypal_plan_id && Deno.env.get("PAYPAL_CLIENT_ID") && Deno.env.get("PAYPAL_CLIENT_SECRET") && Deno.env.get("PAYPAL_WEBHOOK_ID")), detail: "Live plan: " + (paypalPlan?.paypal_plan_id ? "present" : "missing") + "; PayPal credentials/webhook ID: " + (Deno.env.get("PAYPAL_CLIENT_ID") && Deno.env.get("PAYPAL_CLIENT_SECRET") && Deno.env.get("PAYPAL_WEBHOOK_ID") ? "configured" : "incomplete") + "; signed webhook events recorded: " + (webhookCount ?? 0) + "; subscription records: " + (subscriptionCount ?? 0) + "; billing launch flag: " + (billingFlag?.enabled === true ? "enabled" : "disabled") + ". Keep checkout disabled until subscription lifecycle tests pass." }, { name: "Cloud AI gateway", connected: false, detail: "No centralized provider gateway is deployed; Android provider calls currently use user-managed provider credentials." }];
        await audit(user, "admin.overview.view");
        return response(200, { controlPlane: "authenticated", role, items: [{ name: "Administrator", detail: user.email ?? "", status: role }, { name: "Feature flags", detail: flags.length + " stored flags", status: "database" }, { name: "Privileged actions", detail: "Server authorization and audit enabled", status: "protected" }], integrations, flags });
      }
      if (action === "update_flag") {
        if (!["owner","admin"].includes(role)) return fail(403, "Owner or admin role required");
        const key = String(body.key ?? ""), enabled = body.enabled;
        if (!/^[a-z0-9._-]{2,80}$/.test(key) || typeof enabled !== "boolean") return fail(400, "Invalid feature flag");
        if (key === "billing.enabled" && enabled) {
          const { data: webhookRows, error: webhookError } = await adminDb.from("billing_webhook_events").select("event_type").eq("provider","paypal").limit(1000);
          if (webhookError) throw webhookError;
          const eventTypes = new Set((webhookRows ?? []).map((row:any)=>String(row.event_type || "")));
          const { count: lifecycleRows, error: lifecycleError } = await adminDb.from("billing_subscriptions").select("provider_subscription_id",{count:"exact",head:true}).eq("provider","paypal").eq("status","CANCELLED");
          if (lifecycleError) throw lifecycleError;
          const requiredEvents = ["BILLING.SUBSCRIPTION.ACTIVATED","BILLING.SUBSCRIPTION.CANCELLED","PAYMENT.SALE.COMPLETED"];
          const missing = requiredEvents.filter(eventType=>!eventTypes.has(eventType));
          if (missing.length || (lifecycleRows ?? 0) < 1) return fail(409, "Billing launch blocked: PayPal activation, completed-payment, and cancellation events plus a cancelled subscription record must be verified first. No live checkout was enabled.");
        }
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
      if (action === "integrations") {
        const { data: paypalPlan, error: planError } = await adminDb.from("billing_plan_config").select("paypal_plan_id").eq("plan_key","live_test").maybeSingle();
        if (planError) throw planError;
        const { data: billingFlag, error: billingError } = await adminDb.from("admin_feature_flags").select("enabled").eq("key","billing.enabled").maybeSingle();
        if (billingError) throw billingError;
        const { data: recurringFlag, error: recurringError } = await adminDb.from("admin_feature_flags").select("enabled").eq("key","billing.recurring_enabled").maybeSingle();
        if (recurringError) throw recurringError;
        const { data: publicApiFlag, error: publicApiError } = await adminDb.from("admin_feature_flags").select("enabled").eq("key","api.public.enabled").maybeSingle();
        if (publicApiError) throw publicApiError;
        const { data: guardianDiagnosticsFlag, error: guardianDiagnosticsError } = await adminDb.from("admin_feature_flags").select("enabled").eq("key","guardian.diagnostics.enabled").maybeSingle();
        if (guardianDiagnosticsError) throw guardianDiagnosticsError;
        const { data: guardianIngestionFlag, error: guardianIngestionError } = await adminDb.from("admin_feature_flags").select("enabled").eq("key","guardian.ingestion.enabled").maybeSingle();
        if (guardianIngestionError) throw guardianIngestionError;
        const { count: webhookCount, error: eventError } = await adminDb.from("billing_webhook_events").select("event_id",{count:"exact",head:true}).eq("provider","paypal");
        if (eventError) throw eventError;
        const { count: subscriptionCount, error: subscriptionError } = await adminDb.from("billing_subscriptions").select("provider_subscription_id",{count:"exact",head:true}).eq("provider","paypal");
        if (subscriptionError) throw subscriptionError;
        const { data: cloudAiFlag, error: cloudAiFlagError } = await adminDb.from("admin_feature_flags").select("enabled").eq("key","ai.cloud.enabled").maybeSingle();
        if (cloudAiFlagError) throw cloudAiFlagError;
        const { data: cloudAiModel, error: cloudAiModelError } = await adminDb.from("model_registry").select("model_id,enabled").eq("slug","cloudflare-gemma-4-26b-a4b-it").maybeSingle();
        if (cloudAiModelError) throw cloudAiModelError;
        const cloudAiReady = Boolean(cloudAiModel?.enabled && cloudAiModel?.model_id);
        const cloudAiEnabled = cloudAiFlag?.enabled === true;
        const paypalReady = Boolean(paypalPlan?.paypal_plan_id && Deno.env.get("PAYPAL_CLIENT_ID") && Deno.env.get("PAYPAL_CLIENT_SECRET") && Deno.env.get("PAYPAL_WEBHOOK_ID"));
        return response(200, { integrations: [{ name: "Supabase Auth + Postgres", connected: true, detail: "Verified for this request." }, { name: "GitHub Releases", connected: true, detail: "Public release feed available." }, { name: "CenAI Public API", connected: true, detail: "API-key service deployed; public access is " + (publicApiFlag?.enabled === true ? "enabled" : "disabled by launch flag") + "." }, { name: "Model registry", connected: true, detail: "Server-managed registry deployed." }, { name: "Usage analytics", connected: true, detail: "Aggregate usage analytics deployed." }, { name: "Guardian", connected: guardianDiagnosticsFlag?.enabled === true, detail: "Local Android diagnostics are " + (guardianDiagnosticsFlag?.enabled === true ? "enabled" : "disabled") + "; remote ingestion is " + (guardianIngestionFlag?.enabled === true ? "enabled" : "disabled") + " by feature flags." }, { name: "PayPal Live", connected: paypalReady, detail: "Plan " + (paypalPlan?.paypal_plan_id ? "present" : "missing") + "; signed events recorded: " + (webhookCount ?? 0) + "; subscription records: " + (subscriptionCount ?? 0) + "; billing: " + (billingFlag?.enabled === true ? "enabled" : "disabled") + "; recurring subscriptions: " + (recurringFlag?.enabled === true ? "enabled" : "paused") + ". Event receipt does not replace a completed subscription lifecycle test." }, { name: "Cloudflare Workers AI", connected: cloudAiReady, detail: cloudAiReady ? "Workers AI binding and per-user quota enforcement are deployed. Model: " + (cloudAiModel?.model_id || "unknown") + ". Inference is " + (cloudAiEnabled ? "enabled" : "disabled pending administrator opt-in") + "." : "Workers AI model or quota service is not configured." }] });
      }
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