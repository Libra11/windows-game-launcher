// 在样式加载前恢复外观，避免主题或字体在启动时闪动。
try { document.documentElement.dataset.theme = localStorage.getItem('launcher-theme') === 'light' ? 'light' : 'dark'; }
catch { document.documentElement.dataset.theme = 'dark'; }
function restoreFonts() {
  try {
    const saved=JSON.parse(localStorage.getItem('launcher-font-settings')||'null');
    if(typeof saved?.css==='string'&&CSS.supports('font-family',saved.css))document.documentElement.style.setProperty('--app-font-family',saved.css);
  } catch { /* 无法读取偏好时使用样式中的默认字体。 */ }
}
restoreFonts();
window.addEventListener('storage',event=>{if(event.key==='launcher-font-settings')restoreFonts();});
