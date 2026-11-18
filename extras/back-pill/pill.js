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
})();
