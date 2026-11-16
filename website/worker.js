import { connect } from "cloudflare:sockets";

const STATIC_CACHE_VERSION = "20261117-cenai-account-1";
const STATIC_FILES = new Set([
  "/index.html", "/styles.css", "/site-shell.css", "/billing.css", "/home.js", "/app.js", "/api.html",
  "/beta.html", "/beta.js", "/billing.html", "/billing.js", "/billing-success.html",
  "/billing-cancel.html", "/page.html", "/page.js", "/reset-password.html", "/reset-password.js",
  "/roadmap.html", "/security.html", "/updates.html", "/updates.js",
  "/admin/index.html", "/admin/admin.css", "/admin/admin.js", "/account.html", "/account.js"
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

async function sendGmailMessage(env, to, subject, text, html) {
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

async function sendGmailOtp(env, to, code) {
  const text = "Your CenAI verification code is " + code + ". It expires in 10 minutes. Do not share it.";
  const html = '<!doctype html><html><body style="margin:0;background:#080d17;font-family:Arial;color:#eaf0ff;padding:32px 12px"><main style="max-width:520px;margin:auto;background:#101827;border:1px solid #29364c;border-radius:20px;padding:30px"><p style="color:#a9baff;letter-spacing:2px;font-weight:bold">CENAI · ACCOUNT SECURITY</p><h1>Verify it\'s really you.</h1><p style="color:#bdc9dc;line-height:1.7">Use this one-time code to continue. It expires in <b>10 minutes</b>.</p><div style="margin:26px 0;padding:20px;background:#080f1d;border:1px solid #34476a;border-radius:14px;text-align:center;font-size:34px;font-weight:800;letter-spacing:9px;color:#a9baff">' + code + '</div><p style="font-size:13px;color:#91a2bc">If you did not request this code, ignore this message. Never share your code with anyone.</p><small>CenAI · Intelligence with permission</small></main></body></html>';
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
    if (action === "send") {
      const code = String(crypto.getRandomValues(new Uint32Array(1))[0] % 1000000).padStart(6, "0");
      const digest = await otpDigest(env.EMAIL_OTP_SECRET, user.id, email, code);
      const rpc = await fetch(base + "/functions/v1/cenai-email-otp", {
        method: "POST",
        headers: { apikey: env.SUPABASE_PUBLISHABLE_KEY, Authorization: "Bearer " + token, "Content-Type": "application/json" },
        body: JSON.stringify({ action: "issue", email, otpHash: digest })
      });
      if (!rpc.ok) {
        const error = await rpc.json().catch(() => ({}));
        return json(rpc.status, { error: error.error || "Could not create verification challenge" });
      }
      await sendGmailOtp(env, email, code);
      return json(200, { ok: true, message: "Verification code sent. Check your inbox." });
    }
    if (action === "verify") {
      const code = String(body.code || "").trim();
      if (!/^\d{6}$/.test(code)) return json(400, { error: "Enter the six-digit verification code" });
      const digest = await otpDigest(env.EMAIL_OTP_SECRET, user.id, email, code);
      const rpc = await fetch(base + "/functions/v1/cenai-email-otp", {
        method: "POST",
        headers: { apikey: env.SUPABASE_PUBLISHABLE_KEY, Authorization: "Bearer " + token, "Content-Type": "application/json" },
        body: JSON.stringify({ action: "verify", email, otpHash: digest })
      });
      const result = rpc.ok ? await rpc.json().catch(() => ({})) : {};
      if (!rpc.ok || result.verified !== true) return json(rpc.status || 400, { error: result.error || "Code is invalid, expired, or already used" });
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

async function aiChat(request, env) {
  if (request.method !== "POST") return json(405, { error: "Method not allowed" });
  if (!env.AI || typeof env.AI.run !== "function") return json(503, { error: "Cloud AI runtime is not bound to this Worker" });
  const token = (request.headers.get("Authorization") || "").replace(/^Bearer\s+/i, "").trim();
  if (!token) return json(401, { error: "Sign in required" });
  const body = await request.json().catch(() => null);
  if (!body || !Array.isArray(body.messages) || body.messages.length < 1 || body.messages.length > 50) {
    return json(400, { error: "Provide between 1 and 50 chat messages." });
  }
  if (body.stream === true) return json(400, { error: "Streaming is not enabled for this initial Cloud AI release." });
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
  const allowed = new Set(["cenai-paypal-billing", "cenai-billing", "cenai-public-api", "cenai-telemetry", "cenai-control-plane"]);
  if (!allowed.has(slug)) return json(404, { error: "Backend function not found" });
  if (!envReady(env, ["SUPABASE_URL", "SUPABASE_PUBLISHABLE_KEY"])) return json(503, { error: "Supabase integration is not configured" });
  const headers = new Headers();
  for (const name of ["authorization", "content-type", "apikey", "paypal-auth-algo", "paypal-cert-url", "paypal-transmission-id", "paypal-transmission-sig", "paypal-transmission-time"]) {
    const value = request.headers.get(name);
    if (value) headers.set(name, value);
  }
  if (!headers.has("apikey")) headers.set("apikey", env.SUPABASE_PUBLISHABLE_KEY);
  const init = { method: request.method, headers };
  if (!["GET", "HEAD"].includes(request.method)) init.body = await request.arrayBuffer();
  try {
    const response = await fetch(env.SUPABASE_URL.replace(/\/$/, "") + "/functions/v1/" + slug, init);
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
      const types = { html: "text/html; charset=utf-8", css: "text/css; charset=utf-8", js: "text/javascript; charset=utf-8", json: "application/json; charset=utf-8", svg: "image/svg+xml", txt: "text/plain; charset=utf-8" };
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
    "/beta": "/beta.html",
    "/beta/": "/beta.html",
    "/admin": "/admin/index.html",
    "/admin/": "/admin/index.html",
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
  }
};
