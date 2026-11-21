(() => {
  const host = document.createElement('div');
  const root = host.attachShadow({ mode: 'closed' });
  root.innerHTML = `
    <style>
      button { position: fixed; top: 54px; right: 96px; z-index: 2147483647; height: 76px;
        padding: 0 16px 0 24px; border-radius: 38px; border: 2px solid rgba(232,234,237,.35);
        background: rgba(11,13,16,.86); color: #E8EAED; font: 600 28px Inter, system-ui, sans-serif;
        display: flex; align-items: center; gap: 16px; transition: opacity .2s; cursor: pointer; }
      button:hover { background: #E8EAED; color: #0B0D10; }
      kbd { border: 2px solid currentColor; border-radius: 10px; padding: 4px 12px; font: 700 18px inherit; }
    </style>
    <button aria-label="Back to tvdash">← Back to tvdash <kbd>F12</kbd></button>`;
  document.documentElement.appendChild(host);
  const btn = root.querySelector('button');
  let t;
  const show = () => {
    btn.style.opacity = '1';
    btn.style.pointerEvents = 'auto';
    clearTimeout(t);
    t = setTimeout(() => {
      btn.style.opacity = '0';
      btn.style.pointerEvents = 'none';
    }, 3000);
  };
  addEventListener('mousemove', show, { passive: true });
  btn.addEventListener('click', () => chrome.runtime.sendMessage('home'));
  show();

  const card = document.createElement('div');
  card.className = 'sleep';
  card.hidden = true;
  card.innerHTML = `
    <div class="k">SLEEP TIMER · <span class="t">1:00</span></div>
    <div class="h">Still watching?</div>
    <div class="b">tvdash will save your place and close.</div>
    <div class="row">
      <button data-a="extend" class="pri">Keep watching · +30 min</button>
      <button data-a="now">Sleep now</button>
      <button data-a="off">Turn off timer</button>
    </div>`;
  root.appendChild(card);
  root.querySelector('style').textContent += `
    .sleep[hidden] { display: none; }
    .sleep { position: fixed; right: 96px; bottom: 54px; z-index: 2147483647; width: 470px; padding: 20px;
      border-radius: 24px; background: #14171C; color: #E8EAED; font: 700 18px system-ui, sans-serif;
      box-shadow: 0 30px 80px rgba(0,0,0,.5); }
    .sleep .k { color: #F5A524; font-weight: 900; font-size: 16px; letter-spacing: 1.5px; font-variant-numeric: tabular-nums; }
    .sleep .h { font-size: 22px; font-weight: 900; margin: 6px 0 4px; }
    .sleep .b { color: #A3ABB6; margin-bottom: 14px; }
    .sleep .row { display: flex; gap: 10px; flex-wrap: wrap; }
    .sleep button { position: static; height: 52px; padding: 0 18px; border-radius: 26px; border: 0;
      background: #252A31; color: #E8EAED; font: 800 18px system-ui, sans-serif; opacity: 1; pointer-events: auto; }
    .sleep button.pri { background: #F5A524; color: #1A1200; }
    .sleep button:hover { filter: brightness(1.1); }
    .sleep button:focus { outline: 4px solid #F5A524; outline-offset: 4px; }`;

  let left = 0, tick = null;
  const timeEl = card.querySelector('.t');
  const draw = () => { timeEl.textContent = `${Math.floor(left / 60)}:${String(left % 60).padStart(2, '0')}`; };
  const poll = () => chrome.runtime.sendMessage({ t: 'sleep?' }, (s) => {
    const warn = s && s.phase === 'warn';
    if (!warn) { card.hidden = true; clearInterval(tick); tick = null; return; }
    if (card.hidden) { card.hidden = false; card.querySelector('.pri').focus(); }
    left = s.left; draw();
    if (!tick) tick = setInterval(() => { if (left > 0) { left--; draw(); } }, 1000);
  });
  card.addEventListener('click', (e) => {
    const a = e.target.closest('button')?.dataset.a;
    if (!a) return;
    chrome.runtime.sendMessage({ t: 'sleep', action: a });
    card.hidden = true;
    clearInterval(tick);
    tick = null;
  });
  setInterval(poll, 5000);
  poll();
})();
