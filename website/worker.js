import { connect } from "cloudflare:sockets";

// The Worker is copied as a native ES module by build.mjs (not passed through esbuild).
// Preserve function names without relying on esbuild's injected helper.
function __name(target, value) {
  Object.defineProperty(target, "name", { value, configurable: true });
  return target;
}

const STATIC_CACHE_VERSION = "20261121-discord-registration-invites-1";
const STATIC_FILES = new Set([
  "/index.html", "/styles.css", "/site-shell.css", "/editorial.css", "/billing.css", "/home.js", "/app.js", "/api.html",
  "/about.html", "/privacy.html", "/terms.html", "/contact.html", "/robots.txt", "/sitemap.xml",
  "/guides/index.html", "/guides/choosing-ai-models.html", "/guides/approval-first-agents.html",
  "/guides/organizing-ai-projects.html", "/guides/local-vs-cloud-ai.html", "/guides/checking-ai-answers.html",
  "/beta.html", "/beta.js", "/billing.html", "/billing.js", "/billing-success.html",
  "/billing-cancel.html", "/page.html", "/page.js", "/reset-password.html", "/reset-password.js",
  "/roadmap.html", "/security.html", "/updates.html", "/updates.js",
  "/admin/index.html", "/admin/admin.css", "/admin/admin.js", "/register.html", "/register.js", "/account.html", "/account.js"
]);

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
  headers.set("Content-Security-Policy", "default-src 'self'; script-src 'self' https://www.paypal.com https://www.paypalobjects.com https://pagead2.googlesyndication.com https://securepubads.g.doubleclick.net; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com https://www.paypal.com; font-src 'self' https://fonts.gstatic.com https://www.paypalobjects.com; img-src 'self' data: https://www.paypalobjects.com https://*.googlesyndication.com https://*.doubleclick.net; connect-src 'self' https://bqkzehozezvepbthglyj.supabase.co https://*.supabase.co https://www.paypal.com https://api-m.paypal.com https://*.paypal.com https://*.googlesyndication.com https://*.doubleclick.net; frame-src https://www.paypal.com https://*.paypal.com https://www.paypalobjects.com https://googleads.g.doubleclick.net https://tpc.googlesyndication.com; form-action 'self' https://www.paypal.com https://*.paypal.com; base-uri 'self'; frame-ancestors 'none'; upgrade-insecure-requests");
  return new Response(response.body, { status: response.status, statusText: response.statusText, headers });
}

function envReady(env, keys) {
  return keys.every((key) => typeof env[key] === "string" && env[key].length > 0);
}

function sandboxBackendUrl(env) {
  const configured = String(env.CENAI_SANDBOX_BACKEND_URL || "").trim();
  if (!configured) return null;
  try {
    const url = new URL(configured);
    if (url.protocol !== "https:" || url.username || url.password || url.search || url.hash || url.pathname !== "/api/sandbox") {
      return null;
    }
    return url.toString();
  } catch {
    return null;
  }
}

async function sandboxProxy(request, env) {
  if (request.method !== "POST") return json(405, { error: "Method not allowed" });
  const token = (request.headers.get("Authorization") || "").trim();
  if (!/^Bearer\s+\S+/i.test(token)) return json(401, { error: "Sign in required" });
  try {
    const upstream = await fetch(CENAI_SANDBOX_BACKEND, {
      method: "POST",
      headers: {
        Authorization: token,
        "Content-Type": request.headers.get("Content-Type") || "application/json",
        Accept: "application/json"
      },
      body: request.body,
      redirect: "manual"
    });
    const headers = new Headers(corsHeaders);
    headers.set("Content-Type", upstream.headers.get("Content-Type") || "application/json");
    headers.set("Cache-Control", "no-store");
    return new Response(upstream.body, { status: upstream.status, headers });
  } catch (error) {
    console.error("CenAI sandbox proxy failed", error instanceof Error ? error.message : "unknown error");
    return json(503, { error: "CenAI sandbox service is temporarily unavailable" });
  }
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

function base64Lines(value) {
  const encoded = base64(value);
  return encoded.match(/.{1,76}/g)?.join("\r\n") || "";
}

async function sendGmailMessage(env, to, subject, text, html) {
  if (!envReady(env, ["EMAIL_USER", "EMAIL_PASS"])) throw new Error("Gmail SMTP credentials are not configured");
  const socket = connect({ hostname: "smtp.gmail.com", port: 465 }, { secureTransport: "on", allowHalfOpen: false });
  const reader = socket.readable.getReader();
  const writer = socket.writable.getWriter();
  const decoder = new TextDecoder();
  let buffer = "";
  let stage = "connect";
  const withTimeout = (promise, label, timeoutMs = 8000) => new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("Gmail SMTP timed out during " + label)), timeoutMs);
    Promise.resolve(promise).then(
      (value) => { clearTimeout(timer); resolve(value); },
      (error) => { clearTimeout(timer); reject(error); }
    );
  });
  async function readReply(timeoutMs = 8000) {
    for (;;) {
      const end = buffer.indexOf("\n");
      if (end >= 0) {
        const line = buffer.slice(0, end + 1).replace(/\r?\n$/, "");
        buffer = buffer.slice(end + 1);
        if (/^\d{3} /.test(line)) return line;
        continue;
      }
      const chunk = await withTimeout(reader.read(), stage, timeoutMs);
      if (chunk.done) throw new Error("Gmail SMTP connection closed during " + stage);
      buffer += decoder.decode(chunk.value, { stream: true });
    }
  }
  async function expect(codes, timeoutMs = 8000) {
    const reply = await readReply(timeoutMs);
    if (!codes.some((codeValue) => reply.startsWith(String(codeValue)))) throw new Error("Gmail SMTP rejected " + stage + " (" + reply.slice(0, 120) + ")");
    return reply;
  }
  async function command(value, codes, nextStage) {
    stage = nextStage || stage;
    await withTimeout(writer.write(new TextEncoder().encode(value + "\r\n")), stage + " write");
    return expect(codes);
  }
  try {
    // Cloudflare Socket API requires the connection handshake to finish before I/O.
    await withTimeout(socket.opened, "connection");
    await expect([220]);
    await command("EHLO cenai.app", [250], "EHLO");
    await command("AUTH LOGIN", [334], "authentication");
    await command(base64(env.EMAIL_USER), [334], "username authentication");
    await command(base64(env.EMAIL_PASS), [235], "password authentication");
    await command("MAIL FROM:<" + env.EMAIL_USER + ">", [250], "sender validation");
    await command("RCPT TO:<" + to + ">", [250, 251], "recipient validation");
    await command("DATA", [354], "message transfer");
    const boundary = "cenai_" + crypto.randomUUID().replace(/-/g, "");
    const message = [
      "From: CenAI Security <" + env.EMAIL_USER + ">",
      "To: " + to,
      "Subject: " + subject,
      "MIME-Version: 1.0",
      "Date: " + new Date().toUTCString(),
      'Content-Type: multipart/alternative; boundary="' + boundary + '"',
      "",
      "--" + boundary,
      'Content-Type: text/plain; charset="UTF-8"',
      "Content-Transfer-Encoding: base64",
      "",
      base64Lines(text),
      "--" + boundary,
      'Content-Type: text/html; charset="UTF-8"',
      "Content-Transfer-Encoding: base64",
      "",
      base64Lines(html),
      "--" + boundary + "--"
    ].join("\r\n");
    // Dot-stuff message content lines, but keep SMTP's final DATA terminator as one dot.
    const smtpData = message.replace(/(^|\r\n)\./g, "$1..") + "\r\n.\r\n";
    const maxLineBytes = Math.max(...smtpData.split("\r\n").map((line) => new TextEncoder().encode(line).length));
    if (maxLineBytes > 998) throw new Error("CenAI email contains an SMTP line longer than the 998-byte protocol limit");
    stage = "message acceptance";
    const payload = new TextEncoder().encode(smtpData);
    console.log("CenAI SMTP message prepared", { bytes: payload.length, maxLineBytes, mime: "base64-wrapped" });
    await withTimeout(writer.write(payload), "message write", 15000);
    await expect([250], 20000);
    console.log("CenAI administrator email accepted by Gmail SMTP");
    // Gmail has accepted the message at this point. Do not block delivery confirmation
    // on waiting for the optional QUIT reply.
    try { await writer.write(new TextEncoder().encode("QUIT\r\n")); } catch {}
  } catch (error) {
    console.error("CenAI Gmail SMTP delivery failed at", stage, error instanceof Error ? error.message : "unknown error");
    throw error;
  } finally {
    try { socket.close(); } catch {}
    try { reader.releaseLock(); } catch {}
    try { writer.releaseLock(); } catch {}
  }
}

