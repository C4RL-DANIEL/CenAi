import "jsr:@supabase/functions-js/edge-runtime.d.ts";
import { createClient } from "npm:@supabase/supabase-js@2.57.4";

const SUPABASE_URL = Deno.env.get("SUPABASE_URL")!;
const SUPABASE_ANON_KEY = Deno.env.get("SUPABASE_ANON_KEY")!;
const SUPABASE_SERVICE_ROLE_KEY = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!;
const db = createClient(SUPABASE_URL, SUPABASE_SERVICE_ROLE_KEY, { auth: { persistSession: false, autoRefreshToken: false } });
const authDb = createClient(SUPABASE_URL, SUPABASE_ANON_KEY, { auth: { persistSession: false, autoRefreshToken: false } });
const API = "https://api-m.paypal.com";
const cors = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Headers": "authorization, apikey, content-type, paypal-auth-algo, paypal-cert-url, paypal-transmission-id, paypal-transmission-sig, paypal-transmission-time, paypal-transmission-sig",
  "Access-Control-Allow-Methods": "POST, OPTIONS",
  "Vary": "Origin",
};
const json = (status: number, body: unknown) => new Response(JSON.stringify(body), {
  status, headers: { ...cors, "Content-Type": "application/json", "Cache-Control": "no-store" },
});
const fail = (status: number, error: string) => json(status, { error });
const baseUrl = () => "https://cenai-website.cenai-app.workers.dev";

