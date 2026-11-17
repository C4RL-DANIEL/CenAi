(() => {
  const $ = id => document.getElementById(id);
  let supabaseUrl = "", publishableKey = "", accessToken = "", email = "", userId = "";
  const SESSION_KEY = "cenai.account.session.v1";
  let sharedSession = null;
  let paypalGuestSdkPromise = null;
  async function loadPayPalGuestSdk(clientId, currency = "PHP", amount) {
    const normalized = String(currency || "PHP").toUpperCase();
    if (!/^[A-Z]{3}$/.test(normalized)) throw new Error("The payment currency is invalid.");
    if (!clientId) throw new Error("PayPal checkout is not configured. Please use the PayPal account option.");
    if (!paypalGuestSdkPromise) paypalGuestSdkPromise = (async () => {
      if (!window.paypal?.createInstance) await new Promise((resolve, reject) => {
        const script = document.createElement("script");
        script.src = "https://www.paypal.com/web-sdk/v6/core";
        script.async = true;
        script.onload = resolve;
        script.onerror = () => reject(new Error("Could not load PayPal checkout. Check your connection and try again."));
        document.head.append(script);
      });
      if (!window.paypal?.createInstance) throw new Error("PayPal Web SDK v6 did not initialize.");
      return window.paypal.createInstance({ clientId, components: ["paypal-guest-payments"], pageType: "checkout", locale: "en-PH" });
    })().catch(error => { paypalGuestSdkPromise = null; throw error; });
    const sdk = await paypalGuestSdkPromise;
    const options = { currencyCode: normalized };
    if (Number.isFinite(Number(amount)) && Number(amount) > 0) options.amount = Number(amount).toFixed(2);
    const eligible = await sdk.findEligibleMethods(options);
    if (!eligible?.isEligible("card")) throw new Error("PayPal guest debit/credit card checkout is not available for this merchant or buyer region yet. You can still use the PayPal account subscription option.");
    return sdk;
  }
  async function mountPayPalGuestCard(sdk, mount, planKey, status) {
    mount.replaceChildren();
    const container = document.createElement("paypal-basic-card-container");
    const pay = document.createElement("paypal-basic-card-button");
    pay.setAttribute("aria-label", "Pay once by debit or credit card without a PayPal account");
    container.appendChild(pay);
    mount.appendChild(container);
    const guestSession = await sdk.createPayPalGuestOneTimePaymentSession({
      onApprove: async data => {
        const orderId = String(data?.orderId || "");
        if (!orderId) throw new Error("PayPal did not return an approved order ID.");
        status("Payment approved. Capturing and verifying your one-time card payment…");
        const capture = await billingApi("capture_guest_order", { orderId });
        status(capture.message || "One-time payment confirmed. Your 30-day membership is active and will not renew.", false, true);
      },
      onComplete: () => {},
      onCancel: () => { pay.disabled = false; pay.removeAttribute("disabled"); status("Card checkout was cancelled. No membership was activated."); },
      onError: error => { pay.disabled = false; pay.removeAttribute("disabled"); status("PayPal card checkout: " + (error?.message || "Payment could not be completed."), true); },
      onWarn: data => status(data?.message || "PayPal could not process that card. Check the details or try another card.", true),
    });
    pay.addEventListener("click", async () => {
      pay.disabled = true;
      pay.setAttribute("disabled", "");
      status("Opening PayPal's secure guest card checkout…");
      try {
        const orderPromise = billingApi("create_guest_order", { planKey }).then(order => {
          if (!order.orderId) throw new Error("PayPal did not create a one-time payment order.");
          return { orderId: order.orderId };
        });
        await guestSession.start({ presentationMode: "auto", targetElement: pay }, orderPromise);
      } catch (error) {
        pay.disabled = false;
        pay.removeAttribute("disabled");
        status("Card checkout: " + (error.message || "Payment could not be completed."), true);
      }
    });
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
      interval.textContent = "PayPal account: recurring monthly subscription · Guest debit/credit card: one-time PayPal guest payment for 30 days";
      const warning = document.createElement("p");
      warning.className = "small";
      warning.textContent = "Choose your payment type: PayPal account renews monthly; guest debit/credit card is charged once for 30 days.";
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
          const cardMount = document.createElement("div");
          cardMount.className = "paypal-card-button";
          cardMount.setAttribute("aria-label", "One-time debit or credit card payment");
          const walletButton = document.createElement("button");
          walletButton.type = "button";
          walletButton.className = "secondary paypal-recurring-button";
          walletButton.textContent = "PayPal account · " + new Intl.NumberFormat("en-PH", { style: "currency", currency: plan.currency || "PHP" }).format(Number(plan.amount)) + " / month";
          walletButton.setAttribute("aria-label", "Start recurring monthly subscription using a PayPal account");
          checkoutMount.append(cardMount, walletButton);
          walletButton.addEventListener("click", async () => {
            walletButton.disabled = true;
            status("Creating your recurring PayPal subscription…");
            try {
              const result = await billingApi("create_subscription", { planKey: plan.plan_key });
              if (!result.approvalUrl || !/^https:\/\/(?:www\.)?paypal\.com\//i.test(result.approvalUrl)) {
                throw new Error("PayPal did not return a valid subscription approval page.");
              }
              window.location.assign(result.approvalUrl);
            } catch (error) {
              status(error.message || "Could not start the PayPal subscription.", true);
              walletButton.disabled = false;
            }
          });
          try {
            const sdk = await loadPayPalGuestSdk(data.paypalClientId, plan.currency || "PHP", plan.amount);
            await mountPayPalGuestCard(sdk, cardMount, plan.plan_key, status);
          } catch (cardError) {
            const cardNotice = document.createElement("p");
            cardNotice.className = "small muted";
            cardNotice.textContent = cardError.message || "PayPal v6 card checkout could not be initialized.";
            checkoutMount.prepend(cardNotice);
          }
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