async function sendGmailOtp(env, to, code) {
  const text = "Your CenAI verification code is " + code + ". It expires in 10 minutes. If you did not request it, ignore this email. Never share this code with anyone, including CenAI support.";
  const html = [
    '<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width"><meta name="color-scheme" content="light only"><title>Your CenAI security code</title></head>',
    '<body style="margin:0;padding:28px 12px;background:#f1f0e9;font-family:Arial,Helvetica,sans-serif;color:#1b2b23">',
    '<div style="display:none;max-height:0;overflow:hidden;opacity:0">Your one-time CenAI security code. Expires in 10 minutes.</div>',
    '<table role="presentation" width="100%" cellspacing="0" cellpadding="0" border="0"><tr><td align="center">',
    '<table role="presentation" width="100%" cellspacing="0" cellpadding="0" border="0" style="max-width:560px;background:#fbfaf6;border:1px solid #d9dfd4;border-radius:20px;overflow:hidden">',
    '<tr><td style="padding:26px 30px;background:#1b2b23">',
    '<table role="presentation" cellspacing="0" cellpadding="0" border="0"><tr><td width="42" height="42" align="center" valign="middle" style="width:42px;height:42px;border-radius:50%;background:#a6c94b;color:#1b2b23;font-size:21px;font-weight:800">C</td><td style="padding-left:12px;color:#f7f8f1;font-size:20px;font-weight:700;letter-spacing:-1px">CenAI<span style="display:block;margin-top:3px;color:#c4d2b0;font-size:9px;font-weight:600;letter-spacing:2px">ACCOUNT SECURITY</span></td></tr></table>',
    '<p style="margin:24px 0 7px;color:#c6d3b7;font-size:10px;font-weight:700;letter-spacing:2px">SECURE VERIFICATION</p>',
    '<h1 style="margin:0;color:#fff;font-size:28px;line-height:1.2;letter-spacing:-.7px">Confirm it’s you.</h1>',
    '</td></tr><tr><td style="padding:30px">',
    '<p style="margin:0 0 18px;color:#536458;font-size:14px;line-height:1.75">Use this one-time code to finish verifying your CenAI account or administrator session. For your security, the code expires in <strong style="color:#1b2b23">10 minutes</strong>.</p>',
    '<div style="padding:21px 12px;border:1px solid #d9dfd4;border-radius:14px;background:#f1f3e9;text-align:center">',
    '<div style="margin-bottom:9px;color:#718064;font-size:9px;font-weight:700;letter-spacing:2px">YOUR VERIFICATION CODE</div>',
    '<div style="color:#1b2b23;font-size:34px;font-weight:800;letter-spacing:9px;font-variant-numeric:tabular-nums">' + code + '</div></div>',
    '<div style="margin:22px 0 0;padding:14px 15px;border-left:3px solid #a6c94b;background:#f3f4eb;color:#68766b;font-size:12px;line-height:1.7"><strong style="color:#34483a">Didn’t request this?</strong><br>Ignore this message. Never share your code with anyone. CenAI support will never ask you for it.</div>',
    '<p style="margin:24px 0 0;color:#849083;font-size:11px;line-height:1.7">This message was sent to protect access to your CenAI account. If you need help, visit the official CenAI website.</p>',
    '<div style="margin-top:22px;padding-top:17px;border-top:1px solid #e1e5dc;color:#7b877a;font-size:10px">CenAI · Intelligence with permission</div>',
    '</td></tr></table><p style="margin:16px 0 0;color:#98a194;font-size:10px;text-align:center">Automated security notification · Please do not reply</p>',
    '</td></tr></table></body></html>'
  ].join("");
  return sendGmailMessage(env, to, "Your CenAI verification code", text, html);
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
    if (action === "send" || action === "admin_send") {
      const code = String(crypto.getRandomValues(new Uint32Array(1))[0] % 1000000).padStart(6, "0");
      const digest = await otpDigest(env.EMAIL_OTP_SECRET, user.id, email, code);
      const rpc = await fetch(base + "/functions/v1/cenai-email-otp", {
        method: "POST",
        headers: { apikey: env.SUPABASE_PUBLISHABLE_KEY, Authorization: "Bearer " + token, "Content-Type": "application/json" },
        body: JSON.stringify({ action: action === "admin_send" ? "issue_admin" : "issue", email, otpHash: digest })
      });
      if (!rpc.ok) {
        const error = await rpc.json().catch(() => ({}));
        return json(rpc.status, { error: error.error || "Could not create verification challenge" });
      }
      await sendGmailOtp(env, email, code);
      return json(200, { ok: true, message: "Verification code sent. Check your inbox." });
    }
    if (action === "verify" || action === "admin_verify") {
      const code = String(body.code || "").trim();
      if (!/^\d{6}$/.test(code)) return json(400, { error: "Enter the six-digit verification code" });
      const digest = await otpDigest(env.EMAIL_OTP_SECRET, user.id, email, code);
      const rpc = await fetch(base + "/functions/v1/cenai-email-otp", {
        method: "POST",
        headers: { apikey: env.SUPABASE_PUBLISHABLE_KEY, Authorization: "Bearer " + token, "Content-Type": "application/json" },
        body: JSON.stringify({ action: action === "admin_verify" ? "verify_admin" : "verify", email, otpHash: digest })
      });
      const result = rpc.ok ? await rpc.json().catch(() => ({})) : {};
      if (!rpc.ok || result.verified !== true) return json(rpc.status || 400, { error: result.error || "Code is invalid, expired, or already used" });
      return json(200, { ok: true, verified: true, message: "Email verified for this CenAI account" });
    }
    return json(400, { error: "Unknown email verification action" });
  } catch (error) {
    const detail = error instanceof Error ? error.message : "unknown error";
    console.error("CenAI email verification failed", detail);
    let message = "Email verification service is temporarily unavailable. Please try again.";
    if (/timed out during connection|connect/i.test(detail)) {
      message = "CenAI could not connect to the email provider. Please retry shortly.";
    } else if (/password authentication|535|534/i.test(detail)) {
      message = "The email provider rejected the configured sender credentials. The service administrator must update the Gmail app password.";
    } else if (/sender validation|MAIL FROM|553/i.test(detail)) {
      message = "The configured sender address was rejected by the email provider.";
    } else if (/recipient validation|RCPT TO|550|552/i.test(detail)) {
      message = "The email provider rejected the destination address. Check the administrator email address.";
    } else if (/message acceptance|message transfer|DATA|554/i.test(detail)) {
      message = "The email provider could not accept the verification message. Please retry shortly.";
    }
    return json(503, { error: message });
  }
}

