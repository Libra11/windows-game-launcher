import { el, button } from '../lib/dom.js';
import { categories, platformCategories } from '../lib/library-query.js';
import './sidebar.css';
import { sidebarBottom } from './sidebar-bottom.js';
import { sidebarRail } from './domain-switch.js';

export function mountSidebar(host, actions) {
  const rail = sidebarRail('games', { back: actions.back, onDomainChange: actions.domain, settings: actions.settings });

  const navigation = el('nav'); navigation.id = 'navigation'; navigation.setAttribute('aria-label', '主导航');
  for (const [label, values] of [['收藏', ['all', 'recent', 'favorites', 'steam-family']], ['平台', platformCategories.map(([value]) => value)]]) {
    const group = el('div', 'nav-group'); group.setAttribute('role', 'group'); group.setAttribute('aria-label', label);
    if (label === '平台') group.append(el('div', 'nav-group-label', label));
    for (const [value, title, glyph] of categories.filter(([value]) => values.includes(value))) {
      const control = button(title, 'nav-item', () => actions.category(value), glyph);
      control.dataset.filter = value; control.title = title;
      control.append(el('span', 'nav-count', '0')); group.append(control);
    }
    navigation.append(group);
  }

  const bottom = sidebarBottom({ preview: actions.preview }); bottom.id = 'sidebar-bottom';
  const features=el('div','nav-group');features.setAttribute('role','group');features.setAttribute('aria-label','功能');
  features.append(el('div','nav-group-label','功能'));
  const statistics = button('游戏统计', 'nav-item', actions.statistics, 'chart');
  statistics.dataset.page = 'statistics';
  statistics.title = '游戏统计';
  const onlineSearch = button('在线搜索', 'nav-item', actions.onlineSearch, 'search');
  onlineSearch.dataset.page = 'online-search'; onlineSearch.title = '在线搜索 Gamer520';
  const friends=button('好友','nav-item',()=>actions.social('friends'),'family');friends.dataset.page='social';friends.dataset.socialMode='friends';
  const space=button('我的空间','nav-item',()=>actions.social('mine'),'game');space.dataset.page='social';space.dataset.socialMode='mine';
  features.append(friends,space,onlineSearch,statistics);navigation.append(features);
  const menu=el('div','sidebar-menu');menu.append(navigation,bottom);
  host.replaceChildren(rail,menu);
}
