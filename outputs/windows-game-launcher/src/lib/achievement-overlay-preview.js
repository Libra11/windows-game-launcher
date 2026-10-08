import { button, el } from './dom.js';
import '../ui/achievement-overlay-preview.css';

export function previewOverlay(showNotice, hideNotice) {
  document.body.classList.add('overlay-preview');
  const intro = el('header', 'preview-intro');
  intro.append(el('span', 'preview-kicker', '游迹 / 成就通知'), el('h2', '', '每一次突破，都值得被看见。'), el('p', '', '开发预览 · 以下均为示例通知，不会保存解锁记录。'));
  const stage = el('main', 'preview-stage');
  stage.append(document.querySelector('#card'));
  const controls = el('nav', 'preview-controls');
  controls.setAttribute('aria-label', '通知预览');
  const example = { name:'开胃菜', game:'GRIME II · 尘埃异变 2', icon:'' };
  for (const [label, notice] of [
    ['默认图标', example],
    ['示例图片', {...example, name:'新的旅程', icon:`${location.origin}/app-icon.png`}],
    ['长名称', {...example, name:'跨越漫长旅途，终于抵达世界的另一端，收集全部失落的碎片并完成最后的试炼', game:'一款拥有很长很长名称的游戏：完整典藏特别纪念版 · Ultimate Collector’s Edition'}],
    ['图片失效', {...example, icon:`${location.origin}/missing-achievement-icon.png`}],
  ]) controls.append(button(label, '', () => showNotice(notice)));
  controls.append(button('连续通知', '', () => {
    showNotice({...example, name:'上一条通知', icon:`${location.origin}/app-icon.png`});
    showNotice({...example, name:'下一段冒险', game:'连续通知 · 当前条目', icon:''});
  }));
  controls.append(button('隐藏', '', hideNotice));
  const backdrop = button('切换背景', '', () => document.body.classList.toggle('preview-light'));
  controls.append(backdrop);
  document.body.append(intro, stage, controls);
  showNotice(example);
}
