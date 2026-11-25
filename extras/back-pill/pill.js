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
    <button aria-label="Back to tvdash">← Back to tvdash <kbd>F12</kbd></button>
    <button class="sw-btn" aria-label="Quick switch" hidden>Switch <kbd>F9</kbd></button>`;
  document.documentElement.appendChild(host);
  const btn = root.querySelector('button');
  const swBtn = root.querySelector('.sw-btn');
  let t;
  const show = () => {
    btn.style.opacity = '1';
    btn.style.pointerEvents = 'auto';
    if (!swBtn.hidden) {
      swBtn.style.opacity = '1';
      swBtn.style.pointerEvents = 'auto';
    }
    clearTimeout(t);
    t = setTimeout(() => {
      btn.style.opacity = '0';
      btn.style.pointerEvents = 'none';
      swBtn.style.opacity = '0';
      swBtn.style.pointerEvents = 'none';
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
    if (!a || !card.hidden || !wcard.hidden || !wtake.hidden) { hideAlert(); return; }
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

  // Quick switch overlay
  const qs = document.createElement('div');
  qs.className = 'qs';
  qs.hidden = true;
  qs.innerHTML = `
    <div class="qs-box">
      <div class="qs-k">QUICK SWITCH</div>
      <div class="qs-sub" hidden></div>
      <div class="qs-row"></div>
      <button class="qs-home" type="button">← tvdash Home</button>
      <div class="qs-msg" hidden></div>
    </div>`;
  root.appendChild(qs);
  root.querySelector('style').textContent += `
    button.sw-btn { right: 420px; }
    button.sw-btn[hidden] { display: none; }
    .qs[hidden] { display: none; }
    .qs { position: fixed; inset: 0; z-index: 2147483647; background: rgba(0,0,0,.72);
      display: flex; align-items: center; justify-content: center; font-family: Inter, system-ui, sans-serif; }
    .qs-box { display: flex; flex-direction: column; align-items: center; gap: 22px; padding: 36px; }
    .qs-k { color: #F5A524; font-size: 18px; font-weight: 900; letter-spacing: 2.5px; }
    .qs-sub { color: #C9CDD3; font-size: 22px; font-weight: 600; margin-top: -12px; }
    .qs-sub[hidden] { display: none; }
    .qs-row { display: flex; gap: 28px; justify-content: center; flex-wrap: wrap; }
    .qs-tile { position: static; width: 360px; height: 202px; padding: 20px; border-radius: 24px; border: 0;
      display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 14px;
      opacity: 1; pointer-events: auto; cursor: pointer; box-sizing: border-box;
      box-shadow: 0 20px 50px rgba(0,0,0,.5); transition: transform .12s; }
    .qs-tile:hover { transform: scale(1.02); }
    .qs-tile:focus, .qs-home:focus { outline: 6px solid #F5A524; outline-offset: 6px; }
    .qs-logo { display: flex; align-items: center; justify-content: center; width: 100%; height: 101px; }
    .qs-logo svg { max-width: 60%; max-height: 100%; width: auto; height: auto; display: block; }
    .qs-name { font-size: 24px; font-weight: 800; }
    .qs-name.big { font-size: 36px; }
    .qs-home { position: static; height: 60px; padding: 0 28px; border-radius: 30px; border: 2px solid rgba(232,234,237,.35);
      background: rgba(11,13,16,.86); color: #E8EAED; font: 700 22px Inter, system-ui, sans-serif;
      opacity: 1; pointer-events: auto; cursor: pointer; margin-top: 8px; }
    .qs-home:hover { background: #E8EAED; color: #0B0D10; }
    .qs-msg { color: #E8EAED; font-size: 28px; font-weight: 700; }
    .qs-msg[hidden] { display: none; }
    .qs.busy .qs-row, .qs.busy .qs-home { opacity: .35; pointer-events: none; }`;

  const qsSub = qs.querySelector('.qs-sub');
  const qsRow = qs.querySelector('.qs-row');
  const qsHome = qs.querySelector('.qs-home');
  const qsMsg = qs.querySelector('.qs-msg');
  let qState = null;
  let qTiles = [];
  let qFocus = 0;

  const qsItems = () => [...qsRow.querySelectorAll('.qs-tile'), qsHome];
  const focusQs = (idx) => {
    const items = qsItems();
    if (!items.length) return;
    qFocus = Math.max(0, Math.min(idx, items.length - 1));
    items[qFocus].focus();
  };

  const renderQuick = (st) => {
    qState = st;
    const all = st?.tiles || [];
    qTiles = all.filter((tile) => tile.id !== st?.current);
    swBtn.hidden = qTiles.length === 0;
    if (!swBtn.hidden && btn.style.opacity !== '0') {
      swBtn.style.opacity = '1';
      swBtn.style.pointerEvents = 'auto';
    }
    const kbd = swBtn.querySelector('kbd');
    if (kbd && st?.key) kbd.textContent = st.key;

    const curTile = all.find((tile) => tile.id === st?.current);
    if (curTile) {
      qsSub.textContent = `Now watching · ${curTile.name}`;
      qsSub.hidden = false;
    } else {
      qsSub.hidden = true;
    }

    qsRow.innerHTML = '';
    for (const tile of qTiles) {
      const b = document.createElement('button');
      b.type = 'button';
      b.className = 'qs-tile';
      b.style.background = tile.fill || '#1B1F26';
      b.style.color = tile.ink || '#E8EAED';
      let svgOk = false;
      if (tile.logo_svg) {
        try {
          const doc = new DOMParser().parseFromString(tile.logo_svg, 'image/svg+xml');
          if (!doc.querySelector('parsererror') && doc.documentElement?.tagName.toLowerCase() === 'svg') {
            const wrap = document.createElement('div');
            wrap.className = 'qs-logo';
            wrap.appendChild(document.importNode(doc.documentElement, true));
            b.appendChild(wrap);
            svgOk = true;
          }
        } catch {}
      }
      const nm = document.createElement('div');
      nm.className = svgOk ? 'qs-name' : 'qs-name big';
      nm.textContent = tile.name;
      nm.style.color = tile.ink || '#E8EAED';
      b.appendChild(nm);
      b.addEventListener('click', () => chooseQuick(tile));
      qsRow.appendChild(b);
    }
  };

  const chooseQuick = (tile) => {
    document.querySelectorAll('video').forEach((v) => v.pause());
    qs.classList.add('busy');
    qsMsg.textContent = `Opening ${tile.name}…`;
    qsMsg.hidden = false;
    chrome.runtime.sendMessage({ t: 'quick', id: tile.id });
  };

  const closeQuick = () => {
    qs.hidden = true;
    qs.classList.remove('busy');
    qsMsg.hidden = true;
  };

  const openQuick = () => {
    if (!card.hidden || !acard.hidden) return;
    chrome.runtime.sendMessage({ t: 'quick?' }, (st) => {
      if (st) renderQuick(st);
      if (!qTiles.length) return;
      qs.classList.remove('busy');
      qsMsg.hidden = true;
      qs.hidden = false;
      focusQs(0);
    });
  };

  swBtn.addEventListener('click', openQuick);
  qsHome.addEventListener('click', () => {
    document.querySelectorAll('video').forEach((v) => v.pause());
    chrome.runtime.sendMessage('home');
  });
  qs.addEventListener('click', (e) => {
    if (e.target === qs) closeQuick();
  });

  window.addEventListener('keydown', (e) => {
    const keyName = qState?.key || 'F9';
    if (qs.hidden) {
      if (e.key === keyName && card.hidden && acard.hidden && qTiles.length > 0) {
        e.preventDefault();
        e.stopImmediatePropagation();
        openQuick();
      }
      return;
    }
    e.preventDefault();
    e.stopImmediatePropagation();
    const items = qsItems();
    const nTiles = qTiles.length;
    if (e.key === 'ArrowLeft') {
      focusQs((qFocus - 1 + items.length) % items.length);
    } else if (e.key === 'ArrowRight') {
      focusQs((qFocus + 1) % items.length);
    } else if (e.key === 'ArrowDown') {
      if (qFocus < nTiles) focusQs(nTiles);
    } else if (e.key === 'ArrowUp') {
      if (qFocus >= nTiles && nTiles > 0) focusQs(0);
    } else if (e.key === 'Enter') {
      items[qFocus]?.click();
    } else if (e.key === 'Escape' || e.key === 'Backspace' || e.key === 'BrowserBack') {
      closeQuick();
    }
  }, true);

  chrome.runtime.sendMessage({ t: 'quick?' }, (st) => {
    if (st) renderQuick(st);
  });

  // Weather alert takeover & card over streaming apps
  const wtake = document.createElement('div');
  wtake.className = 'wtake';
  wtake.hidden = true;
  wtake.innerHTML = `
    <div class="wt-bar"></div>
    <div class="wt-wrap">
      <div class="wt-main">
        <div class="wt-top"><span class="wt-k">NATIONAL WEATHER SERVICE · WARNING</span><span class="wt-chip">Playback paused</span></div>
        <div class="wt-ev"></div>
        <div class="wt-area"></div>
        <div class="wt-inst"></div>
        <div class="wt-iss"></div>
      </div>
      <div class="wt-exp"><div class="wt-ek">EXPIRES IN</div><div class="wt-en"></div><div class="wt-eu">min</div></div>
    </div>
    <div class="wt-row">
      <button data-wa="got-it" class="pri">Got it</button>
      <button data-wa="resume" class="sec">Resume playback</button>
    </div>`;
  root.appendChild(wtake);

  const wcard = document.createElement('div');
  wcard.className = 'salert wcard';
  wcard.hidden = true;
  wcard.innerHTML = `
    <div class="top"><span class="k wk">WEATHER · WATCH</span><span class="why wchip">Your location</span></div>
    <div class="wc-t"></div>
    <div class="wc-s"></div>
    <div class="div"></div>
    <div class="row">
      <button data-wa="dismiss" class="pri wpri">Dismiss</button>
      <button data-wa="details" class="sec">Details</button>
      <span class="rem wrem"></span>
    </div>`;
  root.appendChild(wcard);

  root.querySelector('style').textContent += `
    .wtake[hidden], .wcard[hidden] { display: none; }
    .wtake { position: fixed; inset: 0; z-index: 2147483647; background: #140A0A; color: #FFF;
      font-family: Inter, system-ui, sans-serif; display: flex; flex-direction: column; justify-content: space-between; padding: 96px 110px; box-sizing: border-box; }
    .wt-bar { position: fixed; top: 0; left: 0; right: 0; height: 14px; background: #F0605D; }
    .wt-wrap { display: flex; justify-content: space-between; gap: 48px; align-items: flex-start; }
    .wt-main { flex: 1; display: flex; flex-direction: column; gap: 18px; }
    .wt-top { display: flex; align-items: center; gap: 16px; }
    .wt-k { color: #F0605D; font-size: 22px; font-weight: 800; letter-spacing: 2px; }
    .wt-chip { padding: 6px 16px; border-radius: 18px; background: rgba(255,255,255,.1); font-size: 16px; font-weight: 600; color: #E8EAED; }
    .wt-ev { font-size: 96px; font-weight: 800; line-height: 1.02; }
    .wt-area { font-size: 30px; font-weight: 600; color: #F8B4B2; }
    .wt-inst { font-size: 32px; color: #E8EAED; line-height: 1.3; max-height: 128px; overflow: hidden; }
    .wt-iss { font-size: 19px; color: #9CA3AE; }
    .wt-exp { width: 220px; padding: 24px 16px; border-radius: 20px; background: #231214; border: 1px solid rgba(240,96,93,.35); text-align: center; }
    .wt-ek { font-size: 15px; font-weight: 800; color: #F8B4B2; letter-spacing: 1.5px; }
    .wt-en { font-size: 60px; font-weight: 800; margin: 6px 0; }
    .wt-eu { font-size: 18px; color: #9CA3AE; font-weight: 600; }
    .wt-row { display: flex; gap: 18px; align-items: center; }
    .wt-row button { position: static; height: 64px; padding: 0 32px; border-radius: 32px; border: 0; font: 800 22px Inter, system-ui, sans-serif; opacity: 1; pointer-events: auto; cursor: pointer; }
    .wt-row button.pri { background: #FFF; color: #140A0A; }
    .wt-row button.sec { background: #2A1618; color: #FFF; border: 1px solid rgba(240,96,93,.4); }
    .wt-row button:focus { outline: 4px solid #F0605D; outline-offset: 4px; }
    .wcard .wk { color: #E5A84B; }
    .wcard .wchip { background: rgba(229,168,75,.16); color: #E5A84B; }
    .wcard .wc-t { font-size: 28px; font-weight: 800; margin-bottom: 6px; }
    .wcard .wc-s { font-size: 17px; color: #A3ABB6; margin-bottom: 12px; }
    .wcard .wpri { background: #E5A84B; color: #1A1200; }`;

  let wId = null, wLevel = 'none', wRem = 0, wTick = null, pausedVideos = [];
  const wRemEl = wcard.querySelector('.wrem');
  const drawWRem = () => { wRemEl.textContent = `Closes in ${Math.max(0, wRem)} s`; };
  const pauseVideos = () => {
    pausedVideos = [];
    document.querySelectorAll('video').forEach((v) => {
      if (!v.paused) { v.pause(); pausedVideos.push(v); }
    });
  };
  const resumeVideos = () => {
    pausedVideos.forEach((v) => v.play().catch(() => {}));
    pausedVideos = [];
  };
  const hideWeather = () => {
    wtake.hidden = true;
    wcard.hidden = true;
    wId = null;
    wLevel = 'none';
    clearInterval(wTick);
    wTick = null;
  };
  const pollWeather = () => chrome.runtime.sendMessage({ t: 'weather?' }, (w) => {
    if (!w || w.level === 'none') { hideWeather(); return; }
    if (w.level === 'takeover') {
      hideAlert();
      wcard.hidden = true;
      const isNew = wLevel !== 'takeover' || wId !== w.id;
      wId = w.id;
      wLevel = 'takeover';
      wtake.querySelector('.wt-ev').textContent = w.event || '';
      wtake.querySelector('.wt-area').textContent = w.area_until || '';
      wtake.querySelector('.wt-inst').textContent = w.instruction || '';
      wtake.querySelector('.wt-iss').textContent = w.issued_line || '';
      wtake.querySelector('.wt-en').textContent = w.expires_in || '—';
      if (isNew) {
        pauseVideos();
        wtake.hidden = false;
        wtake.querySelector('.pri').focus();
      }
      return;
    }
    if (w.level === 'card') {
      hideAlert();
      wtake.hidden = true;
      const isNew = wLevel !== 'card' || wId !== w.id;
      wId = w.id;
      wLevel = 'card';
      wRem = w.closes_in ?? 12;
      wcard.querySelector('.wc-t').textContent = w.event || '';
      wcard.querySelector('.wc-s').textContent = w.area_until || '';
      drawWRem();
      if (isNew) {
        wcard.hidden = false;
        wcard.querySelector('.wpri').focus();
      }
      if (!wTick) {
        wTick = setInterval(() => {
          if (wRem > 0) { wRem--; drawWRem(); }
          else { hideWeather(); }
        }, 1000);
      }
    }
  });
  wtake.addEventListener('click', (e) => {
    const a = e.target.closest('button')?.dataset.wa;
    if (!a || !wId) return;
    chrome.runtime.sendMessage({ t: 'weather', id: wId, action: a });
    if (a === 'resume') resumeVideos();
    hideWeather();
  });
  wcard.addEventListener('click', (e) => {
    const a = e.target.closest('button')?.dataset.wa;
    if (!a || !wId) return;
    chrome.runtime.sendMessage({ t: 'weather', id: wId, action: a });
    hideWeather();
  });
  window.addEventListener('keydown', (e) => {
    if (wtake.hidden) return;
    e.preventDefault();
    e.stopImmediatePropagation();
    const btns = [...wtake.querySelectorAll('button')];
    const cur = btns.indexOf(root.activeElement);
    if (e.key === 'ArrowLeft') btns[0]?.focus();
    else if (e.key === 'ArrowRight') btns[1]?.focus();
    else if (e.key === 'Enter') btns[Math.max(0, cur)]?.click();
    else if (e.key === 'Escape' || e.key === 'Backspace' || e.key === 'BrowserBack') btns[0]?.click();
  }, true);
  setInterval(pollWeather, 3000);
  pollWeather();
})();
