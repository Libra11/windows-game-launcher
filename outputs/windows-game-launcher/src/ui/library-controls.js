import { el, button } from '../lib/dom.js';
import { customSelect } from './custom-select.js';
const sorts = [['az','名称 A → Z'],['za','名称 Z → A'],['recent','最近游玩优先'],['time','累计时长最多']];

export function libraryControls(state, actions, big = false) {
  const controls = el('div', 'library-filter-controls');
  const installed = button('只看已安装', state.installedOnly ? 'installed-filter active' : 'installed-filter', () => actions.installed(!state.installedOnly), 'check');
  installed.setAttribute('aria-pressed', String(state.installedOnly)); installed.dataset.focusKey = 'installed-filter';
  installed.title = 'Steam 已完成安装的游戏，以及启动文件存在的本地游戏';
  const sort = big ? button(sorts.find(([value])=>value === state.sort)[1], 'sort-select', () => {
    const index = sorts.findIndex(([value])=>value === state.sort); actions.sort(sorts[(index + 1) % sorts.length][0]);
  }, 'list') : customSelect(sorts,state.filter==='recent'?'recent':state.sort,'游戏排序','sort-select');
  sort.setAttribute('aria-label', big ? '切换游戏排序' : '游戏排序'); sort.dataset.focusKey = 'library-sort';
  if (!big) {
    sort.value = state.filter === 'recent' ? 'recent' : state.sort; sort.disabled = state.filter === 'recent';
    sort.onchange = () => actions.sort(sort.value);
  }
  controls.append(installed, sort); return controls;
}
