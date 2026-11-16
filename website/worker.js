import { connect } from "cloudflare:sockets";

const corsHeaders = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Headers": "authorization, apikey, content-type, x-client-info",
  "Access-Control-Allow-Methods": "GET, POST, OPTIONS",
  "Vary": "Authorization",
  "Cache-Control": "no-store",
  "X-Content-Type-Options": "nosniff"
};

function json(status, body) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { ...corsHeaders, "Content-Type": "application/json" }
  });
}

function withSecurity(response) {
  const headers = new Headers(response.headers);
  headers.set("X-Content-Type-Options", "nosniff");
  headers.set("Referrer-Policy", "strict-origin-when-cross-origin");
  headers.set("Permissions-Policy", "camera=(), microphone=(), geolocation=()");
  headers.set("X-Frame-Options", "DENY");
  headers.set("Content-Security-Policy", "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; font-src 'self' https://fonts.gstatic.com; img-src 'self' data:; connect-src 'self' https://bqkzehozezvepbthglyj.supabase.co https://*.supabase.co; base-uri 'self'; form-action 'self'; frame-ancestors 'none'; upgrade-insecure-requests");
  return new Response(response.body, { status: response.status, statusText: response.statusText, headers });
}

function envReady(env, keys) {
  return keys.every((key) => typeof env[key] === "string" && env[key].length > 0);
}

async function controlPlaneProxy(request, env) {
  if (request.method !== "POST") return json(405, { error: "Method not allowed" });
  if (!envReady(env, ["SUPABASE_URL", "SUPABASE_PUBLISHABLE_KEY"])) {
    return json(503, { error: "Supabase integration is not configured" });
  }
  const token = (request.headers.get("Authorization") || "").replace(/^Bearer\s+/i, "").trim();
  if (!token) return json(401, { error: "Sign in required" });
  const body = await request.json().catch(() => ({}));
  try {
    const upstream = await fetch(env.SUPABASE_URL.replace(/\/$/, "") + "/functions/v1/cenai-control-plane", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "apikey": env.SUPABASE_PUBLISHABLE_KEY,
        "Authorization": "Bearer " + token
      },
      body: JSON.stringify(body)
    });
    const payload = await upstream.json().catch(() => ({ error: "Invalid control-plane response" }));
    return json(upstream.status, payload);
  } catch (error) {
    console.error("CenAI control-plane proxy failed", error instanceof Error ? error.message : "unknown error");
    return json(503, { error: "CenAI control-plane is temporarily unavailable" });
  }
}

function hex(bytes) {
  return Array.from(new Uint8Array(bytes), (value) => value.toString(16).padStart(2, "0")).join("");
}

