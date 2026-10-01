import './style.css';
import './ui/library.css';
import './ui/detail.css';
import './ui/dialogs.css';
import './ui/responsive.css';
import './ui/theme.css';
import './ui/big-screen.css';
import './ui/motion.css';
import './ui/activity.css';
import { createViewMotion } from './ui/motion.js';
import { bigScreenView } from './ui/big-screen.js';
import { createBigScreenMode } from './ui/big-screen-mode.js';
import { mountSidebar } from './ui/sidebar.js';
import { el, button, icon } from './lib/dom.js';
import { command, onUnlock, onLibraryChange, onLauncherError, preview } from './lib/bridge.js';
import { inCategory, queryGames, bigScreenCategories } from './lib/library-query.js';
import { patchRuntime } from './ui/game-controls.js';
import { launchState } from './lib/launch-state.js';
import { detectionGuide } from './ui/detection-guide.js';
import { libraryView } from './ui/library.js';
import { detailView } from './ui/detail.js';
import { addDialog, editDialog, settingsDialog } from './ui/dialogs.js';
import { mountTitlebar } from './ui/titlebar.js';
import { epicLoginDialog } from './ui/epic-connection.js';

const state = { games: [], selectedId: '', filter: 'all', search: '', sort: 'az', installedOnly:false, view: 'grid', achievements: [], achievementFilter: 'all', bigScreen: false, bigCategory: 'all', bigCollection:'all', bigFocusedId: '' };
const animateView = createViewMotion();
const app = document.querySelector('#app');
app.innerHTML = `
  <aside class="sidebar"></aside>
  <main class="main">
    <header class="topbar"><div class="breadcrumb"><span>收藏室</span><span class="breadcrumb-slash">/</span><strong id="page-name">游戏库</strong>${preview ? '<span class="preview-badge">示例预览</span>' : ''}</div><div class="topbar-actions"><label class="search-box" id="search-box"><input id="search" type="search" placeholder="搜索你的游戏" aria-label="搜索游戏"></label><div id="add-action"></div></div></header>
    <div class="page-intro" id="page-intro"><div><div class="eyebrow">属于你的游戏时光</div><h1>游戏库<span class="title-dot">.</span></h1></div><p>每一次打开，都有新的期待。</p></div>
    <section id="content" aria-label="游戏库内容"></section>
    <footer class="page-footer"><span>让热爱，有迹可循。</span><span>游戏收藏室</span></footer>
  </main>
  <div id="big-screen-host" hidden></div>
  <div id="toast" class="toast" role="status" aria-live="polite"></div>`;
