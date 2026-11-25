// Picture-in-picture for LightBrowser: hover button on videos + PiP that carries subtitles.
// Injected into every frame. Exposes window.__lbTogglePip() for the toolbar button / shortcut.
(() => {
  if (window.__lbPipInstalled) return;
  window.__lbPipInstalled = true;
  const MIN_W = 200, MIN_H = 110;

  const usable = v => v.isConnected && v.readyState > 0 && v.videoWidth > 0 && !v.disablePictureInPicture;

  function pickVideo() {
    const area = v => v.videoWidth * v.videoHeight;
    const vids = [...document.querySelectorAll('video')].filter(usable);
    vids.sort((a, b) => (a.paused ? 1 : 0) - (b.paused ? 1 : 0) || area(b) - area(a));
    return vids[0] || null;
  }

  // Subtitles: YouTube renders them as page elements; other sites use standard text tracks.
  function captionText(v) {
    const lines = [...document.querySelectorAll('.ytp-caption-window-container .caption-visual-line')];
    if (lines.length) return lines.map(l => l.textContent).join('\n');
    const segs = [...document.querySelectorAll('.ytp-caption-segment')];
    if (segs.length) return segs.map(s => s.textContent).join(' ');
    const out = [];
    for (const t of v.textTracks) {
      if ((t.kind === 'subtitles' || t.kind === 'captions') && t.mode !== 'disabled' && t.activeCues) {
        for (const c of t.activeCues) out.push((c.text || '').replace(/<[^>]+>/g, ''));
      }
    }
    return out.join('\n');
  }

  function drawCaption(ctx, text, W, H) {
    if (!text) return;
    const size = Math.max(16, Math.round(H / 20));
    ctx.font = `600 ${size}px "Segoe UI", Arial, sans-serif`;
    ctx.textAlign = 'center';
    ctx.textBaseline = 'bottom';
    const maxW = W * 0.9, lines = [];
    for (const para of text.split('\n')) {
      let cur = '';
      for (const word of para.split(/\s+/).filter(Boolean)) {
        const t = cur ? cur + ' ' + word : word;
        if (ctx.measureText(t).width > maxW && cur) { lines.push(cur); cur = word; } else cur = t;
      }
      if (cur) lines.push(cur);
    }
    const lh = size * 1.25;
    let y = H - H * 0.06;
    for (let i = lines.length - 1; i >= 0; i--) {
      const w = ctx.measureText(lines[i]).width + size * 0.6;
      ctx.fillStyle = 'rgba(0,0,0,.78)';
      ctx.fillRect(W / 2 - w / 2, y - lh + size * 0.15, w, lh);
      ctx.fillStyle = '#fff';
      ctx.fillText(lines[i], W / 2, y);
      y -= lh;
    }
  }

  function setMediaKeys(v) {
    const ms = navigator.mediaSession;
    if (!ms) return;
    try {
      ms.setActionHandler('play', () => v.play());
      ms.setActionHandler('pause', () => v.pause());
      ms.setActionHandler('seekbackward', () => { v.currentTime = Math.max(0, v.currentTime - 10); });
      ms.setActionHandler('seekforward', () => { v.currentTime = v.currentTime + 10; });
    } catch (_) {}
  }

  // While PiP is active, hide the page's own copy of the video over a black backdrop so the page
  // doesn't paint it twice. Returns a function that restores the original styles.
  function blank(v) {
    const undo = [];
    const force = (el, prop, val) => {
      const old = [el.style.getPropertyValue(prop), el.style.getPropertyPriority(prop)];
      el.style.setProperty(prop, val, 'important');
      undo.push(() => { if (old[0]) el.style.setProperty(prop, old[0], old[1]); else el.style.removeProperty(prop); });
    };
    force(v, 'visibility', 'hidden');
    if (v.parentElement) force(v.parentElement, 'background', '#000');
    return () => undo.forEach(f => f());
  }

  // Redraw the video plus subtitles onto a canvas and float that stream.
  async function enter(v) {
    let timer = 0, stream = null, pip = null, unblank = null;
    const cleanup = () => {
      clearInterval(timer);
      if (unblank) unblank();
      if (stream) stream.getTracks().forEach(t => t.stop());
      if (pip) pip.remove();
    };
    try {
      const scale = Math.min(1, 1280 / v.videoWidth);
      const W = Math.round(v.videoWidth * scale), H = Math.round(v.videoHeight * scale);
      const canvas = document.createElement('canvas');
      canvas.width = W; canvas.height = H;
      if (window.__lbDebug) window.__lbLastCanvas = canvas;
      const ctx = canvas.getContext('2d');
      const draw = () => {
        try { ctx.drawImage(v, 0, 0, W, H); drawCaption(ctx, captionText(v), W, H); } catch (_) {}
      };
      draw();
      stream = canvas.captureStream(30); // throws SecurityError if the video is cross-origin
      pip = document.createElement('video');
      pip.muted = true; pip.autoplay = true; pip.playsInline = true;
      pip.srcObject = stream;
      pip.style.cssText = 'position:fixed;left:0;top:0;width:2px;height:2px;opacity:0;pointer-events:none';
      (document.body || document.documentElement).appendChild(pip);
      await pip.play();
      timer = setInterval(draw, 33);
      pip.addEventListener('leavepictureinpicture', cleanup, { once: true });
      setMediaKeys(v);
      await pip.requestPictureInPicture();
      unblank = blank(v);
      return 'entered';
    } catch (e) {
      cleanup();
      // Fallback: plain PiP of the original video (no subtitles).
      await v.requestPictureInPicture();
      return 'entered-plain (' + e.name + ')';
    }
  }

  window.__lbTogglePip = async () => {
    try {
      if (document.pictureInPictureElement) { await document.exitPictureInPicture(); return 'exited'; }
      const v = pickVideo();
      if (!v) return 'no video';
      return await enter(v);
    } catch (e) { return 'error: ' + e.name + ' ' + e.message; }
  };

  // ---- Hover button, top-right of the video (built with DOM APIs: Trusted Types safe) ----
  const NS = 'http://www.w3.org/2000/svg';
  const host = document.createElement('div');
  let current = null, hideTimer = 0, overButton = false, built = false;

  function build() {
    if (built) return;
    built = true;
    host.style.cssText = 'all:initial;position:fixed;z-index:2147483647;display:none;';
    const root = host.attachShadow({ mode: 'closed' });
    const style = document.createElement('style');
    style.textContent =
      'button{all:initial;box-sizing:border-box;display:flex;align-items:center;gap:7px;height:34px;padding:0 10px;' +
      'border-radius:12px;background:rgba(24,25,29,.62);color:#fff;font:600 12.5px "Segoe UI Variable Text","Segoe UI",system-ui,sans-serif;' +
      'cursor:pointer;border:1px solid rgba(255,255,255,.18);box-shadow:0 6px 20px rgba(0,0,0,.35);' +
      'backdrop-filter:blur(14px) saturate(1.5);-webkit-backdrop-filter:blur(14px) saturate(1.5);' +
      'transition:background .15s,border-color .15s,transform .15s}' +
      'button:hover{background:rgba(91,124,250,.92);border-color:rgba(255,255,255,.3);transform:translateY(-1px)}' +
      'svg{width:16px;height:16px;flex:none}' +
      'span{max-width:0;overflow:hidden;white-space:nowrap;transition:max-width .2s ease}' +
      'button:hover span{max-width:140px}';
    const b = document.createElement('button');
    b.title = 'Picture-in-picture (Ctrl+Shift+P)';
    const svg = document.createElementNS(NS, 'svg');
    svg.setAttribute('viewBox', '0 0 24 24');
    svg.setAttribute('fill', 'none');
    svg.setAttribute('stroke', '#fff');
    svg.setAttribute('stroke-width', '1.75');
    const r1 = document.createElementNS(NS, 'rect');
    for (const [k, val] of Object.entries({ x: 3.5, y: 5, width: 17, height: 14, rx: 2.5 })) r1.setAttribute(k, val);
    const r2 = document.createElementNS(NS, 'rect');
    for (const [k, val] of Object.entries({ x: 12, y: 11.5, width: 5.5, height: 4.5, rx: 1, fill: '#fff' })) r2.setAttribute(k, val);
    svg.append(r1, r2);
    const label = document.createElement('span');
    label.textContent = 'Picture in picture';
    b.append(svg, label);
    for (const ev of ['mousedown', 'pointerdown', 'mouseup', 'pointerup', 'dblclick'])
      b.addEventListener(ev, e => e.stopPropagation());
    b.addEventListener('click', e => {
      e.stopPropagation(); e.preventDefault();
      const v = current;
      hide();
      if (document.pictureInPictureElement) document.exitPictureInPicture().catch(() => {});
      else if (v) enter(v).catch(() => {});
    });
    host.addEventListener('mouseenter', () => { overButton = true; clearTimeout(hideTimer); });
    host.addEventListener('mouseleave', () => { overButton = false; });
    root.append(style, b);
    document.documentElement.appendChild(host);
  }

  function hide() { host.style.display = 'none'; current = null; }

  function show(v) {
    build();
    const r = v.getBoundingClientRect();
    current = v;
    host.style.top = Math.max(0, r.top + 8) + 'px';
    host.style.right = Math.max(0, window.innerWidth - r.right + 8) + 'px';
    host.style.display = 'block';
    clearTimeout(hideTimer);
    hideTimer = setTimeout(() => { if (!overButton) hide(); }, 2500);
  }

  let raf = 0, last = null;
  function onMove(e) {
    if (document.fullscreenElement) { hide(); return; }
    for (const v of document.querySelectorAll('video')) {
      if (!usable(v)) continue;
      const r = v.getBoundingClientRect();
      if (r.width < MIN_W || r.height < MIN_H) continue;
      if (e.clientX >= r.left && e.clientX <= r.right && e.clientY >= r.top && e.clientY <= r.bottom) {
        show(v);
        return;
      }
    }
    if (!overButton && current) { clearTimeout(hideTimer); hideTimer = setTimeout(hide, 300); }
  }
  document.addEventListener('mousemove', e => {
    last = e;
    if (!raf) raf = requestAnimationFrame(() => { raf = 0; onMove(last); });
  }, { capture: true, passive: true });
  window.__lbHoverDebug = () => host.style.display + '@' + host.style.top + ',' + host.style.right;
})();
