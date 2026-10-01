const storageKey = 'launcher-theme';
const listeners = new Set();

export const getTheme = () => document.documentElement.dataset.theme === 'light' ? 'light' : 'dark';

export function setTheme(theme) {
  if (theme !== 'light' && theme !== 'dark') return;
  document.documentElement.dataset.theme = theme;
  try { localStorage.setItem(storageKey, theme); } catch { /* 存储不可用时仍允许切换当前界面。 */ }
  listeners.forEach(listener => listener(theme));
}

export function onThemeChange(listener) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}
