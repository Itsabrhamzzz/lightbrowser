// YouTube only: hide ad slots and skip video ads as soon as they start.
(() => {
  if (!/(^|\.)youtube\.com$/.test(location.hostname)) return;
  // Ad blocking turned off for this site (set by the browser's shield button).
  try { if (localStorage.getItem('lb-adblock-off')) return; } catch (_) {}
  const css = '.video-ads,.ytp-ad-module,.ytp-ad-overlay-container,#masthead-ad,#player-ads,' +
    'ytd-ad-slot-renderer,ytd-in-feed-ad-layout-renderer,ytd-banner-promo-renderer,' +
    'ytd-statement-banner-renderer,ytd-promoted-sparkles-web-renderer,ytd-display-ad-renderer,' +
    'ytd-companion-slot-renderer{display:none!important}';
  const st = document.createElement('style');
  st.textContent = css;
  (document.head || document.documentElement).appendChild(st);
  setInterval(() => {
    const p = document.querySelector('.html5-video-player.ad-showing');
    if (!p) return;
    const v = p.querySelector('video');
    if (v && isFinite(v.duration) && v.duration > 0) { v.muted = true; v.currentTime = v.duration; }
    const b = document.querySelector('.ytp-skip-ad-button, .ytp-ad-skip-button, .ytp-ad-skip-button-modern');
    if (b) b.click();
  }, 300);
})();
