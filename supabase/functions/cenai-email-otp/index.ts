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
    if (action === "issue") {
      const { error } = await adminDb.rpc("cenai_issue_email_otp_for_user", { p_user_id: user.id, p_email: email, p_otp_hash: otpHash });
      if (error) {
        const message = String(error.message || "");
        if (message.includes("wait before requesting")) return fail(429, "Please wait before requesting another code");
        console.error("OTP challenge issue failed", message);
        return fail(503, "Could not create verification challenge");
      }
      return json(200, { ok: true });
    }
    if (action === "verify") {
      const { data, error } = await adminDb.rpc("cenai_verify_email_otp_for_user", { p_user_id: user.id, p_email: email, p_otp_hash: otpHash });
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
