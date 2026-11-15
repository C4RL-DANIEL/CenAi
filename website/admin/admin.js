(() => {
  const $ = (id) => document.getElementById(id);
  let accessToken = null;
  let userEmail = "";
  const status = (message, bad = false) => { $("notice").textContent = message; $("notice").classList.toggle("bad", bad); };
  const esc = (value) => String(value ?? "").replace(/[&<>"']/g, c => ({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]));
  async function betaApi(action, payload = {}) { const response=await fetch("/api/beta",{method:"POST",headers:{"Content-Type":"application/json","Authorization":"Bearer "+accessToken},body:JSON.stringify({action,...payload})});const data=await response.json().catch(()=>({}));if(!response.ok)throw new Error(data.error||"Beta request failed");return data;}\n  async function api(action, payload = {}) {
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
    ["overview","flags","audit","integrations","releases","beta","feedback"].forEach(name => $("tab-"+name).hidden = name !== tab);
    try {
      const data = (tab === "beta" || tab === "feedback") ? {} : await api(tab);
      status("Signed in as "+userEmail+" · server authorization verified.");
      if (tab === "overview") {
        $("plane").textContent = data.controlPlane || "Not configured";
        $("access").textContent = "Authorized";
        $("integrations").textContent = data.integrations?.filter(x=>x.connected).length+"/"+(data.integrations?.length||0);
        $("overview-content").innerHTML = (data.items||[]).map(x=>row(x.name,x.detail,x.status)).join("");
      } else if (tab === "flags") $("flags-content").innerHTML = (data.flags||[]).map(f=>'<div class="item"><div><b>'+esc(f.key)+'</b><small>'+esc(f.description||"")+'</small></div><label class="flag"><input type="checkbox" data-flag="'+esc(f.key)+'" '+(f.enabled?"checked":"")+'> Enabled</label></div>').join("") || "<p>No feature flags configured.</p>";
      else if (tab === "audit") $("audit-content").innerHTML = (data.events||[]).map(e=>row(e.action,e.created_at+" · "+e.actor_email,e.status||"recorded")).join("") || "<p>No audit events.</p>";
      else if (tab === "integrations") $("integration-content").innerHTML = (data.integrations||[]).map(x=>row(x.name,x.detail,x.connected?"connected":"not connected")).join("");
      else if (tab === "releases") $("release-content").innerHTML = (data.releases||[]).map(x=>row(x.name,x.tag+" · "+x.publishedAt,"Open release",x.url)).join("") || "<p>No published releases yet.</p>";\n      else if(tab === "beta") await loadBeta();\n      else if(tab === "feedback") { const d=await betaApi("admin_feedback"); $("feedback-content").innerHTML=(d.feedback||[]).map(f=>row(f.title,f.category+" · "+f.app_version+" · "+f.created_at,f.body)).join("")||"<p>No feedback received.</p>"; }
    } catch (error) { status(error.message,true); if (/authorized|admin|token|sign/i.test(error.message)) { accessToken=null; $("dashboard").hidden=true; $("login").hidden=false; } }
  }
  async function loadBeta(){const d=await betaApi("admin_list",{status:$("beta-filter").value});$("beta-content").innerHTML=(d.applications||[]).map(a=>'<article class="item beta-item"><div><b>'+esc(a.display_name)+' · '+esc(a.email)+'</b><small>'+esc(a.status)+' · '+esc(a.experience)+' · '+esc(a.created_at)+'</small><p>'+esc(a.motivation)+'</p><small>Review note: '+esc(a.reviewer_note||"—")+'</small></div><div class="beta-actions" data-id="'+esc(a.id)+'">'+(a.status!=="approved"?'<button data-review="approved">Approve</button>':"")+(a.status!=="rejected"?'<button data-review="rejected">Reject</button>':"")+(a.status==="approved"?'<button data-review="revoked">Revoke</button>':"")+'</div></article>').join("")||"<p>No applications in this filter.</p>";}
  $("beta-filter")?.addEventListener("change",()=>loadBeta().catch(e=>status(e.message,true)));
  $("beta-refresh")?.addEventListener("click",()=>loadBeta().catch(e=>status(e.message,true)));
  $("beta-content")?.addEventListener("click",async e=>{const b=e.target.closest("button[data-review]");if(!b)return;const parent=b.closest("[data-id]");if(!confirm("Are you sure you want to "+b.dataset.review+" this beta application?"))return;try{await betaApi("admin_review",{id:parent.dataset.id,status:b.dataset.review,note:"Reviewed in Send.ai Control"});status("Beta application "+b.dataset.review+".");await loadBeta();}catch(err){status(err.message,true);}});
  function row(name, detail, state, url) { const action = url && /^https:\/\/github\.com\//i.test(url) ? '<a class="pill" target="_blank" rel="noopener noreferrer" href="'+esc(url)+'">'+esc(state)+' ↗</a>' : '<span class="pill">'+esc(state)+'</span>'; return '<div class="item"><div><b>'+esc(name)+'</b><small>'+esc(detail)+'</small></div>'+action+'</div>'; }
  $("login-form").addEventListener("submit",signIn);
  $("signout").addEventListener("click",()=>{accessToken=null;userEmail="";$("dashboard").hidden=true;$("login").hidden=false;$("password").value="";});
  document.querySelectorAll(".tabs button").forEach(b=>b.addEventListener("click",()=>load(b.dataset.tab)));
  $("flags-content").addEventListener("change",async e=>{const key=e.target.dataset.flag;if(!key)return;try{await api("update_flag",{key,enabled:e.target.checked});status("Feature flag saved and audited.");}catch(err){e.target.checked=!e.target.checked;status(err.message,true);}});
  fetch("/api/admin-config").then(r=>r.ok?r.json():{}).then(c=>{window.CENAI_SUPABASE_URL=c.supabaseUrl;window.CENAI_SUPABASE_PUBLISHABLE_KEY=c.publishableKey;}).catch(()=>{});
})();