import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { createTrayMenu } from './lib/tray-menu-view.js';
import './ui/tray-menu.css';

const root = document.querySelector('#menu');
const desktop = isTauri();
const activate = key => desktop ? invoke('tray_menu_activate', { key }) : Promise.resolve();
const dismiss = () => desktop ? invoke('tray_menu_dismiss') : Promise.resolve();
const view = createTrayMenu(root, { activate, dismiss });

document.addEventListener('contextmenu', event => event.preventDefault());

if (desktop) {
  await listen('tray-menu-open', ({ payload }) => view.render(payload, { focus: true }));
  await listen('tray-menu-changed', ({ payload }) => view.render(payload));
  view.render(await invoke('tray_menu_snapshot'));
} else if (import.meta.env.DEV) {
  view.render({
    games: [
      { key: 'preview-game-1', title: '空洞骑士' },
      { key: 'preview-game-2', title: '尘埃异变 GRIME' },
      { key: 'preview-game-3', title: '一款名称特别长的游戏 · 年度完整版' },
    ],
    media: [
      { key: 'preview-media-1', title: '庆余年 · 第 2 季' },
      { key: 'preview-media-2', title: '世界奇妙物语 2012 年春之特别篇' },
    ],
  }, { focus: true });
}
