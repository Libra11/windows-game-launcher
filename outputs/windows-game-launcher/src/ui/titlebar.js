import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { el, appIcon } from '../lib/dom.js';
import './titlebar.css';

const symbols = {
  minimize: '<path d="M2 6.5h8"/>',
  maximize: '<rect x="2.5" y="2.5" width="7" height="7"/>',
  restore: '<path d="M4.5 2.5h5v5m-7-3h5v5h-5z"/>',
  close: '<path d="m2.5 2.5 7 7m-7 0 7-7"/>',
};
function paint(control, symbol, label) {
  control.innerHTML = `<svg viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1" aria-hidden="true">${symbols[symbol]}</svg>`;
  control.setAttribute('aria-label', label);
  control.title = label;
}
export function mountTitlebar(report) {
  const desktop = isTauri();
  const win = desktop ? getCurrentWindow() : null;
  const bar = el('header', 'window-titlebar');
  const drag = el('div', 'window-drag-region');
  // 使用 Tauri 原生拖动区域，同时处理 Windows 和 macOS 的双击最大化。
  drag.setAttribute('data-tauri-drag-region', 'deep');
  drag.append(appIcon('window-brand'), el('span', 'window-title', '游迹'));
  const controls = el('div', 'window-controls');
  controls.setAttribute('data-tauri-drag-region', 'false');
  const add = (symbol, label, operation) => {
    const control = el('button', `window-control window-${symbol}`);
    control.type = 'button'; paint(control, symbol, label);
    control.disabled = !desktop;
    if (!desktop) control.title = `${label}（桌面应用中可用）`;
    control.onclick = async () => {
      try { await operation(); }
      catch (error) { report(`窗口操作失败：${String(error)}`, true); }
    };
    controls.append(control); return control;
  };
  add('minimize', '最小化', () => win.minimize());
  const maximize = add('maximize', '最大化', async () => {
    if (await win.isFullscreen()) await win.setFullscreen(false);
    else await win.toggleMaximize();
    await update();
  });
  add('close', '关闭窗口', () => win.close());
  bar.append(drag, controls); document.body.prepend(bar);
  let alive = true;
  const listeners = [];
  async function update() {
    if (!win || !alive) return;
    const [maximized,fullscreen] = await Promise.all([win.isMaximized(),win.isFullscreen()]);
    const expanded = maximized || fullscreen;
    if (alive) paint(maximize, expanded ? 'restore' : 'maximize', expanded ? '还原窗口' : '最大化');
  }
  if (win) {
    update().catch(error => report(String(error), true));
    win.onResized(() => update().catch(() => {})).then(unlisten => {
      if (alive) listeners.push(unlisten); else unlisten();
    }).catch(error => report(String(error), true));
  }
  const dispose = () => { alive = false; listeners.forEach(unlisten => unlisten()); bar.remove(); };
  if (import.meta.hot) import.meta.hot.dispose(dispose);
  return dispose;
}
