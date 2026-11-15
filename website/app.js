(() => {
  const toggle = document.querySelector('.menu-toggle');
  const nav = document.querySelector('.topbar nav');
  if (toggle && nav) {
    toggle.addEventListener('click', () => {
      const open = nav.classList.toggle('open');
      toggle.setAttribute('aria-expanded', String(open));
      toggle.setAttribute('aria-label', open ? 'Close navigation' : 'Open navigation');
    });
    nav.querySelectorAll('a').forEach(link => link.addEventListener('click', () => {
      nav.classList.remove('open');
      toggle.setAttribute('aria-expanded', 'false');
    }));
  }
  // The console and pricing sections are intentionally informational previews.
  // No mock metrics, API keys, account records, or billing actions are presented as live data.
})();