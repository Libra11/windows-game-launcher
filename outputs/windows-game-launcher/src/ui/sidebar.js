import { el, button, icon, appIcon } from '../lib/dom.js';
import { categories, platformCategories } from '../lib/library-query.js';
import { themeToggle } from './theme-controls.js';
import './sidebar.css';

export function mountSidebar(host, actions) {
  const brand = el('a', 'brand');
  brand.href = '#'; brand.setAttribute('aria-label', '游迹，返回游戏库');
  const symbol = appIcon('brand-symbol');
  brand.append(symbol, el('span', 'brand-name', '游迹'));
  brand.onclick = event => { event.preventDefault(); actions.back(); };

  const caption = el('div', 'nav-caption', '游戏库');
  const navigation = el('nav'); navigation.id = 'navigation'; navigation.setAttribute('aria-label', '游戏库分类');
  for (const [label, values] of [['收藏', ['all', 'recent', 'favorites']], ['平台', platformCategories.map(([value]) => value)]]) {
    const group = el('div', 'nav-group'); group.setAttribute('role', 'group'); group.setAttribute('aria-label', label);
    if (label === '平台') group.append(el('div', 'nav-group-label', label));
    for (const [value, title, glyph] of categories.filter(([value]) => values.includes(value))) {
      const control = button(title, 'nav-item', () => actions.category(value), glyph);
      control.dataset.filter = value; control.title = title;
      control.append(el('span', 'nav-count', '0')); group.append(control);
    }
    navigation.append(group);
  }

  const bottom = el('div', 'sidebar-bottom'); bottom.id = 'sidebar-bottom';
  const statistics = button('游戏统计', 'nav-item', actions.statistics, 'chart');
  statistics.dataset.page = 'statistics';
  statistics.title = '游戏统计';
  navigation.append(statistics);
  const tools = el('div', 'sidebar-tools'); tools.setAttribute('role', 'group'); tools.setAttribute('aria-label', '显示模式');
  const bigScreen = button('大屏模式', 'sidebar-tool', actions.bigScreen, 'screen');
  bigScreen.id = 'big-screen-entry'; bigScreen.title = '大屏模式';
  tools.append(bigScreen, themeToggle(true));
  const steamImport = button('导入 Steam 游戏', 'sidebar-import', actions.importSteam, 'refresh');
  steamImport.title = '导入 Steam 游戏';
  const arrow = icon('arrow', 'sidebar-import-arrow'); steamImport.append(arrow);
  const epicImport = button('导入 Epic 游戏', 'sidebar-import', actions.importEpic, 'epic');
  epicImport.title = '登录 Epic 账号，导入账号游戏库';
  epicImport.append(icon('arrow', 'sidebar-import-arrow'));

  const footer = el('div', 'sidebar-footer');
  const version = el('div', 'app-version');
  version.append(el('span', 'status-dot'), el('span', '', actions.preview ? '设计预览 · 示例数据' : '游迹'));
  const settings = button('', 'sidebar-settings', actions.settings, 'settings');
  settings.setAttribute('aria-label', '设置'); settings.title = '设置';
  footer.append(version, settings);
  bottom.append(tools, steamImport, epicImport, footer);
  host.replaceChildren(brand, caption, navigation, bottom);
}
