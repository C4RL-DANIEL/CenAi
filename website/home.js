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

  // CMS content remains authoritative for editable copy.
  fetch("/api/site-pages?slug=home", { cache: "no-store" })
    .then(response => response.ok ? response.json() : null)
    .then(data => {
      const page = data?.page;
      if (!page || !page.published) return;
      const content = page.content || {};
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
      if (typeof content.announcement === "string" && announcementText) announcementText.textContent = content.announcement;
      if (page.title) document.title = page.title;
      if (page.description) document.querySelector('meta[name="description"]')?.setAttribute("content", page.description);
    })
    .catch(() => {});

  const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const revealTargets = document.querySelectorAll(
    ".hero-copy,.product-stage,.section-kicker,.section-heading,.capability,.security-layout > div:first-child,.security-cards article,.access-panel > div,.footer-main > *"
  );
  if (!reduceMotion && "IntersectionObserver" in window) {
    document.documentElement.classList.add("motion-ready");
    revealTargets.forEach((element, index) => {
      element.classList.add("scroll-reveal");
      const siblingIndex = [...element.parentElement.children].indexOf(element);
      element.style.setProperty("--reveal-delay", Math.min(siblingIndex * 75, 300) + "ms");
    });
    const observer = new IntersectionObserver((entries, instance) => {
      entries.forEach(entry => {
        if (!entry.isIntersecting) return;
        entry.target.classList.add("is-visible");
        instance.unobserve(entry.target);
      });
    }, {threshold: 0.12, rootMargin: "0px 0px -7% 0px"});
    revealTargets.forEach(element => observer.observe(element));
  }

  const progress = document.createElement("div");
  progress.className = "scroll-progress";
  progress.setAttribute("aria-hidden", "true");
  progress.innerHTML = "<span></span>";
  document.body.append(progress);
  const progressBar = progress.firstElementChild;
  const header = document.querySelector(".site-header");
  const stage = document.querySelector(".product-stage");
  let ticking = false;
  const updateScrollEffects = () => {
    const maxScroll = document.documentElement.scrollHeight - window.innerHeight;
    const amount = maxScroll > 0 ? window.scrollY / maxScroll : 0;
    progressBar.style.transform = "scaleX(" + Math.min(1, Math.max(0, amount)) + ")";
    header?.classList.toggle("is-scrolled", window.scrollY > 18);
    if (!reduceMotion && window.matchMedia("(min-width: 901px)").matches && stage) {
      const drift = Math.max(-22, Math.min(22, (window.scrollY - stage.offsetTop) * 0.025));
      stage.style.setProperty("--stage-drift", drift.toFixed(1) + "px");
    }
    ticking = false;
  };
  window.addEventListener("scroll", () => {
    if (ticking) return;
    ticking = true;
    window.requestAnimationFrame(updateScrollEffects);
  }, {passive:true});
  window.addEventListener("resize", updateScrollEffects, {passive:true});
  updateScrollEffects();
})();