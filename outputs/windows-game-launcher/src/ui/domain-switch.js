import { el, icon } from '../lib/dom.js';
import './domain-switch.css';

export function domainSwitch(active, onChange) {
  const media = active === 'media';
  const control = el('button', 'domain-switch');
  control.type = 'button';
  control.onclick = async () => {
    try { await onChange?.(media ? 'games' : 'media'); }
    catch { /* 切换失败沿用原模式，错误由现有命令入口提示。 */ }
  };
  control.setAttribute('aria-label', media ? '切换到游戏库' : '切换到影视库');
  control.dataset.domain = active;
  control.title = media ? '当前：影视库；切换到游戏库' : '当前：游戏库；切换到影视库';
  control.append(icon(media ? 'game' : 'film'), el('span', '', media ? '切换游戏' : '切换影视'));
  return control;
}
