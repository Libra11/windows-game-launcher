import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { mountBigScreenInput } from './big-screen-input.js';

export function createBigScreenMode(state, render, report, actions) {
  let previousFullscreen = false, busy = false, disposeInput;
  const win = isTauri() ? getCurrentWindow() : null;
  async function setActive(active) {
    if (busy || active === state.bigScreen) return;
    busy = true;
    try {
      if (win) {
        if (active) previousFullscreen = await win.isFullscreen();
        await win.setFullscreen(active || previousFullscreen);
      }
      if (active && state.selectedId) { state.bigFocusedId = state.selectedId; state.bigCategory = 'all'; }
      if (active && state.page === 'settings') state.page = 'library';
      state.bigScreen = active;
      document.documentElement.classList.toggle('big-screen-active', active);
      disposeInput?.(); disposeInput = null;
      render(); window.scrollTo(0, 0);
      if (active) disposeInput = mountBigScreenInput(document.querySelector('#big-screen-host'), actions);
      else document.querySelector('#big-screen-entry')?.focus();
    } catch (error) { report(`大屏模式切换失败：${String(error)}`, true); }
    finally { busy = false; }
  }
  if (import.meta.hot) import.meta.hot.dispose(() => {
    disposeInput?.();
    if (state.bigScreen && win) win.setFullscreen(previousFullscreen).catch(() => {});
  });
  return { enter: () => setActive(true), exit: () => setActive(false) };
}
