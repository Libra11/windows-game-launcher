import { el, button } from '../lib/dom.js';
import { command, preview } from '../lib/bridge.js';
import './xbox-connection.css';

export function xboxConnection(actions) {
  const root = el('section', 'xbox-connection form');
  const state = el('p', 'form-hint', '成就列表来自 Exophase 公开资料，缓存后可离线查看。本地解锁独立保存。');
  state.setAttribute('aria-live', 'polite');
  let disposed = false;
  const refresh = button('更新公开成就资料', 'secondary', async () => {
    refresh.disabled = true;
    state.textContent = '正在读取公开资料…';
    try {
      const result = await command('xbox_refresh_definitions');
      if (!disposed) state.textContent = result;
      await actions.refresh();
    } catch (error) { if (!disposed) state.textContent = String(error); }
    finally { if (!disposed) refresh.disabled = false; }
  }, 'refresh');
  refresh.disabled = preview;
  root.append(el('h3', '', 'Xbox 成就资料'), el('p', 'form-hint', '无需注册微软应用或登录 Xbox。添加游戏时识别平台，再关联对应的公开成就页面。'), state, refresh);
  return { element:root, dispose() { disposed = true; } };
}
