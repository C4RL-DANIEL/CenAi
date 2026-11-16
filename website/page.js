(() => {
  const slug = new URLSearchParams(location.search).get("slug") || location.pathname.split("/").filter(Boolean).pop() || "";
  const title = document.getElementById("page-title");
  const description = document.getElementById("page-description");
  const body = document.getElementById("page-body");
  const error = document.getElementById("page-error");
  if (!/^[a-z0-9][a-z0-9-]{0,79}$/.test(slug)) { error.hidden = false; title.textContent = "Page not found"; return; }
  fetch("/api/site-pages?slug=" + encodeURIComponent(slug), { cache: "no-store" })
    .then(r => r.ok ? r.json() : null)
    .then(data => {
      const page = data?.page;
      if (!page || !page.published) throw new Error("Page unavailable");
      title.textContent = page.title || "CenAI";
      description.textContent = page.description || "";
      document.title = page.title ? page.title + " · CenAI" : "CenAI";
      document.querySelector('meta[name="description"]')?.setAttribute("content", page.description || "CenAI");
      const content = page.content || {};
      const raw = typeof content.bodyMarkdown === "string" ? content.bodyMarkdown : (typeof content.body === "string" ? content.body : "");
      body.replaceChildren();
      raw.split(/\n\s*\n/).map(part => part.trim()).filter(Boolean).forEach(part => {
        const paragraph = document.createElement("p");
        paragraph.textContent = part;
        body.append(paragraph);
      });
      if (!raw.trim()) body.textContent = "This page has no published body content yet.";
    })
    .catch(() => { error.hidden = false; title.textContent = "Page not found"; description.textContent = ""; body.replaceChildren(); });
})();