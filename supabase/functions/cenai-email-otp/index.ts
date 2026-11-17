import "jsr:@supabase/functions-js/edge-runtime.d.ts";
import { createClient } from "npm:@supabase/supabase-js@2.57.4";

const url = Deno.env.get("SUPABASE_URL")!;
const anonKey = Deno.env.get("SUPABASE_ANON_KEY")!;
const serviceKey = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!;
const authDb = createClient(url, anonKey, { auth: { persistSession: false, autoRefreshToken: false } });
const adminDb = createClient(url, serviceKey, { auth: { persistSession: false, autoRefreshToken: false } });
const headers = {
  "Content-Type": "application/json",
  "Cache-Control": "no-store",
  "X-Content-Type-Options": "nosniff",
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Headers": "authorization, apikey, content-type, x-client-info",
  "Access-Control-Allow-Methods": "POST, OPTIONS"
};
const json = (status: number, body: unknown) => new Response(JSON.stringify(body), { status, headers });
const fail = (status: number, error: string) => json(status, { error });

Deno.serve(async (req: Request) => {
  if (req.method === "OPTIONS") return new Response("ok", { headers });
  if (req.method !== "POST") return fail(405, "Method not allowed");
  try {
    const token = (req.headers.get("Authorization") || "").replace(/^Bearer\s+/i, "").trim();
    if (!token) return fail(401, "Sign in required");
    const { data: { user }, error: authError } = await authDb.auth.getUser(token);
    if (authError || !user || user.is_anonymous || !user.email) return fail(401, "Valid signed-in account required");

    const body = await req.json().catch(() => ({}));
    const action = String(body.action || "");
    const email = String(body.email || user.email).trim().toLowerCase();
    const otpHash = String(body.otpHash || "").trim().toLowerCase();
    if (email !== String(user.email).toLowerCase()) return fail(403, "You can only verify the email on your signed-in account");
    if (!/^[a-f0-9]{64}$/.test(otpHash)) return fail(400, "Invalid verification challenge");

    if (action === "issue_admin" || action === "verify_admin") {
      const { data: admin, error: adminError } = await adminDb.from("admin_users").select("role,is_active").eq("user_id", user.id).maybeSingle();
      if (adminError) throw adminError;
      if (!admin?.is_active || !["owner","admin","operator"].includes(admin.role)) return fail(403, "Administrator access denied");
    }
    if (action === "issue_admin") {
      const { data: prior } = await adminDb.from("admin_email_mfa").select("updated_at").eq("user_id", user.id).maybeSingle();
      if (prior?.updated_at && Date.now() - Date.parse(String(prior.updated_at)) < 60000) return fail(429, "Please wait one minute before requesting another administrator code.");
      const { error } = await adminDb.from("admin_email_mfa").upsert({
        user_id: user.id, email, otp_hash: otpHash,
        expires_at: new Date(Date.now() + 10 * 60 * 1000).toISOString(),
        attempts: 0, verified_until: null, updated_at: new Date().toISOString()
      }, { onConflict: "user_id" });
      if (error) throw error;
      return json(200, { ok: true });
    }
    if (action === "verify_admin") {
      const { data: challenge, error } = await adminDb.from("admin_email_mfa")
        .select("email,otp_hash,expires_at,attempts").eq("user_id", user.id).maybeSingle();
      if (error) throw error;
      if (!challenge || challenge.email !== email || !challenge.otp_hash ||
          !challenge.expires_at || Date.parse(challenge.expires_at) <= Date.now() ||
          Number(challenge.attempts || 0) >= 5) return fail(400, "Code is invalid, expired, or locked. Request a new code.");
      if (challenge.otp_hash !== otpHash) {
        await adminDb.from("admin_email_mfa").update({ attempts: Number(challenge.attempts || 0) + 1, updated_at: new Date().toISOString() }).eq("user_id", user.id);
        return fail(400, "Code is incorrect. Check your email and try again.");
      }
      const { error: updateError } = await adminDb.from("admin_email_mfa").update({
        otp_hash: null, expires_at: null, attempts: 0,
        verified_until: new Date(Date.now() + 10 * 60 * 1000).toISOString(),
        updated_at: new Date().toISOString()
      }).eq("user_id", user.id);
      if (updateError) throw updateError;
      return json(200, { ok: true, verified: true });
    }
    if (action === "issue") {
      const { error } = await adminDb.rpc("cenai_issue_email_otp_for_user", {
        p_user_id: user.id,
        p_email: email,
        p_otp_hash: otpHash
      });
      if (error) {
        const message = String(error.message || "");
        if (message.includes("wait before requesting")) return fail(429, "Please wait before requesting another code");
        console.error("OTP challenge issue failed", message);
        return fail(503, "Could not create verification challenge");
      }
      return json(200, { ok: true });
    }

    if (action === "verify") {
      const { data, error } = await adminDb.rpc("cenai_verify_email_otp_for_user", {
        p_user_id: user.id,
        p_email: email,
        p_otp_hash: otpHash
      });
      if (error) {
        console.error("OTP challenge verification failed", error.message);
        return fail(503, "Email verification service is temporarily unavailable");
      }
      if (data !== true) return fail(400, "Code is invalid, expired, or already used");
      return json(200, { ok: true, verified: true, message: "Email verified for this CenAI account" });
    }

    return fail(400, "Unknown email verification action");
  } catch (error) {
    console.error("CenAI email OTP service failed", error instanceof Error ? error.message : "unknown error");
    return fail(503, "Email verification service is temporarily unavailable");
  }
});
