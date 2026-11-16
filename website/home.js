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
      const titleNode = document.querySelector("[data-cms-title]");
      const descriptionNode = document.querySelector("[data-cms-description]");
      const announcementNode = document.querySelector("[data-cms-announcement]");
      if (title && titleNode) titleNode.textContent = title;
      if (description && descriptionNode) descriptionNode.textContent = description;
      const announcementText = announcementNode?.querySelector("[data-cms-announcement-text]");
      if (announcement && announcementText) announcementText.textContent = announcement;
      if (page.title) document.title = page.title;
      if (page.description) document.querySelector('meta[name="description"]')?.setAttribute("content", page.description);
    })
    .catch(() => {});
})();