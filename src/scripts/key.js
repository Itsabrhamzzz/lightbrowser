// Forwards browser shortcuts out of the page (the webview has keyboard focus while you browse).
//
// Any page can call window.ipc.postMessage, so the browser only acts on shortcut messages that
// carry a per-launch secret. The secret lives in this closure (pages can't read injected scripts),
// and the real postMessage is captured before page scripts run, so a page that later replaces
// window.chrome.webview.postMessage can't see what we send.
(() => {
  const token = '__LB_TOKEN__';
  let raw;
  try {
    const wv = window.chrome.webview;
    raw = wv.postMessage.bind(wv);
  } catch (_) {
    return;
  }
  const post = a => raw('k:' + token + ':' + a);

  window.addEventListener('keydown', e => {
    if (!e.isTrusted) return; // ignore key events a page synthesizes
    const k = e.key.toLowerCase();
    let a = null;
    if (e.ctrlKey && !e.shiftKey && !e.altKey) {
      if (k === 't') a = 'new';
      else if (k === 'w') a = 'close';
      else if (k === 'l') a = 'focus';
      else if (k === 'r') a = 'reload';
      else if (k === 'f') a = 'find';
      else if (k === 'd') a = 'bookmark';
      else if (k === 'h') a = 'history';
      else if (k === 'j') a = 'downloads';
      else if (k === 'tab') a = 'next';
      else if (/^[1-9]$/.test(k)) a = 'tab:' + k;
    } else if (e.ctrlKey && e.shiftKey && !e.altKey) {
      if (k === 'p') a = 'pip';
      else if (k === 't') a = 'reopen';
      else if (k === 'b') a = 'bmbar';
      else if (k === 's') a = 'split';
      else if (k === 'tab') a = 'prev';
    } else if (e.altKey && !e.ctrlKey && e.key === 'ArrowLeft') a = 'back';
    else if (e.altKey && !e.ctrlKey && e.key === 'ArrowRight') a = 'forward';
    else if (e.key === 'F12' && !e.ctrlKey && !e.altKey && !e.shiftKey) a = 'devtools';
    else if (e.key === 'Escape') { post('esc'); return; } // page keeps its own Esc handling
    if (a) { e.preventDefault(); e.stopPropagation(); post(a); }
  }, true);

  // Clicking the page closes an open dropdown panel (extensions menu / popup).
  window.addEventListener('pointerdown', e => { if (e.isTrusted) post('pagedown'); }, true);
  // Fullscreen is detected by the browser itself (WebView2's fullscreen event), not reported here.
})();
