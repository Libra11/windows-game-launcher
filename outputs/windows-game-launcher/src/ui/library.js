import { el, button, icon, artwork, sourceName, sourceIcon } from '../lib/dom.js';
import { queryGames, playedDate, categories, featuredGame } from '../lib/library-query.js';
import { favoriteButton, runtimeBadge, installationBadge, playtimeBadge } from './game-controls.js';
import { libraryFeature } from './library-feature.js';
import { libraryControls } from './library-controls.js';
export function libraryView(state, actions) {
  const root = el('div', 'library-view');
  const shown = queryGames(state.games,{category:state.filter,search:state.search,sort:state.sort,installedOnly:state.installedOnly});
  if (!state.search && shown.length) {
    const featured = featuredGame(shown);
    root.append(libraryFeature(featured, actions));
  }
  const toolbar = el('div', 'collection-toolbar');
  const title = el('div', 'collection-title');
  title.append(el('h2', '', state.search ? '搜索结果' : categories.find(([value])=>value === state.filter)?.[1] || '全部游戏'), el('span', 'count-pill', String(shown.length)));
  const controls = el('div', 'collection-controls');
  const views = el('div', 'view-switch');
  for (const [value, glyph, label] of [['grid', 'library', '封面视图'], ['list', 'list', '列表视图']]) {
    const control = button('', state.view === value ? 'icon-button active' : 'icon-button', () => actions.view(value), glyph);
    control.dataset.focusKey = `view-${value}`;
    control.setAttribute('aria-label', label); control.setAttribute('aria-pressed', String(state.view === value)); control.title = label; views.append(control);
  }
  controls.append(libraryControls(state, actions), views); toolbar.append(title, controls); root.append(toolbar);
  if (!shown.length) {
    if (state.installedOnly) {
      const empty = el('div', 'empty-state');
      empty.append(icon('folder'), el('h2', '', '没有符合条件的已安装游戏'), el('p', '', '安装检测完成后，已安装的游戏会自动显示在这里。'), button('显示全部安装状态', 'secondary', () => actions.installed(false), 'library'));
      root.append(empty); return root;
    }
    const special = state.filter === 'recent' || state.filter === 'favorites';
    const empty = el('div', 'empty-state'); empty.append(icon(state.search ? 'search' : 'library'), el('h2', '', state.search ? '没有找到这款游戏' : state.filter === 'recent' ? '还没有游玩记录' : state.filter === 'favorites' ? '还没有收藏游戏' : '收藏，从第一款游戏开始'), el('p', '', state.search ? '试试游戏名称的其他关键词，或切换分类。' : state.filter === 'recent' ? '从启动器打开游戏后，会自动记录最近游玩。' : state.filter === 'favorites' ? '点击游戏封面右上角的星标，将常玩的游戏收藏置顶。' : '连接 Steam 账号，或添加电脑上的游戏。'));
    if (!state.search && !special) empty.append(button('添加本地游戏', 'primary', actions.add, 'plus'), button('导入 Steam 游戏', 'secondary', actions.importSteam, 'steam'));
    if (!state.search && !special) empty.append(button('导入 Epic 游戏', 'secondary', actions.importEpic, 'epic'));
    root.append(empty); return root;
  }
  const grid = el('div', state.view === 'list' ? 'game-list' : 'game-grid');
  shown.forEach(game => {
    const tile = el('div','game-tile');
    const card = button('', 'game-card', () => actions.select(game));
    card.setAttribute('aria-label',`${game.title} ${sourceName(game)}`); card.dataset.focusKey = `game-${game.id}`;
    const art = el('div', 'game-art'); artwork(art, game, false, {defer:true});
    const hover = el('span', 'card-open'); hover.append(icon('arrow')); art.append(hover);
    const copy = el('div', 'game-copy'); copy.append(el('h3', '', game.title));
    const source = el('span', 'game-source'); source.append(icon(sourceIcon(game)), document.createTextNode(sourceName(game))); copy.append(source);
    copy.append(playtimeBadge(game));
    if (game.lastPlayed) copy.append(el('span','game-played',`上次 · ${playedDate(game.lastPlayed)}`));
    copy.append(runtimeBadge(game),installationBadge(game,true));
    card.append(art, copy); tile.append(card,favoriteButton(game,actions,true,`favorite-${game.id}`)); grid.append(tile);
  });
  root.append(grid); return root;
}
