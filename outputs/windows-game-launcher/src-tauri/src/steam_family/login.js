(() => {
  if (window.location.origin !== 'https://store.steampowered.com') return;
  if (window.__youjiFamilyLoginStarted) return;
  window.__youjiFamilyLoginStarted = true;
  const state = __LOGIN_NONCE_JSON__;
  let sent = false, busy = false, attempts = 0, failures = 0;
  const finish = (key, value) => {
    if (sent) return;
    sent = true;
    clearInterval(timer);
    // 原生导航回调取消此地址，不向任何网站发送登录凭证。
    window.location.href = 'youji-steam-family://complete?state=' + encodeURIComponent(state)
      + '&' + key + '=' + encodeURIComponent(value);
  };
  const poll = async () => {
    if (sent || busy) return;
    if (++attempts > 300) { finish('error', 'Steam 登录等待已超时，请重新打开登录窗口'); return; }
    busy = true;
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 8000);
    try {
      const response = await fetch('/pointssummary/ajaxgetasyncconfig', {
        credentials: 'same-origin', cache: 'no-store', signal: controller.signal,
      });
      if (!response.ok) return;
      const data = await response.json();
      failures = 0;
      const token = data?.data?.webapi_token;
      if (data?.success === 1 && typeof token === 'string' && token) finish('token', token);
    } catch {
      if (++failures >= 3) finish('error', '无法读取 Steam 登录授权，请检查网络后重试');
    } finally {
      clearTimeout(timeout); busy = false;
    }
  };
  const timer = setInterval(poll, 2000);
  window.addEventListener('pagehide', () => { sent = true; clearInterval(timer); }, {once:true});
  poll();
})();