async function health(env) {
  const required = ["SUPABASE_URL", "SUPABASE_PUBLISHABLE_KEY"];
  const optional = ["CENAI_API_BASE_URL", "AI_GATEWAY_API_KEY", "EMAIL_USER", "EMAIL_PASS", "EMAIL_OTP_SECRET", "CENAI_PUBLIC_URL"];
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
    cloudAI: env.AI && typeof env.AI.run === "function" ? "binding-present; feature-flag-controlled" : "not-configured",
    paypal: "managed-by-supabase-paypal-edge-function",
    integrations: configured,
    missingConfiguration: missing,
    missingEmailOtpConfiguration: missingOtp,
    note: "Configuration status only; authenticated API requests and real email delivery must be tested separately."
  });
}

function estimateTokens(value) {
  return Math.max(1, Math.ceil(String(value || "").length / 4));
}

function streamAiChatResponse(upstream, env, token, reservation, model, inputTokens, started) {
  const base = env.SUPABASE_URL.replace(/\/$/, "");
  let reader;
  let cancelled = false;
  let finalized = false;
  let output = "";
  let buffer = "";
  const decoder = new TextDecoder();

  function collectText(line) {
    const data = line.trim().startsWith("data:") ? line.trim().slice(5).trim() : "";
    if (!data || data === "[DONE]") return;
    try {
      const value = JSON.parse(data);
      const text = value?.choices?.[0]?.delta?.content
        ?? value?.response
        ?? value?.generated_text
        ?? value?.text
        ?? "";
      if (typeof text === "string") output += text;
    } catch {
      // Ignore non-JSON SSE control events; forward the original event unchanged.
    }
  }

  async function record(status, errorCode = "") {
    if (finalized) return;
    finalized = true;
    try {
      const response = await fetch(base + "/functions/v1/cenai-ai-quota", {
        method: "POST",
        headers: { apikey: env.SUPABASE_PUBLISHABLE_KEY, Authorization: "Bearer " + token, "Content-Type": "application/json" },
        body: JSON.stringify({
          action: "record",
          eventId: reservation.eventId,
          model,
          inputTokens,
          outputTokens: estimateTokens(output),
          latencyMs: Math.max(0, Date.now() - started),
          status,
          ...(errorCode ? { errorCode } : {})
        })
      });
      if (!response.ok) console.error("CenAI streamed usage record returned HTTP", response.status);
    } catch (error) {
      console.error("CenAI streamed usage record failed", error instanceof Error ? error.message : "unknown error");
    }
  }

  const body = new ReadableStream({
    async start(controller) {
      reader = upstream.getReader();
      try {
        while (true) {
          const { done, value } = await reader.read();
          if (done) break;
          buffer += decoder.decode(value, { stream: true });
          const lines = buffer.split(/\r?\n/);
          buffer = lines.pop() || "";
          for (const line of lines) collectText(line);
          controller.enqueue(value);
        }
        buffer += decoder.decode();
        if (buffer.trim()) collectText(buffer);
        if (cancelled) return;
        if (!output) throw new Error("AI provider returned an empty streamed response");
        await record("success");
        controller.close();
      } catch (error) {
        if (!cancelled) {
          await record("error", "AI_STREAM_FAILED");
          controller.error(error);
        }
      } finally {
        try { reader.releaseLock(); } catch {}
      }
    },
    async cancel(reason) {
      cancelled = true;
      try { await reader?.cancel(reason); } catch {}
      await record("cancelled", "CLIENT_CANCELLED");
    }
  });

  return new Response(body, {
    status: 200,
    headers: {
      ...corsHeaders,
      "Content-Type": "text/event-stream; charset=utf-8",
      "Cache-Control": "no-cache, no-transform",
      "X-Accel-Buffering": "no"
    }
  });
}
__name(streamAiChatResponse, "streamAiChatResponse");

