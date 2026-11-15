const required = ["CENAI_API_BASE_URL", "CENAI_DATABASE_URL", "CENAI_AUTH_SECRET"];
export default function handler(req, res) {
  if (req.method !== "GET") {
    res.setHeader("Allow", "GET");
    return res.status(405).json({ error: "Method not allowed" });
  }
  const configured = Object.fromEntries(required.map((key) => [key, Boolean(process.env[key])]));
  const missing = required.filter((key) => !process.env[key]);
  res.setHeader("Cache-Control", "no-store");
  return res.status(200).json({
    service: "cenai-website",
    status: "online",
    checkedAt: new Date().toISOString(),
    controlPlane: missing.length === 0 ? "configured" : "not-configured",
    integrations: configured,
    missingConfiguration: missing,
    note: missing.length
      ? "The website is online, but the Send.ai backend is not fully configured. No credentials are exposed by this endpoint."
      : "Required configuration variables are present. This is a configuration check, not a database or provider connectivity test."
  });
}