async function currentUser(req: Request) {
  const token = (req.headers.get("Authorization") || "").replace(/^Bearer\s+/i, "");
  if (!token) return null;
  const { data, error } = await authDb.auth.getUser(token);
  if (error || !data.user || data.user.is_anonymous) return null;
  return data.user;
}
async function isAdmin(userId: string) {
  const { data, error } = await db.from("admin_users").select("role,is_active").eq("user_id", userId).maybeSingle();
  if (error) throw error;
  return Boolean(data?.is_active && ["owner", "admin"].includes(data.role));
}
function tokenAal(req: Request) {
  try {
    const token = (req.headers.get("Authorization") || "").trim().split(" ").pop() || "";
    const part = token.split(".")[1] || "";
    const normalized = part.replace(/-/g, "+").replace(/_/g, "/").padEnd(Math.ceil(part.length / 4) * 4, "=");
    return String(JSON.parse(atob(normalized)).aal || "aal1");
  } catch { return "aal1"; }
}
function paypalConfig() {
  const clientId = Deno.env.get("PAYPAL_CLIENT_ID") || "";
  const clientSecret = Deno.env.get("PAYPAL_CLIENT_SECRET") || "";
  const webhookId = Deno.env.get("PAYPAL_WEBHOOK_ID") || "";
  if (!clientId || !clientSecret) throw new Error("PayPal Live credentials are not configured on the server.");
  return { clientId, clientSecret, webhookId };
}
async function accessToken() {
  const { clientId, clientSecret } = paypalConfig();
  const basic = btoa(clientId + ":" + clientSecret);
  const response = await fetch(API + "/v1/oauth2/token", {
    signal: AbortSignal.timeout(20000),
    method: "POST",
    headers: { Authorization: "Basic " + basic, "Content-Type": "application/x-www-form-urlencoded" },
    body: "grant_type=client_credentials",
  });
  const result = await response.json().catch(() => ({}));
  if (!response.ok || !result.access_token) throw new Error("PayPal authentication failed. Check the server-side Live API credentials.");
  return String(result.access_token);
}
async function paypal(path: string, method: string, token: string, body?: unknown) {
  const response = await fetch(API + path, {
    signal: AbortSignal.timeout(30000),
    method,
    headers: {
      Authorization: "Bearer " + token,
      "Content-Type": "application/json",
      Accept: "application/json",
      Prefer: "return=representation",
      "PayPal-Request-Id": crypto.randomUUID(),
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  const data = await response.json().catch(() => ({}));
  if (!response.ok) {
    const issueSummary = Array.isArray(data.details)
      ? data.details.map((item: any) => ({ issue: String(item.issue || ""), description: String(item.description || "").slice(0, 240) }))
      : [];
    console.error("PayPal API error", method, path, response.status, JSON.stringify({
      name: String(data.name || "unknown"),
      message: String(data.message || "").slice(0, 400),
      debug_id: String(data.debug_id || ""),
      details: issueSummary,
    }));
    throw new Error("PayPal rejected the request (" + response.status + "). " +
      (issueSummary[0]?.issue || String(data.name || "Check PayPal account eligibility and subscription permissions.")));
  }
  return data;
}
async function billingEnabled() {
  const { data, error } = await db.from("admin_feature_flags").select("enabled").eq("key", "billing.enabled").maybeSingle();
  if (error) throw error;
  return data?.enabled === true;
}
async function hasEmailMfa(userId: string) {
  const { data, error } = await db.from("admin_email_mfa").select("verified_until").eq("user_id", userId).maybeSingle();
  if (error) throw error;
  return Boolean(data?.verified_until && Date.parse(String(data.verified_until)) > Date.now());
}
async function createOrRefreshPlan(body: Record<string, unknown>, userId: string) {
  const planKey = String(body.planKey || "").trim().toLowerCase();
  const name = String(body.displayName || "").trim();
  const amount = Number(body.amount);
  const currency = String(body.currency || "PHP").toUpperCase();
  const intervalUnit = String(body.intervalUnit || "MONTH").toUpperCase();
  const intervalCount = Number(body.intervalCount || 1);
  const enabled = body.enabled === true;
  const isTest = false;
  if (!/^[a-z0-9][a-z0-9_-]{1,63}$/.test(planKey) || name.length < 2 || name.length > 80 ||
      !Number.isFinite(amount) || amount < 1 || amount > 100000 ||
      !/^[A-Z]{3}$/.test(currency) || !["DAY", "WEEK", "MONTH", "YEAR"].includes(intervalUnit) ||
      !Number.isInteger(intervalCount) || intervalCount < 1 || intervalCount > 12) {
    throw new Error("Invalid plan settings. Amount must be at least 1.00 and the billing interval must be valid.");
  }
  const token = await accessToken();
  const { data: current, error: lookupError } = await db.from("billing_plan_config").select("paypal_product_id,paypal_plan_id").eq("plan_key", planKey).maybeSingle();
  if (lookupError) throw lookupError;
  let productId = current?.paypal_product_id || "";
  if (!productId) {
    const product = await paypal("/v1/catalogs/products", "POST", token, {
      name: "CenAI " + name,
      description: "CenAI subscription: " + name,
      type: "SERVICE",
      category: "SOFTWARE",
    });
    productId = String(product.id || "");
    if (!productId) throw new Error("PayPal did not return a product ID.");
  }
  const plan = await paypal("/v1/billing/plans", "POST", token, {
    product_id: productId,
    name: "CenAI " + name + " · " + amount.toFixed(2) + " " + currency,
    description: "CenAI " + name + " recurring subscription",
    status: "CREATED",
    billing_cycles: [{
      frequency: { interval_unit: intervalUnit, interval_count: intervalCount },
      tenure_type: "REGULAR",
      sequence: 1,
      total_cycles: 0,
      pricing_scheme: { fixed_price: { value: amount.toFixed(2), currency_code: currency } },
    }],
    payment_preferences: {
      auto_bill_outstanding: true,
      setup_fee_failure_action: "CONTINUE",
      payment_failure_threshold: 3,
    },
  });
  const paypalPlanId = String(plan.id || "");
  if (!paypalPlanId) throw new Error("PayPal did not return a billing plan ID.");
  await paypal("/v1/billing/plans/" + encodeURIComponent(paypalPlanId) + "/activate", "POST", token, {});
  const { data, error } = await db.from("billing_plan_config").upsert({
    plan_key: planKey, display_name: name, amount, currency, interval_unit: intervalUnit,
    interval_count: intervalCount, enabled, is_test: isTest, paypal_product_id: productId,
    paypal_plan_id: paypalPlanId, updated_by: userId, updated_at: new Date().toISOString(),
  }, { onConflict: "plan_key" }).select("plan_key,display_name,amount,currency,interval_unit,interval_count,enabled,is_test,paypal_plan_id").single();
  if (error) throw error;
  // PayPal plans are immutable once created. When an administrator replaces a
  // plan, retire the old plan only after the new plan is stored successfully.
  if (current?.paypal_plan_id && current.paypal_plan_id !== paypalPlanId) {
    try {
      await paypal("/v1/billing/plans/" + encodeURIComponent(String(current.paypal_plan_id)) + "/deactivate", "POST", token, {});
    } catch (retireError) {
      console.error("Could not deactivate the replaced PayPal plan", String(current.paypal_plan_id));
    }
  }
  return data;
}
async function verifyWebhook(req: Request, event: unknown) {
  const { webhookId } = paypalConfig();
  if (!webhookId) throw new Error("PayPal webhook ID is not configured.");
  const token = await accessToken();
  const header = (name: string) => req.headers.get(name) || "";
  const verification = await paypal("/v1/notifications/verify-webhook-signature", "POST", token, {
    auth_algo: header("paypal-auth-algo"),
    cert_url: header("paypal-cert-url"),
    transmission_id: header("paypal-transmission-id"),
    transmission_sig: header("paypal-transmission-sig"),
    transmission_time: header("paypal-transmission-time"),
    webhook_id: webhookId,
    webhook_event: event,
  });
  return verification.verification_status === "SUCCESS";
}
async function handleWebhook(event: Record<string, any>) {
  const resource = event.resource || {};
  const eventType = String(event.event_type || "");
  let subscriptionId = String(resource.id || resource.billing_agreement_id || "");
  if (eventType.startsWith("PAYMENT.SALE.") && resource.billing_agreement_id) subscriptionId = String(resource.billing_agreement_id);
  if (!subscriptionId) return;
  const { data: existing, error: lookupError } = await db.from("billing_subscriptions")
    .select("user_id,plan_key").eq("provider", "paypal")
    .eq("provider_subscription_id", subscriptionId).maybeSingle();
  if (lookupError) throw lookupError;
  const { data: attempt } = await db.from("billing_checkout_attempts").select("user_id,plan_key").eq("provider_subscription_id", subscriptionId).maybeSingle();
  let userId = String(resource.custom_id || resource.custom || existing?.user_id || attempt?.user_id || "");
  if (!userId && resource.supplementary_data?.related_ids?.order_id) {
    const { data: row } = await db.from("billing_subscriptions").select("user_id")
      .eq("provider_subscription_id", String(resource.supplementary_data.related_ids.order_id)).maybeSingle();
    userId = row?.user_id || "";
  }
  if (!userId) return;
  const statusMap: Record<string, string> = {
    "BILLING.SUBSCRIPTION.UPDATED": ["CANCELLED", "SUSPENDED", "EXPIRED"].includes(String(resource.status || "")) ? String(resource.status) : "",
    "BILLING.SUBSCRIPTION.CANCELLED": "CANCELLED",
    "BILLING.SUBSCRIPTION.SUSPENDED": "SUSPENDED",
    "BILLING.SUBSCRIPTION.EXPIRED": "EXPIRED",
    "BILLING.SUBSCRIPTION.PAYMENT.FAILED": "PAST_DUE",
    "PAYMENT.SALE.COMPLETED": "ACTIVE",
    "PAYMENT.SALE.REFUNDED": "REFUNDED",
    "PAYMENT.SALE.REVERSED": "REVERSED",
  };
  const status = statusMap[eventType];
  if (!status) return;
  const nextBilling = resource.billing_info?.next_billing_time || resource.billing_info?.next_billing_date;
  let planKey = String(existing?.plan_key || attempt?.plan_key || "live_test");
  const paypalPlanId = String(resource.plan_id || "");
  if (paypalPlanId) {
    const { data: configuredPlan, error: planError } = await db.from("billing_plan_config")
      .select("plan_key").eq("paypal_plan_id", paypalPlanId).maybeSingle();
    if (planError) throw planError;
    if (configuredPlan?.plan_key) planKey = configuredPlan.plan_key;
  }
  const { error } = await db.from("billing_subscriptions").upsert({
    user_id: userId,
    provider: "paypal",
    provider_subscription_id: subscriptionId,
    plan_key: planKey,
    status,
    current_period_end: nextBilling && !Number.isNaN(Date.parse(String(nextBilling))) ? new Date(String(nextBilling)).toISOString() : null,
    cancel_at_period_end: status === "CANCELLED",
    updated_at: new Date().toISOString(),
  }, { onConflict: "provider_subscription_id" });
  if (error) throw error;
  if (attempt) {
    const attemptStatus = status === "ACTIVE" ? "ACTIVE" : status === "CANCELLED" ? "CANCELLED" : status === "EXPIRED" ? "EXPIRED" : status === "SUSPENDED" || status === "PAST_DUE" ? "FAILED" : null;
    if (attemptStatus) await db.from("billing_checkout_attempts").update({ status: attemptStatus, updated_at: new Date().toISOString() }).eq("provider_subscription_id", subscriptionId);
  }
}

async function settleCheckoutAttempt(userId: string, row: Record<string, any>, forceCancel = false) {
  const subscriptionId = String(row.provider_subscription_id || "");
  if (!/^I-[A-Z0-9]+$/i.test(subscriptionId)) return "INVALID";
  const token = await accessToken();
  const detail = await paypal("/v1/billing/subscriptions/" + encodeURIComponent(subscriptionId), "GET", token);
  if (String(detail.custom_id || "") !== userId) throw new Error("PayPal checkout ownership verification failed.");
  const status = String(detail.status || "UNKNOWN");
  const lastPaymentAmount = Number(detail.billing_info?.last_payment?.amount?.value || 0);
  const firstPaymentConfirmed = Number.isFinite(lastPaymentAmount) && lastPaymentAmount > 0 && Boolean(detail.billing_info?.last_payment?.time);
  if (status === "ACTIVE" && firstPaymentConfirmed) {
    const { error } = await db.from("billing_subscriptions").upsert({
      user_id: userId, provider: "paypal", provider_subscription_id: subscriptionId,
      plan_key: String(row.plan_key || "live_test"), status: "ACTIVE",
      current_period_end: detail.billing_info?.next_billing_time ? new Date(detail.billing_info.next_billing_time).toISOString() : null,
      cancel_at_period_end: false, updated_at: new Date().toISOString(),
    }, { onConflict: "provider_subscription_id" });
    if (error) throw error;
    await db.from("billing_checkout_attempts").update({ status: "ACTIVE", updated_at: new Date().toISOString() }).eq("provider_subscription_id", subscriptionId);
    return "ACTIVE";
  }
  if (status === "CANCELLED" || status === "EXPIRED") {
    await db.from("billing_checkout_attempts").update({ status, updated_at: new Date().toISOString() }).eq("provider_subscription_id", subscriptionId);
    return status;
  }
  const expired = row.expires_at && Date.parse(String(row.expires_at)) <= Date.now();
  if ((status === "APPROVAL_PENDING" || status === "APPROVED" || status === "ACTIVE") && (forceCancel || expired)) {
    try {
      await paypal("/v1/billing/subscriptions/" + encodeURIComponent(subscriptionId) + "/cancel", "POST", token, { reason: "CenAI checkout was not completed before its expiry." });
      await db.from("billing_checkout_attempts").update({ status: "CANCELLED", updated_at: new Date().toISOString() }).eq("provider_subscription_id", subscriptionId);
      return "CANCELLED";
    } catch (error) {
      console.error("PayPal refused cancellation of an unapproved checkout", subscriptionId, error instanceof Error ? error.message : "unknown");
      await db.from("billing_checkout_attempts").update({ status: "EXPIRED", updated_at: new Date().toISOString() }).eq("provider_subscription_id", subscriptionId);
      return "EXPIRED";
    }
  }
  return status === "ACTIVE" ? "AWAITING_PAYMENT" : status;
}

function constantTimeEqual(a: string, b: string): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return diff === 0;
}

async function cleanupExpiredAttempts() {
  const now = new Date().toISOString();
  const { data: attempts, error } = await db.from("billing_checkout_attempts")
    .select("user_id,provider_subscription_id,plan_key,expires_at")
    .eq("status", "AWAITING_APPROVAL").lte("expires_at", now).limit(100);
  if (error) throw error;
  let cancelled = 0;
  let expired = 0;
  let stillWaiting = 0;
  for (const attempt of attempts || []) {
    try {
      const result = await settleCheckoutAttempt(String(attempt.user_id), attempt, false);
      if (result === "CANCELLED") cancelled++;
      else if (result === "EXPIRED" || result === "CANCELLED") expired++;
      else stillWaiting++;
    } catch (error) {
      console.error("Scheduled PayPal checkout cleanup failed", String(attempt.provider_subscription_id), error instanceof Error ? error.message : "unknown");
      stillWaiting++;
    }
  }
  return { scanned: (attempts || []).length, cancelled, expired, stillWaiting };
}

Deno.serve(async (req: Request) => {
  if (req.method === "OPTIONS") return new Response(null, { status: 204, headers: cors });
  if (req.method !== "POST") return fail(405, "POST required");
  try {
    const signatureHeader = req.headers.get("paypal-transmission-sig");
    if (signatureHeader) {
      const event = await req.json();
      if (!await verifyWebhook(req, event)) return fail(400, "Invalid PayPal webhook signature");
      const eventId = String(event.id || "");
      if (!eventId) return fail(400, "Missing PayPal webhook event ID");
      const { data: prior, error: priorError } = await db.from("billing_webhook_events")
        .select("event_id").eq("provider", "paypal").eq("event_id", eventId).maybeSingle();
      if (priorError) throw priorError;
      if (prior) return json(200, { received: true, duplicate: true });
      await handleWebhook(event);
      const resource = event.resource || {};
      const { error: insertError } = await db.from("billing_webhook_events").insert({
        provider: "paypal", event_id: eventId,
        event_type: String(event.event_type || "").slice(0,120),
        resource_id: String(resource.id || resource.billing_agreement_id || "").slice(0,160),
        processing_result: "signature_verified_and_handler_completed"
      });
      if (insertError && insertError.code !== "23505") throw insertError;
      return json(200, { received: true });
    }

    const cronSecret = req.headers.get("x-cenai-checkout-cron") || "";
    if (cronSecret) {
      const { data: storedSecret, error: secretError } = await db.from("cenai_internal_secrets")
        .select("secret_value").eq("name", "paypal_checkout_cleanup").maybeSingle();
      if (secretError) throw secretError;
      if (!storedSecret?.secret_value || !constantTimeEqual(cronSecret, String(storedSecret.secret_value))) return fail(401, "Invalid scheduled cleanup credential");
      return json(200, await cleanupExpiredAttempts());
    }

    const user = await currentUser(req);
    if (!user) return fail(401, "Sign in required");
    const body = await req.json().catch(() => ({}));
    const action = String(body.action || "create_subscription");

    if (action === "configure_plan") {
      if (!await isAdmin(user.id)) return fail(403, "Owner or administrator access required");
      if (tokenAal(req) !== "aal2" && !await hasEmailMfa(user.id)) return fail(403, "MFA_REQUIRED: Verify the email code sent to your administrator address.");
      const plan = await createOrRefreshPlan(body, user.id);
      return json(200, { plan, message: "PayPal Live plan created and activated. New subscriptions can now use this plan." });
    }
    if (["list_public_plans", "get_plans", "billing_plans", "billing"].includes(action)) {
      const { data, error } = await db.from("billing_plan_config")
        .select("plan_key,display_name,amount,currency,interval_unit,interval_count,enabled,is_test,paypal_plan_id")
        .eq("enabled", true).order("created_at");
      if (error) throw error;
      return json(200, { plans: data || [], billingEnabled: await billingEnabled() });
    }
    if (action === "list_plans") {
      if (!await isAdmin(user.id)) return fail(403, "Owner or administrator access required");
      if (tokenAal(req) !== "aal2" && !await hasEmailMfa(user.id)) return fail(403, "MFA_REQUIRED: Verify the email code sent to your administrator address.");
      const { data, error } = await db.from("billing_plan_config")
        .select("plan_key,display_name,amount,currency,interval_unit,interval_count,enabled,is_test,paypal_plan_id,updated_at")
        .order("created_at");
      if (error) throw error;
      return json(200, { plans: data || [] });
    }
    if (action === "diagnose_recent_checkouts") {
      const { data: attempts, error: attemptsError } = await db.from("billing_checkout_attempts")
        .select("provider_subscription_id,plan_key,status,expires_at,created_at")
        .eq("user_id", user.id).order("created_at", { ascending: false }).limit(5);
      if (attemptsError) throw attemptsError;
      const token = await accessToken();
      const diagnostics: Record<string, unknown>[] = [];
      for (const attempt of attempts || []) {
        const subscriptionId = String(attempt.provider_subscription_id || "");
        try {
          const detail = await paypal("/v1/billing/subscriptions/" + encodeURIComponent(subscriptionId), "GET", token);
          const planId = String(detail.plan_id || "");
          let planStatus = "UNKNOWN";
          let planAmount = "";
          let planCurrency = "";
          if (planId) {
            const plan = await paypal("/v1/billing/plans/" + encodeURIComponent(planId), "GET", token);
            planStatus = String(plan.status || "UNKNOWN");
            const fixedPrice = plan.billing_cycles?.[0]?.pricing_scheme?.fixed_price;
            planAmount = String(fixedPrice?.value || "");
            planCurrency = String(fixedPrice?.currency_code || "");
          }
          const lastPayment = detail.billing_info?.last_payment || {};
          diagnostics.push({
            subscriptionId,
            recordedStatus: String(attempt.status || ""),
            paypalStatus: String(detail.status || "UNKNOWN"),
            statusUpdateTime: String(detail.status_update_time || ""),
            statusChangeNote: String(detail.status_change_note || "").slice(0, 300),
            planId,
            planStatus,
            planAmount,
            planCurrency,
            failedPaymentsCount: Number(detail.billing_info?.failed_payments_count || 0),
            lastFailedPaymentReasonCode: String(detail.billing_info?.last_failed_payment?.reason_code || ""),
            lastFailedPaymentTime: String(detail.billing_info?.last_failed_payment?.time || ""),
            nextPaymentRetryTime: String(detail.billing_info?.next_payment_retry_time || ""),
            lastPaymentTime: String(lastPayment.time || ""),
            lastPaymentAmount: String(lastPayment.amount?.value || ""),
            lastPaymentCurrency: String(lastPayment.amount?.currency_code || ""),
          });
        } catch (diagnosticError) {
          diagnostics.push({
            subscriptionId,
            recordedStatus: String(attempt.status || ""),
            error: diagnosticError instanceof Error ? diagnosticError.message : "PayPal could not retrieve this subscription.",
          });
        }
      }
      return json(200, { diagnostics, checkedAt: new Date().toISOString() });
    }

    if (action === "create_subscription") {
      if (!await billingEnabled()) return fail(503, "Billing is disabled until the PayPal Live integration has passed verification.");
      const planKey = String(body.planKey || "live_test");
      const { data: plan, error } = await db.from("billing_plan_config")
        .select("plan_key,display_name,amount,currency,interval_unit,interval_count,paypal_plan_id,is_test")
        .eq("plan_key", planKey).eq("enabled", true).maybeSingle();
      if (error) throw error;
      if (!plan?.paypal_plan_id) return fail(400, "This subscription plan is not configured yet.");
      
      const token = await accessToken();
      const subscription = await paypal("/v1/billing/subscriptions", "POST", token, {
        plan_id: plan.paypal_plan_id,
        custom_id: user.id,
        application_context: {
          brand_name: "CenAI",
          locale: "en-PH",
          user_action: "SUBSCRIBE_NOW",
          // Do not pin the payer to the PayPal wallet. UNRESTRICTED lets
          // PayPal present every funding source enabled for this merchant and buyer,
          // including eligible guest card/bank funding.
          payment_method: { payee_preferred: "UNRESTRICTED" },
          return_url: baseUrl() + "/billing/success",
          cancel_url: baseUrl() + "/billing/cancel",
        },
      });
      const subscriptionId = String(subscription.id || "");
      const approvalUrl = (subscription.links || []).find((link: any) => link.rel === "approve")?.href;
      if (!subscriptionId) return fail(502, "PayPal did not return a subscription ID.");
      let safeApprovalUrl = false;
      try {
        const parsedApproval = new URL(String(approvalUrl || ""));
        safeApprovalUrl = parsedApproval.protocol === "https:" &&
          (parsedApproval.hostname === "paypal.com" || parsedApproval.hostname.endsWith(".paypal.com"));
      } catch {}
      if (!safeApprovalUrl) {
        try {
          await paypal("/v1/billing/subscriptions/" + encodeURIComponent(subscriptionId) + "/cancel", "POST", token, { reason: "PayPal did not return a valid CenAI approval page." });
        } catch (cancelError) {
          console.error("Could not cancel PayPal checkout with invalid approval URL", subscriptionId, cancelError instanceof Error ? cancelError.message : "unknown");
        }
        return fail(502, "PayPal did not return a safe approval page. The unfinished checkout was sent for cancellation.");
      }
      const { error: saveError } = await db.from("billing_checkout_attempts").insert({
        user_id: user.id, provider_subscription_id: subscriptionId, plan_key: plan.plan_key,
        status: "AWAITING_APPROVAL", expires_at: new Date(Date.now() + 20 * 60 * 1000).toISOString(),
      });
      if (saveError) {
        try { await paypal("/v1/billing/subscriptions/" + encodeURIComponent(subscriptionId) + "/cancel", "POST", token, { reason: "CenAI could not record the checkout attempt." }); } catch {}
        throw saveError;
      }
      return json(200, { approvalUrl, subscriptionId, plan: { name: plan.display_name, amount: plan.amount, currency: plan.currency, intervalUnit: plan.interval_unit, intervalCount: plan.interval_count } });
    }
    if (action === "cancel_open_checkouts" || action === "status") {
      const { data: attempts, error: attemptsError } = await db.from("billing_checkout_attempts")
        .select("provider_subscription_id,plan_key,expires_at").eq("user_id", user.id).eq("status", "AWAITING_APPROVAL");
      if (attemptsError) throw attemptsError;
      for (const attempt of attempts || []) {
        try { await settleCheckoutAttempt(user.id, attempt, action === "cancel_open_checkouts"); }
        catch (error) { console.error("CenAI checkout cleanup failed", String(attempt.provider_subscription_id), error instanceof Error ? error.message : "unknown"); }
      }
      const { data: legacy, error: legacyError } = await db.from("billing_subscriptions")
        .select("provider_subscription_id,plan_key,updated_at").eq("user_id", user.id).eq("provider", "paypal").eq("status", "APPROVAL_PENDING");
      if (legacyError) throw legacyError;
      for (const row of legacy || []) {
        const expired = Date.parse(String(row.updated_at || "")) < Date.now() - 20 * 60 * 1000;
        if (action === "cancel_open_checkouts" || expired) {
          try {
            const token = await accessToken();
            const detail = await paypal("/v1/billing/subscriptions/" + encodeURIComponent(String(row.provider_subscription_id)), "GET", token);
            const paid = Number(detail.billing_info?.last_payment?.amount?.value || 0) > 0 && Boolean(detail.billing_info?.last_payment?.time);
            if (String(detail.custom_id || "") === user.id && String(detail.status) === "ACTIVE" && paid) {
              await db.from("billing_subscriptions").update({ status: "ACTIVE", current_period_end: detail.billing_info?.next_billing_time || null, cancel_at_period_end: false, updated_at: new Date().toISOString() }).eq("provider_subscription_id", row.provider_subscription_id).eq("user_id", user.id);
            } else {
              if (String(detail.custom_id || "") === user.id && ["APPROVAL_PENDING", "APPROVED", "ACTIVE"].includes(String(detail.status))) {
                try {
                  await paypal("/v1/billing/subscriptions/" + encodeURIComponent(String(row.provider_subscription_id)) + "/cancel", "POST", token, { reason: "CenAI checkout expired without a confirmed first payment." });
                } catch (cancelError) {
                  console.error("PayPal could not cancel unpaid legacy checkout", String(row.provider_subscription_id), cancelError instanceof Error ? cancelError.message : "unknown");
                }
              }
              await db.from("billing_subscriptions").update({ status: "EXPIRED", cancel_at_period_end: false, updated_at: new Date().toISOString() }).eq("provider_subscription_id", row.provider_subscription_id).eq("user_id", user.id);
            }
          } catch (error) {
            console.error("Legacy pending PayPal cleanup failed", String(row.provider_subscription_id), error instanceof Error ? error.message : "unknown");
            await db.from("billing_subscriptions").update({ status: "EXPIRED", cancel_at_period_end: false, updated_at: new Date().toISOString() }).eq("provider_subscription_id", row.provider_subscription_id).eq("user_id", user.id);
          }
        }
      }
      if (action === "cancel_open_checkouts") return json(200, { cancelled: (attempts || []).length + (legacy || []).length, message: "Unfinished PayPal checkouts were cancelled where PayPal allowed cancellation." });
      const { data, error } = await db.from("billing_subscriptions")
        .select("provider,provider_subscription_id,plan_key,status,current_period_end,cancel_at_period_end,updated_at")
        .eq("user_id", user.id).eq("provider", "paypal").neq("status", "APPROVAL_PENDING").neq("status", "EXPIRED").order("updated_at", { ascending: false }).limit(5);
      if (error) throw error;
      return json(200, { subscriptions: data || [] });
    }
    if (action === "cancel") {
      const subscriptionId = String(body.subscriptionId || "");
      if (!/^I-[A-Z0-9]+$/i.test(subscriptionId)) return fail(400, "Invalid subscription ID");
      const { data: owned, error } = await db.from("billing_subscriptions").select("provider_subscription_id")
        .eq("user_id", user.id).eq("provider", "paypal").eq("provider_subscription_id", subscriptionId).maybeSingle();
      if (error) throw error;
      if (!owned) return fail(404, "Subscription not found");
      const token = await accessToken();
      await paypal("/v1/billing/subscriptions/" + encodeURIComponent(subscriptionId) + "/cancel", "POST", token, { reason: "Cancelled by CenAI account holder" });
      const { error: updateError } = await db.from("billing_subscriptions")
        .update({ status: "CANCELLED", cancel_at_period_end: false, updated_at: new Date().toISOString() })
        .eq("provider", "paypal").eq("provider_subscription_id", subscriptionId).eq("user_id", user.id);
      if (updateError) throw updateError;
      return json(200, { cancelled: true, subscriptionId });
    }
    return fail(400, "Unknown billing action");
  } catch (error) {
    console.error("CenAI PayPal billing request failed", error instanceof Error ? error.message : "unknown error");
    return fail(500, error instanceof Error ? error.message : "Billing request failed");
  }
});
