// Forwards browser shortcuts out of the page (the webview has keyboard focus while you browse).
window.addEventListener('keydown', e => {
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
    else if (k === 'tab') a = 'prev';
  } else if (e.altKey && !e.ctrlKey && e.key === 'ArrowLeft') a = 'back';
  else if (e.altKey && !e.ctrlKey && e.key === 'ArrowRight') a = 'forward';
  else if (e.key === 'F12' && !e.ctrlKey && !e.altKey && !e.shiftKey) a = 'devtools';
  else if (e.key === 'Escape') { window.ipc.postMessage('esc'); return; } // page keeps its own Esc handling
  if (a) { e.preventDefault(); e.stopPropagation(); window.ipc.postMessage(a); }
}, true);
// Clicking the page closes an open dropdown panel (extensions menu / popup).
window.addEventListener('pointerdown', () => window.ipc.postMessage('pagedown'), true);
document.addEventListener('fullscreenchange', () =>
  window.ipc.postMessage(document.fullscreenElement ? 'fs:1' : 'fs:0'));
