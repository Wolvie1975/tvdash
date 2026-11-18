chrome.runtime.onMessage.addListener((msg) => {
  if (msg === 'home') {
    fetch('http://127.0.0.1:47800/home', { method: 'POST' }).catch(() => {});
  }
});
