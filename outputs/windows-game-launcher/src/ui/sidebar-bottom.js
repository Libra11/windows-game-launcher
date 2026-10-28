import { el } from '../lib/dom.js';

export function sidebarBottom({ preview = false } = {}) {
  const bottom = el('div', 'sidebar-bottom');
  const footer = el('div', 'sidebar-footer');
  const version = el('div', 'app-version');
  version.append(el('span', 'status-dot'), el('span', '', preview ? '设计预览 · 示例数据' : '游迹'));
  footer.append(version);
  bottom.append(footer);
  return bottom;
}
