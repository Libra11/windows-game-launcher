import { el, button, icon } from '../lib/dom.js';
import { getTheme, setTheme, getThemeColor, setThemeColor, themeColors, onThemeChange } from '../lib/theme.js';

export function themePicker() {
  const root = el('section', 'theme-setting');
  root.append(el('h3', '', '显示模式'), el('p', '', '亮暗模式与主题色可以独立选择，更改自动保存在本机。'));
  const group = el('div', 'theme-options');
  group.setAttribute('role', 'group'); group.setAttribute('aria-label', '界面主题');
  for (const [value, label, glyph] of [['light', '亮色模式', 'sun'], ['dark', '暗色模式', 'moon']]) {
    const control = button(label, 'theme-option', () => setTheme(value), glyph);
    control.dataset.theme = value; group.append(control);
  }
  const colors=el('section','theme-color-group'),choices=el('div','theme-color-options');
  choices.setAttribute('role','group');choices.setAttribute('aria-label','主题色');
  for(const [value,label] of themeColors){
    const control=button('','theme-color-option',()=>setThemeColor(value));control.dataset.color=value;
    const swatch=el('span','theme-color-swatch');swatch.setAttribute('aria-hidden','true');
    control.append(swatch,el('span','',label),icon('check'));control.setAttribute('aria-label',label+'主题色');choices.append(control);
  }
  colors.append(el('h3','','主题色'),choices);
  const update = () => {
    group.querySelectorAll('button').forEach(control=>{
      const active=control.dataset.theme===getTheme();control.classList.toggle('active',active);control.setAttribute('aria-pressed',String(active));
    });
    choices.querySelectorAll('button').forEach(control=>{
      const active=control.dataset.color===getThemeColor();control.classList.toggle('active',active);control.setAttribute('aria-pressed',String(active));
    });
  };
  update(); root.append(group,colors);
  return { element: root, dispose: onThemeChange(update) };
}
