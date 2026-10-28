import { el, button } from '../lib/dom.js';
import { sidebarBrand } from './sidebar-brand.js';
import './domain-switch.css';

export function domainSwitch(active, onChange) {
  const group = el('nav', 'domain-switch');
  group.setAttribute('aria-label', '切换资料库');
  for (const [domain, label, glyph] of [['games', '游戏', 'game'], ['media', '影视', 'film']]) {
    const selected = active === domain;
    const control = button('', 'domain-switch-option', () => onChange?.(domain), glyph);
    control.dataset.domainOption = domain;
    control.setAttribute('aria-label', selected ? `当前：${label}库` : `切换到${label}库`);
    control.setAttribute('aria-pressed', String(selected));
    if (selected) control.setAttribute('aria-current', 'page');
    control.title = selected ? `当前：${label}库` : `切换到${label}库`;
    group.append(control);
  }
  return group;
}

export function sidebarRail(domain, { back, onDomainChange, settings } = {}) {
  const rail = el('div', 'sidebar-rail');
  const control = button('', 'nav-item rail-settings', settings, 'settings');
  control.dataset.page = 'settings';
  control.setAttribute('aria-label', '设置');
  control.title = '设置';
  rail.append(sidebarBrand(domain, { back }), domainSwitch(domain, onDomainChange), control);
  return rail;
}
