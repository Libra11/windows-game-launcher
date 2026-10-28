import { el, appIcon } from '../lib/dom.js';
import { domainSwitch } from './domain-switch.js';

export function sidebarBrand(domain, { back, onDomainChange }) {
  const header = el('div', 'sidebar-brand');
  const brand = el('a', 'brand');
  brand.href = '#';
  brand.setAttribute('aria-label', `游迹，返回${domain === 'media' ? '影视' : '游戏'}库`);
  brand.append(appIcon('brand-symbol'), el('span', 'brand-name', '游迹'));
  brand.onclick = event => { event.preventDefault(); back(); };
  header.append(brand, domainSwitch(domain, onDomainChange));
  return header;
}
