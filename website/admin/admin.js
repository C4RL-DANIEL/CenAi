(() => {
  const $ = (id) => document.getElementById(id);
  let accessToken = null;
  let userEmail = "";
  const status = (message, bad = false) => { $("notice").textContent = message; $("notice").classList.toggle("bad", bad); };
  const esc = (value) => String(value ?? "").replace(/[&<>"']/g, c => ({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]));
  async function api(action, payload = {}) {
    const response = await fetch("/api/admin", { method:"POST", headers:{"Content-Type":"application/json","Authorization":"Bearer "+accessToken}, body:JSON.stringify({action,...payload}) });
    const data = await response.json().catch(() => ({}));
    if (!response.ok) throw new Error(data.error || "Request failed");
    return data;
  }
  async function signIn(event) {
    event.preventDefault(); $("login-error").textContent = "";
    const url = window.CENAI_SUPABASE_URL;
    const key = window.CENAI_SUPABASE_PUBLISHABLE_KEY;
    if (!url || !key) { $("login-error").textContent = "Admin authentication is not configured yet. Contact the platform owner."; return; }
    try {
      const response = await fetch(url.replace(/\/$/,"")+"/auth/v1/token?grant_type=password", {method:"POST",headers:{"Content-Type":"application/json","apikey":key},body:JSON.stringify({email:$("email").value,password:$("password").value})});
      const data = await response.json();
      if (!response.ok || !data.access_token) throw new Error(data.msg || data.message || "Sign-in failed");
      accessToken = data.access_token; userEmail = data.user?.email || $("email").value;
      $("login").hidden = true; $("dashboard").hidden = false;
      await load("overview");
    } catch (error) { accessToken = null; $("login-error").textContent = error.message; }
  }
  async function load(tab) {
    document.querySelectorAll(".tabs button").forEach(b => b.classList.toggle("active", b.dataset.tab === tab));
    ["overview","flags","audit","integrations","releases"].forEach(name => $("tab-"+name).hidden = name !== tab);
    try {
      const data = await api(tab);
      status("Signed in as "+userEmail+" · server authorization verified.");
      if (tab === "overview") {
        $("plane").textContent = data.controlPlane || "Not configured";
        $("access").textContent = "Authorized";
        $("integrations").textContent = data.integrations?.filter(x=>x.connected).length+"/"+(data.integrations?.length||0);
        $("overview-content").innerHTML = (data.items||[]).map(x=>row(x.name,x.detail,x.status)).join("");
      } else if (tab === "flags") $("flags-content").innerHTML = (data.flags||[]).map(f=>'<div class="item"><div><b>'+esc(f.key)+'</b><small>'+esc(f.description||"")+'</small></div><label class="flag"><input type="checkbox" data-flag="'+esc(f.key)+'" '+(f.enabled?"checked":"")+'> Enabled</label></div>').join("") || "<p>No feature flags configured.</p>";
      else if (tab === "audit") $("audit-content").innerHTML = (data.events||[]).map(e=>row(e.action,e.created_at+" · "+e.actor_email,e.status||"recorded")).join("") || "<p>No audit events.</p>";
      else if (tab === "integrations") $("integration-content").innerHTML = (data.integrations||[]).map(x=>row(x.name,x.detail,x.connected?"connected":"not connected")).join("");
      else if (tab === "releases") $("release-content").innerHTML = (data.releases||[]).map(x=>row(x.name,x.tag+" · "+x.publishedAt,'<a target="_blank" rel="noopener" href="'+esc(x.url)+'">Open release ↗</a>')).join("") || "<p>No published releases yet.</p>";
    } catch (error) { status(error.message,true); if (/authorized|admin|token|sign/i.test(error.message)) { accessToken=null; $("dashboard").hidden=true; $("login").hidden=false; } }
  }
  function row(name, detail, state) { return '<div class="item"><div><b>'+esc(name)+'</b><small>'+esc(detail)+'</small></div><span class="pill">'+esc(state)+'</span></div>'; }
  $("login-form").addEventListener("submit",signIn);
  $("signout").addEventListener("click",()=>{accessToken=null;userEmail="";$("dashboard").hidden=true;$("login").hidden=false;$("password").value="";});
  document.querySelectorAll(".tabs button").forEach(b=>b.addEventListener("click",()=>load(b.dataset.tab)));
  $("flags-content").addEventListener("change",async e=>{const key=e.target.dataset.flag;if(!key)return;try{await api("update_flag",{key,enabled:e.target.checked});status("Feature flag saved and audited.");}catch(err){e.target.checked=!e.target.checked;status(err.message,true);}});
  fetch("/api/admin-config").then(r=>r.ok?r.json():{}).then(c=>{window.CENAI_SUPABASE_URL=c.supabaseUrl;window.CENAI_SUPABASE_PUBLISHABLE_KEY=c.publishableKey;}).catch(()=>{});
})();