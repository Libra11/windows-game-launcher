import { el, button, icon } from '../lib/dom.js';
import { getTheme, setTheme, onThemeChange } from '../lib/theme.js';

export function themePicker() {
  const root = el('section', 'theme-setting');
  root.append(el('h3', '', '外观'), el('p', '', '选择喜欢的界面风格，自动保存在本机。'));
  const group = el('div', 'theme-options');
  group.setAttribute('role', 'group'); group.setAttribute('aria-label', '界面主题');
  for (const [value, label, glyph] of [['light', '亮色模式', 'sun'], ['dark', '暗色模式', 'moon']]) {
    const control = button(label, 'theme-option', () => setTheme(value), glyph);
    control.dataset.theme = value; group.append(control);
  }
  const update = () => group.querySelectorAll('button').forEach(control => {
    const active = control.dataset.theme === getTheme();
    control.classList.toggle('active', active);
    control.setAttribute('aria-pressed', String(active));
  });
  update(); root.append(group);
  return { element: root, dispose: onThemeChange(update) };
}
