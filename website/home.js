(() => {
  const nav = document.getElementById("main-nav");
  const toggle = document.getElementById("nav-toggle");
  toggle?.addEventListener("click", () => {
    const open = nav.classList.toggle("open");
    toggle.setAttribute("aria-expanded", String(open));
    toggle.setAttribute("aria-label", open ? "Close navigation" : "Open navigation");
  });
  nav?.querySelectorAll("a").forEach(link => link.addEventListener("click", () => {
    nav.classList.remove("open");
    toggle?.setAttribute("aria-expanded", "false");
  }));
  fetch("/api/site-pages?slug=home", { cache: "no-store" })
    .then(response => response.ok ? response.json() : null)
    .then(data => {
      const page = data?.page;
      if (!page || !page.published) return;
      const content = page.content || {};
      const title = typeof content.heroTitle === "string" ? content.heroTitle : "";
      const description = typeof content.heroDescription === "string" ? content.heroDescription : "";
      const announcement = typeof content.announcement === "string" ? content.announcement : "";
      const fields = {
        heroTitle: "[data-cms-title]",
        heroDescription: "[data-cms-description]",
        platformTitle: "[data-cms-platform-title]",
        platformDescription: "[data-cms-platform-description]",
        workflowTitle: "[data-cms-workflow-title]",
        workflowDescription: "[data-cms-workflow-description]",
        securityTitle: "[data-cms-security-title]",
        securityDescription: "[data-cms-security-description]",
        ctaTitle: "[data-cms-cta-title]",
        ctaDescription: "[data-cms-cta-description]"
      };
      for (const [key, selector] of Object.entries(fields)) {
        if (typeof content[key] !== "string" || !content[key].trim()) continue;
        const node = document.querySelector(selector);
        if (node) node.textContent = content[key].trim();
      }
      const announcementText = document.querySelector("[data-cms-announcement-text]");
      if (announcement && announcementText) announcementText.textContent = announcement;
      if (page.title) document.title = page.title;
      if (page.description) document.querySelector('meta[name="description"]')?.setAttribute("content", page.description);
    })
    .catch(() => {});
})();