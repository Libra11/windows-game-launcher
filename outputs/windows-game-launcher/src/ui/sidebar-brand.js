import { el, appIcon } from '../lib/dom.js';

export function sidebarBrand(domain, { back }) {
  const brand = el('a', 'rail-brand');
  brand.href = '#';
  brand.setAttribute('aria-label', `游迹，返回${domain === 'media' ? '影视' : '游戏'}库`);
  brand.title = '游迹';
  brand.append(appIcon('brand-symbol'));
  brand.onclick = event => { event.preventDefault(); back(); };
  return brand;
}
