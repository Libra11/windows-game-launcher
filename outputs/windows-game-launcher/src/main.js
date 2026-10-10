import { createAppUpdateController } from './lib/app-update-controller.js';
import { updateAvailable } from './lib/app-update.js';
import { createOrganizationController } from './lib/organization-controller.js';
import { clearOrganizationSelection, reconcileOrganization } from './lib/library-organization.js';
import { mountOrganizationSidebar, patchOrganizationSelection } from './ui/organization-controls.js';
import './style.css';
import './ui/library.css';
import './ui/detail.css';
import './ui/dialogs.css';
import './ui/responsive.css';
import './ui/theme.css';
import './ui/big-screen.css';
import './ui/motion.css';
import './ui/activity.css';
import './ui/artwork.css';
import { createViewMotion } from './ui/motion.js';
import { bigScreenView } from './ui/big-screen.js';
import { createBigScreenMode } from './ui/big-screen-mode.js';
import { mountSidebar } from './ui/sidebar.js';
import { el, button, icon } from './lib/dom.js';
import { command, onAppUpdateProgress, onUnlock, onLibraryChange, onLauncherError, preview } from './lib/bridge.js';
import { inCategory, queryGames, bigScreenCategories } from './lib/library-query.js';
import { patchRuntime } from './ui/game-controls.js';
import { launchState } from './lib/launch-state.js';
import { detectionGuide } from './ui/detection-guide.js';
import { libraryView } from './ui/library.js';
import { detailView } from './ui/detail.js';
import { addDialog, editDialog } from './ui/dialogs.js';
import { createSettingsPage } from './ui/settings-dialog.js';
import { mountTitlebar } from './ui/titlebar.js';
import { librarySnapshot } from './lib/library-refresh.js';
import { createArtworkRetainer } from './lib/artwork-retainer.js';
import { createLibraryViewCache } from './lib/library-view-cache.js';
import { activateArtwork } from './lib/artwork-loader.js';
import { createStatisticsController } from './lib/statistics.js';
import { statisticsView } from './ui/statistics-view.js';
import { closeModal } from './ui/modal.js';
import { mountProgramDrop } from './ui/program-drop.js';
import { mountCollectionDrag } from './ui/collection-drag.js';
import { watchNetworkImages } from './lib/network-images.js';

