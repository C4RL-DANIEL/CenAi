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
const PAYPAL_SUBSCRIPTION_WEBHOOK_EVENTS = [
  "BILLING.SUBSCRIPTION.CREATED",
  "BILLING.SUBSCRIPTION.ACTIVATED",
  "BILLING.SUBSCRIPTION.UPDATED",
  "BILLING.SUBSCRIPTION.EXPIRED",
  "BILLING.SUBSCRIPTION.CANCELLED",
  "BILLING.SUBSCRIPTION.SUSPENDED",
  "BILLING.SUBSCRIPTION.PAYMENT.FAILED",
  "PAYMENT.SALE.COMPLETED",
  "PAYMENT.SALE.REFUNDED",
  "PAYMENT.SALE.REVERSED",
  "PAYMENT.CAPTURE.COMPLETED",
  "PAYMENT.CAPTURE.REFUNDED",
  "PAYMENT.CAPTURE.REVERSED",
  "PAYMENT.CAPTURE.DENIED",
] as const;

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
    const issue = issueSummary[0]?.issue || String(data.name || "UNKNOWN_PAYPAL_ERROR");
    const description = issueSummary[0]?.description || String(data.message || "No description returned.");
    const debugId = String(data.debug_id || "");
    throw new Error("PayPal " + method + " " + path + " failed (HTTP " + response.status + "): " + issue + " — " + description.slice(0, 220) + (debugId ? " [debug " + debugId + "]" : ""));
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
async function ensurePayPalPlanCurrency(plan: Record<string, any>, userId: string) {
  const expectedCurrency = String(plan.currency || "PHP").toUpperCase();
  const expectedAmount = Number(plan.amount);
  if (plan.plan_key === "live_test" && (expectedCurrency !== "PHP" || expectedAmount !== 1)) {
    throw new Error("CenAI Starter is locked to PHP 1.00. Checkout was stopped to prevent an incorrect charge.");
  }
  const inspect = async (planId: string) => {
    const token = await accessToken();
    const detail = await paypal("/v1/billing/plans/" + encodeURIComponent(planId), "GET", token);
    const cycles = Array.isArray(detail.billing_cycles) ? detail.billing_cycles : [];
    return cycles.some((cycle: any) => {
      const price = cycle?.pricing_scheme?.fixed_price || {};
      return String(cycle?.tenure_type || "").toUpperCase() === "REGULAR" &&
        String(price.currency_code || "").toUpperCase() === expectedCurrency &&
        Number(price.value) === expectedAmount;
    });
  };
  if (await inspect(String(plan.paypal_plan_id || ""))) return plan;
  console.warn("PayPal plan currency/amount differs from CenAI configuration; rebuilding before checkout", {
    planKey: String(plan.plan_key), expectedCurrency, expectedAmount,
  });
  const refreshed = await createOrRefreshPlan({
    planKey: String(plan.plan_key),
    displayName: String(plan.display_name || "CenAI membership"),
    amount: expectedAmount,
    currency: expectedCurrency,
    intervalUnit: String(plan.interval_unit || "MONTH"),
    intervalCount: Number(plan.interval_count || 1),
    enabled: true,
  }, userId);
  if (!refreshed?.paypal_plan_id || !await inspect(String(refreshed.paypal_plan_id))) {
    throw new Error("PayPal's subscription plan does not match the configured price and currency. Checkout was stopped; no charge was requested.");
  }
  return { ...plan, paypal_plan_id: refreshed.paypal_plan_id };
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
  if (eventType.startsWith("PAYMENT.CAPTURE.")) {
    const orderId = String(resource.supplementary_data?.related_ids?.order_id || resource.related_ids?.order_id || "");
    if (!orderId) return;
    const { data: payment, error: paymentError } = await db.from("billing_one_time_payments")
      .select("user_id,provider_order_id,plan_key,amount,currency,status,paid_at,access_expires_at")
      .eq("provider_order_id", orderId).maybeSingle();
    if (paymentError) throw paymentError;
    if (!payment) return;
    const captureAmount = resource.amount || {};
    if (captureAmount.value && (Number(captureAmount.value) !== Number(payment.amount) ||
        String(captureAmount.currency_code || "").toUpperCase() !== String(payment.currency).toUpperCase())) {
      throw new Error("PayPal capture webhook amount does not match the CenAI order.");
    }
    if (eventType === "PAYMENT.CAPTURE.COMPLETED") {
      if (["REFUNDED", "REVERSED", "EXPIRED"].includes(String(payment.status))) return;
      const paidAt = resource.create_time && !Number.isNaN(Date.parse(String(resource.create_time)))
        ? new Date(String(resource.create_time)).toISOString() : new Date().toISOString();
      const expiresAt = payment.access_expires_at || new Date(Date.parse(paidAt) + 30 * 24 * 60 * 60 * 1000).toISOString();
      const { error } = await db.from("billing_one_time_payments").update({
        status: "COMPLETED", paid_at: payment.paid_at || paidAt, access_expires_at: expiresAt,
        provider_capture_id: String(resource.id || ""), updated_at: new Date().toISOString(),
      }).eq("provider_order_id", orderId);
      if (error) throw error;
      const { error: entitlementError } = await db.from("billing_subscriptions").upsert({
        user_id: payment.user_id, provider: "paypal", provider_subscription_id: "ONE_TIME:" + orderId,
        plan_key: payment.plan_key, status: "ACTIVE", current_period_end: expiresAt,
        cancel_at_period_end: false, updated_at: new Date().toISOString(),
      }, { onConflict: "provider_subscription_id" });
      if (entitlementError) throw entitlementError;
    } else if (eventType === "PAYMENT.CAPTURE.REFUNDED" || eventType === "PAYMENT.CAPTURE.REVERSED") {
      const nextStatus = eventType.endsWith("REFUNDED") ? "REFUNDED" : "REVERSED";
      const { error } = await db.from("billing_one_time_payments").update({ status: nextStatus, updated_at: new Date().toISOString() })
        .eq("provider_order_id", orderId);
      if (error) throw error;
      const { error: entitlementError } = await db.from("billing_subscriptions").update({
        status: nextStatus, cancel_at_period_end: false, updated_at: new Date().toISOString(),
      }).eq("provider", "paypal").eq("provider_subscription_id", "ONE_TIME:" + orderId).eq("user_id", payment.user_id);
      if (entitlementError) throw entitlementError;
    } else if (eventType === "PAYMENT.CAPTURE.DENIED") {
      const { error } = await db.from("billing_one_time_payments").update({ status: "FAILED", updated_at: new Date().toISOString() })
        .eq("provider_order_id", orderId).eq("status", "CREATED");
      if (error) throw error;
    }
    return;
  }
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
    // Activation means the buyer approved the recurring agreement. Keep it
    // separate from ACTIVE entitlement until PayPal confirms the first sale.
    "BILLING.SUBSCRIPTION.ACTIVATED": "APPROVED",
    "BILLING.SUBSCRIPTION.UPDATED": (() => {
      const current = String(resource.status || "");
      if (current === "ACTIVE") return "APPROVED";
      return ["CANCELLED", "SUSPENDED", "EXPIRED"].includes(current) ? current : "";
    })(),
    "BILLING.SUBSCRIPTION.CANCELLED": "CANCELLED",
    "BILLING.SUBSCRIPTION.SUSPENDED": "SUSPENDED",
    "BILLING.SUBSCRIPTION.EXPIRED": "EXPIRED",
    "BILLING.SUBSCRIPTION.PAYMENT.FAILED": "PAST_DUE",
    "PAYMENT.SALE.PENDING": "PENDING_PAYMENT",
    "PAYMENT.SALE.COMPLETED": "ACTIVE",
    "PAYMENT.SALE.DENIED": "PAST_DUE",
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
    const attemptStatus = status === "ACTIVE" ? "ACTIVE" : status === "APPROVED" || status === "PENDING_PAYMENT" ? "APPROVED" : status === "CANCELLED" ? "CANCELLED" : status === "EXPIRED" ? "EXPIRED" : status === "SUSPENDED" || status === "PAST_DUE" ? "FAILED" : null;
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
  if (status === "APPROVED") {
    await db.from("billing_checkout_attempts").update({ status: "APPROVED", updated_at: new Date().toISOString() }).eq("provider_subscription_id", subscriptionId);
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

async function repairWebhookConfiguration() {
  const { webhookId } = paypalConfig();
  if (!webhookId) return { repaired: false, reason: "PAYPAL_WEBHOOK_ID is not configured." };
  const token = await accessToken();
  const expectedUrl = "https://bqkzehozezvepbthglyj.supabase.co/functions/v1/cenai-paypal-billing";
  const detail = await paypal("/v1/notifications/webhooks/" + encodeURIComponent(webhookId), "GET", token);
  if (String(detail.url || "") !== expectedUrl) return { repaired: false, reason: "Configured PayPal webhook URL does not match the CenAI billing endpoint. No changes were made." };
  const required = PAYPAL_SUBSCRIPTION_WEBHOOK_EVENTS;
  const current = Array.isArray(detail.event_types) ? detail.event_types : [];
  const names = new Set(current.map((item: any) => String(item.name || "")));
  const merged = [...current];
  for (const name of required) if (!names.has(name)) merged.push({ name });
  const addedEvents = required.filter(name => !names.has(name));
  if (addedEvents.length) {
    await paypal("/v1/notifications/webhooks/" + encodeURIComponent(webhookId), "PATCH", token, [
      { op: "replace", path: "/event_types", value: merged }
    ]);
  }
  const verified = await paypal("/v1/notifications/webhooks/" + encodeURIComponent(webhookId), "GET", token);
  const verifiedNames = (verified.event_types || []).map((item: any) => String(item.name || ""));
  const missing = required.filter(name => !verifiedNames.includes(name));
  return {
    repaired: missing.length === 0,
    webhookUrl: String(verified.url || ""),
    addedEvents,
    requiredEventsMissing: missing,
    totalSubscribedEvents: verifiedNames.length
  };
}

async function cleanupExpiredMfaFactors() {
  const headers = {
    apikey: SUPABASE_SERVICE_ROLE_KEY,
    Authorization: "Bearer " + SUPABASE_SERVICE_ROLE_KEY,
    Accept: "application/json"
  };
  const cutoff = Date.now() - 10 * 60 * 1000;
  let scannedUsers = 0;
  let removed = 0;
  let failed = 0;
  for (let page = 1; page <= 5; page++) {
    const usersResponse = await fetch(SUPABASE_URL.replace(/\/$/, "") + "/auth/v1/admin/users?page=" + page + "&per_page=100", {
      headers, signal: AbortSignal.timeout(15000)
    });
    const usersPayload = await usersResponse.json().catch(() => ({}));
    if (!usersResponse.ok) throw new Error("Auth admin user listing failed (HTTP " + usersResponse.status + ").");
    const users = Array.isArray(usersPayload.users) ? usersPayload.users : [];
    scannedUsers += users.length;
    for (const user of users) {
      if (!user?.id) continue;
      try {
        const factorsResponse = await fetch(SUPABASE_URL.replace(/\/$/, "") + "/auth/v1/admin/users/" + encodeURIComponent(String(user.id)) + "/factors", {
          headers, signal: AbortSignal.timeout(10000)
        });
        const factorsPayload = await factorsResponse.json().catch(() => []);
        if (!factorsResponse.ok) { failed++; continue; }
        const factors = Array.isArray(factorsPayload) ? factorsPayload : Array.isArray(factorsPayload.factors) ? factorsPayload.factors : [];
        for (const factor of factors) {
          if (factor.factor_type !== "totp" || factor.status === "verified" || factor.friendly_name !== "CenAI account authenticator") continue;
          const created = Date.parse(String(factor.created_at || ""));
          if (!Number.isFinite(created) || created > cutoff || !factor.id) continue;
          const deleted = await fetch(SUPABASE_URL.replace(/\/$/, "") + "/auth/v1/admin/users/" + encodeURIComponent(String(user.id)) + "/factors/" + encodeURIComponent(String(factor.id)), {
            method: "DELETE", headers, signal: AbortSignal.timeout(10000)
          });
          if (deleted.ok) removed++;
          else failed++;
        }
      } catch {
        failed++;
      }
    }
    if (users.length < 100) break;
  }
  return { scannedUsers, removedPendingFactors: removed, failedOperations: failed, expiryMinutes: 10 };
}

async function cleanupExpiredAttempts() {
  const now = new Date().toISOString();
  const { data: attempts, error } = await db.from("billing_checkout_attempts")
    .select("user_id,provider_subscription_id,plan_key,expires_at")
    .in("status", ["AWAITING_APPROVAL", "APPROVED", "EXPIRED"]).lte("expires_at", now).limit(100);
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
  const { data: oneTime, error: oneTimeError } = await db.from("billing_one_time_payments")
    .select("provider_order_id,user_id").eq("status", "COMPLETED").lte("access_expires_at", now).limit(100);
  if (oneTimeError) throw oneTimeError;
  const staleBefore = new Date(Date.now() - 30 * 60 * 1000).toISOString();
  const { data: staleOrders, error: staleError } = await db.from("billing_one_time_payments")
    .select("provider_order_id").eq("status", "CREATED").lte("created_at", staleBefore).limit(100);
  if (staleError) throw staleError;
  if (staleOrders?.length) {
    const ids = staleOrders.map((row: Record<string, unknown>) => String(row.provider_order_id));
    const { error: expireOrdersError } = await db.from("billing_one_time_payments")
      .update({ status: "EXPIRED", updated_at: now }).in("provider_order_id", ids).eq("status", "CREATED");
    if (expireOrdersError) throw expireOrdersError;
  }
  let expiredOneTime = 0;
  for (const payment of oneTime || []) {
    const { error: paymentError } = await db.from("billing_one_time_payments")
      .update({ status: "EXPIRED", updated_at: now })
      .eq("provider_order_id", payment.provider_order_id).eq("status", "COMPLETED");
    if (paymentError) throw paymentError;
    const { error: entitlementError } = await db.from("billing_subscriptions")
      .update({ status: "EXPIRED", cancel_at_period_end: false, updated_at: now })
      .eq("provider", "paypal").eq("provider_subscription_id", "ONE_TIME:" + payment.provider_order_id)
      .eq("user_id", payment.user_id);
    if (entitlementError) throw entitlementError;
    expiredOneTime++;
  }
  return { scanned: (attempts || []).length, cancelled, expired, stillWaiting, expiredOneTime };
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
      const cleanup = await cleanupExpiredAttempts();
      let mfaCleanup: Record<string, unknown>;
      try { mfaCleanup = await cleanupExpiredMfaFactors(); }
      catch (mfaError) {
        mfaCleanup = { error: mfaError instanceof Error ? mfaError.message : "Automatic MFA cleanup failed." };
        console.error("Scheduled CenAI MFA cleanup failed", mfaError instanceof Error ? mfaError.message : "unknown error");
      }
      let webhook: Record<string, unknown>;
      try { webhook = await repairWebhookConfiguration(); }
      catch (webhookError) {
        webhook = { repaired: false, error: webhookError instanceof Error ? webhookError.message : "Automatic PayPal webhook repair failed." };
        console.error("Scheduled PayPal webhook repair failed", webhookError instanceof Error ? webhookError.message : "unknown error");
      }
      return json(200, { ...cleanup, mfaCleanup, webhook });
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
      return json(200, { plans: data || [], billingEnabled: await billingEnabled(), paypalClientId: paypalConfig().clientId });
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
    if (action === "repair_paypal_webhook") {
      if (!await isAdmin(user.id)) return fail(403, "Owner or administrator access required");
      if (tokenAal(req) !== "aal2" && !await hasEmailMfa(user.id)) return fail(403, "MFA_REQUIRED: Verify the email code sent to your administrator address.");
      const { webhookId } = paypalConfig();
      if (!webhookId) return fail(400, "PAYPAL_WEBHOOK_ID is not configured.");
      const token = await accessToken();
      const detail = await paypal("/v1/notifications/webhooks/" + encodeURIComponent(webhookId), "GET", token);
      const expectedUrl = "https://bqkzehozezvepbthglyj.supabase.co/functions/v1/cenai-paypal-billing";
      if (String(detail.url || "") !== expectedUrl) {
        return fail(409, "Configured PayPal webhook URL does not match the CenAI billing endpoint. No webhook changes were made.");
      }
      const required = PAYPAL_SUBSCRIPTION_WEBHOOK_EVENTS;
      const current = Array.isArray(detail.event_types) ? detail.event_types : [];
      const names = new Set(current.map((item: any) => String(item.name || "")));
      const merged = [...current];
      for (const name of required) if (!names.has(name)) merged.push({ name });
      if (merged.length !== current.length) {
        await paypal("/v1/notifications/webhooks/" + encodeURIComponent(webhookId), "PATCH", token, [
          { op: "replace", path: "/event_types", value: merged },
        ]);
      }
      const verified = await paypal("/v1/notifications/webhooks/" + encodeURIComponent(webhookId), "GET", token);
      const verifiedNames = (verified.event_types || []).map((item: any) => String(item.name || ""));
      const missing = required.filter(name => !verifiedNames.includes(name));
      if (missing.length) throw new Error("PayPal webhook update completed but verification still reports missing events: " + missing.join(", "));
      return json(200, {
        repaired: true,
        webhookUrl: String(verified.url || ""),
        addedEvents: required.filter(name => !names.has(name)),
        requiredEventsMissing: [],
        totalSubscribedEvents: verifiedNames.length,
      });
    }
    if (action === "diagnose_recent_checkouts") {
      const { data: attempts, error: attemptsError } = await db.from("billing_checkout_attempts")
        .select("provider_subscription_id,plan_key,status,expires_at,created_at")
        .eq("user_id", user.id).order("created_at", { ascending: false }).limit(5);
      if (attemptsError) throw attemptsError;
      const token = await accessToken();
      const webhookId = Deno.env.get("PAYPAL_WEBHOOK_ID") || "";
      let webhook: Record<string, unknown> = { configuredIdPresent: Boolean(webhookId), registered: false, requiredEventsMissing: [...PAYPAL_SUBSCRIPTION_WEBHOOK_EVENTS] };
      if (webhookId) {
        try {
          const webhookList = await paypal("/v1/notifications/webhooks", "GET", token);
          const registered = (webhookList.webhooks || []).find((item: any) => String(item.id || "") === webhookId);
          const eventNames = (registered?.event_types || []).map((item: any) => String(item.name || ""));
          const required = PAYPAL_SUBSCRIPTION_WEBHOOK_EVENTS;
          webhook = { configuredIdPresent: true, registered: Boolean(registered), url: String(registered?.url || ""), events: eventNames, requiredEventsMissing: required.filter(name => !eventNames.includes(name)) };
        } catch (webhookError) {
          webhook = { configuredIdPresent: true, registered: false, error: webhookError instanceof Error ? webhookError.message : "Could not inspect PayPal webhook registration." };
        }
      }
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
      return json(200, { diagnostics, webhook, checkedAt: new Date().toISOString() });
    }

    if (action === "create_guest_order") {
      if (!await billingEnabled()) return fail(503, "Billing is disabled until the PayPal Live integration has passed verification.");
      const planKey = String(body.planKey || "live_test").trim().toLowerCase();
      const { data: plan, error: planError } = await db.from("billing_plan_config")
        .select("plan_key,display_name,amount,currency,enabled").eq("plan_key", planKey).eq("enabled", true).maybeSingle();
      if (planError) throw planError;
      if (!plan) return fail(404, "The selected CenAI plan is unavailable.");
      const amount = Number(plan.amount);
      const currency = String(plan.currency || "PHP").toUpperCase();
      if (!Number.isFinite(amount) || amount <= 0 || !/^[A-Z]{3}$/.test(currency)) return fail(400, "The selected plan has an invalid price.");
      if (plan.plan_key === "live_test" && (currency !== "PHP" || amount !== 1)) {
        return fail(409, "CenAI Starter is locked to PHP 1.00. Checkout was stopped to prevent an incorrect charge.");
      }
      const token = await accessToken();
      const order = await paypal("/v2/checkout/orders", "POST", token, {
        intent: "CAPTURE",
        purchase_units: [{
          reference_id: plan.plan_key,
          custom_id: user.id,
          description: "CenAI " + String(plan.display_name || "membership") + " — 30-day access",
          amount: { currency_code: currency, value: amount.toFixed(2) },
        }],
        payment_source: undefined,
        application_context: {
          brand_name: "CenAI",
          locale: "en-PH",
          user_action: "PAY_NOW",
          shipping_preference: "NO_SHIPPING",
          return_url: baseUrl() + "/billing/success",
          cancel_url: baseUrl() + "/billing/cancel",
        },
      });
      const orderId = String(order.id || "");
      if (!orderId) return fail(502, "PayPal did not return a guest payment order.");
      const { error: saveError } = await db.from("billing_one_time_payments").insert({
        user_id: user.id, provider_order_id: orderId, plan_key: plan.plan_key,
        amount, currency, status: "CREATED",
      });
      if (saveError) throw saveError;
      return json(200, { orderId, amount: amount.toFixed(2), currency, accessDays: 30, paymentType: "ONE_TIME" });
    }

    if (action === "capture_guest_order") {
      const orderId = String(body.orderId || "").trim();
      if (!/^[A-Z0-9-]{8,80}$/i.test(orderId)) return fail(400, "Invalid PayPal order ID.");
      const { data: payment, error: paymentError } = await db.from("billing_one_time_payments")
        .select("user_id,provider_order_id,plan_key,amount,currency,status,access_expires_at")
        .eq("provider_order_id", orderId).eq("user_id", user.id).maybeSingle();
      if (paymentError) throw paymentError;
      if (!payment) return fail(404, "This guest checkout does not belong to your CenAI account.");
      if (payment.status === "COMPLETED" && payment.access_expires_at) {
        return json(200, { captured: true, alreadyCaptured: true, accessExpiresAt: payment.access_expires_at, accessDays: 30 });
      }
      if (payment.status !== "CREATED") return fail(409, "This guest payment is no longer available.");
      const token = await accessToken();
      const orderDetail = await paypal("/v2/checkout/orders/" + encodeURIComponent(orderId), "GET", token);
      const unit = Array.isArray(orderDetail.purchase_units) ? orderDetail.purchase_units[0] : null;
      const orderAmount = unit?.amount || {};
      if (String(unit?.custom_id || "") !== user.id ||
          String(unit?.reference_id || "") !== String(payment.plan_key) ||
          String(orderAmount.currency_code || "").toUpperCase() !== String(payment.currency).toUpperCase() ||
          Number(orderAmount.value) !== Number(payment.amount) ||
          !["CREATED", "APPROVED"].includes(String(orderDetail.status || ""))) {
        return fail(403, "PayPal order ownership, price, or approval verification failed.");
      }
      const captured = await paypal("/v2/checkout/orders/" + encodeURIComponent(orderId) + "/capture", "POST", token, {});
      const capturedUnit = Array.isArray(captured.purchase_units) ? captured.purchase_units[0] : null;
      const capture = capturedUnit?.payments?.captures?.[0];
      const captureAmount = capture?.amount || {};
      if (String(captured.status || "") !== "COMPLETED" || String(capture?.status || "") !== "COMPLETED" ||
          String(capturedUnit?.custom_id || unit.custom_id || "") !== user.id ||
          String(captureAmount.currency_code || "").toUpperCase() !== String(payment.currency).toUpperCase() ||
          Number(captureAmount.value) !== Number(payment.amount)) {
        await db.from("billing_one_time_payments").update({ status: "FAILED", updated_at: new Date().toISOString() })
          .eq("provider_order_id", orderId).eq("user_id", user.id).eq("status", "CREATED");
        return fail(409, "PayPal did not confirm the full one-time payment. No membership access was granted.");
      }
      const paidAt = capture?.create_time && !Number.isNaN(Date.parse(String(capture.create_time)))
        ? new Date(String(capture.create_time)).toISOString() : new Date().toISOString();
      const expiresAt = new Date(Date.parse(paidAt) + 30 * 24 * 60 * 60 * 1000).toISOString();
      const { error: saveError } = await db.from("billing_one_time_payments").update({
        status: "COMPLETED", paid_at: paidAt, access_expires_at: expiresAt,
        provider_capture_id: String(capture.id || ""), updated_at: new Date().toISOString(),
      }).eq("provider_order_id", orderId).eq("user_id", user.id).eq("status", "CREATED");
      if (saveError) throw saveError;
      const { error: entitlementError } = await db.from("billing_subscriptions").upsert({
        user_id: user.id, provider: "paypal", provider_subscription_id: "ONE_TIME:" + orderId,
        plan_key: payment.plan_key, status: "ACTIVE", current_period_end: expiresAt,
        cancel_at_period_end: false, updated_at: new Date().toISOString(),
      }, { onConflict: "provider_subscription_id" });
      if (entitlementError) throw entitlementError;
      return json(200, { captured: true, paymentType: "ONE_TIME", accessDays: 30, accessExpiresAt: expiresAt,
        message: "Your one-time payment is confirmed. CenAI membership is active for 30 days and will not renew automatically." });
    }

    if (action === "record_sdk_subscription") {
      const subscriptionId = String(body.subscriptionId || "").trim();
      const planKey = String(body.planKey || "").trim().toLowerCase();
      if (!/^I-[A-Z0-9]+$/i.test(subscriptionId)) return fail(400, "Invalid PayPal subscription ID.");
      const { data: plan, error: planError } = await db.from("billing_plan_config")
        .select("plan_key,paypal_plan_id").eq("plan_key", planKey).eq("enabled", true).maybeSingle();
      if (planError) throw planError;
      if (!plan?.paypal_plan_id) return fail(400, "The selected subscription plan is unavailable.");
      const token = await accessToken();
      const detail = await paypal("/v1/billing/subscriptions/" + encodeURIComponent(subscriptionId), "GET", token);
      if (String(detail.custom_id || "") !== user.id || String(detail.plan_id || "") !== String(plan.paypal_plan_id)) {
        return fail(403, "PayPal subscription ownership or plan verification failed.");
      }
      const providerStatus = String(detail.status || "UNKNOWN");
      if (!["APPROVAL_PENDING", "APPROVED", "ACTIVE"].includes(providerStatus)) {
        return fail(409, "PayPal has not confirmed subscription approval. Current status: " + providerStatus);
      }
      const attempt = {
        user_id: user.id, provider_subscription_id: subscriptionId, plan_key: plan.plan_key,
        status: "APPROVED", expires_at: new Date(Date.now() + 24 * 60 * 60 * 1000).toISOString(),
      };
      const { error: saveError } = await db.from("billing_checkout_attempts").upsert(attempt, { onConflict: "provider_subscription_id" });
      if (saveError) throw saveError;
      const result = await settleCheckoutAttempt(user.id, attempt, false);
      return json(200, { verified: true, status: result, message: result === "ACTIVE"
        ? "PayPal confirmed your first payment. Your membership is active."
        : "PayPal approval was verified. Membership access will activate after the first payment is confirmed." });
    }

    if (action === "create_subscription") {
      if (!await billingEnabled()) return fail(503, "Billing is disabled until the PayPal Live integration has passed verification.");
      const planKey = String(body.planKey || "live_test");
      const { data: plan, error } = await db.from("billing_plan_config")
        .select("plan_key,display_name,amount,currency,interval_unit,interval_count,paypal_plan_id,is_test")
        .eq("plan_key", planKey).eq("enabled", true).maybeSingle();
      if (error) throw error;
      if (!plan?.paypal_plan_id) return fail(400, "This subscription plan is not configured yet.");
      const verifiedPlan = await ensurePayPalPlanCurrency(plan, user.id);
      
      const token = await accessToken();
      const subscription = await paypal("/v1/billing/subscriptions", "POST", token, {
        plan_id: verifiedPlan.paypal_plan_id,
        custom_id: user.id,
        application_context: {
          brand_name: "CenAI",
          locale: "en-PH",
          user_action: "SUBSCRIBE_NOW",
          // Do not force a specific payer funding source. Let PayPal show
          // every funding method the merchant and buyer are eligible to use.
          // Guest card payments remain available through the separate one-time Orders v2 flow.
          payment_method: { payee_preferred: "UNRESTRICTED" },
          shipping_preference: "NO_SHIPPING",
          return_url: baseUrl() + "/billing/success",
          cancel_url: baseUrl() + "/billing/cancel",
        },
      });
      const subscriptionId = String(subscription.id || "");
      const approvalUrl = (subscription.links || []).find((link: any) => link.rel === "approve")?.href;
      console.info("PayPal subscription created", JSON.stringify({
        subscriptionId,
        status: String(subscription.status || "UNKNOWN"),
        planId: String(subscription.plan_id || verifiedPlan.paypal_plan_id),
        approvalHost: (() => { try { return new URL(String(approvalUrl || "")).hostname; } catch { return "missing"; } })(),
        approveLinkPresent: Boolean(approvalUrl),
      }));
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
        .select("provider_subscription_id,plan_key,expires_at").eq("user_id", user.id).in("status", ["AWAITING_APPROVAL", "APPROVED", "EXPIRED"]);
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
