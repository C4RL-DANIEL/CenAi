(() => {
  const $ = (id) => document.getElementById(id);
  let accessToken = null;
  let refreshToken = null;
  let userEmail = "";
  let mfaFactorId = "";
  let mfaMode = "";
  let sitePages = [];
  const ADMIN_USERNAME = "cenai";
  const ADMIN_EMAIL = "aglibotcarldaniel@gmail.com";
  const loginEmail = () => $("email").value.trim().toLowerCase() === ADMIN_USERNAME ? ADMIN_EMAIL : $("email").value.trim();
  const status = (message, bad = false) => { $("notice").textContent = message; $("notice").classList.toggle("bad", bad); };
  const esc = (value) => String(value ?? "").replace(/[&<>"']/g, c => ({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]));
  async function ensureConfig() {
    if (window.CENAI_SUPABASE_URL && window.CENAI_SUPABASE_PUBLISHABLE_KEY) return;
    const response = await fetch("/api/admin-config", { cache: "no-store" });
    if (!response.ok) throw new Error("Administrator authentication is not configured.");
    const config = await response.json();
    window.CENAI_SUPABASE_URL = config.supabaseUrl;
    window.CENAI_SUPABASE_PUBLISHABLE_KEY = config.publishableKey;
    if (!window.CENAI_SUPABASE_URL || !window.CENAI_SUPABASE_PUBLISHABLE_KEY) {
      throw new Error("Administrator authentication is not configured.");
    }
  }
  async function paypalApi(action, payload = {}) {
    const url = window.CENAI_SUPABASE_URL;
    const key = window.CENAI_SUPABASE_PUBLISHABLE_KEY;
    if (!url || !key || !accessToken) throw new Error("Sign in again before managing billing.");
    const response = await fetch("/api/functions/cenai-paypal-billing", {
      method: "POST",
      headers: { "Content-Type": "application/json", "apikey": key, "Authorization": "Bearer " + accessToken },
      body: JSON.stringify({ action, ...payload })
    });
    const data = await response.json().catch(() => ({}));
    if (!response.ok) throw new Error(data.error || "PayPal billing request failed.");
    return data;
  }
  async function betaApi(action, payload = {}) { const response=await fetch("/api/beta",{method:"POST",headers:{"Content-Type":"application/json","Authorization":"Bearer "+accessToken},body:JSON.stringify({action,...payload})});const data=await response.json().catch(()=>({}));if(!response.ok)throw new Error(data.error||"Beta request failed");return data;}
  async function api(action, payload = {}) {
    const response = await fetch("/api/admin", { method:"POST", headers:{"Content-Type":"application/json","Authorization":"Bearer "+accessToken}, body:JSON.stringify({action,...payload}) });
    const data = await response.json().catch(() => ({}));
    if (!response.ok) throw new Error(data.error || "Request failed");
    return data;
  }
  function jwtAal(token) {
    try {
      const part = token.split(".")[1] || "";
      const normalized = part.replace(/-/g, "+").replace(/_/g, "/").padEnd(Math.ceil(part.length / 4) * 4, "=");
      return JSON.parse(atob(normalized)).aal || "aal1";
    } catch { return "aal1"; }
  }
  async function authMfa(path, method = "GET", body) {
    const url = window.CENAI_SUPABASE_URL.replace(/\/$/, "");
    const key = window.CENAI_SUPABASE_PUBLISHABLE_KEY;
    const response = await fetch(url + "/auth/v1/" + path, {
      method,
      headers: { "Content-Type": "application/json", "apikey": key, "Authorization": "Bearer " + accessToken },
      ...(body === undefined ? {} : { body: JSON.stringify(body) })
    });
    const data = await response.json().catch(() => ({}));
    if (!response.ok) throw new Error(data.msg || data.message || data.error_description || "Authenticator verification failed.");
    return data;
  }
  async function beginMfa() {
    try {
      await api("admin_preflight");
      if (jwtAal(accessToken) === "aal2") {
        $("mfa-gate").hidden = true; $("dashboard").hidden = false;
        await load("overview"); return;
      }
      const factors = await authMfa("factors");
      const verified = (factors.totp || []).filter(f => f.status === "verified");
      $("mfa-qr").hidden = true; $("mfa-secret-wrap").hidden = true;
      if (verified.length) {
        mfaFactorId = verified[0].id; mfaMode = "challenge";
        $("mfa-instructions").textContent = "Enter the current six-digit code from your authenticator app to continue.";
      } else {
        mfaFactorId = ""; mfaMode = "email";
        $("mfa-instructions").textContent = "No authenticator is enrolled. We’ll send a one-time verification code to your administrator email.";
        $("mfa-verify").disabled = true;
        $("mfa-status").textContent = "Sending email verification code…";
        const sent = await fetch("/api/email-otp", {
          method: "POST",
          headers: { "Content-Type": "application/json", "Authorization": "Bearer " + accessToken },
          body: JSON.stringify({ action: "admin_send" })
        });
        const sentData = await sent.json().catch(() => ({}));
        if (!sent.ok) throw new Error(sentData.error || "Could not send the email verification code.");
        $("mfa-status").textContent = sentData.message || "Verification code sent. Check your email.";
      }
      if (mfaMode !== "email") $("mfa-status").textContent = "";
      else $("mfa-verify").disabled = false;
    } catch (error) {
      $("mfa-verify").disabled = false;
      $("mfa-status").textContent = error.message;
    }
  }
  async function verifyMfa() {
    const code = $("mfa-code").value.trim();
    if (!/^\d{6}$/.test(code) || (mfaMode !== "email" && !mfaFactorId)) { $("mfa-status").textContent = "Enter a valid six-digit code."; return; }
    $("mfa-verify").disabled = true; $("mfa-status").textContent = mfaMode === "email" ? "Verifying email code…" : "Verifying authenticator…";
    try {
      if (mfaMode === "email") {
        const response = await fetch("/api/email-otp", { method: "POST", headers: { "Content-Type": "application/json", "Authorization": "Bearer " + accessToken }, body: JSON.stringify({ action: "admin_verify", code }) });
        const result = await response.json().catch(() => ({}));
        if (!response.ok || result.verified !== true) throw new Error(result.error || "Email verification failed.");
        $("mfa-gate").hidden = true; $("dashboard").hidden = false; $("mfa-code").value = "";
        status("Email OTP verified · signed in as " + userEmail + ".");
        await load("overview");
        return;
      }
      const challenge = await authMfa("factors/" + encodeURIComponent(mfaFactorId) + "/challenge", "POST", {});
      const verified = await authMfa("factors/" + encodeURIComponent(mfaFactorId) + "/verify", "POST", { challenge_id: challenge.id, code });
      if (verified.access_token) accessToken = verified.access_token;
      if (verified.refresh_token) refreshToken = verified.refresh_token;
      if (jwtAal(accessToken) !== "aal2" && refreshToken) {
        const refreshed = await authMfa("token?grant_type=refresh_token", "POST", { refresh_token: refreshToken });
        if (refreshed.access_token) accessToken = refreshed.access_token;
        if (refreshed.refresh_token) refreshToken = refreshed.refresh_token;
      }
      if (jwtAal(accessToken) !== "aal2") throw new Error("Authenticator code was accepted but the session has not reached AAL2. Please sign in and retry.");
      $("mfa-gate").hidden = true; $("dashboard").hidden = false; $("mfa-code").value = "";
      status("Authenticator verified · signed in as " + userEmail + ".");
      await load("overview");
    } catch (error) { $("mfa-status").textContent = error.message; }
    finally { $("mfa-verify").disabled = false; }
  }
  async function signIn(event) {
    event.preventDefault(); $("login-error").textContent = "";
    try {
      await ensureConfig();
      const url = window.CENAI_SUPABASE_URL;
      const key = window.CENAI_SUPABASE_PUBLISHABLE_KEY;
      const response = await fetch(url.replace(/\/$/,"")+"/auth/v1/token?grant_type=password", {method:"POST",headers:{"Content-Type":"application/json","apikey":key},body:JSON.stringify({email:loginEmail(),password:$("password").value})});
      const data = await response.json();
      if (!response.ok || !data.access_token) throw new Error(data.msg || data.message || "Sign-in failed");
      accessToken = data.access_token; refreshToken = data.refresh_token || null; userEmail = data.user?.email || loginEmail();
      $("login").hidden = true; $("dashboard").hidden = true; $("mfa-gate").hidden = false;
      await beginMfa();
    } catch (error) { accessToken = null; $("login-error").textContent = error.message; }
  }
  async function resetPassword() {
    const email = loginEmail();
    if (!email.includes("@")) {
      $("login-error").textContent = "Enter CenAI or the administrator email first.";
      return;
    }
    try {
      await ensureConfig();
      const url = window.CENAI_SUPABASE_URL;
      const key = window.CENAI_SUPABASE_PUBLISHABLE_KEY;
      const recoveryUrl = new URL("/auth/v1/recover", url);
      recoveryUrl.searchParams.set("redirect_to", window.location.origin + "/admin");
      const response = await fetch(recoveryUrl.toString(), {
        method: "POST",
        headers: { "Content-Type": "application/json", "apikey": key },
        body: JSON.stringify({ email })
      });
      if (!response.ok) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.msg || data.message || "Password reset could not be requested.");
      }
      $("login-error").textContent = "If the administrator account exists, a password reset email has been sent. Check the inbox and spam folder.";
    } catch (error) {
      $("login-error").textContent = error.message;
    }
  }
  async function changePassword(event) {
    event.preventDefault();
    const password = $("new-password").value;
    const confirmation = $("confirm-password").value;
    const url = window.CENAI_SUPABASE_URL;
    const key = window.CENAI_SUPABASE_PUBLISHABLE_KEY;
    if (!accessToken || !url || !key) { $("password-status").textContent = "Sign in again before changing the password."; return; }
    if (password.length < 12) { $("password-status").textContent = "Use at least 12 characters."; return; }
    if (password !== confirmation) { $("password-status").textContent = "The passwords do not match."; return; }
    try {
      const response = await fetch(new URL("/auth/v1/user", url).toString(), {
        method: "PUT",
        headers: { "Content-Type": "application/json", "apikey": key, "Authorization": "Bearer " + accessToken },
        body: JSON.stringify({ password, data: { username: "CenAI", name: "CenAI" } })
      });
      const data = await response.json().catch(() => ({}));
      if (!response.ok) throw new Error(data.msg || data.message || "Password update failed.");
      $("password-form").reset();
      $("password-status").textContent = "Administrator password updated successfully.";
    } catch (error) {
      $("password-status").textContent = error.message;
    }
  }
  async function handleRecoveryLink() {
    const fragment = new URLSearchParams(window.location.hash.replace(/^#/, ""));
    const token = fragment.get("access_token");
    if (fragment.get("type") !== "recovery" || !token) return;
    try {
      await ensureConfig();
      accessToken = token;
      userEmail = ADMIN_EMAIL;
      $("login").hidden = true;
      $("dashboard").hidden = false;
      $("password-status").textContent = "Recovery link verified. Set your new administrator password below.";
      window.history.replaceState({}, document.title, window.location.pathname);
      $("password-panel").scrollIntoView({ behavior: "smooth", block: "center" });
    } catch (error) {
      $("login-error").textContent = error.message;
    }
  }
  async function load(tab) {
    document.querySelectorAll(".tabs button").forEach(b => b.classList.toggle("active", b.dataset.tab === tab));
    ["overview","members","flags","audit","integrations","releases","models","api-keys","analytics","website","quotas","billing","beta","feedback"].forEach(name => $("tab-"+name).hidden = name !== tab);
    try {
      const data = ["beta","feedback","models","api-keys","analytics","website","quotas","billing","members"].includes(tab) ? {} : await api(tab);
      status("Signed in as "+userEmail+" · server authorization verified.");
      if (tab === "overview") {
        $("plane").textContent = data.controlPlane || "Not configured";
        $("access").textContent = "Authorized";
        $("integrations").textContent = data.integrations?.filter(x=>x.connected).length+"/"+(data.integrations?.length||0);
        $("overview-content").innerHTML = (data.items||[]).map(x=>row(x.name,x.detail,x.status)).join("");
      } else if (tab === "flags") $("flags-content").innerHTML = (data.flags||[]).map(f=>'<div class="item"><div><b>'+esc(f.key)+'</b><small>'+esc(f.description||"")+'</small></div><label class="flag"><input type="checkbox" data-flag="'+esc(f.key)+'" '+(f.enabled?"checked":"")+'> Enabled</label></div>').join("") || "<p>No feature flags configured.</p>";
      else if (tab === "audit") $("audit-content").innerHTML = (data.events||[]).map(e=>row(e.action,e.created_at+" · "+e.actor_email,e.status||"recorded")).join("") || "<p>No audit events.</p>";
      else if (tab === "integrations") $("integration-content").innerHTML = (data.integrations||[]).map(x=>row(x.name,x.detail,x.connected?"connected":"not connected")).join("");
      else if (tab === "releases") $("release-content").innerHTML = (data.releases||[]).map(x=>row(x.name,x.tag+" · "+x.publishedAt,"Open release",x.url)).join("") || "<p>No published releases yet.</p>";
      else if (tab === "members") await loadMembers();
      else if (tab === "models") await loadModels();
      else if (tab === "api-keys") await loadApiKeys();
      else if (tab === "analytics") await loadAnalytics();
      else if (tab === "website") await loadSitePages();
      else if (tab === "quotas") await loadQuotas();
      else if (tab === "billing") await loadBillingPlans();
      else if(tab === "beta") await loadBeta();
      else if(tab === "feedback") { const d=await betaApi("admin_feedback"); $("feedback-content").innerHTML=(d.feedback||[]).map(f=>row(f.title,f.category+" · "+f.app_version+" · "+f.created_at,f.body)).join("")||"<p>No feedback received.</p>"; }
    } catch (error) { status(error.message,true); if (/authorized|admin|token|sign/i.test(error.message)) { accessToken=null; $("dashboard").hidden=true; $("login").hidden=false; } }
  }
  let memberRows = [];
  let memberPage = 1;
  const memberPerPage = 100;
  let memberHasMore = false;
  async function loadMembers(page = memberPage) {
    const data = await api("members_list", { page, perPage:memberPerPage });
    memberRows = data.members || [];
    memberPage = Number(data.page || page);
    memberHasMore = memberRows.length === memberPerPage;
    renderMembers();
    $("members-summary").textContent = "Page " + memberPage + " · " + memberRows.length + " accounts loaded. Search filters this page. Membership is active only when a verified billing record is ACTIVE and its period has not expired.";
    $("members-page-label").textContent = "Page " + memberPage;
    $("members-prev").disabled = memberPage <= 1;
    $("members-next").disabled = !memberHasMore;
  }
  function renderMembers() {
    const query = ($("members-search")?.value || "").trim().toLowerCase();
    const rows = memberRows.filter(m => !query || (m.email+" "+m.displayName).toLowerCase().includes(query));
    $("members-content").innerHTML = rows.map(m => {
      const active = m.subscriptions.filter(s => s.status === "ACTIVE" && (!s.currentPeriodEnd || Date.parse(s.currentPeriodEnd) > Date.now()));
      const subscriptions = m.subscriptions.length ? m.subscriptions.map(s => {
        const oneTime = String(s.subscriptionId || "").startsWith("ONE_TIME:");
        const effectiveStatus = s.status === "ACTIVE" && s.currentPeriodEnd && Date.parse(s.currentPeriodEnd) <= Date.now() ? "EXPIRED" : s.status;
        const cancelAction = !oneTime && effectiveStatus === "ACTIVE" ? '<button data-member-action="cancel-subscription" data-subscription-id="'+esc(s.subscriptionId)+'" type="button" class="danger">Cancel recurring plan</button>' : '';
        return '<div class="member-subscription"><b>'+esc(s.planName)+'</b><small>'+esc(s.provider)+' · '+esc(effectiveStatus.replaceAll("_"," "))+(s.amount !== null ? ' · '+esc(s.currency||"PHP")+' '+esc(s.amount) : '')+(s.currentPeriodEnd ? ' · ends '+esc(s.currentPeriodEnd) : '')+(s.cancelAtPeriodEnd ? ' · cancels at period end' : '')+(oneTime ? ' · one-time' : '')+'</small>'+cancelAction+'</div>';
      }).join("") : '<p class="muted">No membership or payment-backed entitlement recorded.</p>';
      const payments = (m.payments || []).map(p => {
        const canRefund = p.status === "COMPLETED" && p.captureId;
        const refundAction = canRefund ? '<button data-member-action="refund-one-time" data-order-id="'+esc(p.orderId)+'" type="button" class="danger">Full refund</button>' : '';
        return '<div class="member-subscription"><b>One-time card payment</b><small>'+esc(p.currency)+' '+esc(p.amount)+' · '+esc(p.status)+' · paid '+esc(p.paidAt || "unknown")+' · access until '+esc(p.accessExpiresAt || "unknown")+'</small>'+refundAction+'</div>';
      }).join("");
      const suspended = m.bannedUntil && Date.parse(m.bannedUntil) > Date.now();
      const admin = Boolean(m.adminRole);
      return '<article class="item member-card" data-member-id="'+esc(m.id)+'"><div class="member-main"><div class="member-heading"><b>'+esc(m.displayName || m.email || m.id)+'</b><span class="pill">'+(admin ? esc(m.adminRole) : 'member')+'</span><span class="pill">'+(suspended ? 'suspended' : 'account active')+'</span></div><small>'+esc(m.email)+' · joined '+esc(m.createdAt || 'unknown')+' · last sign-in '+esc(m.lastSignInAt || 'never')+'</small><small>Email: '+(m.emailConfirmedAt ? 'verified' : 'not verified')+' · Beta: '+esc(m.betaStatus)+'</small><div class="member-subscriptions">'+subscriptions+payments+'</div><div class="member-actions"><label>Display name<input data-member-name maxlength="80" value="'+esc(m.displayName)+'" placeholder="Display name"></label><button data-member-action="save" type="button">Save profile</button>'+(admin ? '' : '<button data-member-action="'+(suspended?'restore':'suspend')+'" type="button" class="'+(suspended?'':'danger')+'">'+(suspended?'Restore account':'Suspend account')+'</button>')+'</div></div></article>';
    }).join("") || "<p>No members match this search.</p>";
  }
  $("members-search")?.addEventListener("input", renderMembers);
  $("members-refresh")?.addEventListener("click", () => loadMembers().catch(e => status(e.message,true)));
  $("members-prev")?.addEventListener("click", () => loadMembers(Math.max(1, memberPage - 1)).catch(e => status(e.message,true)));
  $("members-next")?.addEventListener("click", () => loadMembers(memberPage + 1).catch(e => status(e.message,true)));
  $("members-content")?.addEventListener("click", async e => {
    const button = e.target.closest("button[data-member-action]");
    if (!button) return;
    const card = button.closest("[data-member-id]");
    const userId = card?.dataset.memberId;
    if (!userId) return;
    const action = button.dataset.memberAction;
    try {
      button.disabled = true;
      if (action === "save") {
        await api("member_update", { userId, displayName:card.querySelector("[data-member-name]").value.trim() });
        status("Member profile updated and audited.");
      } else if (action === "cancel-subscription") {
        const subscriptionId = button.dataset.subscriptionId;
        if (!subscriptionId || !confirm("Cancel this recurring PayPal membership now? PayPal cancellation is immediate and CenAI access will be removed. This does not issue a refund.")) return;
        await paypalApi("admin_cancel_subscription", { subscriptionId, reason:"Cancelled by CenAI administrator" });
        status("Recurring PayPal membership cancelled. No refund was issued.");
      } else if (action === "refund-one-time") {
        const orderId = button.dataset.orderId;
        if (!orderId || !confirm("Issue a FULL refund for this completed one-time PayPal card payment? This cannot be undone. CenAI access will be revoked only after PayPal confirms the full refund.")) return;
        const result = await paypalApi("admin_refund_one_time_payment", { orderId });
        status(result.message || "PayPal refund request processed.");
      } else {
        const verb = action === "suspend" ? "suspend" : "restore";
        if (!confirm("Are you sure you want to "+verb+" this member account?")) return;
        await api(action === "suspend" ? "member_suspend" : "member_restore", { userId });
        status("Member account "+(action === "suspend" ? "suspended" : "restored")+".");
      }
      await loadMembers();
    } catch (err) { status(err.message,true); }
    finally { button.disabled = false; }
  });
  async function loadBillingPlans() {
    const data = await paypalApi("list_plans");
    const plans = data.plans || [];
    const form = $("billing-plan-form");
    // Never show a misleading default price: hydrate the editor from the saved
    // database/PayPal plan until the administrator intentionally edits it.
    if (form && !form.dataset.dirty && plans.length) {
      const current = plans.find(p => p.plan_key === $("billing-plan-key").value) || plans[0];
      $("billing-plan-key").value = current.plan_key;
      $("billing-plan-name").value = current.display_name;
      $("billing-plan-amount").value = Number(current.amount).toFixed(2);
      $("billing-plan-interval").value = current.interval_unit;
      $("billing-plan-enabled").checked = Boolean(current.enabled);
    }
    $("billing-plan-content").innerHTML = plans.map(p =>
      '<div class="item"><div><b>' + esc(p.display_name) + '</b><small>' +
      esc(p.amount) + ' ' + esc(p.currency) + ' / ' + esc(p.interval_count) + ' ' + esc(p.interval_unit) +
      ' · PayPal plan ' + esc(p.paypal_plan_id || "not created") + '</small></div><span class="pill">' +
      (p.enabled ? "Live · enabled" : "disabled") + '</span></div>'
    ).join("") || "<p>No PayPal plans configured.</p>";
  }
  async function loadModels() {
    const d=await api("models");
    $("model-content").innerHTML=(d.models||[]).map(m=>'<div class="item"><div><b>'+esc(m.display_name)+' · '+esc(m.slug)+'</b><small>'+esc(m.provider)+' / '+esc(m.model_id)+' · '+esc((m.tasks||[]).join(", "))+'</small></div><span class="pill">'+(m.enabled?"enabled":"disabled")+(m.public?" · public":" · private")+'</span></div>').join("")||"<p>No models registered yet.</p>";
  }
  async function loadApiKeys() {
    const d=await api("api_keys");
    $("api-key-content").innerHTML=(d.apiKeys||[]).map(k=>'<div class="item"><div><b>'+esc(k.name)+' · '+esc(k.key_prefix)+'…</b><small>'+esc(k.created_at)+' · Last used: '+esc(k.last_used_at||"Never")+'</small></div>'+(k.enabled?'<button data-revoke-key="'+esc(k.id)+'">Revoke</button>':'<span class="pill">Revoked</span>')).join("")||"<p>No API keys created.</p>";
  }
  function analyticsLineChart(daily) {
    if (!daily.length) return '<svg class="analytics-line" viewBox="0 0 680 190" role="img" aria-label="Request activity chart awaiting telemetry"><line x1="24" y1="24" x2="24" y2="166" stroke="#d9e1e8"/><line x1="24" y1="166" x2="656" y2="166" stroke="#d9e1e8"/><line x1="24" y1="95" x2="656" y2="95" stroke="#edf1f4" stroke-dasharray="4 5"/><text x="340" y="84" text-anchor="middle" fill="#8793a1" font-size="13">No telemetry data yet</text><text x="340" y="106" text-anchor="middle" fill="#a1acb7" font-size="10">Charts populate as users opt in</text></svg><div class="chart-labels"><span>Start</span><span>Today</span></div>';
    const width=680,height=190,pad=24,max=Math.max(1,...daily.map(x=>Number(x.requests)||0));
    const points=daily.map((x,i)=>{const px=pad+(daily.length===1?(width-pad*2)/2:i*(width-pad*2)/(daily.length-1));const py=height-pad-(Number(x.requests)||0)/max*(height-pad*2);return px.toFixed(1)+","+py.toFixed(1);}).join(" ");
    const labels=daily.filter((x,i)=>i===0||i===daily.length-1||i%Math.ceil(daily.length/5)===0).map(x=>'<span>'+esc(String(x.date).slice(5))+'</span>').join("");
    return '<svg class="analytics-line" viewBox="0 0 '+width+' '+height+'" role="img" aria-label="Requests per day line chart"><line x1="'+pad+'" y1="'+(height-pad)+'" x2="'+(width-pad)+'" y2="'+(height-pad)+'" stroke="#d9e1e8"/><line x1="'+pad+'" y1="'+(height/2)+'" x2="'+(width-pad)+'" y2="'+(height/2)+'" stroke="#edf1f4"/><polyline points="'+points+'" fill="none" stroke="#6382dc" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>'+daily.map((x,i)=>{const px=pad+(daily.length===1?(width-pad*2)/2:i*(width-pad*2)/(daily.length-1));const py=height-pad-(Number(x.requests)||0)/max*(height-pad*2);return '<circle cx="'+px.toFixed(1)+'" cy="'+py.toFixed(1)+'" r="4" fill="#fff" stroke="#4569d4" stroke-width="2"><title>'+esc(x.date)+': '+Number(x.requests||0)+' requests</title></circle>';}).join("")+'</svg><div class="chart-labels">'+labels+'</div>';
  }
  function analyticsBars(items, valueKey, labelKey) {
    if (!items.length) return '<div class="analytics-bars"><div class="analytics-bar-row"><span>Awaiting data</span><div class="bar-track"><i style="width:0%"></i></div><b>0</b></div></div><div class="chart-empty">No recorded events in this period.</div>';
    const max=Math.max(1,...items.map(x=>Number(x[valueKey])||0));
    return '<div class="analytics-bars">'+items.slice(0,7).map(x=>'<div class="analytics-bar-row"><span>'+esc(x[labelKey])+'</span><div class="bar-track"><i style="width:'+Math.max(2,(Number(x[valueKey])||0)/max*100)+'%"></i></div><b>'+Number(x[valueKey]||0).toLocaleString()+'</b></div>').join("")+'</div>';
  }
  async function loadAnalytics() {
    const d=await api("analytics_dashboard",{days:Number($("analytics-days").value)});
    const metrics=[["Requests",d.total||0,d.days+" days"],["Successful",d.statuses?.success||0,"successful events"],["Failed",d.statuses?.error||0,"failed events"],["Avg. latency",(d.averageLatencyMs||0)+" ms","P95 "+(d.p95LatencyMs||0)+" ms"]];
    $("analytics-content").innerHTML='<div class="analytics-metrics">'+metrics.map(x=>'<div class="analytics-metric"><small>'+x[0]+'</small><b>'+x[1]+'</b><span>'+x[2]+'</span></div>').join("")+'</div><div class="analytics-chart-grid"><article class="chart-card chart-wide"><div class="chart-title"><b>Request activity</b><small>Daily events · UTC</small></div>'+analyticsLineChart(d.daily||[])+'</article><article class="chart-card"><div class="chart-title"><b>Provider usage</b><small>Requests by provider</small></div>'+analyticsBars(d.providers||[],"requests","name")+'</article><article class="chart-card"><div class="chart-title"><b>Failure categories</b><small>Recorded error events</small></div>'+analyticsBars(d.failureCategories||[],"count","category")+'</article></div>'+(d.truncated?'<p class="muted">Showing the most recent 5,000 events.</p>':"");
  }
  async function loadSitePages() {
    const d=await api("site_pages"); sitePages=d.pages||[];
    $("page-picker").innerHTML='<option value="">Select a saved page</option>'+sitePages.map(p=>'<option value="'+esc(p.slug)+'">'+esc(p.title)+' · '+esc(p.slug)+'</option>').join("");
    $("site-page-content").innerHTML=sitePages.map(p=>row(p.title,p.slug+" · "+(p.published?"published":"draft")+" · "+p.updated_at,p.published?"live":"draft")+(p.published?'<p><a class="inline-link" href="/pages/'+encodeURIComponent(p.slug)+'" target="_blank" rel="noopener">Open published page ↗</a></p>':"")).join("")||"<p>No editable pages yet.</p>";
    const selected=sitePages.find(p=>p.slug===$("page-slug").value)||sitePages.find(p=>p.slug==="home")||sitePages[0];
    if(selected) fillPage(selected);
  }
  function fillPage(page) {
    $("page-slug").value=page.slug; $("page-title").value=page.title||""; $("page-description").value=page.description||"";
    $("page-content-json").value=JSON.stringify(Object.fromEntries(Object.entries(page.content||{}).filter(([key])=>key!=="bodyMarkdown"&&key!=="body")),null,2); $("page-body").value=page.content?.bodyMarkdown||page.content?.body||""; $("page-published").checked=page.published===true; $("page-picker").value=page.slug;
  }
  async function loadQuotas() {
    const d=await api("quota_list");
    $("quota-content").innerHTML='<p class="muted">Public API default: '+d.defaults.dailyRequestLimit+' requests/day and '+Number(d.defaults.monthlyTokenLimit).toLocaleString()+' tokens/month. Cloud AI default without an override: 20 requests/day and 20,000 tokens/month. Per-user overrides below apply to both services.</p>'+(d.policies||[]).map(p=>row(p.owner_user_id,"Daily "+p.daily_request_limit+" requests · Monthly "+Number(p.monthly_token_limit).toLocaleString()+" tokens · "+p.updated_at,p.enabled?"enforced":"disabled")).join("")||"<p>No per-user quota overrides. Default policy applies.</p>";
  }
  async function loadBeta(){const d=await betaApi("admin_list",{status:$("beta-filter").value});$("beta-content").innerHTML=(d.applications||[]).map(a=>'<article class="item beta-item"><div><b>'+esc(a.display_name)+' · '+esc(a.email)+'</b><small>'+esc(a.status)+' · '+esc(a.experience)+' · '+esc(a.created_at)+'</small><p>'+esc(a.motivation)+'</p><small>Review note: '+esc(a.reviewer_note||"—")+'</small></div><div class="beta-actions" data-id="'+esc(a.id)+'">'+(a.status!=="approved"?'<button data-review="approved">Approve</button>':"")+(a.status!=="rejected"?'<button data-review="rejected">Reject</button>':"")+(a.status==="approved"?'<button data-review="revoked">Revoke</button>':"")+'</div></article>').join("")||"<p>No applications in this filter.</p>";}
  $("beta-filter")?.addEventListener("change",()=>loadBeta().catch(e=>status(e.message,true)));
  $("beta-refresh")?.addEventListener("click",()=>loadBeta().catch(e=>status(e.message,true)));
  $("beta-content")?.addEventListener("click",async e=>{const b=e.target.closest("button[data-review]");if(!b)return;const parent=b.closest("[data-id]");if(!confirm("Are you sure you want to "+b.dataset.review+" this beta application?"))return;try{await betaApi("admin_review",{id:parent.dataset.id,status:b.dataset.review,note:"Reviewed in CenAI Control"});try{const mail=await fetch("/api/beta-notification",{method:"POST",headers:{"Content-Type":"application/json","Authorization":"Bearer "+accessToken},body:JSON.stringify({applicationId:parent.dataset.id,status:b.dataset.review,note:"Reviewed in CenAI Control"})});const md=await mail.json().catch(()=>({}));if(!mail.ok)throw new Error(md.error||"Email send failed");status("Decision saved and notification email sent.");}catch(mailError){status("Decision saved, but notification email failed: "+mailError.message,true);}await loadBeta();}catch(err){status(err.message,true);}});
  function row(name, detail, state, url) { const action = url && /^https:\/\/github\.com\//i.test(url) ? '<a class="pill" target="_blank" rel="noopener noreferrer" href="'+esc(url)+'">'+esc(state)+' ↗</a>' : '<span class="pill">'+esc(state)+'</span>'; return '<div class="item"><div><b>'+esc(name)+'</b><small>'+esc(detail)+'</small></div>'+action+'</div>'; }
  $("billing-plan-form")?.addEventListener("input", e => { e.currentTarget.dataset.dirty = "true"; });
  $("billing-plan-form")?.addEventListener("change", e => { e.currentTarget.dataset.dirty = "true"; });
  $("billing-plan-form")?.addEventListener("submit", async e => {
    e.preventDefault();
    const statusNode = $("billing-plan-status");
    statusNode.textContent = "Creating the PayPal Live subscription plan…";
    try {
      const data = await paypalApi("configure_plan", {
        planKey: $("billing-plan-key").value.trim(),
        displayName: $("billing-plan-name").value.trim(),
        amount: Number($("billing-plan-amount").value),
        currency: "PHP",
        intervalUnit: $("billing-plan-interval").value,
        intervalCount: 1,
        isTest: false,
        enabled: $("billing-plan-enabled").checked
      });
      statusNode.textContent = "PayPal Live plan created and activated: " + data.plan.paypal_plan_id + ". Customers can subscribe and will be charged according to the displayed recurring price after approving PayPal checkout.";
      e.currentTarget.dataset.dirty = "";
      await loadBillingPlans();
    } catch (error) {
      statusNode.textContent = error.message;
      statusNode.classList.add("bad");
    }
  });
  $("model-form")?.addEventListener("submit",async e=>{e.preventDefault();try{await api("model_upsert",{slug:$("model-slug").value,provider:$("model-provider").value,modelId:$("model-id").value,displayName:$("model-name").value,description:$("model-description").value,enabled:$("model-enabled").checked,public:$("model-public").checked,tasks:["chat"],modalities:["text"]});status("Model saved and audited.");await loadModels();}catch(err){status(err.message,true);}});
  $("api-key-form")?.addEventListener("submit",async e=>{e.preventDefault();try{const d=await api("api_key_create",{name:$("api-key-name").value});$("api-key-secret").textContent=d.secret;$("api-key-secret").hidden=false;$("copy-api-key").hidden=false;$("api-key-name").value="";status("API key created. Copy it now; it will not be shown again.");await loadApiKeys();}catch(err){status(err.message,true);}});
  $("copy-api-key")?.addEventListener("click",async()=>{try{await navigator.clipboard.writeText($("api-key-secret").textContent);status("API key copied.");}catch(e){status("Clipboard access unavailable; copy the displayed key manually.",true);}});
  $("api-key-content")?.addEventListener("click",async e=>{const b=e.target.closest("[data-revoke-key]");if(!b)return;if(!confirm("Revoke this API key immediately?"))return;try{await api("api_key_revoke",{id:b.dataset.revokeKey});status("API key revoked.");await loadApiKeys();}catch(err){status(err.message,true);}});
  $("page-form")?.addEventListener("submit",async e=>{e.preventDefault();try{let content;try{content=JSON.parse($("page-content-json").value||"{}");}catch{throw new Error("Page content must be valid JSON.");}content.bodyMarkdown=$("page-body").value;const d=await api("site_page_save",{slug:$("page-slug").value,title:$("page-title").value,description:$("page-description").value,content,published:$("page-published").checked});status("Website page saved. "+(d.page.published?"It is published.":"It remains a draft."));await loadSitePages();}catch(err){status(err.message,true);}});
  $("page-picker")?.addEventListener("change",()=>{const page=sitePages.find(p=>p.slug===$("page-picker").value);if(page)fillPage(page);});
  $("page-refresh")?.addEventListener("click",()=>loadSitePages().catch(e=>status(e.message,true)));
  $("quota-form")?.addEventListener("submit",async e=>{e.preventDefault();try{await api("quota_update",{ownerUserId:$("quota-user-id").value.trim(),dailyRequestLimit:Number($("quota-daily").value),monthlyTokenLimit:Number($("quota-monthly").value),enabled:$("quota-enabled").checked});status("Public API quota policy saved and audited.");await loadQuotas();}catch(err){status(err.message,true);}});
  $("mfa-verify")?.addEventListener("click",verifyMfa);
  $("mfa-signout")?.addEventListener("click",()=>{accessToken=null;refreshToken=null;userEmail="";mfaFactorId="";$("mfa-gate").hidden=true;$("login").hidden=false;$("mfa-code").value="";$("mfa-status").textContent="";});
  $("mfa-code")?.addEventListener("keydown",e=>{if(e.key==="Enter"){e.preventDefault();verifyMfa();}});
  $("analytics-refresh")?.addEventListener("click",()=>loadAnalytics().catch(e=>status(e.message,true)));
  $("analytics-days")?.addEventListener("change",()=>loadAnalytics().catch(e=>status(e.message,true)));
  $("login-form").addEventListener("submit",signIn);
  $("password-form")?.addEventListener("submit",changePassword);
  $("reset-password")?.addEventListener("click",resetPassword);
  $("signout").addEventListener("click",()=>{accessToken=null;refreshToken=null;userEmail="";mfaFactorId="";$("dashboard").hidden=true;$("mfa-gate").hidden=true;$("login").hidden=false;$("password").value="";$("mfa-code").value="";});
  document.querySelectorAll(".tabs button").forEach(b=>b.addEventListener("click",()=>load(b.dataset.tab)));
  $("flags-content").addEventListener("change",async e=>{const key=e.target.dataset.flag;if(!key)return;try{await api("update_flag",{key,enabled:e.target.checked});status("Feature flag saved and audited.");}catch(err){e.target.checked=!e.target.checked;status(err.message,true);}});
  fetch("/api/admin-config").then(r=>r.ok?r.json():{}).then(c=>{window.CENAI_SUPABASE_URL=c.supabaseUrl;window.CENAI_SUPABASE_PUBLISHABLE_KEY=c.publishableKey;}).catch(()=>{});
  handleRecoveryLink();
})();