async function readRequestTextLimited(request, maxBytes) {
  if (!request.body) return "";
  const reader = request.body.getReader();
  const chunks = [];
  let total = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      const chunk = value instanceof Uint8Array ? value : new Uint8Array(value);
      total += chunk.byteLength;
      if (total > maxBytes) {
        await reader.cancel("Request body exceeds the configured limit.");
        return null;
      }
      chunks.push(chunk);
    }
  } finally {
    reader.releaseLock();
  }
  const bytes = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
}

async function cloudVoiceStatus(request, env) {
  if (request.method !== "POST") return json(405, { error: "Method not allowed" });
  if (!envReady(env, ["SUPABASE_URL", "SUPABASE_PUBLISHABLE_KEY"]) || !env.AI || typeof env.AI.run !== "function") {
    return json(503, { error: "CenAI Voice Workers AI binding is not available." });
  }
  const token = (request.headers.get("Authorization") || "").replace(/^Bearer\s+/i, "").trim();
  if (!token) return json(401, { error: "Sign in to CenAI before checking cloud voice." });
  try {
    const base = env.SUPABASE_URL.replace(/\/$/, "");
    const identity = await fetch(base + "/auth/v1/user", { headers: { apikey: env.SUPABASE_PUBLISHABLE_KEY, Authorization: "Bearer " + token }, cache: "no-store" });
    if (!identity.ok) return json(401, { error: "Your CenAI session is invalid or expired." });
    const user = await identity.json().catch(() => ({}));
    if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(String(user.id || "")) || user.is_anonymous) return json(401, { error: "A verified CenAI account is required for cloud voice." });
    const quotaResponse = await fetch(base + "/functions/v1/cenai-ai-quota", { method: "POST", headers: { apikey: env.SUPABASE_PUBLISHABLE_KEY, Authorization: "Bearer " + token, "Content-Type": "application/json" }, body: JSON.stringify({ action: "voice_status" }), signal: AbortSignal.timeout(10000), cache: "no-store" });
    const quota = await quotaResponse.json().catch(() => ({}));
    if (!quotaResponse.ok || quota.ok !== true) return json(quotaResponse.status === 401 ? 401 : quotaResponse.status === 403 ? 403 : 503, { error: quota.error || "CenAI Voice quota status is unavailable.", code: quota.code || "VOICE_QUOTA_STATUS_UNAVAILABLE" });
    return json(200, { ready: true, service: "CenAI Voice", provider: "Cloudflare Workers AI", model: "@cf/deepgram/aura-1", profile: "CenAI English · Aura-1", language: "English", quota: quota.quota });
  } catch (error) {
    console.error("CenAI Voice health check failed", error instanceof Error ? error.message : "unknown error");
    return json(503, { error: "CenAI Voice cloud service is temporarily unavailable." });
  }
}
__name(cloudVoiceStatus, "cloudVoiceStatus");

