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
const baseUrl = () => (Deno.env.get("CENAI_PUBLIC_URL") || "https://cenai-website.vercel.app").replace(/\/$/, "");

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
    console.error("PayPal API error", method, path, response.status, String(data.name || "unknown"));
    throw new Error("PayPal rejected the request (" + response.status + "). Check the PayPal account and subscription permissions.");
  }
  return data;
}
async function billingEnabled() {
  const { data, error } = await db.from("admin_feature_flags").select("enabled").eq("key", "billing.enabled").maybeSingle();
  if (error) throw error;
  return data?.enabled === true;
}
async function createOrRefreshPlan(body: Record<string, unknown>, userId: string) {
  const planKey = String(body.planKey || "").trim().toLowerCase();
  const name = String(body.displayName || "").trim();
  const amount = Number(body.amount);
  const currency = String(body.currency || "PHP").toUpperCase();
  const intervalUnit = String(body.intervalUnit || "MONTH").toUpperCase();
  const intervalCount = Number(body.intervalCount || 1);
  const enabled = body.enabled === true;
  const isTest = body.isTest === true;
  if (!/^[a-z0-9][a-z0-9_-]{1,63}$/.test(planKey) || name.length < 2 || name.length > 80 ||
      !Number.isFinite(amount) || amount < 1 || amount > 100000 ||
      !/^[A-Z]{3}$/.test(currency) || !["DAY", "WEEK", "MONTH", "YEAR"].includes(intervalUnit) ||
      !Number.isInteger(intervalCount) || intervalCount < 1 || intervalCount > 12) {
    throw new Error("Invalid plan settings. Amount must be at least 1.00 and the billing interval must be valid.");
  }
  const token = await accessToken();
  const { data: current, error: lookupError } = await db.from("billing_plan_config").select("paypal_product_id").eq("plan_key", planKey).maybeSingle();
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
  let userId = String(resource.custom_id || resource.custom || existing?.user_id || "");
  if (!userId && resource.supplementary_data?.related_ids?.order_id) {
    const { data: row } = await db.from("billing_subscriptions").select("user_id")
      .eq("provider_subscription_id", String(resource.supplementary_data.related_ids.order_id)).maybeSingle();
    userId = row?.user_id || "";
  }
  if (!userId) return;
  const statusMap: Record<string, string> = {
    "BILLING.SUBSCRIPTION.CREATED": "APPROVAL_PENDING",
    "BILLING.SUBSCRIPTION.ACTIVATED": "ACTIVE",
    "BILLING.SUBSCRIPTION.UPDATED": String(resource.status || "ACTIVE"),
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
  let planKey = String(existing?.plan_key || "live_test");
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
      const { error: insertError } = await db.from("billing_webhook_events").insert({ provider: "paypal", event_id: eventId });
      if (insertError && insertError.code !== "23505") throw insertError;
      return json(200, { received: true });
    }

    const user = await currentUser(req);
    if (!user) return fail(401, "Sign in required");
    const body = await req.json().catch(() => ({}));
    const action = String(body.action || "create_subscription");

    if (action === "configure_plan") {
      if (!await isAdmin(user.id)) return fail(403, "Owner or administrator access required");
      const plan = await createOrRefreshPlan(body, user.id);
      return json(200, { plan, message: "PayPal Live plan created and activated. New subscriptions can now use this plan." });
    }
    if (action === "list_plans") {
      if (!await isAdmin(user.id)) return fail(403, "Owner or administrator access required");
      const { data, error } = await db.from("billing_plan_config")
        .select("plan_key,display_name,amount,currency,interval_unit,interval_count,enabled,is_test,paypal_plan_id,updated_at")
        .order("created_at");
      if (error) throw error;
      return json(200, { plans: data || [] });
    }
    if (action === "create_subscription") {
      if (!await billingEnabled()) return fail(503, "Billing is disabled until the PayPal Live integration has passed verification.");
      const planKey = String(body.planKey || "live_test");
      const { data: plan, error } = await db.from("billing_plan_config")
        .select("plan_key,display_name,amount,currency,interval_unit,interval_count,paypal_plan_id,is_test")
        .eq("plan_key", planKey).eq("enabled", true).maybeSingle();
      if (error) throw error;
      if (!plan?.paypal_plan_id) return fail(400, "This subscription plan is not configured yet.");
      if (plan.is_test !== true && planKey === "live_test") return fail(409, "The live test plan configuration is inconsistent.");
      const token = await accessToken();
      const subscription = await paypal("/v1/billing/subscriptions", "POST", token, {
        plan_id: plan.paypal_plan_id,
        custom_id: user.id,
        application_context: {
          brand_name: "CenAI",
          locale: "en-PH",
          user_action: "SUBSCRIBE_NOW",
          payment_method: { payer_selected: "PAYPAL", payee_preferred: "IMMEDIATE_PAYMENT_REQUIRED" },
          return_url: baseUrl() + "/billing/success",
          cancel_url: baseUrl() + "/billing/cancel",
        },
      });
      const subscriptionId = String(subscription.id || "");
      const approvalUrl = (subscription.links || []).find((link: any) => link.rel === "approve")?.href;
      if (!subscriptionId || !approvalUrl) return fail(502, "PayPal did not return a subscription approval URL.");
      const { error: saveError } = await db.from("billing_subscriptions").upsert({
        user_id: user.id, provider: "paypal", provider_subscription_id: subscriptionId,
        plan_key: plan.plan_key, status: String(subscription.status || "APPROVAL_PENDING"),
        current_period_end: null, cancel_at_period_end: false, updated_at: new Date().toISOString(),
      }, { onConflict: "provider_subscription_id" });
      if (saveError) throw saveError;
      return json(200, { approvalUrl, subscriptionId, plan: { name: plan.display_name, amount: plan.amount, currency: plan.currency, intervalUnit: plan.interval_unit, intervalCount: plan.interval_count } });
    }
    if (action === "status") {
      const { data, error } = await db.from("billing_subscriptions")
        .select("provider,provider_subscription_id,plan_key,status,current_period_end,cancel_at_period_end,updated_at")
        .eq("user_id", user.id).eq("provider", "paypal").order("updated_at", { ascending: false }).limit(5);
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
        .update({ status: "CANCELLED", cancel_at_period_end: true, updated_at: new Date().toISOString() })
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