async function otpDigest(secret, userId, email, code) {
  const key = await crypto.subtle.importKey("raw", new TextEncoder().encode(secret), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
  const signature = await crypto.subtle.sign("HMAC", key, new TextEncoder().encode(userId + "|" + email.toLowerCase() + "|" + code));
  return hex(signature);
}

function base64(value) {
  return btoa(unescape(encodeURIComponent(value)));
}

async function sendGmailOtp(env, to, code) {
  if (!envReady(env, ["EMAIL_USER", "EMAIL_PASS"])) throw new Error("Gmail SMTP credentials are not configured");
  const socket = connect({ hostname: "smtp.gmail.com", port: 465 }, { secureTransport: "on", allowHalfOpen: false });
  const reader = socket.readable.getReader();
  const writer = socket.writable.getWriter();
  const decoder = new TextDecoder();
  let buffer = "";
  async function readReply() {
    for (;;) {
      const end = buffer.indexOf("\n");
      if (end >= 0) {
        const line = buffer.slice(0, end + 1).replace(/\r?\n$/, "");
        buffer = buffer.slice(end + 1);
        if (/^\d{3} /.test(line)) return line;
        continue;
      }
      const chunk = await reader.read();
      if (chunk.done) throw new Error("Gmail SMTP connection closed unexpectedly");
      buffer += decoder.decode(chunk.value, { stream: true });
    }
  }
  async function expect(codes) {
    const reply = await readReply();
    if (!codes.some((codeValue) => reply.startsWith(String(codeValue)))) throw new Error("Gmail SMTP rejected a command: " + reply.slice(0, 120));
    return reply;
  }
  async function command(value, codes) {
    await writer.write(new TextEncoder().encode(value + "\r\n"));
    return expect(codes);
  }
  try {
    await expect([220]);
    await command("EHLO cenai.app", [250]);
    await command("AUTH LOGIN", [334]);
    await command(base64(env.EMAIL_USER), [334]);
    await command(base64(env.EMAIL_PASS), [235]);
    await command("MAIL FROM:<" + env.EMAIL_USER + ">", [250]);
    await command("RCPT TO:<" + to + ">", [250, 251]);
    await command("DATA", [354]);
    const subject = "Your CenAI verification code";
    const text = "Your CenAI verification code is " + code + ". It expires in 10 minutes. Do not share it.";
    const html = '<!doctype html><html><body style="margin:0;background:#080d17;font-family:Arial;color:#eaf0ff;padding:32px 12px"><main style="max-width:520px;margin:auto;background:#101827;border:1px solid #29364c;border-radius:20px;padding:30px"><p style="color:#a9baff;letter-spacing:2px;font-weight:bold">CENAI · ACCOUNT SECURITY</p><h1>Verify it\'s really you.</h1><p style="color:#bdc9dc;line-height:1.7">Use this one-time code to continue. It expires in <b>10 minutes</b>.</p><div style="margin:26px 0;padding:20px;background:#080f1d;border:1px solid #34476a;border-radius:14px;text-align:center;font-size:34px;font-weight:800;letter-spacing:9px;color:#a9baff">' + code + '</div><p style="font-size:13px;color:#91a2bc">If you did not request this code, ignore this message. Never share your code with anyone.</p><small>CenAI · Intelligence with permission</small></main></body></html>';
    const boundary = "cenai_" + crypto.randomUUID().replace(/-/g, "");
    const message = [
      "From: CenAI Security <" + env.EMAIL_USER + ">",
      "To: " + to,
      "Subject: " + subject,
      "MIME-Version: 1.0",
      'Content-Type: multipart/alternative; boundary="' + boundary + '"',
      "",
      "--" + boundary,
      'Content-Type: text/plain; charset="UTF-8"',
      "Content-Transfer-Encoding: 8bit",
      "",
      text,
      "--" + boundary,
      'Content-Type: text/html; charset="UTF-8"',
      "Content-Transfer-Encoding: 8bit",
      "",
      html,
      "--" + boundary + "--",
      "."
    ].join("\r\n").replace(/\r?\n\./g, "\r\n..");
    await writer.write(new TextEncoder().encode(message + "\r\n"));
    await expect([250]);
    await command("QUIT", [221]);
  } finally {
    try { await writer.close(); } catch {}
    try { await reader.cancel(); } catch {}
    try { socket.close(); } catch {}
  }
}

async function emailOtp(request, env) {
  if (request.method !== "POST") return json(405, { error: "Method not allowed" });
  if (!envReady(env, ["SUPABASE_URL", "SUPABASE_PUBLISHABLE_KEY", "EMAIL_USER", "EMAIL_PASS", "EMAIL_OTP_SECRET"])) {
    return json(503, { error: "Email verification is not fully configured on Cloudflare" });
  }
  const token = (request.headers.get("Authorization") || "").replace(/^Bearer\s+/i, "").trim();
  if (!token) return json(401, { error: "Sign in required" });
  try {
    const base = env.SUPABASE_URL.replace(/\/$/, "");
    const userResponse = await fetch(base + "/auth/v1/user", { headers: { apikey: env.SUPABASE_PUBLISHABLE_KEY, Authorization: "Bearer " + token } });
    const user = await userResponse.json().catch(() => ({}));
    if (!userResponse.ok || !user.id || !user.email) return json(401, { error: "Valid signed-in account required" });
    const body = await request.json().catch(() => ({}));
    const action = String(body.action || "");
    const email = String(body.email || user.email).trim().toLowerCase();
    if (email !== String(user.email).toLowerCase()) return json(403, { error: "You can only verify the email on your signed-in account" });
    if (action === "send") {
      const code = String(crypto.getRandomValues(new Uint32Array(1))[0] % 1000000).padStart(6, "0");
      const digest = await otpDigest(env.EMAIL_OTP_SECRET, user.id, email, code);
      const rpc = await fetch(base + "/rest/v1/rpc/cenai_issue_email_otp", {
        method: "POST",
        headers: { apikey: env.SUPABASE_PUBLISHABLE_KEY, Authorization: "Bearer " + token, "Content-Type": "application/json" },
        body: JSON.stringify({ p_email: email, p_otp_hash: digest })
      });
      if (!rpc.ok) {
        const error = await rpc.json().catch(() => ({}));
        return json(rpc.status, { error: error.message || "Could not create verification challenge" });
      }
      await sendGmailOtp(env, email, code);
      return json(200, { ok: true, message: "Verification code sent. Check your inbox." });
    }
    if (action === "verify") {
      const code = String(body.code || "").trim();
      if (!/^\d{6}$/.test(code)) return json(400, { error: "Enter the six-digit verification code" });
      const digest = await otpDigest(env.EMAIL_OTP_SECRET, user.id, email, code);
      const rpc = await fetch(base + "/rest/v1/rpc/cenai_verify_email_otp", {
        method: "POST",
        headers: { apikey: env.SUPABASE_PUBLISHABLE_KEY, Authorization: "Bearer " + token, "Content-Type": "application/json" },
        body: JSON.stringify({ p_email: email, p_otp_hash: digest })
      });
      const ok = rpc.ok ? await rpc.json() : false;
      if (!ok) return json(400, { error: "Code is invalid, expired, or already used" });
      return json(200, { ok: true, verified: true, message: "Email verified for this CenAI account" });
    }
    return json(400, { error: "Unknown email verification action" });
  } catch (error) {
    console.error("CenAI email verification failed", error instanceof Error ? error.message : "unknown error");
    return json(503, { error: "Email verification service temporarily unavailable" });
  }
}

async function health(env) {
  const required = ["SUPABASE_URL", "SUPABASE_PUBLISHABLE_KEY"];
  const optional = ["CENAI_API_BASE_URL", "AI_GATEWAY_API_KEY", "EMAIL_USER", "EMAIL_PASS", "EMAIL_OTP_SECRET", "PAYPAL_CLIENT_ID", "PAYPAL_CLIENT_SECRET", "PAYPAL_WEBHOOK_ID", "CENAI_PUBLIC_URL"];
  const configured = Object.fromEntries([...required, ...optional].map((key) => [key, Boolean(env[key])]));
  const missing = required.filter((key) => !env[key]);
  const missingOtp = ["EMAIL_USER", "EMAIL_PASS", "EMAIL_OTP_SECRET"].filter((key) => !env[key]);
  return json(200, {
    service: "cenai-website",
    hosting: "cloudflare-workers",
    status: "online",
    checkedAt: new Date().toISOString(),
    adminPanel: "available-at-/admin",
    emailOtp: missingOtp.length === 0 ? "configuration-present-needs-delivery-test" : "not-configured",
    controlPlane: missing.length === 0 ? "configuration-present" : "not-configured",
    paypal: ["PAYPAL_CLIENT_ID", "PAYPAL_CLIENT_SECRET", "PAYPAL_WEBHOOK_ID"].every((key) => Boolean(env[key])) ? "configuration-present" : "not-configured",
    integrations: configured,
    missingConfiguration: missing,
    missingEmailOtpConfiguration: missingOtp,
    note: "Configuration status only; authenticated API requests and real email delivery must be tested separately."
  });
}

function rewritePath(url) {
  const path = url.pathname;
  const exact = {
    "/beta": "/beta.html",
    "/beta/": "/beta.html",
    "/admin": "/admin/index.html",
    "/admin/": "/admin/index.html",
    "/reset-password": "/reset-password.html",
    "/reset-password/": "/reset-password.html",
    "/billing": "/billing.html",
    "/billing/": "/billing.html",
    "/billing/success": "/billing-success.html",
    "/billing/cancel": "/billing-cancel.html"
  };
  if (exact[path]) url.pathname = exact[path];
  else if (path.startsWith("/pages/")) {
    url.pathname = "/page.html";
    url.searchParams.set("slug", path.slice("/pages/".length));
  }
  return url;
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.pathname.startsWith("/api/")) {
      if (request.method === "OPTIONS") return new Response(null, { status: 204, headers: corsHeaders });
      if (url.pathname === "/api/admin" || url.pathname === "/api/beta") return controlPlaneProxy(request, env);
      if (url.pathname === "/api/email-otp") return emailOtp(request, env);
      if (url.pathname === "/api/health") {
        if (request.method !== "GET") return json(405, { error: "Method not allowed" });
        return health(env);
      }
      return json(404, { error: "API route not found" });
    }
    const rewritten = rewritePath(url);
    const assetRequest = new Request(rewritten.toString(), request);
    const response = await env.ASSETS.fetch(assetRequest);
    return withSecurity(response);
  }
};
