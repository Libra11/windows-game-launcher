const storageKey = 'launcher-theme';
const colorStorageKey = 'launcher-theme-color';
const listeners = new Set();
export const themeColors=[['sage','苔绿'],['blue','雾蓝'],['violet','鸢紫'],['rose','玫瑰'],['amber','琥珀'],['teal','青瓷']];

export const getTheme = () => document.documentElement.dataset.theme === 'light' ? 'light' : 'dark';
export const getThemeColor = () => themeColors.some(([key])=>key===document.documentElement.dataset.themeColor)?document.documentElement.dataset.themeColor:'sage';

function notify() {listeners.forEach(listener=>listener(getTheme()));}

export function setTheme(theme) {
  if (theme !== 'light' && theme !== 'dark') return;
  document.documentElement.dataset.theme = theme;
  try { localStorage.setItem(storageKey, theme); } catch { /* 存储不可用时仍允许切换当前界面。 */ }
  notify();
}

export function setThemeColor(color) {
  if(!themeColors.some(([key])=>key===color))return;
  document.documentElement.dataset.themeColor=color;
  try {localStorage.setItem(colorStorageKey,color);}catch{/* 存储不可用时仍应用当前配色。 */}
  notify();
}

window.addEventListener('storage',event=>{if(event.key===storageKey||event.key===colorStorageKey)notify();});

export function onThemeChange(listener) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}
