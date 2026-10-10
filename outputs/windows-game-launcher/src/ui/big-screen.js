import { el, button, artwork, icon, sourceName, appIcon } from '../lib/dom.js';
import { detailView } from './detail.js';
import { queryGames, bigScreenCategories } from '../lib/library-query.js';
import { runtimeBadge, installationBadge, playtimeBadge } from './game-controls.js';
import { showBigScreenGame } from './big-screen-hero.js';
import './big-screen-gallery.css';
import { libraryControls } from './library-controls.js';
import { statisticsView } from './statistics-view.js';

function control(label, className, action, glyph, key) {
  const node = button(label, className, action, glyph);
  node.dataset.focusKey = key;
  return node;
}

export function bigScreenView(state, actions, statistics) {
  const root = el('section', 'big-screen'); root.id = 'big-screen-root';
  const backdrop = el('div', 'big-screen-backdrop'); backdrop.setAttribute('aria-hidden', 'true'); root.append(backdrop);
  const header = el('header', 'big-screen-header');
  const brand = el('div', 'big-screen-brand'); brand.append(appIcon(), el('strong', '', '游迹'), el('small', '', '大屏模式'));
  const tabs = el('nav', 'big-screen-tabs'); tabs.setAttribute('aria-label', '大屏游戏库分类');
  for (const [value, label, glyph] of bigScreenCategories) {
    const tab = control(label, state.bigCategory === value ? 'active' : '', () => actions.bigCategory(value), glyph, `category-${value}`);
    tab.setAttribute('aria-pressed', String(state.bigCategory === value)); tabs.append(tab);
  }
  const tools = el('div', 'big-screen-tools');
  const settings=control('','big-screen-tool icon-only',actions.settings,'settings','settings');settings.setAttribute('aria-label','设置');settings.title='设置';
  tools.append(control('游戏统计', 'big-screen-tool', actions.statistics, 'chart', 'statistics'), settings, control('退出大屏', 'big-screen-tool', actions.exitBigScreen, 'screen', 'exit'));
  header.append(brand, tabs, tools); root.append(header);
  const content = el('div', 'big-screen-content');
  const game = state.games.find(item => item.id === state.selectedId);
  root.dataset.view = game?.id || state.page || 'library';
  if (game) {
    const scene = el('div', 'big-screen-scene'); artwork(scene, game, true); backdrop.append(scene);
    content.classList.add('big-screen-detail');
    content.append(detailView(game, state.achievements, state.achievementFilter, actions));
    // 用稳定的标识恢复定时同步和成就筛选后的焦点。
    content.querySelectorAll('button,summary').forEach((node, index) => { if (!node.dataset.focusKey) node.dataset.focusKey = `detail-${node.getAttribute('aria-label') || '检测详情'}-${index}`; });
  } else if (state.page === 'statistics') {
    content.classList.add('big-screen-statistics');
    content.append(statisticsView(statistics, actions));
  } else {
    const games = queryGames(state.games,{category:state.bigCategory,collection:state.bigCollection,sort:state.sort,installedOnly:state.installedOnly});
    const selected = games.find(item => item.id === state.bigFocusedId) || games[0];
    if (selected) state.bigFocusedId = selected.id;
    const hero = el('section', 'big-screen-hero');
    function showGame(item, animate = false) {
      state.bigFocusedId = item.id;
      showBigScreenGame(hero, backdrop, actions.game(item.id)||item, actions, animate);
    }
    if (selected) showGame(selected);
    content.append(hero);
    const heading = el('div', 'big-screen-collection-title');
    const collection = el('div','big-collection-controls'); collection.setAttribute('aria-label','游戏库视图');
    for (const [value,label] of [['all','全部'],['favorites','我的收藏'],['recent','最近游玩']]) {
      const tab = control(label,state.bigCollection === value ? 'active' : '',()=>actions.bigCollection(value),null,`collection-${value}`);
      tab.setAttribute('aria-pressed',String(state.bigCollection === value)); collection.append(tab);
    }
    const title = el('div', 'big-screen-collection-heading'); title.append(el('h2', '', '游戏库'), el('span', '', `${games.length} 款收藏`));
    const filters = el('div', 'big-screen-library-controls'); filters.append(collection, libraryControls(state, actions, true));
    heading.append(title,filters); content.append(heading);
    const grid = el('div', 'big-screen-grid');
    games.forEach(item => {
      const card = control('', 'big-screen-card', () => actions.select(item), null, `game-${item.id}`);
      card.setAttribute('aria-label', `${item.title} ${sourceName(item)}`);
      card.classList.toggle('selected', item.id === state.bigFocusedId);
      card.setAttribute('aria-current', item.id === state.bigFocusedId ? 'true' : 'false');
      const cover = el('div', 'big-screen-cover'); artwork(cover, item, false, {defer:true});
      const marker = el('span', 'big-screen-card-marker'); marker.append(icon('arrow')); cover.append(marker);
      if (item.favorite) { const mark = el('span','favorite-mark'); mark.append(icon('star')); cover.append(mark); }
      const caption = el('div', 'big-screen-card-caption'); caption.append(el('h3', '', item.title), el('span', '', sourceName(item))); cover.append(caption);
      card.append(cover,playtimeBadge(item),runtimeBadge(item),installationBadge(item,true));
      card.addEventListener('focus', () => {
        const previous = grid.querySelector('.selected'); previous?.classList.remove('selected'); previous?.setAttribute('aria-current', 'false');
        card.classList.add('selected'); card.setAttribute('aria-current', 'true');
        if (state.bigFocusedId !== item.id) showGame(item, true);
      });
      grid.append(card);
    });
    if (!games.length) {
      hero.remove();
      const empty = el('div', 'empty-state', state.installedOnly ? '没有符合条件的已安装游戏。' : state.bigCollection === 'favorites' ? '还没有收藏。打开游戏详情，点击收藏置顶。' : state.bigCollection === 'recent' ? '还没有游玩记录。从启动器打开游戏后会自动记录。' : '这个分类还没有游戏，请退出大屏模式后添加或导入。');
      if (state.installedOnly) empty.append(button('显示全部安装状态', 'secondary', () => actions.installed(false), 'library'));
      grid.append(empty);
    }
    content.append(grid);
  }
  root.append(content); return root;
}
