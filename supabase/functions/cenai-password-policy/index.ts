import "jsr:@supabase/functions-js/edge-runtime.d.ts";
import { createClient } from "npm:@supabase/supabase-js@2.57.4";
import { isBreached, validatePassword } from "./policy.ts";

const supabaseUrl = Deno.env.get("SUPABASE_URL")!;
const anonKey = Deno.env.get("SUPABASE_ANON_KEY")!;
const serviceKey = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!;
const authClient = createClient(supabaseUrl, anonKey, { auth: { persistSession: false, autoRefreshToken: false } });
const adminClient = createClient(supabaseUrl, serviceKey, { auth: { persistSession: false, autoRefreshToken: false } });
const corsHeaders = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Headers": "authorization, apikey, content-type, x-client-info",
  "Access-Control-Allow-Methods": "POST, OPTIONS",
  "Content-Type": "application/json",
  "Cache-Control": "no-store",
};

function json(status: number, body: Record<string, unknown>) {
  return new Response(JSON.stringify(body), { status, headers: corsHeaders });
}


Deno.serve(async (request: Request) => {
  if (request.method === "OPTIONS") return new Response("ok", { headers: corsHeaders });
  if (request.method !== "POST") return json(405, { error: "Method not allowed." });

  const authorization = request.headers.get("Authorization") ?? "";
  const token = authorization.replace(/^Bearer\s+/i, "").trim();
  if (!token) return json(401, { error: "Sign in or open a valid recovery link before changing your password." });

  try {
    const { data: { user }, error: authError } = await authClient.auth.getUser(token);
    if (authError || !user || user.is_anonymous) return json(401, { error: "Your password session is invalid or expired. Sign in again or request a fresh recovery link." });

    const body = await request.json().catch(() => ({}));
    const password = typeof body?.password === "string" ? body.password : "";
    const validationError = validatePassword(password);
    if (validationError) return json(400, { error: validationError });

    if (user.email) {
      const localPart = user.email.split("@")[0]?.toLowerCase() ?? "";
      if (localPart.length >= 4 && password.toLowerCase().includes(localPart)) {
        return json(400, { error: "Do not include your email address in your password." });
      }
    }

    let breached: boolean;
    try {
      breached = await isBreached(password);
    } catch {
      // Fail closed: do not silently skip the breach check during an outage.
      return json(503, { error: "Password safety check is temporarily unavailable. Please try again shortly; your password was not changed." });
    }
    if (breached) {
      return json(400, { error: "This password appears in known data breaches. Choose a different password that you have never used before." });
    }

    const { error: updateError } = await adminClient.auth.admin.updateUserById(user.id, { password });
    if (updateError) {
      // Do not return provider internals or password-related request data.
      return json(400, { error: "Supabase could not update this password. Check the password requirements and try again." });
    }
    return json(200, { updated: true });
  } catch {
    return json(500, { error: "Password update failed safely. Please try again." });
  }
});
