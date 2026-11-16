(() => {
  const $ = id => document.getElementById(id);
  let supabaseUrl = "", publishableKey = "", accessToken = "", email = "";
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
    const response = await fetch(supabaseUrl + "/functions/v1/cenai-paypal-billing", {
      method: "POST",
      headers: { "Content-Type": "application/json", apikey: publishableKey, Authorization: "Bearer " + accessToken },
      body: JSON.stringify({ action, ...payload })
    });
    const data = await response.json().catch(() => ({}));
    if (!response.ok) throw new Error(data.error || "Billing request failed.");
    return data;
  }
  async function loadPlans() {
    const data = await billingApi("list_public_plans");
    $("login-form").classList.add("hidden");
    $("account").classList.remove("hidden");
    $("signed-in").textContent = "Signed in as " + email;
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
      const button = document.createElement("button");
      button.className = "primary";
      button.type = "button";
      button.textContent = "Continue to PayPal";
      button.addEventListener("click", async () => {
        button.disabled = true;
        button.textContent = "Preparing secure checkout…";
        try {
          const checkout = await billingApi("create_subscription", { planKey: plan.plan_key });
          const approval = new URL(checkout.approvalUrl);
          if (approval.protocol !== "https:" || !(approval.hostname !== "paypal.com" && !approval.hostname.endsWith(".paypal.com"))) throw new Error("PayPal returned an unexpected approval destination.");
          location.assign(approval.toString());
        } catch (error) {
          status(error.message, true);
          button.disabled = false;
          button.textContent = "Continue to PayPal";
        }
      });
      card.append(title, price, interval, warning, button);
      $("plans").append(card);
    }
    status("Your available plans are ready. Review recurring charges before approving on PayPal.", false, true);
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
      await loadPlans();
    } catch (error) { status(error.message, true); }
    finally { $("signin").disabled = false; }
  });
  $("signout").addEventListener("click", () => {
    accessToken = ""; email = ""; $("password").value = "";
    $("account").classList.add("hidden"); $("login-form").classList.remove("hidden");
    $("plans").replaceChildren(); status("You have signed out.");
  });
  configure().then(() => status("Secure billing configuration loaded. Sign in to check plan availability."))
    .catch(error => status(error.message, true));
})();