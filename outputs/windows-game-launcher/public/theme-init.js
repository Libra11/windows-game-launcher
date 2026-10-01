// 在样式加载前恢复主题，使用本地脚本以符合桌面端 CSP。
try { document.documentElement.dataset.theme = localStorage.getItem('launcher-theme') === 'light' ? 'light' : 'dark'; }
catch { document.documentElement.dataset.theme = 'dark'; }
