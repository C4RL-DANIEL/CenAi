(() => {
  const SUPABASE_URL = "https://bqkzehozezvepbthglyj.supabase.co";
  const SUPABASE_KEY = "sb_publishable_l6YDudJWsFgUJ2YZc2Xq1Q_hNatLp3C";
  const status = document.getElementById("status");
  const reset = document.getElementById("reset");
  const request = document.getElementById("request");
  const form = document.getElementById("reset-form");
  const requestForm = document.getElementById("request-form");
  const submit = document.getElementById("submit");
  const send = document.getElementById("send");
  const params = new URLSearchParams(location.hash.slice(1));
  const accessToken = params.get("access_token");
  const tokenType = params.get("type");
  const query = new URLSearchParams(location.search);
  const code = query.get("error_description") || query.get("error");
  const hasRecoverySession = !!accessToken && (!tokenType || tokenType === "recovery");

  function message(text, kind) {
    status.textContent = text;
    status.className = "status show " + kind;
  }
  function showRequest(text) {
    reset.hidden = true;
    request.hidden = false;
    if (text) message(text, "error");
  }

  if (hasRecoverySession) {
    // Keep the bearer token in memory only; never persist it or send it to analytics.
    history.replaceState(null, "", location.pathname + location.search);
  } else {
    showRequest(code ? decodeURIComponent(code.replace(/\+/g, " ")) : "This recovery link is invalid, expired, or already used. Request a new one below.");
  }

  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    const password = document.getElementById("password").value;
    const confirm = document.getElementById("confirm").value;
    if (Array.from(password).length < 12) return message("Use at least 12 characters.", "error");
    if (Array.from(password).length > 128) return message("Use no more than 128 characters.", "error");
    if (password !== confirm) return message("The passwords do not match.", "error");
    submit.disabled = true;
    submit.textContent = "Updating…";
    try {
      const response = await fetch(SUPABASE_URL + "/functions/v1/cenai-password-policy", {
        method: "POST",
        headers: { "apikey": SUPABASE_KEY, "Authorization": "Bearer " + accessToken, "Content-Type": "application/json" },
        body: JSON.stringify({ password }),
        cache: "no-store"
      });
      const data = await response.json().catch(() => ({}));
      if (!response.ok || data.updated !== true) throw new Error(data.error || "Password update failed.");
      form.hidden = true;
      message("Your CenAI password has been updated. You can now return to the app and sign in with your new password.", "ok");
      history.replaceState(null, "", location.pathname);
    } catch (error) {
      message(error.message || "Could not update the password. Request a fresh recovery link and try again.", "error");
    } finally {
      submit.disabled = false;
      submit.textContent = "Update password";
    }
  });

  requestForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    send.disabled = true;
    send.textContent = "Sending…";
    try {
      const email = document.getElementById("email").value.trim();
      const redirectTo = "https://cenai-website.cenai-app.workers.dev/reset-password";
      const response = await fetch(SUPABASE_URL + "/auth/v1/recover?redirect_to=" + encodeURIComponent(redirectTo), {
        method: "POST",
        headers: { "apikey": SUPABASE_KEY, "Content-Type": "application/json" },
        body: JSON.stringify({ email })
      });
      const data = await response.json().catch(() => ({}));
      if (!response.ok) throw new Error(data.msg || data.message || data.error_description || "Could not send recovery email.");
      message("If an account exists for that address, a recovery email has been sent. Check your inbox and spam folder.", "ok");
    } catch (error) {
      message(error.message || "Could not send recovery email.", "error");
    } finally {
      send.disabled = false;
      send.textContent = "Send recovery email";
    }
  });
})();