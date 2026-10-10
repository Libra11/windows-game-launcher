// 在样式加载前恢复外观，避免主题或字体在启动时闪动。
function restoreTheme() {
  try {
    document.documentElement.dataset.theme=localStorage.getItem('launcher-theme')==='light'?'light':'dark';
    document.documentElement.dataset.themeColor=localStorage.getItem('launcher-theme-color')||'sage';
  }catch{document.documentElement.dataset.theme='dark';document.documentElement.dataset.themeColor='sage';}
}
restoreTheme();
function restoreFonts() {
  try {
    const saved=JSON.parse(localStorage.getItem('launcher-font-settings')||'null');
    if(typeof saved?.css==='string'&&CSS.supports('font-family',saved.css))document.documentElement.style.setProperty('--app-font-family',saved.css);
  } catch { /* 无法读取偏好时使用样式中的默认字体。 */ }
}
restoreFonts();
window.addEventListener('storage',event=>{
  if(event.key==='launcher-font-settings')restoreFonts();
  if(event.key==='launcher-theme'||event.key==='launcher-theme-color')restoreTheme();
});