async function cloudVoiceSpeech(request, env, ctx) {
  if (request.method !== "POST") return json(405, { error: "Method not allowed" });
  if (!envReady(env, ["SUPABASE_URL", "SUPABASE_PUBLISHABLE_KEY"]) || !env.AI || typeof env.AI.run !== "function") return json(503, { error: "CenAI Voice Workers AI binding is not available." });
  const token = (request.headers.get("Authorization") || "").replace(/^Bearer\s+/i, "").trim();
  if (!token) return json(401, { error: "Sign in to CenAI before using cloud voice." });
  const declaredLength = Number(request.headers.get("Content-Length") || 0);
  if (declaredLength > 16384) return json(413, { error: "Voice request exceeds the 16 KB limit." });
  let rawBody;
  try {
    rawBody = await readRequestTextLimited(request, 16384);
  } catch {
    return json(400, { error: "Voice request must contain valid UTF-8 JSON." });
  }
  if (rawBody === null) return json(413, { error: "Voice request exceeds the 16 KB limit." });
  let body;
  try { body = JSON.parse(rawBody); } catch { return json(400, { error: "Voice request must be valid JSON." }); }
  const input = typeof body?.input === "string" ? body.input.trim() : "";
  const language = typeof body?.language === "string" ? body.language : "English";
  const voice = typeof body?.voice === "string" ? body.voice : "cenai-en-v1";
  if (!input || input.length > 1000 || /[\u0000-\u0008\u000b\u000c\u000e-\u001f]/.test(input)) return json(400, { error: "Speech text must contain 1–1,000 supported characters." });
  if (language !== "English") return json(400, { error: "CenAI Aura-1 currently supports English only." });
  if (voice !== "cenai-en-v1") return json(400, { error: "The requested CenAI Voice profile is not available." });
  const base = env.SUPABASE_URL.replace(/\/$/, "");
  try {
    const identity = await fetch(base + "/auth/v1/user", { method: "GET", headers: { apikey: env.SUPABASE_PUBLISHABLE_KEY, Authorization: "Bearer " + token }, cache: "no-store" });
    if (!identity.ok) return json(401, { error: "Your CenAI session is invalid or expired." });
    const user = await identity.json().catch(() => ({}));
    if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(String(user.id || "")) || user.is_anonymous) return json(401, { error: "A verified CenAI account is required for cloud voice." });
    const quotaResponse = await fetch(base + "/functions/v1/cenai-ai-quota", { method: "POST", headers: { apikey: env.SUPABASE_PUBLISHABLE_KEY, Authorization: "Bearer " + token, "Content-Type": "application/json" }, body: JSON.stringify({ action: "voice_authorize", characters: input.length }), signal: AbortSignal.timeout(10000), cache: "no-store" });
    const quota = await quotaResponse.json().catch(() => ({}));
    if (!quotaResponse.ok || quota.ok !== true || !Number.isSafeInteger(Number(quota.eventId))) return json(quotaResponse.status === 429 ? 429 : quotaResponse.status === 403 ? 403 : quotaResponse.status === 503 ? 503 : 429, { error: quota.error || "CenAI Voice quota could not be reserved.", code: quota.code || "VOICE_QUOTA_BLOCKED", quota: quota.quota });
    const eventId = Number(quota.eventId);
    const record = (status) => fetch(base + "/functions/v1/cenai-ai-quota", { method: "POST", headers: { apikey: env.SUPABASE_PUBLISHABLE_KEY, Authorization: "Bearer " + token, "Content-Type": "application/json" }, body: JSON.stringify({ action: "voice_record", eventId, status }), signal: AbortSignal.timeout(10000), cache: "no-store" }).then(async (r) => { if (!r.ok) console.error("CenAI Voice usage record returned HTTP", r.status); }).catch((error) => console.error("CenAI Voice usage record failed", error instanceof Error ? error.message : "unknown error"));
    let audio;
    try {
      audio = await env.AI.run("@cf/deepgram/aura-1", { text: input, speaker: "angus", encoding: "linear16", container: "wav" }, { returnRawResponse: true });
    } catch (error) {
      ctx.waitUntil(record("error"));
      console.error("CenAI Aura-1 inference failed", error instanceof Error ? error.message : "unknown error");
      return json(503, { error: "CenAI Voice synthesis is temporarily unavailable." });
    }
    if (!(audio instanceof Response) || !audio.ok) {
      ctx.waitUntil(record("error"));
      return json(503, { error: "CenAI Aura-1 returned an invalid audio response." });
    }
    const contentType = (audio.headers.get("Content-Type") || "").toLowerCase();
    const contentLength = Number(audio.headers.get("Content-Length") || 0);
    if ((!contentType.startsWith("audio/wav") && !contentType.startsWith("audio/x-wav")) || (contentLength && contentLength > 33554432)) {
      ctx.waitUntil(record("error"));
      return json(502, { error: "CenAI Aura-1 returned an unsupported or oversized audio format." });
    }
    ctx.waitUntil(record("success"));
    const headers = new Headers(corsHeaders);
    headers.set("Content-Type", "audio/wav");
    headers.set("Cache-Control", "no-store, no-transform");
    headers.set("X-CenAI-Voice-Profile", voice);
    headers.set("X-CenAI-Voice-Model", "@cf/deepgram/aura-1");
    return new Response(audio.body, { status: 200, headers });
  } catch (error) {
    console.error("CenAI Voice gateway failed", error instanceof Error ? error.message : "unknown error");
    return json(503, { error: "CenAI Voice cloud service is temporarily unavailable." });
  }
}
__name(cloudVoiceSpeech, "cloudVoiceSpeech");

