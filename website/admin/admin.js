(() => {
  const $ = (id) => document.getElementById(id);
  let accessToken = null;
  let userEmail = "";
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
    const response = await fetch(url.replace(/\/$/, "") + "/functions/v1/cenai-paypal-billing", {
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
  async function signIn(event) {
    event.preventDefault(); $("login-error").textContent = "";
    try {
      await ensureConfig();
      const url = window.CENAI_SUPABASE_URL;
      const key = window.CENAI_SUPABASE_PUBLISHABLE_KEY;
      const response = await fetch(url.replace(/\/$/,"")+"/auth/v1/token?grant_type=password", {method:"POST",headers:{"Content-Type":"application/json","apikey":key},body:JSON.stringify({email:loginEmail(),password:$("password").value})});
      const data = await response.json();
      if (!response.ok || !data.access_token) throw new Error(data.msg || data.message || "Sign-in failed");
      accessToken = data.access_token; userEmail = data.user?.email || loginEmail();
      $("login").hidden = true; $("dashboard").hidden = false;
      await load("overview");
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
    ["overview","flags","audit","integrations","releases","models","api-keys","analytics","billing","beta","feedback"].forEach(name => $("tab-"+name).hidden = name !== tab);
    try {
      const data = ["beta","feedback","models","api-keys","analytics","billing"].includes(tab) ? {} : await api(tab);
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
      else if (tab === "models") await loadModels();
      else if (tab === "api-keys") await loadApiKeys();
      else if (tab === "analytics") await loadAnalytics();\n      else if (tab === "billing") await loadBillingPlans();
      else if(tab === "beta") await loadBeta();
      else if(tab === "feedback") { const d=await betaApi("admin_feedback"); $("feedback-content").innerHTML=(d.feedback||[]).map(f=>row(f.title,f.category+" · "+f.app_version+" · "+f.created_at,f.body)).join("")||"<p>No feedback received.</p>"; }
    } catch (error) { status(error.message,true); if (/authorized|admin|token|sign/i.test(error.message)) { accessToken=null; $("dashboard").hidden=true; $("login").hidden=false; } }
  }
  async function loadBillingPlans() {
    const data = await paypalApi("list_plans");
    $("billing-plan-content").innerHTML = (data.plans || []).map(p =>
      '<div class="item"><div><b>' + esc(p.display_name) + ' · ' + esc(p.plan_key) + '</b><small>' +
      esc(p.amount) + ' ' + esc(p.currency) + ' / ' + esc(p.interval_count) + ' ' + esc(p.interval_unit) +
      ' · PayPal plan ' + esc(p.paypal_plan_id || "not created") + '</small></div><span class="pill">' +
      (p.enabled ? "enabled" : "disabled") + (p.is_test ? " · test" : "") + '</span></div>'
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
  async function loadAnalytics() {
    const d=await api("usage_analytics",{days:Number($("analytics-days").value)});
    $("analytics-content").innerHTML=[row("Requests",String(d.total||0),d.days+" days"),row("Successful / failed",(d.succeeded||0)+" / "+(d.failed||0),"events"),row("Input / output tokens",(d.inputTokens||0)+" / "+(d.outputTokens||0),"estimated/recorded"),row("Average latency",String(d.averageLatencyMs||0)+" ms","aggregate"),row("Provider breakdown",Object.entries(d.providers||{}).map(([k,v])=>k+": "+v).join(" · ")||"No events","source")].join("")+(d.truncated?"<p class='muted'>Showing the most recent 5,000 events.</p>":"");
  }
  async function loadBeta(){const d=await betaApi("admin_list",{status:$("beta-filter").value});$("beta-content").innerHTML=(d.applications||[]).map(a=>'<article class="item beta-item"><div><b>'+esc(a.display_name)+' · '+esc(a.email)+'</b><small>'+esc(a.status)+' · '+esc(a.experience)+' · '+esc(a.created_at)+'</small><p>'+esc(a.motivation)+'</p><small>Review note: '+esc(a.reviewer_note||"—")+'</small></div><div class="beta-actions" data-id="'+esc(a.id)+'">'+(a.status!=="approved"?'<button data-review="approved">Approve</button>':"")+(a.status!=="rejected"?'<button data-review="rejected">Reject</button>':"")+(a.status==="approved"?'<button data-review="revoked">Revoke</button>':"")+'</div></article>').join("")||"<p>No applications in this filter.</p>";}
  $("beta-filter")?.addEventListener("change",()=>loadBeta().catch(e=>status(e.message,true)));
  $("beta-refresh")?.addEventListener("click",()=>loadBeta().catch(e=>status(e.message,true)));
  $("beta-content")?.addEventListener("click",async e=>{const b=e.target.closest("button[data-review]");if(!b)return;const parent=b.closest("[data-id]");if(!confirm("Are you sure you want to "+b.dataset.review+" this beta application?"))return;try{await betaApi("admin_review",{id:parent.dataset.id,status:b.dataset.review,note:"Reviewed in CenAI Control"});try{const mail=await fetch("/api/beta-notification",{method:"POST",headers:{"Content-Type":"application/json","Authorization":"Bearer "+accessToken},body:JSON.stringify({applicationId:parent.dataset.id,status:b.dataset.review,note:"Reviewed in CenAI Control"})});const md=await mail.json().catch(()=>({}));if(!mail.ok)throw new Error(md.error||"Email send failed");status("Decision saved and notification email sent.");}catch(mailError){status("Decision saved, but notification email failed: "+mailError.message,true);}await loadBeta();}catch(err){status(err.message,true);}});
  function row(name, detail, state, url) { const action = url && /^https:\/\/github\.com\//i.test(url) ? '<a class="pill" target="_blank" rel="noopener noreferrer" href="'+esc(url)+'">'+esc(state)+' ↗</a>' : '<span class="pill">'+esc(state)+'</span>'; return '<div class="item"><div><b>'+esc(name)+'</b><small>'+esc(detail)+'</small></div>'+action+'</div>'; }
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
        isTest: $("billing-plan-test").checked,
        enabled: $("billing-plan-enabled").checked
      });
      statusNode.textContent = "PayPal plan created and activated: " + data.plan.paypal_plan_id + ". This only makes the plan available; it does not charge anyone until they approve a subscription.";
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
  $("analytics-refresh")?.addEventListener("click",()=>loadAnalytics().catch(e=>status(e.message,true)));
  $("analytics-days")?.addEventListener("change",()=>loadAnalytics().catch(e=>status(e.message,true)));
  $("login-form").addEventListener("submit",signIn);
  $("password-form")?.addEventListener("submit",changePassword);
  $("reset-password")?.addEventListener("click",resetPassword);
  $("signout").addEventListener("click",()=>{accessToken=null;userEmail="";$("dashboard").hidden=true;$("login").hidden=false;$("password").value="";});
  document.querySelectorAll(".tabs button").forEach(b=>b.addEventListener("click",()=>load(b.dataset.tab)));
  $("flags-content").addEventListener("change",async e=>{const key=e.target.dataset.flag;if(!key)return;try{await api("update_flag",{key,enabled:e.target.checked});status("Feature flag saved and audited.");}catch(err){e.target.checked=!e.target.checked;status(err.message,true);}});
  fetch("/api/admin-config").then(r=>r.ok?r.json():{}).then(c=>{window.CENAI_SUPABASE_URL=c.supabaseUrl;window.CENAI_SUPABASE_PUBLISHABLE_KEY=c.publishableKey;}).catch(()=>{});
  handleRecoveryLink();
})();