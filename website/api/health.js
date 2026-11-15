const required = ["SUPABASE_URL", "SUPABASE_PUBLISHABLE_KEY", "SUPABASE_SECRET_KEY"];
const optional = ["CENAI_API_BASE_URL", "AI_GATEWAY_API_KEY", "BILLING_SECRET_KEY"];
export default function handler(req, res) {
  if (req.method !== "GET") { res.setHeader("Allow", "GET"); return res.status(405).json({ error: "Method not allowed" }); }
  const configured = Object.fromEntries([...required, ...optional].map(key => [key, Boolean(process.env[key])]));
  const missing = required.filter(key => !process.env[key]);
  res.setHeader("Cache-Control", "no-store");
  res.setHeader("X-Content-Type-Options", "nosniff");
  return res.status(200).json({
    service: "cenai-website",
    status: "online",
    checkedAt: new Date().toISOString(),
    adminPanel: "available-at-/admin",
    controlPlane: missing.length === 0 ? "configuration-present" : "not-configured",
    integrations: configured,
    missingConfiguration: missing,
    note: missing.length
      ? "Public website is online. Admin authentication and database access require the missing Supabase configuration. No credentials are exposed."
      : "Required Supabase configuration is present. This endpoint checks configuration only; authenticated admin API requests verify actual connectivity."
  });
}