async function aiChat(request, env) {
  if (request.method !== "POST") return json(405, { error: "Method not allowed" });
  if (!env.AI || typeof env.AI.run !== "function") return json(503, { error: "Cloud AI runtime is not bound to this Worker" });
  const token = (request.headers.get("Authorization") || "").replace(/^Bearer\s+/i, "").trim();
  if (!token) return json(401, { error: "Sign in required" });
  let rawBody;
  try {
    rawBody = await readRequestTextLimited(request, 1_048_576);
  } catch {
    return json(400, { error: "Chat request must contain valid UTF-8 JSON." });
  }
  if (rawBody === null) return json(413, { error: "Chat request exceeds the 1 MB limit." });
  let body;
  try { body = JSON.parse(rawBody); } catch { return json(400, { error: "Chat request must be valid JSON." }); }
  if (!body || typeof body !== "object" || Array.isArray(body) || !Array.isArray(body.messages) || body.messages.length < 1 || body.messages.length > 50) {
    return json(400, { error: "Provide between 1 and 50 chat messages." });
  }
  const messages = [];
  for (const item of body.messages) {
    const role = String(item?.role || "");
    const content = typeof item?.content === "string" ? item.content : "";
    if (!["system", "user", "assistant"].includes(role) || !content || content.length > 20000) {
      return json(400, { error: "Each message must have a supported role and text content under 20,000 characters." });
    }
    messages.push({ role, content });
  }
  const totalChars = messages.reduce((sum, item) => sum + item.content.length, 0);
  if (totalChars > 80000) return json(413, { error: "Conversation is too large for this endpoint." });
  const model = String(body.model || "cloudflare-gemma-4-26b-a4b-it");
  const maxTokens = Math.max(1, Math.min(1024, Math.floor(Number(body.max_tokens) || 1024)));
  const temperature = Math.max(0, Math.min(1.2, Number.isFinite(Number(body.temperature)) ? Number(body.temperature) : 0.7));
  const inputTokens = estimateTokens(messages.map(item => item.content).join("\n"));
  const base = env.SUPABASE_URL.replace(/\/$/, "");
  let reservation = null;
  try {
    const reserveResponse = await fetch(base + "/functions/v1/cenai-ai-quota", {
      method: "POST",
      headers: { apikey: env.SUPABASE_PUBLISHABLE_KEY, Authorization: "Bearer " + token, "Content-Type": "application/json" },
      body: JSON.stringify({ action: "authorize", model, inputTokens, maxOutputTokens: maxTokens })
    });
    reservation = await reserveResponse.json().catch(() => ({}));
    if (!reserveResponse.ok) return json(reserveResponse.status, reservation);
    const started = Date.now();
    if (body.stream === true) {
      const upstream = await env.AI.run(reservation.modelId, { messages, max_tokens: maxTokens, temperature, stream: true });
      if (!upstream || typeof upstream.getReader !== "function") throw new Error("AI runtime did not return a readable stream");
      return streamAiChatResponse(upstream, env, token, reservation, model, inputTokens, started);
    }
    const result = await env.AI.run(reservation.modelId, { messages, max_tokens: maxTokens, temperature });
    const latencyMs = Date.now() - started;
    const content = typeof result?.response === "string" ? result.response
      : typeof result?.generated_text === "string" ? result.generated_text
      : typeof result?.choices?.[0]?.message?.content === "string" ? result.choices[0].message.content
      : typeof result?.text === "string" ? result.text : "";
    if (!content) throw new Error("AI provider returned an empty response");
    const outputTokens = estimateTokens(content);
    await fetch(base + "/functions/v1/cenai-ai-quota", {
      method: "POST",
      headers: { apikey: env.SUPABASE_PUBLISHABLE_KEY, Authorization: "Bearer " + token, "Content-Type": "application/json" },
      body: JSON.stringify({ action: "record", eventId: reservation.eventId, model, inputTokens, outputTokens, latencyMs, status: "success" })
    });
    return json(200, {
      id: "chatcmpl-" + reservation.eventId,
      object: "chat.completion",
      created: Math.floor(Date.now() / 1000),
      model,
      choices: [{ index: 0, message: { role: "assistant", content }, finish_reason: "stop" }],
      usage: { prompt_tokens: inputTokens, completion_tokens: outputTokens, total_tokens: inputTokens + outputTokens },
      quota: reservation.quota
    });
  } catch (error) {
    if (reservation?.eventId) {
      await fetch(base + "/functions/v1/cenai-ai-quota", {
        method: "POST",
        headers: { apikey: env.SUPABASE_PUBLISHABLE_KEY, Authorization: "Bearer " + token, "Content-Type": "application/json" },
        body: JSON.stringify({ action: "record", eventId: reservation.eventId, model, inputTokens, outputTokens: 0, latencyMs: 0, status: "error", errorCode: "AI_INFERENCE_FAILED" })
      }).catch(() => null);
    }
    console.error("CenAI cloud inference failed", error instanceof Error ? error.message : "unknown error");
    return json(502, { error: "Cloud AI inference failed. Try again later." });
  }
}

async function adminConfig(env) {
  return json(200, { supabaseUrl: env.SUPABASE_URL || "", publishableKey: env.SUPABASE_PUBLISHABLE_KEY || "" });
}

async function sitePages(request, env) {
  if (request.method !== "GET") return json(405, { error: "Method not allowed" });
  if (!envReady(env, ["SUPABASE_URL", "SUPABASE_PUBLISHABLE_KEY"])) return json(503, { error: "Website content service is not configured" });
  const url = new URL(request.url);
  const slug = String(url.searchParams.get("slug") || "").trim().toLowerCase();
  if (slug && !/^[a-z0-9][a-z0-9-]{0,79}$/.test(slug)) return json(400, { error: "Invalid page slug" });
  const endpoint = new URL(env.SUPABASE_URL.replace(/\/$/, "") + "/rest/v1/site_pages");
  endpoint.searchParams.set("select", "slug,title,description,content,published,updated_at");
  endpoint.searchParams.set("published", "eq.true");
  if (slug) endpoint.searchParams.set("slug", "eq." + slug);
  try {
    const response = await fetch(endpoint, { headers: { apikey: env.SUPABASE_PUBLISHABLE_KEY, Authorization: "Bearer " + env.SUPABASE_PUBLISHABLE_KEY, Accept: "application/json" } });
    if (!response.ok) return json(502, { error: "Website content could not be loaded" });
    const pages = await response.json();
    return json(200, { page: slug ? (Array.isArray(pages) ? pages[0] || null : null) : null, pages: Array.isArray(pages) ? pages : [] });
  } catch (error) {
    console.error("CenAI CMS read failed", error instanceof Error ? error.message : "unknown error");
    return json(503, { error: "Website content is temporarily unavailable" });
  }
}

async function releaseFeed() {
  try {
    const response = await fetch("https://api.github.com/repos/C4RL-DANIEL/CenAi/releases?per_page=20", {
      headers: { Accept: "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28", "User-Agent": "CenAI-Release-Feed" }
    });
    if (!response.ok) throw new Error("GitHub release service returned " + response.status);
    const raw = await response.json();
    const releases = raw.filter((r) => !r.draft).map((r) => ({
      id: r.id, tag: r.tag_name, name: r.name || r.tag_name, channel: r.prerelease ? "beta" : "stable",
      publishedAt: r.published_at, notes: String(r.body || "").slice(0, 12000), htmlUrl: r.html_url,
      assets: (r.assets || []).map((a) => ({ name: a.name, size: a.size, downloadUrl: a.browser_download_url, contentType: a.content_type }))
    }));
    return json(200, { source: "github-releases", repository: "C4RL-DANIEL/CenAi", releases });
  } catch (error) {
    return json(502, { error: "Release feed temporarily unavailable", detail: error instanceof Error ? error.message : "unknown error" });
  }
}

