const required = ["SUPABASE_URL", "SUPABASE_PUBLISHABLE_KEY"];
const optional = ["CENAI_API_BASE_URL", "AI_GATEWAY_API_KEY", "EMAIL_USER", "EMAIL_PASS", "EMAIL_OTP_SECRET", "STRIPE_SECRET_KEY", "STRIPE_WEBHOOK_SECRET", "STRIPE_PRICE_SUPPORTER", "STRIPE_PRICE_PRO", "CENAI_PUBLIC_URL"];
export default function handler(req, res) {
  if (req.method !== "GET") { res.setHeader("Allow", "GET"); return res.status(405).json({ error: "Method not allowed" }); }
  const configured = Object.fromEntries([...required, ...optional].map(key => [key, Boolean(process.env[key])]));
  const missing = required.filter(key => !process.env[key]);
  const otpRequired = ["EMAIL_USER", "EMAIL_PASS", "EMAIL_OTP_SECRET"];
  const missingOtp = otpRequired.filter(key => !process.env[key]);
  res.setHeader("Cache-Control", "no-store");
  res.setHeader("X-Content-Type-Options", "nosniff");
  return res.status(200).json({
    service: "cenai-website",
    status: "online",
    checkedAt: new Date().toISOString(),
    adminPanel: "available-at-/admin",
    emailOtp: missingOtp.length === 0 ? "configuration-present" : "not-configured",
    publicApi: "deployed-feature-flag-disabled-by-default",
    modelRegistry: "deployed",
    usageAnalytics: "deployed",
    billing: ["STRIPE_SECRET_KEY","STRIPE_WEBHOOK_SECRET","STRIPE_PRICE_SUPPORTER","STRIPE_PRICE_PRO","CENAI_PUBLIC_URL"].every(key => Boolean(process.env[key])) ? "configuration-present-needs-webhook-test" : "not-configured",
    missingEmailOtpConfiguration: missingOtp,
    controlPlane: missing.length === 0 ? "configuration-present" : "not-configured",
    integrations: configured,
    missingConfiguration: missing,
    note: missing.length
      ? "Public website is online. Admin authentication and database access require the missing Supabase configuration. No credentials are exposed."
      : "Required Supabase configuration is present. This endpoint checks configuration only; authenticated admin API requests verify actual connectivity.",
    emailNote: missingOtp.length ? "Gmail SMTP OTP delivery needs the listed server-side environment variables." : "Gmail SMTP OTP configuration is present; actual message delivery is verified when a code is sent.",
    billingNote: "Stripe checkout remains unavailable until server-side Stripe secrets, price IDs and the public site URL are configured and webhook events are verified."
  });
}
