(() => {
  const state = document.getElementById("feed-state");
  const root = document.getElementById("releases");
  const esc = (value) => String(value ?? "").replace(/[&<>"']/g, (c) => ({ "&":"&amp;", "<":"&lt;", ">":"&gt;", '"':"&quot;", "'":"&#39;" })[c]);
  async function load() {
    try {
      const response = await fetch("/api/releases", { headers: { Accept: "application/json" } });
      if (!response.ok) throw new Error("Release feed is unavailable (" + response.status + ")");
      const data = await response.json();
      state.textContent = data.releases.length ? "Official release feed · " + data.releases.length + " release(s)" : "No published releases are available yet.";
      if (!data.releases.length) {
        root.innerHTML = '<div class="empty">There are no published releases in the official repository yet. Check back after a signed release is published.</div>';
        return;
      }
      root.innerHTML = data.releases.map((r) => {
        const assets = r.assets.map((a) => '<p><a href="' + esc(a.downloadUrl) + '" rel="noopener noreferrer">Download ' + esc(a.name) + '</a> · ' + (a.size / 1048576).toFixed(1) + ' MB</p>').join("");
        return '<article class="release"><div class="release-head"><span class="channel">' + esc(r.channel.toUpperCase()) + '</span><time>' + esc(r.publishedAt ? new Date(r.publishedAt).toLocaleString() : "Date unavailable") + '</time></div><h3>' + esc(r.name) + ' <small>' + esc(r.tag) + '</small></h3><p>' + esc(r.notes.slice(0, 420)) + '</p>' + (assets || '<p>No downloadable assets attached.</p>') + '<a href="' + esc(r.htmlUrl) + '" rel="noopener noreferrer">Full release notes →</a></article>';
      }).join("");
    } catch (error) {
      state.textContent = "Release feed unavailable";
      root.innerHTML = '<div class="empty">' + esc(error.message) + '. You can still visit the <a href="https://github.com/C4RL-DANIEL/CenAi/releases">official GitHub releases page</a>.</div>';
    }
  }
  load();
})();