const OWNER = "C4RL-DANIEL";
const REPO = "CenAi";
export default async function handler(req, res) {
  if (req.method !== "GET") {
    res.setHeader("Allow", "GET");
    return res.status(405).json({ error: "Method not allowed" });
  }
  try {
    const response = await fetch(`https://api.github.com/repos/${OWNER}/${REPO}/releases?per_page=20`, {
      headers: { Accept: "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28", "User-Agent": "CenAI-Release-Feed" }
    });
    if (!response.ok) throw new Error(`GitHub release service returned ${response.status}`);
    const raw = await response.json();
    const releases = raw.filter((r) => !r.draft).map((r) => ({
      id: r.id,
      tag: r.tag_name,
      name: r.name || r.tag_name,
      channel: r.prerelease ? "beta" : "stable",
      publishedAt: r.published_at,
      notes: (r.body || "").slice(0, 12000),
      htmlUrl: r.html_url,
      assets: (r.assets || []).map((a) => ({
        name: a.name,
        size: a.size,
        downloadUrl: a.browser_download_url,
        contentType: a.content_type
      }))
    }));
    res.setHeader("Cache-Control", "s-maxage=60, stale-while-revalidate=300");
    return res.status(200).json({ source: "github-releases", repository: `${OWNER}/${REPO}`, releases });
  } catch (error) {
    return res.status(502).json({ error: "Release feed temporarily unavailable", detail: error.message });
  }
}