const $ = selector => document.querySelector(selector);
$('#search-box').prepend(icon('search'));
let toastTimer;
function toast(message, error = false) {
  const node = $('#toast'); node.replaceChildren(icon(error ? 'info' : 'check'), el('span', '', message));
  node.className = `toast visible ${error ? 'error' : ''}`;
  clearTimeout(toastTimer); toastTimer = setTimeout(() => node.classList.remove('visible'), 5500);
}
async function run(name, args) {
  try { return await command(name, args); }
  catch (error) { toast(String(error), true); throw error; }
}
mountTitlebar(toast);
let selectionVersion = 0;
async function select(game) {
  const version = ++selectionVersion;
  const items = await run('list_achievements', { gameId: game.id });
  if (version !== selectionVersion) return;
  state.selectedId = game.id; state.achievements = items; state.achievementFilter = 'all'; render(); window.scrollTo(0, 0);
}
async function launch(game) {
  const current = actions.game(game.id) || game;
  if (launchState(current).disabled) return;
  current.runtime = {state:'starting',message:'正在发送启动请求',elapsedSeconds:0};
  patchRuntime(app,state.games);
  try {
    const runtime = await run('launch_game',{gameId:game.id});
    (actions.game(game.id) || current).runtime = runtime;
    toast(runtime.state === 'running' ? `${game.title} 已在运行` : `已发送 ${game.title} 的启动请求`); await refresh(true);
  } catch (error) {
    (actions.game(game.id) || current).runtime = {state:'error',message:String(error),elapsedSeconds:0};
    await refresh(true).catch(()=>{}); throw error;
  } finally { patchRuntime(app,state.games); }
}
let refreshTask, refreshAgain = false, forceRefresh = false;
function refresh(silent = false) {
  refreshAgain = true; forceRefresh ||= !silent;
  if (refreshTask) return refreshTask;
  refreshTask = (async()=> {
    do { const force = forceRefresh; refreshAgain = false; forceRefresh = false; await refreshOnce(!force); } while (refreshAgain);
  })().finally(()=> {refreshTask = null;});
  return refreshTask;
}
async function refreshOnce(silent) {
    const games = await (silent ? command : run)('list_games');
    const selectedId = state.selectedId;
    const selectedExists = games.some(game => game.id === selectedId);
    const items = selectedId && selectedExists ? await (silent ? command : run)('list_achievements', { gameId: selectedId }) : [];
    if (selectedId && !selectedExists && selectedId === state.selectedId) {
      selectionVersion++; state.selectedId = ''; state.achievements = [];
    }
    const stable = games => games.map(({lastScan,runtime,playedSeconds,playtime,installation,...rest})=>({...rest,playtime:playtime && {...playtime,seconds:undefined},installation:installation && {state:installation.state,reason:installation.reason}}));
    const timeOrderChanged = state.sort === 'time' && queryGames(games,{sort:'time'}).map(game=>game.id).join('|') !== queryGames(state.games,{sort:'time'}).map(game=>game.id).join('|');
    const changed = timeOrderChanged || JSON.stringify(stable(games)) !== JSON.stringify(stable(state.games)) || (selectedId === state.selectedId && JSON.stringify(items) !== JSON.stringify(state.achievements));
    state.games = games;
    if (selectedId === state.selectedId) state.achievements = items;
    if (!silent || changed) render();
    patchRuntime(app,state.games);
}
const dialogActions = { run, refresh, toast, added: async game => { await refresh(); await select(game); toast('游戏已加入收藏'); actions.guide(actions.game(game.id) || game); } };
const actions = {
  select, launch, exitBigScreen: () => bigMode.exit(),
  game: id => state.games.find(game=>game.id === id),
  favorite: async game => {const current = actions.game(game.id) || game; await run('set_favorite',{gameId:game.id,favorite:!current.favorite}); await refresh();},
  guide: game => detectionGuide(game,{...dialogActions,game:actions.game,edit:actions.edit}),
  bigCategory: value => { selectionVersion++; state.bigCategory = value; state.selectedId = ''; render(`category-${value}`); },
  bigCollection: value => {selectionVersion++; state.bigCollection = value; state.selectedId = ''; render(`collection-${value}`);},
  add: () => addDialog(dialogActions),
  importSteam: async () => { toast('正在从 Steam 导入游戏…'); const count = await run('import_steam'); await refresh(); toast(`新增 ${count} 款 Steam 游戏`); },
  importEpic: async () => {
    const connection = await run('epic_connection_status');
    if (!connection.connected) return epicLoginDialog(dialogActions, actions.importEpic);
    toast('正在读取 Epic 账号游戏库…');
    const count = await run('import_epic'); await refresh();
    toast(count ? `新增 ${count} 款 Epic 游戏` : 'Epic 游戏库已同步，没有新增游戏');
  },
  sort: value => { state.sort = value; render(); },
  installed: value => { state.installedOnly = value; render(); },
  view: value => { state.view = value; render(); },
  back: () => { selectionVersion++; state.selectedId = ''; render(); },
  sync: async game => {
    try { toast(await run('sync_game', { gameId: game.id })); }
    finally { await refresh(); }
  },
  scan: async game => { await run('scan_now', { gameId: game.id }); await refresh(); toast('已检查本地记录'); },
  remove: async game => {
    await run('remove_local_game', { gameId:game.id });
    selectionVersion++;
    if (state.selectedId === game.id) { state.selectedId = ''; state.achievements = []; }
    if (state.bigFocusedId === game.id) state.bigFocusedId = '';
    await refresh(); toast('已从游戏库移除，游戏文件保留');
  },
  edit: (game,checkAfterSave = false) => editDialog(game, {...dialogActions, afterSaved:checkAfterSave ? ()=>actions.guide(actions.game(game.id) || game) : null}),
  filter: value => { state.achievementFilter = value; render(); },
  manual: async (game, item) => { await run('toggle_manual', { gameId: game.id, apiName: item.apiName }); await refresh(); },
};
const bigMode = createBigScreenMode(state, render, toast, {
  back: () => state.selectedId ? actions.back() : bigMode.exit(),
  category: direction => {
    const categories = bigScreenCategories.map(([value]) => value);
    const index = categories.indexOf(state.bigCategory);
    actions.bigCategory(categories[(index + direction + categories.length) % categories.length]);
  },
});
mountSidebar($('.sidebar'), {
  category: value => { selectionVersion++; state.filter = value; state.selectedId = ''; render(); },
  back: actions.back, bigScreen: bigMode.enter, importSteam: actions.importSteam, importEpic: actions.importEpic,
  settings: () => settingsDialog(dialogActions), preview,
});
$('#add-action').append(button('添加游戏', 'primary add-button', actions.add, 'plus'));
$('#search').oninput = event => { selectionVersion++; state.search = event.target.value; state.selectedId = ''; render(); };
function render(preferredFocus) {
  const host = $('#big-screen-host');
  host.hidden = !state.bigScreen;
  if (state.bigScreen) {
    const focused = host.contains(document.activeElement) ? document.activeElement.dataset.focusKey : null;
    const diagnosticOpen = host.querySelector('details')?.open;
    if (!state.games.some(game => game.id === state.selectedId)) state.selectedId = '';
    const samePage = host.firstElementChild?.dataset.view === (state.selectedId || 'library');
    const scrollTop = samePage && !preferredFocus ? host.querySelector('.big-screen-content')?.scrollTop || 0 : 0;
    host.replaceChildren(bigScreenView(state, actions));
    if (diagnosticOpen && host.querySelector('details')) host.querySelector('details').open = true;
    host.querySelector('.big-screen-content').scrollTop = scrollTop;
    const key = preferredFocus || focused || `game-${state.bigFocusedId}`;
    const target = [...host.querySelectorAll('[data-focus-key]')].find(node => node.dataset.focusKey === key && !node.disabled)
      || host.querySelector('.game-actions .primary:not(:disabled)') || host.querySelector('.game-actions button:not(:disabled)') || host.querySelector('.big-screen-card.selected') || host.querySelector('.big-screen-tabs .active');
    target?.focus({ preventScroll: true });
    target?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
    animateView(host.firstElementChild, state);
    return;
  }
  host.replaceChildren();
  const content = $('#content'); const diagnosticOpen = content.querySelector('details')?.open;
  const focusKey = content.contains(document.activeElement) ? document.activeElement.dataset.focusKey : null;
  $('#navigation').querySelectorAll('button').forEach(node => {
    const category = node.dataset.filter; node.classList.toggle('active', category === state.filter);
    node.setAttribute('aria-current', category === state.filter ? 'page' : 'false');
    node.querySelector('.nav-count').textContent = state.games.filter(game => inCategory(game,category)).length;
  });
  const game = state.games.find(item => item.id === state.selectedId);
  if (!game) state.selectedId = '';
  $('#page-name').textContent = game ? game.title : '游戏库';
  $('#page-intro').hidden = !!game;
  content.replaceChildren(game ? detailView(game, state.achievements, state.achievementFilter, actions) : libraryView(state, actions));
  if (diagnosticOpen && content.querySelector('details')) content.querySelector('details').open = true;
  if (focusKey) [...content.querySelectorAll('[data-focus-key]')].find(node=>node.dataset.focusKey === focusKey)?.focus({preventScroll:true});
  animateView(content.firstElementChild, state);
}
onUnlock(async event => {
  toast(`${event.payload.gameTitle} · 成就解锁：${event.payload.achievementName}`);
  await refresh();
}).catch(() => {});
onLibraryChange(()=>refresh(true).catch(()=>{})).catch(()=>{});
onLauncherError(event=>toast(String(event.payload),true)).catch(()=>{});
setInterval(() => { if (!document.hidden && !document.querySelector('dialog[open]')) refresh(true).catch(() => {}); }, 5000);
$('#content').append(el('div', 'loading-state', '正在打开你的收藏…'));
refresh().catch(() => { $('#content').replaceChildren(el('div', 'empty-state', '暂时无法读取游戏库，请在桌面启动器中打开。')); });
