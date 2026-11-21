(() => {
  "use strict";
  const $ = (id) => document.getElementById(id);
  const INVITE_KEY = "cenai.registration.invite.v1";
  const PKCE_KEY = "cenai.registration.pkce.v1";
  const SESSION_KEY = "cenai.account.session.v1";
  let config = null;
  let invite = "";
  let busy = false;

  function note(message, kind = "") {
    $("status").textContent = message;
    $("status").className = "status" + (kind ? " " + kind : "");
  }
  function cleanUrl(keepHash = false) {
    history.replaceState({}, document.title, location.pathname + (keepHash ? location.hash : ""));
  }
  function base64Url(bytes) {
    let binary = "";
    for (const byte of bytes) binary += String.fromCharCode(byte);
    return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
  }
  async function loadConfig() {
    const response = await fetch("/api/admin-config", { cache: "no-store" });
    if (!response.ok) throw new Error("CenAI registration is temporarily unavailable.");
    config = await response.json();
    if (!config.supabaseUrl || !config.publishableKey) throw new Error("The secure registration service is not configured.");
  }
  async function exchangeCode(code) {
    const verifier = sessionStorage.getItem(PKCE_KEY);
    if (!verifier) throw new Error("The secure sign-in session expired. Restart Discord sign-in from your invitation link.");
    const response = await fetch(config.supabaseUrl.replace(/\/$/, "") + "/auth/v1/token?grant_type=pkce", {
      method: "POST",
      headers: { "Content-Type": "application/json", apikey: config.publishableKey },
      body: JSON.stringify({ auth_code: code, code_verifier: verifier }),
      cache: "no-store"
    });
    const session = await response.json().catch(() => ({}));
    if (!response.ok || !session.access_token) throw new Error(session.msg || session.message || "Discord authorization code could not be exchanged.");
    sessionStorage.removeItem(PKCE_KEY);
    return session;
  }
  async function finishRegistration(authSession) {
    if (busy) return;
    busy = true;
    $("discord").disabled = true;
    const accessToken = authSession.access_token;
    try {
      const userResponse = await fetch(config.supabaseUrl.replace(/\/$/, "") + "/auth/v1/user", {
        headers: { apikey: config.publishableKey, Authorization: "Bearer " + accessToken },
        cache: "no-store"
      });
      const user = await userResponse.json().catch(() => ({}));
      if (!userResponse.ok || !user.id) throw new Error("Discord sign-in could not be verified. Please retry.");
      const response = await fetch("/api/beta", {
        method: "POST",
        headers: { "Content-Type": "application/json", Authorization: "Bearer " + accessToken },
        body: JSON.stringify({ action: "registration_redeem", inviteToken: invite })
      });
      const result = await response.json().catch(() => ({}));
      if (!response.ok || result.registered !== true) throw new Error(result.error || "The invitation could not be redeemed.");
      localStorage.setItem(SESSION_KEY, JSON.stringify({
        access_token: accessToken,
        refresh_token: authSession.refresh_token || "",
        expires_at: Math.floor(Date.now() / 1000) + Number(authSession.expires_in || 3600)
      }));
      sessionStorage.removeItem(INVITE_KEY);
      sessionStorage.removeItem(PKCE_KEY);
      cleanUrl(false);
      $("discord").classList.add("hidden");
      $("password-panel").classList.remove("hidden");
      $("intro").textContent = "Your Discord identity has been verified and your invitation has been redeemed.";
      note("One last step: create a password for signing in to the CenAI Android app.", "success");
    } catch (error) {
      cleanUrl(false);
      note(error.message, "error");
      $("discord").disabled = false;
    } finally {
      busy = false;
    }
  }
  async function startDiscord() {
    if (!invite) {
      note("This page needs a valid administrator-issued invitation link. Ask the CenAI owner to generate a new one.", "error");
      return;
    }
    try {
      $("discord").disabled = true;
      note("Opening Discord's secure authorization…");
      const verifier = base64Url(crypto.getRandomValues(new Uint8Array(32)));
      const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier));
      const challenge = base64Url(new Uint8Array(digest));
      sessionStorage.setItem(PKCE_KEY, verifier);
      const authorize = new URL(config.supabaseUrl.replace(/\/$/, "") + "/auth/v1/authorize");
      authorize.searchParams.set("provider", "discord");
      authorize.searchParams.set("scopes", "identify email");
      authorize.searchParams.set("redirect_to", location.origin + "/register/complete");
      authorize.searchParams.set("code_challenge", challenge);
      authorize.searchParams.set("code_challenge_method", "s256");
      location.assign(authorize.toString());
    } catch (error) {
      $("discord").disabled = false;
      note(error.message, "error");
    }
  }
  async function resumePasswordSetup() {
    const saved = JSON.parse(localStorage.getItem(SESSION_KEY) || "null");
    if (!saved?.access_token) return false;
    const response = await fetch(config.supabaseUrl.replace(/\/$/, "") + "/auth/v1/user", {
      headers: { apikey: config.publishableKey, Authorization: "Bearer " + saved.access_token },
      cache: "no-store"
    });
    if (!response.ok) return false;
    const user = await response.json().catch(() => ({}));
    if (user.app_metadata?.cenai_registration_verified !== true || user.app_metadata?.cenai_registration_method !== "discord") return false;
    $("discord").classList.add("hidden");
    $("password-panel").classList.remove("hidden");
    $("intro").textContent = "Your Discord invitation has already been redeemed. Finish setting your CenAI app password.";
    note("Your invitation is already verified. You can safely finish or retry password setup.", "success");
    return true;
  }
  async function init() {
    const params = new URLSearchParams(location.search);
    const fragmentParams = new URLSearchParams(location.hash.replace(/^#/, ""));
    const oauthCode = params.get("code");
    const oauthError = params.get("error_description") || params.get("error") || fragmentParams.get("error_description") || fragmentParams.get("error");
    invite = params.get("invite") || fragmentParams.get("invite") || sessionStorage.getItem(INVITE_KEY) || "";
    if (invite) sessionStorage.setItem(INVITE_KEY, invite);
    fragmentParams.delete("invite");
    const remainingHash = fragmentParams.toString();
    history.replaceState({}, document.title, location.pathname + (remainingHash ? "#" + remainingHash : ""));
    try {
      await loadConfig();
      if (!invite || !/^[A-Za-z0-9_-]{40,64}$/.test(invite)) {
        if (await resumePasswordSetup()) return;
        $("discord").disabled = true;
        note("This invitation link is missing or malformed. Ask the CenAI owner to generate a new link.", "error");
        return;
      }
      if (oauthError) {
        cleanUrl(false);
        note("Discord authorization was not completed. You can retry when ready.", "error");
      } else if (oauthCode) {
        cleanUrl(false);
        const session = await exchangeCode(oauthCode);
        await finishRegistration(session);
        return;
      } else {
        const fragment = new URLSearchParams(location.hash.replace(/^#/, ""));
        if (fragment.get("access_token")) {
          await finishRegistration({
            access_token: fragment.get("access_token"),
            refresh_token: fragment.get("refresh_token") || "",
            expires_in: Number(fragment.get("expires_in") || 3600)
          });
          return;
        }
        note("Invitation format is valid. It will be checked by the server after Discord sign-in.");
      }
      $("discord").disabled = false;
      $("discord").addEventListener("click", startDiscord);
    } catch (error) {
      note(error.message, "error");
      $("discord").disabled = false;
      $("discord").addEventListener("click", startDiscord);
    }
  }
  $("password-form").addEventListener("submit", async (event) => {
    event.preventDefault();
    const password = $("new-password").value;
    const confirmation = $("confirm-password").value;
    if (password.length < 12) { note("Use at least 12 characters for your password.", "error"); return; }
    if (password !== confirmation) { note("The passwords do not match.", "error"); return; }
    const saved = JSON.parse(localStorage.getItem(SESSION_KEY) || "null");
    if (!saved?.access_token) { note("Your registration session expired. Reopen the invitation link and sign in again.", "error"); return; }
    const button = event.currentTarget.querySelector("button[type=submit]");
    button.disabled = true;
    try {
      const response = await fetch(config.supabaseUrl.replace(/\/$/, "") + "/auth/v1/user", {
        method: "PUT",
        headers: { "Content-Type": "application/json", apikey: config.publishableKey, Authorization: "Bearer " + saved.access_token },
        body: JSON.stringify({ password }),
        cache: "no-store"
      });
      const result = await response.json().catch(() => ({}));
      if (!response.ok) throw new Error(result.msg || result.message || "Password could not be saved.");
      $("password-form").classList.add("hidden");
      note("Registration complete. Sign in to the CenAI Android app with your Discord email and the password you just created.", "success");
    } catch (error) {
      note(error.message, "error");
    } finally {
      button.disabled = false;
    }
  });
  init();
})();
