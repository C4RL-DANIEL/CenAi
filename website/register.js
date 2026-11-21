(() => {
  "use strict";
  const $ = (id) => document.getElementById(id);
  const INVITE_KEY = "cenai.registration.invite.v1";
  const SESSION_KEY = "cenai.account.session.v1";
  let config = null;
  let invite = "";
  let busy = false;
  function note(message, kind = "") {
    $("status").textContent = message;
    $("status").className = "status" + (kind ? " " + kind : "");
  }
  function cleanUrl() {
    history.replaceState({}, document.title, location.pathname);
  }
  async function loadConfig() {
    const response = await fetch("/api/admin-config", { cache: "no-store" });
    if (!response.ok) throw new Error("CenAI registration is temporarily unavailable.");
    config = await response.json();
    if (!config.supabaseUrl || !config.publishableKey) throw new Error("The secure registration service is not configured.");
  }
  async function finishRegistration(accessToken, authSession = null) {
    if (busy) return;
    busy = true;
    $("discord").disabled = true;
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
        refresh_token: authSession?.refresh_token || "",
        expires_at: Math.floor(Date.now() / 1000) + Number(authSession?.expires_in || 3600)
      }));
      sessionStorage.removeItem(INVITE_KEY);
      cleanUrl();
      $("discord").classList.add("hidden");
      $("intro").textContent = "Your Discord identity has been verified and your invitation has been redeemed.";
      note("CenAI registration successful. Your account is ready. You can now sign in to the CenAI app with this Discord account.", "success");
    } catch (error) {
      cleanUrl();
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
      const redirectTo = location.origin + "/register/complete";
      const authorize = new URL(config.supabaseUrl.replace(/\/$/, "") + "/auth/v1/authorize");
      authorize.searchParams.set("provider", "discord");
      authorize.searchParams.set("redirect_to", redirectTo);
      location.assign(authorize.toString());
    } catch (error) {
      $("discord").disabled = false;
      note(error.message, "error");
    }
  }
  async function init() {
    const params = new URLSearchParams(location.search);
    const oauthCode = params.get("code");
    const oauthError = params.get("error_description") || params.get("error");
    invite = params.get("invite") || sessionStorage.getItem(INVITE_KEY) || "";
    if (invite) sessionStorage.setItem(INVITE_KEY, invite);
    cleanUrl();
    try {
      await loadConfig();
      if (!invite || !/^[A-Za-z0-9_-]{40,64}$/.test(invite)) {
        $("discord").disabled = true;
        note("This invitation link is missing or malformed. Ask the CenAI owner to generate a new link.", "error");
        return;
      }
      const fragment = new URLSearchParams(location.hash.replace(/^#/, ""));
      const oauthError = fragment.get("error_description") || fragment.get("error");
      if (oauthError) {
        cleanUrl();
        note("Discord authorization was not completed. You can retry when ready.", "error");
      } else if (fragment.get("access_token")) {
        await finishRegistration(fragment.get("access_token"));
        return;
      } else {
        note("Invitation format is valid. It will be checked by the server after Discord sign-in.");
      }
      $("discord").disabled = false;
      $("discord").addEventListener("click", startDiscord);
    } catch (error) {
      note(error.message, "error");
    }
  }
  init();
})();
