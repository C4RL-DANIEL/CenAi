import "jsr:@supabase/functions-js/edge-runtime.d.ts";
import { createClient } from "npm:@supabase/supabase-js@2.57.4";

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

function validatePassword(password: string) {
  const length = Array.from(password).length;
  if (length < 12) return "Use at least 12 characters.";
  if (new TextEncoder().encode(password).length > 72) return "Use a password no longer than 72 UTF-8 bytes.";
  if (!password.trim() || /[\u0000-\u001f\u007f]/.test(password)) return "Choose a password containing printable characters.";
  if (/^(.)\1{11,}$/su.test(password)) return "Avoid passwords made from one repeated character.";
  return null;
}

async function isBreached(password: string): Promise<boolean> {
  const digest = await crypto.subtle.digest("SHA-1", new TextEncoder().encode(password));
  const hash = Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, "0")).join("").toUpperCase();
  const prefix = hash.slice(0, 5);
  const suffix = hash.slice(5);
  // k-anonymity: only the first five SHA-1 characters leave CenAI. The password
  // and complete hash are never sent to HIBP and are never logged.
  const response = await fetch("https://api.pwnedpasswords.com/range/" + prefix, {
    headers: { "Add-Padding": "true", "User-Agent": "CenAI-Password-Policy/1.0" },
    signal: AbortSignal.timeout(8000),
  });
  if (!response.ok) throw new Error("BREACH_CHECK_UNAVAILABLE");
  const rows = await response.text();
  for (const line of rows.split("\n")) {
    const [candidate, countText] = line.trim().split(":");
    if (candidate?.toUpperCase() === suffix && Number(countText) > 0) return true;
  }
  return false;
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
