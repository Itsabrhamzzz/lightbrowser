// Runs in the dropdown panel (extension popups and the extensions menu): tells the browser how big
// the content is, and lets the page close the panel the way an extension popup expects.
(() => {
  const post = m => { try { window.ipc.postMessage(m); } catch (_) {} };
  window.close = () => post('panel:close');
  addEventListener('keydown', e => { if (e.key === 'Escape') post('panel:close'); }, true);
  let last = '';
  const measure = () => {
    const b = document.body;
    if (!b) return;
    // Measure the body (not the viewport, which is at least as big as the panel already is).
    const cs = getComputedStyle(b);
    const r = b.getBoundingClientRect();
    const w = Math.ceil(Math.max(r.width, b.scrollWidth) + parseFloat(cs.marginLeft) + parseFloat(cs.marginRight));
    const h = Math.ceil(Math.max(r.height, b.scrollHeight) + parseFloat(cs.marginTop) + parseFloat(cs.marginBottom));
    const key = w + ':' + h;
    if (key !== last) { last = key; post('panel:size:' + key); }
  };
  addEventListener('DOMContentLoaded', () => {
    measure();
    try { new ResizeObserver(measure).observe(document.body); } catch (_) {}
  });
  addEventListener('load', () => { measure(); setTimeout(measure, 200); setTimeout(measure, 800); });
})();