async function betaNotification(request, env) {
  if (request.method !== "POST") return json(405, { error: "Method not allowed" });
  if (!envReady(env, ["SUPABASE_URL", "SUPABASE_PUBLISHABLE_KEY", "EMAIL_USER", "EMAIL_PASS"])) return json(503, { error: "Email delivery is not configured on Cloudflare" });
  const token = (request.headers.get("Authorization") || "").replace(/^Bearer\s+/i, "").trim();
  if (!token) return json(401, { error: "Sign in required" });
  try {
    const base = env.SUPABASE_URL.replace(/\/$/, "");
    const auth = await fetch(base + "/auth/v1/user", { headers: { apikey: env.SUPABASE_PUBLISHABLE_KEY, Authorization: "Bearer " + token } });
    const user = await auth.json().catch(() => ({}));
    if (!auth.ok || !user.id) return json(401, { error: "Invalid session" });
    const input = await request.json().catch(() => ({}));
    const id = String(input.applicationId || ""), status = String(input.status || ""), note = String(input.note || "").slice(0, 1000);
    if (!/^[0-9a-f-]{36}$/i.test(id) || !["approved", "rejected", "revoked"].includes(status)) return json(400, { error: "Invalid notification request" });
    const response = await fetch(base + "/functions/v1/cenai-control-plane", {
      method: "POST", headers: { apikey: env.SUPABASE_PUBLISHABLE_KEY, Authorization: "Bearer " + token, "Content-Type": "application/json" },
      body: JSON.stringify({ action: "admin_list", status: "all" })
    });
    const data = await response.json().catch(() => ({}));
    if (!response.ok) return json(response.status, { error: data.error || "Administrator authorization failed" });
    const app = (data.applications || []).find((item) => item.id === id && item.status === status);
    if (!app || !app.email) return json(404, { error: "Reviewed application was not found" });
    const accepted = status === "approved";
    const heading = accepted ? "You're in. Welcome to the CenAI beta." : status === "rejected" ? "Your CenAI beta application update" : "Your CenAI beta access has changed";
    const detail = accepted ? "Your application has been approved. You can now access the public beta and share feedback with the team." : status === "rejected" ? "Thank you for applying. We aren't able to offer beta access at this time." : "Your CenAI beta access has been revoked. If you believe this is a mistake, contact the team.";
    const escape = (value) => String(value || "").replace(/[&<>"']/g, (c) => ({ "&":"&amp;", "<":"&lt;", ">":"&gt;", '"':"&quot;", "'":"&#39;" })[c]);
    const html = '<!doctype html><html><body style="margin:0;background:#080d17;color:#eaf0ff;font-family:Arial,sans-serif;padding:32px 12px"><main style="max-width:560px;margin:auto;background:#101827;border:1px solid #29364c;border-radius:20px;padding:30px"><div style="font-size:12px;letter-spacing:3px;color:#a9baff;font-weight:bold">CENAI · BETA PROGRAM</div><h1>' + heading + '</h1><p style="line-height:1.7;color:#bdc9dc">' + detail + '</p>' + (note ? '<div style="padding:14px;background:#080f1d;border-radius:12px;color:#bdc9dc"><b>Team note</b><p>' + escape(note) + '</p></div>' : "") + '<p style="margin-top:28px;color:#71829b;font-size:11px">CenAI · Intelligence with permission</p></main></body></html>';
    const text = detail + (note ? "\n\nTeam note: " + note : "");
    await sendGmailMessage(env, app.email, accepted ? "You're accepted into the CenAI beta" : status === "rejected" ? "CenAI beta application update" : "CenAI beta access update", text, html);
    return json(200, { ok: true, sent: true });
  } catch (error) {
    console.error("CenAI beta notification failed", error instanceof Error ? error.message : "unknown error");
    return json(503, { error: "Could not send beta notification" });
  }
}

async function proxyFunction(request, env, slug) {
  if (request.method !== "POST") return json(405, { error: "Method not allowed" });
  const declaredLength = Number(request.headers.get("Content-Length") || 0);
  if (declaredLength > 1_048_576) return json(413, { error: "Request body exceeds the 1 MB limit" });
  const allowed = new Set(["cenai-paypal-billing", "cenai-billing", "cenai-public-api", "cenai-telemetry", "cenai-control-plane"]);
  if (!/^[a-z0-9-]+$/.test(slug) || !allowed.has(slug)) return json(404, { error: "Backend function not found" });
  if (!envReady(env, ["SUPABASE_URL", "SUPABASE_PUBLISHABLE_KEY"])) return json(503, { error: "Supabase integration is not configured" });
  const headers = new Headers();
  for (const name of ["authorization", "content-type", "apikey", "paypal-auth-algo", "paypal-cert-url", "paypal-transmission-id", "paypal-transmission-sig", "paypal-transmission-time"]) {
    const value = request.headers.get(name);
    if (value) headers.set(name, value);
  }
  if (!headers.has("apikey")) headers.set("apikey", env.SUPABASE_PUBLISHABLE_KEY);
  const init = { method: request.method, headers };
  if (!["GET", "HEAD"].includes(request.method)) {
    const body = await request.arrayBuffer();
    if (body.byteLength > 1_048_576) return json(413, { error: "Request body exceeds the 1 MB limit" });
    init.body = body;
  }
  try {
        const supabaseBase = env.SUPABASE_URL.endsWith("/") ? env.SUPABASE_URL.slice(0, -1) : env.SUPABASE_URL;
    const response = await fetch(supabaseBase + "/functions/v1/" + encodeURIComponent(slug), init);
    const body = await response.arrayBuffer();
    const resultHeaders = new Headers(response.headers);
    resultHeaders.set("Cache-Control", "no-store");
    resultHeaders.set("X-Content-Type-Options", "nosniff");
    return new Response(body, { status: response.status, headers: resultHeaders });
  } catch (error) {
    console.error("CenAI backend proxy failed", slug, error instanceof Error ? error.message : "unknown error");
    return json(503, { error: "CenAI backend service is temporarily unavailable" });
  }
}

async function staticAsset(request, url, ctx) {
  const rewritten = rewritePath(url);
  const assetPath = rewritten.pathname === "/" ? "/index.html" : rewritten.pathname;
  if (!STATIC_FILES.has(assetPath)) return withSecurity(new Response("Not found", { status: 404 }));
  if (!["GET", "HEAD"].includes(request.method)) return withSecurity(new Response("Method not allowed", { status: 405 }));
  const cacheKey = new Request(url.origin + assetPath + "?v=" + STATIC_CACHE_VERSION);
  let response = await caches.default.match(cacheKey);
  if (!response) {
    const origin = "https://raw.githubusercontent.com/C4RL-DANIEL/CenAi/main/website" + assetPath;
    try {
      const upstream = await fetch(origin);
      if (!upstream.ok) return withSecurity(new Response("Website asset unavailable", { status: 502 }));
      const extension = assetPath.split(".").pop()?.toLowerCase();
      const types = { html: "text/html; charset=utf-8", css: "text/css; charset=utf-8", js: "text/javascript; charset=utf-8", json: "application/json; charset=utf-8", svg: "image/svg+xml", txt: "text/plain; charset=utf-8", xml: "application/xml; charset=utf-8" };
      const headers = new Headers(upstream.headers);
      headers.set("Content-Type", types[extension] || "application/octet-stream");
      headers.set("Cache-Control", extension === "html" ? "public, max-age=300" : "public, max-age=3600");
      headers.set("X-Content-Type-Options", "nosniff");
      response = new Response(upstream.body, { status: 200, headers });
      ctx.waitUntil(caches.default.put(cacheKey, response.clone()));
    } catch (error) {
      console.error("CenAI static origin failed", assetPath, error instanceof Error ? error.message : "unknown error");
      return withSecurity(new Response("Website origin temporarily unavailable", { status: 502 }));
    }
  }
  return withSecurity(request.method === "HEAD" ? new Response(null, { status: response.status, headers: response.headers }) : response);
}

function rewritePath(url) {
  const path = url.pathname;
  const exact = {
    "/about": "/about.html", "/about/": "/about.html",
    "/privacy": "/privacy.html", "/privacy/": "/privacy.html",
    "/terms": "/terms.html", "/terms/": "/terms.html",
    "/contact": "/contact.html", "/contact/": "/contact.html",
    "/guides": "/guides/index.html", "/guides/": "/guides/index.html",
    "/beta": "/beta.html",
    "/beta/": "/beta.html",
    "/admin": "/admin/index.html",
    "/admin/": "/admin/index.html",
    "/register": "/register.html",
    "/register/": "/register.html",
    "/register/complete": "/register.html",
    "/reset-password": "/reset-password.html",
    "/reset-password/": "/reset-password.html",
    "/account": "/account.html",
    "/account/": "/account.html",
    "/billing": "/account.html",
    "/billing/": "/account.html",
    "/billing/success": "/billing-success.html",
    "/billing/cancel": "/billing-cancel.html"
  };
  if (exact[path]) { url.pathname = exact[path]; if (path === "/billing" || path === "/billing/") url.searchParams.set("tab", "membership"); }
  else if (path.startsWith("/guides/") && !path.endsWith(".html")) { url.pathname = path + ".html"; }
  else if (path.startsWith("/pages/")) {
    url.pathname = "/page.html";
    url.searchParams.set("slug", path.slice("/pages/".length));
  }
  return url;
}

export default {
  async fetch(request, env, ctx) {
    const url = new URL(request.url);
    if (url.pathname.startsWith("/api/")) {
      if (request.method === "OPTIONS") return new Response(null, { status: 204, headers: corsHeaders });
      if (url.pathname === "/api/admin" || url.pathname === "/api/beta") return controlPlaneProxy(request, env);
      if (url.pathname === "/api/sandbox") return sandboxProxy(request, env);
      if (url.pathname === "/api/admin-config") {
        if (request.method !== "GET") return json(405, { error: "Method not allowed" });
        return adminConfig(env);
      }
      if (url.pathname === "/api/site-pages") return sitePages(request, env);
      if (url.pathname === "/api/releases") {
        if (request.method !== "GET") return json(405, { error: "Method not allowed" });
        return releaseFeed();
      }
      if (url.pathname === "/api/beta-notification") return betaNotification(request, env);
      if (url.pathname === "/api/voice/status") return cloudVoiceStatus(request, env);
      if (url.pathname === "/api/voice/speech") return cloudVoiceSpeech(request, env, ctx);
      if (url.pathname === "/api/ai/chat/completions") return aiChat(request, env);
      if (url.pathname === "/api/email-otp") return emailOtp(request, env);
      if (url.pathname === "/api/webhooks/paypal") return proxyFunction(request, env, "cenai-paypal-billing");
      if (url.pathname.startsWith("/api/functions/")) return proxyFunction(request, env, url.pathname.slice("/api/functions/".length));
      if (url.pathname === "/api/health") {
        if (request.method !== "GET") return json(405, { error: "Method not allowed" });
        return health(env);
      }
      return json(404, { error: "API route not found" });
    }
    return staticAsset(request, url, ctx);
  },

  async scheduled(controller, env, ctx) {
    const task = (async () => {
      if (!env.CENAI_CHECKOUT_CRON_SECRET || !env.SUPABASE_URL || !env.SUPABASE_PUBLISHABLE_KEY) {
        console.error("CenAI scheduled checkout cleanup is missing required bindings.");
        return;
      }
      try {
        const supabaseBase = env.SUPABASE_URL.endsWith("/") ? env.SUPABASE_URL.slice(0, -1) : env.SUPABASE_URL;
        const response = await fetch(supabaseBase + "/functions/v1/cenai-paypal-billing", {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "apikey": env.SUPABASE_PUBLISHABLE_KEY,
            "x-cenai-checkout-cron": env.CENAI_CHECKOUT_CRON_SECRET
          },
          body: JSON.stringify({ action: "cleanup_expired" })
        });
        const result = await response.json().catch(() => ({}));
        if (!response.ok) throw new Error("Supabase cleanup returned HTTP " + response.status);
        console.log("CenAI scheduled PayPal cleanup", JSON.stringify(result));
      } catch (error) {
        console.error("CenAI scheduled PayPal cleanup failed", error instanceof Error ? error.message : "unknown error");
      }
    })();
    ctx.waitUntil(task);
  }
};
