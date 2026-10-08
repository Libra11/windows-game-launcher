import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import './ui/achievement-overlay.css';
const card = document.querySelector('#card');
await listen('achievement-overlay', ({payload}) => {
  document.querySelector('#name').textContent = payload.name;
  document.querySelector('#game').textContent = payload.game;
  const art = document.querySelector('#art');
  art.hidden = true;
  art.onload = () => { art.hidden = false; };
  art.onerror = () => { art.hidden = true; };
  art.src = /^https?:\/\//.test(payload.icon) ? payload.icon : '';
  // 后台 WebView 可能暂停动画帧；通知可见性不能依赖帧回调。
  card.classList.add('visible');
});
await listen('achievement-overlay-hidden', () => card.classList.remove('visible'));
await invoke('achievement_overlay_ready');