const state = { page:'library', games: [], selectedId: '', filter: 'all', search: '', sort: 'az', installedOnly:false, view: 'grid', achievements: [], achievementFilter: 'all', bigScreen: false, bigCategory: 'all', bigCollection:'all', bigFocusedId: '' };
const appUpdates=createAppUpdateController(command,onAppUpdateProgress);
const animateView = createViewMotion();
const retainArtworkView = createArtworkRetainer();
const cachedLibraryView = createLibraryViewCache();
const app = document.querySelector('#app');
app.innerHTML = `
  <aside class="sidebar"></aside>
  <main class="main">
    <header class="topbar"><div class="breadcrumb"><span>收藏室</span><span class="breadcrumb-slash">/</span><strong id="page-name">游戏库</strong>${preview ? '<span class="preview-badge">示例预览</span>' : ''}</div><div class="topbar-tools"><div class="topbar-actions"><label class="search-box" id="search-box"><input id="search" type="search" placeholder="搜索你的游戏" aria-label="搜索游戏"></label><div id="add-action"></div></div><div id="header-display-action"></div></div></header>
    <div class="page-intro" id="page-intro"><div><div class="eyebrow">属于你的游戏时光</div><h1>游戏库<span class="title-dot">.</span></h1></div><p>每一次打开，都有新的期待。</p></div>
    <section id="content" aria-label="游戏库内容"></section>
    <footer class="page-footer"><span>让热爱，有迹可循。</span><span>游迹</span></footer>
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
let libraryReturnPosition;
async function select(game) {
  const version = ++selectionVersion;
  const items = await run('list_achievements', { gameId: game.id });
  if (version !== selectionVersion) return;
  if (state.page === 'library' && !state.selectedId) {
    libraryReturnPosition = {
      bigScreen: state.bigScreen,
      scroll: state.bigScreen ? $('#big-screen-host .big-screen-content')?.scrollTop || 0 : window.scrollY,
      focus: document.activeElement?.dataset.focusKey || `game-${game.id}`,
    };
  }
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
let settingsPage;
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
    const timeOrderChanged = state.sort === 'time' && queryGames(games,{sort:'time'}).map(game=>game.id).join('|') !== queryGames(state.games,{sort:'time'}).map(game=>game.id).join('|');
    const changed = timeOrderChanged || librarySnapshot(games) !== librarySnapshot(state.games) || (selectedId === state.selectedId && JSON.stringify(items) !== JSON.stringify(state.achievements));
    state.games = games;
    reconcileOrganization(state);
    if (selectedId === state.selectedId) state.achievements = items;
    if (!silent || changed) render();
    else patchRuntime(app,state.games);
    statistics.refresh();
}
const dialogActions = { run, refresh, toast, added: async game => { await refresh(); await select(game); toast('游戏已加入收藏'); actions.guide(actions.game(game.id) || game); } };
mountProgramDrop(dialogActions).catch(error => toast(`拖入导入暂不可用：${String(error)}`, true));
const statistics = createStatisticsController({
  command, render, visible:()=>state.page==='statistics' && !state.selectedId && !document.hidden && !document.querySelector('dialog[open]'),
});
const actions = { run, toast,
  select, launch, exitBigScreen: () => bigMode.exit(),
  get backLabel(){return state.page==='statistics'?'返回统计':state.page==='settings'?'返回设置':'返回游戏库';},
  settings:async(category)=>{
    if(state.bigScreen)await bigMode.exit();
    selectionVersion++;state.page='settings';state.selectedId='';
    settingsPage ||= createSettingsPage({...dialogActions,updates:appUpdates,add:actions.add,importSteam:actions.importSteam});
    if(category)settingsPage.openCategory(category);
    render();window.scrollTo(0,0);
  },
  statistics:()=>{selectionVersion++; state.page='statistics'; state.selectedId=''; render(); statistics.refresh(true);},
  selectStatistic:async id=>{
    const game=actions.game(id); if(!game)return;
    statistics.state.scroll=state.bigScreen ? $('#big-screen-host .big-screen-content')?.scrollTop||0 : window.scrollY;
    for(const dialog of document.querySelectorAll('dialog[open]')) await closeModal(dialog);
    statistics.state.returnFocus=document.activeElement?.dataset.focusKey||'';
    await select(game);
  },
  game: id => state.games.find(game=>game.id === id),
  favorite: async game => {const current = actions.game(game.id) || game; await run('set_favorite',{gameId:game.id,favorite:!current.favorite}); await refresh();},
  guide: game => detectionGuide(game,{...dialogActions,game:actions.game,edit:actions.edit}),
  bigCategory: value => { clearOrganizationSelection(state);selectionVersion++; state.page='library'; state.bigCategory = value; state.selectedId = ''; render(`category-${value}`); },
  bigCollection: value => {clearOrganizationSelection(state);state.collectionId='';selectionVersion++; state.bigCollection = value; state.selectedId = ''; render(`collection-${value}`);},
  add: () => addDialog(dialogActions),
  importSteam: async () => { toast('正在从 Steam 导入游戏…'); const count = await run('import_steam'); await refresh(); toast(`新增 ${count} 款 Steam 游戏`); },
  sort: value => { state.sort = value; render(); },
  installed: value => { clearOrganizationSelection(state);state.installedOnly = value; render(); },
  view: value => { state.view = value; render(); },
  back: () => {
    selectionVersion++; const returning=!!state.selectedId && state.page==='statistics';
    const libraryPosition = state.selectedId && state.page === 'library' && libraryReturnPosition?.bigScreen === state.bigScreen ? libraryReturnPosition : null;
    if(!state.selectedId) state.page='library';
    state.selectedId = ''; render(returning?statistics.state.returnFocus:libraryPosition?.focus);
    if(returning){if(state.bigScreen)$('#big-screen-host .big-screen-content').scrollTop=statistics.state.scroll;else window.scrollTo(0,statistics.state.scroll);statistics.refresh();}
    if (libraryPosition) {
      if (state.bigScreen) $('#big-screen-host .big-screen-content').scrollTop = libraryPosition.scroll;
      else window.scrollTo(0, libraryPosition.scroll);
    }
  },
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
const organizationController=createOrganizationController(state,render,actions,()=>selectionVersion++);
const bigMode = createBigScreenMode(state, render, toast, {
  back: () => state.selectedId || state.page==='statistics' ? actions.back() : bigMode.exit(),
  category: direction => {
    if(state.page==='statistics' && !state.selectedId) return statistics.cycle(direction);
    const categories = bigScreenCategories.map(([value]) => value);
    const index = categories.indexOf(state.bigCategory);
    actions.bigCategory(categories[(index + direction + categories.length) % categories.length]);
  },
});
mountSidebar($('.sidebar'), {
  category: value => { clearOrganizationSelection(state);state.collectionId='';selectionVersion++; state.page='library'; state.filter = value; state.selectedId = ''; render(); },
  back:()=>{
    if (state.page === 'library' && state.selectedId) return actions.back();
    selectionVersion++;state.page='library';state.selectedId='';render();
  },
  settings: actions.settings, preview,
  statistics:actions.statistics,
});
const organizationSidebar=mountOrganizationSidebar($('#navigation'),state,actions);
const collectionDrag=mountCollectionDrag(state,actions,organizationSidebar);
$('#add-action').append(button('添加游戏', 'primary add-button', actions.add, 'plus'));
const displayMode=button('','header-icon-button',bigMode.enter,'screen');
displayMode.id='big-screen-entry';displayMode.title='大屏模式';displayMode.setAttribute('aria-label','大屏模式');
$('#header-display-action').append(displayMode);
let searchFrame;
$('#search').oninput = event => {
  clearOrganizationSelection(state);selectionVersion++;state.page='library';state.search=event.target.value;state.selectedId='';
  cancelAnimationFrame(searchFrame);
  searchFrame=requestAnimationFrame(()=>render());
};
function render(preferredFocus) {
  collectionDrag.reconcile();
  // 后台刷新只更新内容，不能重新获取焦点、打断前台游戏。
  const restoreFocus = document.hasFocus();
  if(state.page!=='settings'&&settingsPage){settingsPage.dispose();settingsPage=null;}
  const host = $('#big-screen-host');
  host.hidden = !state.bigScreen;
  if (state.bigScreen) {
    app.classList.remove('statistics-active');
    app.classList.remove('settings-active');
    const focused = host.contains(document.activeElement) ? document.activeElement.dataset.focusKey : null;
    const diagnosticOpen = host.querySelector('details')?.open;
    if (!state.games.some(game => game.id === state.selectedId)) state.selectedId = '';
    const samePage = host.firstElementChild?.dataset.view === (state.selectedId || state.page);
    const scrollTop = samePage && !preferredFocus ? host.querySelector('.big-screen-content')?.scrollTop || 0 : 0;
    const library=!state.selectedId&&state.page==='library';
    const next = retainArtworkView(host,'big-screen',library,()=>library
      ? cachedLibraryView('big-screen',state,()=>bigScreenView(state,actions,statistics))
      : bigScreenView(state,actions,statistics));
    if(host.firstElementChild!==next)host.replaceChildren(next);
    activateArtwork(next);
    patchRuntime(next,state.games);
    if (diagnosticOpen && host.querySelector('details')) host.querySelector('details').open = true;
    host.querySelector('.big-screen-content').scrollTop = scrollTop;
    const key = preferredFocus || focused || `game-${state.bigFocusedId}`;
    const target = [...host.querySelectorAll('[data-focus-key]')].find(node => node.dataset.focusKey === key && !node.disabled)
      || host.querySelector('.game-actions .primary:not(:disabled)') || host.querySelector('.game-actions button:not(:disabled)') || host.querySelector('.big-screen-card.selected') || host.querySelector('.big-screen-tabs .active');
    if (restoreFocus) target?.focus({ preventScroll: true });
    if (restoreFocus && state.page !== 'statistics') target?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
    animateView(host.firstElementChild, state);
    return;
  }
  host.replaceChildren();
  const content = $('#content'); const diagnosticOpen = content.querySelector('details')?.open;
  const focusKey = content.contains(document.activeElement) ? document.activeElement.dataset.focusKey : null;
  const statisticsScroll=state.page==='statistics' && !state.selectedId ? window.scrollY : null;
  $('#navigation').querySelectorAll('button').forEach(node => {
    const category = node.dataset.filter; const active=node.dataset.page ? state.page===node.dataset.page : state.page==='library' && !state.collectionId && category===state.filter;
    node.classList.toggle('active',active);node.setAttribute('aria-current',active?'page':'false');
    if(category) node.querySelector('.nav-count').textContent = state.games.filter(game => inCategory(game,category)).length;
  });
  organizationSidebar.update();
  const game = state.games.find(item => item.id === state.selectedId);
  if (!game) state.selectedId = '';
  app.classList.toggle('statistics-active',state.page==='statistics'&&!game);
  app.classList.toggle('settings-active',state.page==='settings'&&!game);
  $('#page-name').textContent = game ? game.title : state.page==='statistics' ? '游戏统计' : state.page==='settings'?'设置':'游戏库';
  $('#page-intro').hidden = !!game || state.page!=='library';
  $('.topbar-actions').hidden=state.page!=='library' && !game;
  content.setAttribute('aria-label',state.page==='settings'&&!game?'设置内容':state.page==='statistics' && !game ? '游戏统计内容' : '游戏库内容');
  const next = retainArtworkView(content,'desktop',!game&&state.page==='library',()=>game?detailView(game,state.achievements,state.achievementFilter,actions):state.page==='settings'?settingsPage.element:state.page==='statistics'?statisticsView(statistics,actions):cachedLibraryView('desktop',state,()=>libraryView(state,actions)));
  // 设置页保留表单节点，后台游戏库刷新不会覆盖用户正在输入的内容。
  if(content.firstElementChild!==next)content.replaceChildren(next);
  activateArtwork(next);
  patchRuntime(next,state.games);
  patchOrganizationSelection(next,state);
  if (diagnosticOpen && content.querySelector('details')) content.querySelector('details').open = true;
  if (restoreFocus && (preferredFocus||focusKey)) [...content.querySelectorAll('[data-focus-key]')].find(node=>node.dataset.focusKey === (preferredFocus||focusKey))?.focus({preventScroll:true});
  animateView(content.firstElementChild, state);
  if(statisticsScroll!=null)window.scrollTo(0,statisticsScroll);
}
onUnlock(async event => {
  toast(`${event.payload.gameTitle} · 成就解锁：${event.payload.achievementName}`);
  await refresh();
}).catch(() => {});
onLibraryChange(()=>refresh(true).catch(()=>{})).catch(()=>{});
onLauncherError(event=>toast(String(event.payload),true)).catch(()=>{});
watchNetworkImages().catch(()=>{});
if(!preview)command('get_backup_restore_status').then(async notice=>{
  if(!notice){
    if(window.__youjiRestoredPreferencesId)await command('ack_backup_restore',{restoreId:window.__youjiRestoredPreferencesId,appearanceApplied:true});
    return;
  }
  if(!notice.acknowledged)toast(notice.message+(notice.missingPaths?` 尚有 ${notice.missingPaths} 款游戏需要定位。`:''),!notice.success);
  await command('ack_backup_restore',{restoreId:notice.id,appearanceApplied:window.__youjiRestoredPreferencesId===notice.id});
}).catch(()=>{});
setInterval(() => { if (!document.hidden && !document.querySelector('dialog[open]')) refresh(true).catch(() => {}); }, 5000);
$('#content').append(el('div', 'loading-state', '正在打开你的收藏…'));
appUpdates.subscribe(status=>$('#navigation [data-page="settings"]')?.classList.toggle('update-available',updateAvailable(status)));
appUpdates.start().catch(()=>{});
organizationController.start().catch(error=>toast(String(error),true));
refresh().catch(() => { $('#content').replaceChildren(el('div', 'empty-state', '暂时无法读取游戏库，请在桌面启动器中打开。')); });
