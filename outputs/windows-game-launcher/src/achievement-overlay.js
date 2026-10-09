import { listen } from '@tauri-apps/api/event';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { icon } from './lib/dom.js';
import { setImageSource, watchNetworkImages } from './lib/network-images.js';
import './ui/achievement-overlay.css';

const card = document.querySelector('#card');
const artHost = document.querySelector('#art-host');
document.querySelector('#unlocked-mark').append(icon('check'));

function showNotice(payload) {
  document.querySelector('#name').textContent = payload.name;
  document.querySelector('#game').textContent = payload.game;
  artHost.classList.remove('has-art');
  artHost.replaceChildren(icon('trophy'));
  if (/^https?:\/\//.test(payload.icon)) {
    const image = new Image();
    image.alt = '';
    image.hidden = true;
    // 每条通知使用独立图片，上一条的迟到回调不会覆盖当前图标。
    image.onload = () => {
      if (image.parentElement !== artHost) return;
      image.hidden = false;
      artHost.classList.add('has-art');
    };
    image.onerror = () => { image.hidden = true; artHost.classList.remove('has-art'); };
    artHost.append(image);
    setImageSource(image, payload.icon);
  }
  // 后台 WebView 可能暂停动画帧；通知可见性不能依赖帧回调。
  card.classList.add('visible');
  card.setAttribute('aria-hidden', 'false');
}

function hideNotice() {
  card.classList.remove('visible');
  card.setAttribute('aria-hidden', 'true');
}

if (isTauri()) {
  await watchNetworkImages();
  await listen('achievement-overlay', ({payload}) => showNotice(payload));
  await listen('achievement-overlay-hidden', hideNotice);
  await invoke('achievement_overlay_ready');
} else if (import.meta.env.DEV) {
  const { previewOverlay } = await import('./lib/achievement-overlay-preview.js');
  previewOverlay(showNotice, hideNotice);
}
