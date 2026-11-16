export default async function handler(req, res) {
  res.setHeader("Cache-Control", "no-store");
  res.setHeader("X-Content-Type-Options", "nosniff");
  if (req.method !== "GET") {
    res.setHeader("Allow", "GET");
    return res.status(405).json({ error: "Method not allowed" });
  }
  const base = process.env.SUPABASE_URL;
  const key = process.env.SUPABASE_PUBLISHABLE_KEY;
  if (!base || !key) return res.status(503).json({ error: "Website content service is not configured" });
  try {
    const upstreamUrl = new URL(base.replace(/\/$/, "") + "/rest/v1/site_pages");
    upstreamUrl.searchParams.set("select", "slug,title,description,content,published,updated_at");
    upstreamUrl.searchParams.set("published", "eq.true");
    const slug = String(req.query?.slug || "").trim().toLowerCase();
    if (slug) upstreamUrl.searchParams.set("slug", "eq." + slug);
    const upstream = await fetch(upstreamUrl, {
      headers: { apikey: key, Authorization: "Bearer " + key, Accept: "application/json" }
    });
    if (!upstream.ok) return res.status(502).json({ error: "Website content could not be loaded" });
    const pages = await upstream.json();
    const page = slug ? (Array.isArray(pages) ? pages[0] || null : null) : null;
    return res.status(200).json({ page, pages: Array.isArray(pages) ? pages : [] });
  } catch (error) {
    console.error("site pages request failed", error instanceof Error ? error.message : "unknown error");
    return res.status(503).json({ error: "Website content is temporarily unavailable" });
  }
}