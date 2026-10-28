import { el, button } from '../lib/dom.js';

export function sidebarBottom({ settings, preview = false } = {}) {
  const bottom = el('div', 'sidebar-bottom');
  const control = button('设置', 'nav-item', settings, 'settings');
  control.dataset.page = 'settings';
  control.title = '设置';
  const footer = el('div', 'sidebar-footer');
  const version = el('div', 'app-version');
  version.append(el('span', 'status-dot'), el('span', '', preview ? '设计预览 · 示例数据' : '游迹'));
  footer.append(version);
  bottom.append(control, footer);
  return bottom;
}
