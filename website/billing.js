(() => {
  const $ = id => document.getElementById(id);
  let supabaseUrl = "", publishableKey = "", accessToken = "", email = "", userId = "";
  const SESSION_KEY = "cenai.account.session.v1";
  let sharedSession = null;
  let paypalSdkPromise = null;
  function loadPayPalSdk(clientId, currency = "PHP") {
    const normalizedCurrency = String(currency || "PHP").toUpperCase();
    if (!/^[A-Z]{3}$/.test(normalizedCurrency)) {
      return Promise.reject(new Error("The subscription currency is invalid."));
    }
    if (window.paypal?.Buttons) {
      const existingScript = document.querySelector('script[src*="paypal.com/sdk/js"]');
      if (existingScript) {
        const existingUrl = new URL(existingScript.src);
        const existingClientId = existingUrl.searchParams.get("client-id");
        const existingCurrency = (existingUrl.searchParams.get("currency") || "USD").toUpperCase();
        if (existingClientId && existingClientId !== clientId) {
          return Promise.reject(new Error("A different PayPal merchant SDK is already loaded. Reload the billing page before checkout."));
        }
        if (existingCurrency !== normalizedCurrency) {
          return Promise.reject(new Error("The loaded PayPal SDK currency does not match this subscription plan. Reload the billing page."));
        }
      }
      return Promise.resolve(window.paypal);
    }
    if (paypalSdkPromise) return paypalSdkPromise;
    paypalSdkPromise = new Promise((resolve, reject) => {
      const script = document.createElement("script");
      const params = new URLSearchParams({
        "client-id": clientId,
        currency: normalizedCurrency,
        vault: "true",
        intent: "subscription",
        components: "buttons",
        "enable-funding": "card"
      });
      script.src = "https://www.paypal.com/sdk/js?" + params.toString();
      script.async = true;
      script.dataset.cenaiPaypalSdk = "true";
      script.onload = () => window.paypal?.Buttons ? resolve(window.paypal) : reject(new Error("PayPal checkout components did not initialize."));
      script.onerror = () => {
        paypalSdkPromise = null;
        reject(new Error("Could not load PayPal checkout. Check your connection and try again."));
      };
      document.head.append(script);
    });
    return paypalSdkPromise;
  }
  function saveSharedSession(value) { sharedSession = value; localStorage.setItem(SESSION_KEY, JSON.stringify({ access_token:value.access_token, refresh_token:value.refresh_token, expires_at:value.expires_at || Math.floor(Date.now()/1000) + Number(value.expires_in || 3600) })); }
  function clearSharedSession() { sharedSession = null; localStorage.removeItem(SESSION_KEY); }
  async function restoreSharedSession() {
    const saved = JSON.parse(localStorage.getItem(SESSION_KEY) || "null"); if (!saved?.access_token) return false;
    sharedSession = saved; accessToken = saved.access_token;
    if (saved.expires_at && saved.expires_at * 1000 < Date.now() + 60000 && saved.refresh_token) {
      const refreshed = await fetch(supabaseUrl + "/auth/v1/token?grant_type=refresh_token", {method:"POST",headers:{"Content-Type":"application/json",apikey:publishableKey},body:JSON.stringify({refresh_token:saved.refresh_token})});
      const data = await refreshed.json().catch(()=>({})); if (refreshed.ok && data.access_token) { saveSharedSession(data); accessToken=data.access_token; }
    }
    const check = await fetch(supabaseUrl + "/auth/v1/user", {headers:{apikey:publishableKey,Authorization:"Bearer "+accessToken}});
    const user = await check.json().catch(()=>({})); if (!check.ok || !user.id) { if(saved.refresh_token && sharedSession===saved) { const rr=await fetch(supabaseUrl+"/auth/v1/token?grant_type=refresh_token",{method:"POST",headers:{"Content-Type":"application/json",apikey:publishableKey},body:JSON.stringify({refresh_token:saved.refresh_token})});const rd=await rr.json().catch(()=>({}));if(rr.ok&&rd.access_token){saveSharedSession(rd);accessToken=rd.access_token;email=rd.user?.email||"";userId=rd.user?.id||"";return true;} } clearSharedSession(); return false; }
    email=user.email||""; userId=user.id||""; return true;
  }
  const status = (message, bad = false, good = false) => {
    $("status").textContent = message;
    $("status").className = "notice" + (bad ? " bad" : good ? " good" : "");
  };
  async function configure() {
    const response = await fetch("/api/admin-config", { cache: "no-store" });
    if (!response.ok) throw new Error("Secure billing configuration is unavailable.");
    const config = await response.json();
    supabaseUrl = String(config.supabaseUrl || "").replace(/\/$/, "");
    publishableKey = String(config.publishableKey || "");
    if (!supabaseUrl || !publishableKey) throw new Error("Billing authentication is not configured.");
  }
  async function billingApi(action, payload = {}) {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 45000);
    let response;
    try { response = await fetch("/api/functions/cenai-paypal-billing", {
      signal: controller.signal,
      method: "POST",
      headers: { "Content-Type": "application/json", apikey: publishableKey, Authorization: "Bearer " + accessToken },
      body: JSON.stringify({ action, ...payload })
    }); } catch (error) { if (error?.name === "AbortError") throw new Error("PayPal checkout timed out after 45 seconds. No approval page was returned. Please retry; any incomplete attempt will be cancelled."); throw error; } finally { clearTimeout(timeout); }
    const data = await response.json().catch(() => ({}));
    if (!response.ok) throw new Error(data.error || "Billing request failed.");
    return data;
  }
  async function loadPlans() {
    const data = await billingApi("list_public_plans");
    $("login-form").classList.add("hidden");
    $("account").classList.remove("hidden");
    $("signed-in").textContent = "Signed in as " + email;
    const checkoutEnabled = data.billingEnabled === true;
    if (!checkoutEnabled) status("PayPal Live is configured, but checkout is temporarily disabled until subscription lifecycle verification is complete.");
    if (!data.plans?.length) {
      $("plans").textContent = "No subscriptions are currently available.";
      return;
    }
    $("plans").replaceChildren();
    for (const plan of data.plans) {
      const card = document.createElement("article");
      card.className = "plan";
      const title = document.createElement("h2");
      title.textContent = plan.display_name;
      const price = document.createElement("p");
      price.className = "price";
      price.textContent = new Intl.NumberFormat("en-PH", { style: "currency", currency: plan.currency || "PHP" }).format(Number(plan.amount));
      const interval = document.createElement("p");
      interval.className = "muted";
      interval.textContent = "Every " + plan.interval_count + " " + String(plan.interval_unit).toLowerCase();
      const warning = document.createElement("p");
      warning.className = "small";
      warning.textContent = plan.is_test ? "Temporary live-price plan — this is a real recurring payment, not PayPal Sandbox." : "Recurring subscription billed through PayPal.";
      const checkoutMount = document.createElement("div");
      checkoutMount.className = "paypal-button-mount";
      checkoutMount.setAttribute("aria-label", "PayPal subscription checkout");
      card.append(title, price, interval, warning, checkoutMount);
      if (!checkoutEnabled) {
        const disabled = document.createElement("p");
        disabled.className = "small muted";
        disabled.textContent = "Checkout temporarily unavailable.";
        card.append(disabled);
      } else {
        try {
          if (!data.paypalClientId) throw new Error("PayPal checkout is not fully configured. Please contact CenAI support.");
          const paypal = await loadPayPalSdk(data.paypalClientId, plan.currency || "PHP");
          const subscriptionCallbacks = {
            createSubscription: (_data, actions) => {
              if (!userId) throw new Error("Your CenAI session expired. Please sign in again.");
              return actions.subscription.create({ plan_id: plan.paypal_plan_id, custom_id: userId });
            },
            onApprove: async (approval) => {
              status("PayPal approval received. Verifying your subscription with PayPal…");
              try {
                const result = await billingApi("record_sdk_subscription", { subscriptionId: approval.subscriptionID, planKey: plan.plan_key });
                status(result.message || "PayPal approval verified. Membership activation is being confirmed.", false, true);
              } catch (error) {
                status("PayPal approval was received, but CenAI could not verify it: " + error.message + ". No membership access was granted yet.", true);
              }
            },
            onCancel: async () => {
              try { await billingApi("cancel_open_checkouts"); } catch {}
              status("Checkout cancelled. No subscription access was granted.");
            },
            onError: async (error) => {
              const detail = error?.message || "PayPal could not complete checkout.";
              status("PayPal checkout error: " + detail + " If PayPal displays a generic error page, check the seller account for restrictions or required verification.", true);
            }
          };
          const cardMount = document.createElement("div");
          cardMount.className = "paypal-card-button";
          cardMount.setAttribute("aria-label", "Debit or credit card subscription checkout");
          const walletMount = document.createElement("div");
          walletMount.className = "paypal-wallet-button";
          walletMount.setAttribute("aria-label", "PayPal wallet subscription checkout");
          checkoutMount.append(cardMount, walletMount);
          let renderedAny = false;
          let cardEligible = false;
          if (paypal.FUNDING?.CARD) {
            const cardButtons = paypal.Buttons({
              ...subscriptionCallbacks,
              fundingSource: paypal.FUNDING.CARD,
              style: { shape: "pill", color: "black", layout: "vertical", label: "subscribe", tagline: false }
            });
            if (cardButtons.isEligible()) {
              await cardButtons.render(cardMount);
              cardEligible = true;
              renderedAny = true;
            }
          }
          const walletButtons = paypal.Buttons({
            ...subscriptionCallbacks,
            style: { shape: "pill", color: "gold", layout: "vertical", label: "subscribe", tagline: false }
          });
          if (walletButtons.isEligible()) {
            await walletButtons.render(walletMount);
            renderedAny = true;
          }
          if (!cardEligible) {
            const cardNotice = document.createElement("p");
            cardNotice.className = "small muted";
            cardNotice.textContent = "Debit/credit card guest checkout is not enabled for this merchant yet. PayPal must activate Enhanced Automatic Payments for eligible subscription card payments.";
            checkoutMount.append(cardNotice);
          }
          if (!renderedAny) throw new Error("PayPal has not enabled an eligible subscription funding method for this account or region.");
        } catch (error) {
          checkoutMount.textContent = "";
          const message = document.createElement("p");
          message.className = "small";
          message.textContent = error.message || "PayPal checkout could not be initialized.";
          checkoutMount.append(message);
          status(message.textContent, true);
        }
      }

      $("plans").append(card);
    }

    let diagnosticPanel = $("billing-diagnostics");
    if (!diagnosticPanel) {
      diagnosticPanel = document.createElement("section");
      diagnosticPanel.id = "billing-diagnostics";
      diagnosticPanel.className = "billing-diagnostics";
      const heading = document.createElement("h3");
      heading.textContent = "Checkout diagnostics";
      const explanation = document.createElement("p");
      explanation.className = "small muted";
      explanation.textContent = "If PayPal shows an error, check the latest subscription status and the reason PayPal reports.";
      const diagnose = document.createElement("button");
      diagnose.id = "diagnose-checkout";
      diagnose.type = "button";
      diagnose.className = "secondary";
      diagnose.textContent = "Check latest PayPal checkout";
      const output = document.createElement("pre");
      output.id = "diagnostic-output";
      output.className = "small";
      output.setAttribute("aria-live", "polite");
      diagnosticPanel.append(heading, explanation, diagnose, output);
      $("account").append(diagnosticPanel);
      diagnose.addEventListener("click", async () => {
        diagnose.disabled = true;
        diagnose.textContent = "Checking PayPal…";
        output.textContent = "Retrieving the latest subscription details from PayPal.";
        try {
          const result = await billingApi("diagnose_recent_checkouts");
          const rows = Array.isArray(result.diagnostics) ? result.diagnostics : [];
          output.textContent = rows.length ? rows.map(row => {
            const id = String(row.subscriptionId || "").slice(-8);
            if (row.error) return `Checkout …${id}\nPayPal lookup: ${row.error}`;
            return [
              `Checkout …${id}: ${row.paypalStatus || "UNKNOWN"} (CenAI: ${row.recordedStatus || "unknown"})`,
              `Plan: ${row.planAmount || "?"} ${row.planCurrency || ""} · ${row.planStatus || "unknown"}`,
              `Last failed payment: ${row.lastFailedPaymentReasonCode || "none reported"}`,
              `PayPal status note: ${row.statusChangeNote || "none supplied"}`,
              `Next retry: ${row.nextPaymentRetryTime || "not scheduled"}`,
            ].join("\n");
          }).join("\n\n") : "No recent checkout attempts were found for this account.";
        } catch (error) {
          output.textContent = error.message || "Could not retrieve PayPal diagnostics.";
        } finally {
          diagnose.disabled = false;
          diagnose.textContent = "Check latest PayPal checkout";
        }
      });
    }
    if (checkoutEnabled) status("Your available plans are ready. Review recurring charges before approving on PayPal.", false, true);
  }
  $("login-form").addEventListener("submit", async event => {
    event.preventDefault();
    $("signin").disabled = true;
    try {
      await configure();
      email = $("email").value.trim();
      const response = await fetch(supabaseUrl + "/auth/v1/token?grant_type=password", {
        method: "POST", headers: { "Content-Type": "application/json", apikey: publishableKey },
        body: JSON.stringify({ email, password: $("password").value })
      });
      const auth = await response.json().catch(() => ({}));
      if (!response.ok || !auth.access_token) throw new Error(auth.msg || auth.message || "Sign in failed.");
      accessToken = auth.access_token;
      userId=auth.user?.id||"";
      saveSharedSession(auth);
      await loadPlans();
    } catch (error) { status(error.message, true); }
    finally { $("signin").disabled = false; }
  });
  $("signout").addEventListener("click", () => {
    accessToken = ""; email = ""; clearSharedSession(); $("password").value = "";
    $("account").classList.add("hidden"); $("login-form").classList.remove("hidden");
    $("plans").replaceChildren(); status("You have signed out.");
  });
  configure().then(async () => { if (await restoreSharedSession()) await loadPlans(); else status("Secure billing configuration loaded. Sign in to check plan availability."); })
    .catch(error => status(error.message, true));
})();