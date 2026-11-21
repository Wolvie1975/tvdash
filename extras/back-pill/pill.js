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

  // Sports alert card (Concept 2 · Action card)
  const acard = document.createElement('div');
  acard.className = 'salert';
  acard.hidden = true;
  acard.innerHTML = `
    <div class="top"><span class="k"></span><span class="why"></span></div>
    <div class="team away"><span class="bdg"></span><span class="nm"></span><span class="rt"></span></div>
    <div class="team home"><span class="bdg"></span><span class="nm"></span><span class="rt"></span></div>
    <div class="meta"></div>
    <div class="div"></div>
    <div class="row">
      <button data-a="primary" class="pri"></button>
      <button data-a="dismiss" class="sec">Dismiss</button>
      <span class="rem"></span>
    </div>`;
  root.appendChild(acard);
  root.querySelector('style').textContent += `
    .salert[hidden] { display: none; }
    .salert { position: fixed; right: 96px; bottom: 120px; z-index: 2147483647; width: 560px; box-sizing: border-box;
      padding: 22px 24px; border-radius: 26px; background: #1C2026; border: 1px solid #2A303A;
      color: #E8EAED; font: 700 18px system-ui, sans-serif; box-shadow: 0 20px 50px rgba(0,0,0,.5); }
    .salert .top { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-bottom: 14px; }
    .salert .k { color: #F5A524; font-size: 16px; font-weight: 900; letter-spacing: 2px; text-transform: uppercase; }
    .salert .k.fin { color: #A3ABB6; }
    .salert .why { padding: 4px 10px; border-radius: 9px; background: rgba(245,165,36,.16); color: #F5A524; font-size: 15px; font-weight: 900; }
    .salert .team { display: flex; align-items: center; gap: 14px; margin-bottom: 12px; }
    .salert .bdg { width: 44px; height: 44px; border-radius: 12px; display: flex; align-items: center; justify-content: center;
      color: #FFF; font-size: 15px; font-weight: 900; flex: 0 0 44px; }
    .salert .nm { font-size: 24px; font-weight: 800; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .salert .rt { font-size: 17px; font-weight: 800; font-variant-numeric: tabular-nums; }
    .salert.fin .rt { font-size: 30px; }
    .salert .team.win .nm, .salert .team.win .rt { font-weight: 900; color: #E8EAED; }
    .salert .team.lose .nm, .salert .team.lose .rt { font-weight: 700; color: #A3ABB6; }
    .salert .meta { color: #A3ABB6; font-size: 17px; font-weight: 700; margin: 2px 0 10px; }
    .salert .div { height: 1px; background: #2A303A; margin-bottom: 14px; }
    .salert .row { display: flex; align-items: center; gap: 12px; }
    .salert button { position: static; height: 56px; padding: 0 22px; border-radius: 28px; border: 0;
      background: #252A31; color: #E8EAED; font: 800 20px system-ui, sans-serif; opacity: 1; pointer-events: auto; cursor: pointer; }
    .salert button.pri { background: #F5A524; color: #1A1200; font-weight: 900; }
    .salert button[hidden] { display: none; }
    .salert button:focus { outline: 4px solid #F5A524; outline-offset: 4px; }
    .salert .rem { margin-left: auto; color: #A3ABB6; font-size: 16px; font-weight: 800; font-variant-numeric: tabular-nums; }`;

  let aKey = null, aRem = 0, aTick = null;
  const remEl = acard.querySelector('.rem');
  const drawRem = () => { remEl.textContent = `Closes in ${Math.max(0, aRem)} s`; };
  const hideAlert = () => {
    acard.hidden = true;
    aKey = null;
    clearInterval(aTick);
    aTick = null;
  };
  const renderSide = (el, side, isFinal) => {
    el.querySelector('.bdg').textContent = side.init || '';
    el.querySelector('.bdg').style.background = side.color || '#252A31';
    el.querySelector('.nm').textContent = side.name || '';
    el.querySelector('.rt').textContent = side.right || '';
    el.classList.toggle('win', isFinal && !!side.winner);
    el.classList.toggle('lose', isFinal && !side.winner && side.right !== '');
  };
  const pollAlert = () => chrome.runtime.sendMessage({ t: 'alert?' }, (a) => {
    if (!a || !card.hidden) { hideAlert(); return; }
    const isNew = aKey !== a.key;
    aKey = a.key;
    aRem = a.closes_in ?? 12;
    const isFinal = a.kind === 'final';
    acard.classList.toggle('fin', isFinal);
    const kEl = acard.querySelector('.k');
    kEl.textContent = a.kicker || '';
    kEl.classList.toggle('fin', isFinal);
    acard.querySelector('.why').textContent = a.why || '';
    renderSide(acard.querySelector('.team.away'), a.away || {}, isFinal);
    renderSide(acard.querySelector('.team.home'), a.home || {}, isFinal);
    acard.querySelector('.meta').textContent = a.meta || '';
    acard.querySelector('.meta').hidden = !a.meta;
    acard.querySelector('.div').hidden = !a.meta;
    const priBtn = acard.querySelector('button.pri');
    const disBtn = acard.querySelector('button.sec');
    if (a.primary) {
      priBtn.hidden = false;
      priBtn.textContent = a.primary;
    } else {
      priBtn.hidden = true;
    }
    drawRem();
    if (isNew) {
      acard.hidden = false;
      (a.primary ? priBtn : disBtn).focus();
    }
    if (!aTick) {
      aTick = setInterval(() => {
        if (aRem > 0) { aRem--; drawRem(); }
        else { hideAlert(); }
      }, 1000);
    }
  });
  acard.addEventListener('click', (e) => {
    const act = e.target.closest('button')?.dataset.a;
    if (!act || !aKey) return;
    chrome.runtime.sendMessage({ t: 'alert', key: aKey, action: act });
    hideAlert();
  });
  acard.addEventListener('keydown', (e) => {
    if (e.key === 'Escape' && aKey) {
      chrome.runtime.sendMessage({ t: 'alert', key: aKey, action: 'dismiss' });
      hideAlert();
    }
  });
  setInterval(pollAlert, 3000);
  pollAlert();
})();
