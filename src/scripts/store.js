// On Chrome Web Store item pages, offer an "Add to LightBrowser" button.
(() => {
  if (location.hostname !== 'chromewebstore.google.com') return;
  let btn = null, shownFor = '';
  const sync = () => {
    const m = location.pathname.match(/\/detail\/(?:[^/]+\/)?([a-p]{32})/);
    if (!m) { if (btn) btn.style.display = 'none'; shownFor = ''; return; }
    if (!btn) {
      btn = document.createElement('button');
      btn.style.cssText = 'position:fixed;right:24px;bottom:24px;z-index:2147483647;padding:12px 20px;border:0;' +
        'border-radius:24px;background:#4285f4;color:#fff;font:600 14px "Segoe UI",sans-serif;cursor:pointer;' +
        'box-shadow:0 6px 20px rgba(0,0,0,.3)';
      btn.onclick = () => {
        if (!confirm('Install this extension in LightBrowser?')) return;
        btn.textContent = 'Installing…';
        window.ipc.postMessage('ext:install:' + location.href);
      };
      document.documentElement.appendChild(btn);
    }
    btn.style.display = '';
    if (shownFor !== m[1]) { shownFor = m[1]; btn.textContent = 'Add to LightBrowser'; }
  };
  setInterval(sync, 1000);
  document.addEventListener('DOMContentLoaded', sync);
})();
