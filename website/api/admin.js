export default async function handler(req, res) {
  res.setHeader("Cache-Control", "no-store");
  res.setHeader("X-Content-Type-Options", "nosniff");
  res.setHeader("Vary", "Authorization");

  if (req.method !== "POST") {
    res.setHeader("Allow", "POST");
    return res.status(405).json({ error: "Method not allowed" });
  }

  const base = process.env.SUPABASE_URL;
  const key = process.env.SUPABASE_PUBLISHABLE_KEY;
  const token = (req.headers.authorization || "").replace(/^Bearer\s+/i, "");

  if (!base || !key) {
    return res.status(503).json({ error: "Supabase integration is not configured" });
  }
  if (!token) return res.status(401).json({ error: "Sign in required" });

  try {
    const supabaseUrl = base.replace(/\/$/, "");
    const upstream = await fetch(supabaseUrl + "/functions/v1/cenai-control-plane", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "apikey": key,
        "Authorization": "Bearer " + token
      },
      body: JSON.stringify(req.body || {})
    });
    const data = await upstream.json().catch(() => ({ error: "Invalid control-plane response" }));
    return res.status(upstream.status).json(data);
  } catch (error) {
    console.error("CenAI control-plane proxy failed", error instanceof Error ? error.message : "unknown error");
    return res.status(503).json({ error: "CenAI control-plane is temporarily unavailable" });
  }
}
