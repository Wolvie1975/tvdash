const BASE = 'http://127.0.0.1:47800';
chrome.runtime.onMessage.addListener((msg, _sender, reply) => {
  if (msg === 'home') {
    fetch(BASE + '/home', { method: 'POST' }).catch(() => {});
    return;
  }
  if (msg?.t === 'sleep?') {
    fetch(BASE + '/sleep').then((r) => r.json()).then(reply, () => reply(null));
    return true;
  }
  if (msg?.t === 'sleep') {
    fetch(BASE + '/sleep', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ action: msg.action }),
    }).catch(() => {});
  }
  if (msg?.t === 'alert?') {
    fetch(BASE + '/alerts').then((r) => r.json()).then(reply, () => reply(null));
    return true;
  }
  if (msg?.t === 'alert') {
    fetch(BASE + '/alerts', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ key: msg.key, action: msg.action }),
    }).catch(() => {});
  }
});
