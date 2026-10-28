import './media.css';
import { isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { el, button, icon } from '../../lib/dom.js';
import {
  mediaCategories,
  queryMedia,
  progressPercent,
  secondsLabel,
} from '../../lib/media-query.js';
import { mountMediaSidebar } from './sidebar.js';
import { libraryPage } from './library.js';
import { filesPage } from './files.js';
import { detailPage } from './detail.js';
import { createPresentationLoader, presentationKey } from '../../lib/media-presentation.js';
import { seriesContext } from '../../lib/media-detail-model.js';
import { applyMediaPlayback } from '../../lib/media-playback.js';
import { createMediaNavigation } from '../../lib/media-navigation.js';
import { searchPage, personPage } from './discovery.js';
import { addSourceDialog } from './manage.js';
import { historyPage } from './history.js';
import { emptyState, mediaCard, retainedMediaArtwork } from './components.js';
import { createMediaViews, reconcileMediaChildren, mediaViewSignature, mediaDetailScope } from '../../lib/media-view.js';
import { createPlaybackStatus } from './playback-status.js';
import { localArtworkKey, mediaArtworkSource, mediaPosterOwner } from '../../lib/media-artwork-model.js';
import { createMediaSearchRunner, mediaItemContext } from '../../lib/media-search-index.js';
import { createMediaVirtualGrid } from './virtual-grid.js';
import { ignorePathDialog, ignoredPathsDialog } from './ignored.js';
import {
  newItemDialog,
  collectionDialog,
  collectionManager,
  organizeDialog,
  identifyDialog,
  recordDialog,
  technicalDialog,
} from './forms.js';
import { createMediaContextMenu } from './context-menu.js';
import { seasonMatchDialog } from './season-match.js';
import { cleanupDialog } from './cleanup.js';

const emptyData = () => ({
  items: [],
  sources: [],
  collections: [],
  tags: [],
  jobs: [],
  people: [],
  peopleIndex: { total: 0, completed: 0, failed: 0 },
  settings: {},
});
export function createMediaController({
  command,
  chooseFile = async () => null,
  toast = () => {},
  onNavigate = () => {},
}) {
  const contextMenu = createMediaContextMenu(message => toast(message, true));
  const state = {
    page: 'library',
    category: 'all',
    collectionId: '',
    search: '',
    searchQuery: '',
    searchResults: null,
    searchPending: false,
    searchError: '',
    searchPeoplePage: 0,
    searchItemsPage: 0,
    person: null,
    personQuery: {},
    sort: 'az',
    view: 'grid',
    onlyRecognized: false,
    onlyPlayable: false,
    watchedFilter: 'all',
    tagIds: [],
    batch: false,
    selection: new Set(),
    collectionsExpanded: false,
    collectionsCollapsed: false,
    data: emptyData(),
    selectedId: '',
    detail: null,
    detailSeasonId: '',
    detailEpisodeId: '',
    detailResourceId: '',
    detailOnlyResources: false,
    playerStatus: { state: 'idle', active: false },
    sourceId: '',
    browsePath: '',
    browse: null,
    browsePending: false,
    browseError: '',
    historyQuery: { period: 'month', source: 'all', kind: 'all' },
    historyDay: '',
    historyMonth: '',
    statistics: null,
    statisticsError: '',
    loading: true,
    error: '',
  };
  const element = el('div', 'media-root');
  const pageHost = el('div', 'media-page-host');
  const subscribers = new Set();
  const views = createMediaViews();
  const libraryGrid = createMediaVirtualGrid(views);
  const filesGrid = createMediaVirtualGrid(views);
  const personGrid = createMediaVirtualGrid(views);
  const searchRunner = createMediaSearchRunner();
  let itemsById = new Map();
  let itemContext = mediaItemContext([]);
  let searchRequest = 0;
  let detailRequest = 0;
  let peopleLoaded = false, peopleLoadedAt = 0, peopleTask = null;
  const detailCache = new Map();
  let pendingUi = null,
    renderDeferred = false,
    searchDraft = null,
    searchComposing = false;
  let sidebar = null,
    disposed = false,
    active = true,
    refreshTask = null,
    navigation = 0,
    statisticsGeneration = 0,
    fingerprint = '',
    lastStatisticsAt = 0,
    refreshTimer = null,
    lastRefreshAt = 0,
    playerStatus = { state: 'idle', active: false };
  const history = createMediaNavigation(state, {
    capture: captureView,
    restore: (ui) => {
      pendingUi = ui;
    },
  });
  const playback = createPlaybackStatus({
    toast,
    getItem: (id) => state.data.items.find((item) => item.id === id),
    onOpen: (id) => open({ id }),
    onStop: async () => {
      await run('media_player_stop');
      await refresh();
    },
  });
  element.append(pageHost, playback.element);
  const presentations = createPresentationLoader({
    run,
    onChange(id, key) {
      if (
        !disposed &&
        active &&
        ((state.selectedId &&
          (presentationKey(state.detail?.item) === key ||
            presentationKey(state.data.items.find((v) => v.id === state.detailSeasonId)) ===
              key)) ||
          Array.from(element.querySelectorAll('[data-media-presentation-key]')).some(
            (node) => node.dataset.mediaPresentationKey === key
          ))
      )
        render();
    },
  });
  const controller = {
    element,
    state,
    views,
    libraryGrid,
    filesGrid,
    personGrid,
    itemContext: () => itemContext,
    setSearchPage(kind, page) {
      if (kind === 'people') state.searchPeoplePage = Math.max(0, page);
      else state.searchItemsPage = Math.max(0, page);
      render();
    },
    posterItem: (item) => mediaPosterOwner(item, itemsById),
    posterKey(item) {
      const owner = controller.posterItem(item);
      return [owner.id, mediaArtworkSource(owner), localArtworkKey(owner)];
    },
    card(item, options = {}, scope = 'library', actions = controller.cardActions) {
      const owner = controller.posterItem(item);
      return views.use(`card:${scope}:${item.id}`, [item, options, controller.posterKey(item)], () => mediaCard(item, actions, {
        ...options, artwork: (value) => retainedMediaArtwork(views, `card:${scope}:${item.id}`,
          owner === item ? value : { ...owner, title: value.title }),
      }));
    },
    chooseFile,
    toast,
    run,
    presentation: (item) => presentations.get(item),
    loadPresentation: (item, refresh) => presentations.load(item, refresh),
    navigate,
    refresh,
    async refreshSearch() {
      await refresh();
      if (disposed || state.page !== 'search') return;
      if (peopleTask) await peopleTask.catch(() => {});
      peopleLoaded = false;
      startSearch(true);
      render();
    },
    refreshDetail,
    render,
    open,
    openPerson,
    search,
    back,
    browse,
    canGoBack: () => history.length > 0,
    browseParent(sourceId, path) {
      const previous = history.previous;
      if (previous?.page === 'library' && previous.category === 'files' && previous.sourceId === sourceId && previous.browsePath === path) back();
      else browse(sourceId, path);
    },
    play,
    receivePlayback,
    ignorePath: (target) => ignorePathDialog(controller, target),
    matchSeason: (target) => seasonMatchDialog(controller, target),
    cleanup: (item) => cleanupDialog(controller, currentItem(item)),
    async afterCleanup(itemIds) {
      const removed = new Set(itemIds);
      history.discardItems(removed);
      if (removed.has(state.selectedId)) back();
      await refreshDetail();
    },
    ignoredPaths: (sourceId) => ignoredPathsDialog(controller, sourceId),
    openSource(source) {
      if (isTauri()) return run('media_open_source', {
        provider: source.provider, providerType: source.providerType, externalId: source.externalId,
      });
      window.open(source.href, '_blank', 'noopener,noreferrer');
    },
    async refreshLocalFiles(sourceId) {
      if (active && state.category === 'files' && state.sourceId === sourceId && !state.selectedId)
        await browse(sourceId, state.browsePath);
    },
    pathContext: (item, target, event) =>
      contextDialog(currentItem(item), [
        ...(target.directory ? [['打开目录', 'folder', () => browse(target.sourceId, target.path)],
          ['匹配／更正本季剧集', 'scan-check', () => seasonMatchDialog(controller, { ...target, item: currentItem(item) })]] : []),
        ['忽略此路径', 'close', () => ignorePathDialog(controller, target)],
      ], event),
    fileContext: (target, event) => contextMenu.open(target.path.split(/[\\/]/).pop(), [[
      [target.directory ? '打开目录' : '查看文件', target.directory ? 'folder' : 'info', () =>
        target.directory ? browse(target.sourceId, target.path) : controller.openResource(target.resource)],
    ], [
      ...(target.directory ? [['匹配／更正本季剧集', 'scan-check', () => seasonMatchDialog(controller, target)]] : []),
      ...(target.sourceRoot ? [['忽略列表', 'folder', () => ignoredPathsDialog(controller, target.sourceId)]]
        : [['忽略此路径', 'close', () => ignorePathDialog(controller, target)]]),
    ]], event),
    loadStatistics,
    subscribe(callback) {
      subscribers.add(callback);
      return () => subscribers.delete(callback);
    },
    mountSidebar(host, options) {
      sidebar?.dispose();
      sidebar = mountMediaSidebar(host, controller, options);
      return sidebar;
    },
    newCollection: () => collectionDialog(controller),
    manageCollections: () => collectionManager(controller),
    addSource: () => addSourceDialog(controller),
    async addToCollection(collectionId, itemIds) {
      await run('media_organization', {
        action: 'setCollectionItems',
        collectionId,
        itemIds,
        mode: 'add',
      });
      await refresh();
      toast('已加入收藏夹');
    },
    async favorite(item) {
      await run('media_set_record', {
        itemId: item.id,
        patch: { favorite: !item.favorite },
      });
      await refreshDetail();
    },
    async scan(sourceId) {
      await run('media_scan', { sourceId });
      await refresh();
      if (state.sourceId === sourceId) await browse(sourceId, state.browsePath);
      toast('媒体扫描已启动');
    },
    async openResource(resource) {
      const id = resource.itemIds?.[0];
      if (id) return open({ id });
      return technicalDialog(controller, resource);
    },
    setActive(value) {
      const wasActive = active;
      active = !!value;
      if (!active) {
        contextMenu.close();
        cancelSearch();
        searchRunner.cancel();
        clearTimeout(refreshTimer);
        refreshTimer = null;
        statisticsGeneration++;
      } else if (!wasActive && state.page === 'search' && state.searchPending) {
        startSearch();
        render();
      }
    },
    dispose() {
      contextMenu.close();
      disposed = true;
      searchRunner.dispose();
      libraryGrid.hide();
      filesGrid.hide();
      personGrid.hide();
      clearInterval(timer);
      clearTimeout(refreshTimer);
      sidebar?.dispose();
      subscribers.clear();
      views.clear();
      detailCache.clear();
      listeners.forEach((unlisten) => unlisten());
      element.remove();
    },
  };
  controller.cardActions = {
    open,
    play: (item) => play(currentItem(item)),
    favorite: (item) => controller.favorite(currentItem(item)),
    toggleSelection(id) {
      state.selection.has(id) ? state.selection.delete(id) : state.selection.add(id);
      render();
    },
    dragIds(id) {
      return state.selection.has(id) ? [...state.selection] : [id];
    },
    context: (item, event) => contextDialog(currentItem(item), [], event),
  };
  function currentItem(item) {
    return state.data.items.find((value) => value.id === item.id) || item;
  }
  async function run(name, input = {}, notify = true) {
    try {
      return await command(name, { input });
    } catch (error) {
      if (notify) toast(String(error), true);
      throw error;
    }
  }
  async function refresh() {
    if (disposed) return;
    if (refreshTask) return refreshTask;
    refreshTask = (async () => {
      try {
        lastRefreshAt = Date.now();
        const [response] = await Promise.all([
          run('media_snapshot', { includePeople: false }, false),
          run('media_player_status', {}, false)
            .then((value) => {
              if (disposed) return;
              receivePlayback(value || { state: 'idle', active: false }, false);
            })
            .catch(() => {}),
        ]);
        if (disposed) return;
        const next = { ...emptyData(), ...response,
          people: Array.isArray(response.people) ? response.people : state.data.people };
        if (Array.isArray(response.people)) {
          peopleLoaded = true;
          peopleLoadedAt = Date.now();
        } else if (JSON.stringify(response.peopleIndex) !== JSON.stringify(state.data.peopleIndex)) peopleLoaded = false;
        for (const key of ['items', 'sources', 'collections', 'tags', 'jobs', 'people'])
          if (!Array.isArray(next[key])) next[key] = [];
        const nextFingerprint = JSON.stringify({
          ...next,
          jobs: [],
          people: [],
          items: next.items.map((item) => ({
            ...item,
            positionSeconds: undefined,
            durationSeconds: undefined,
            updatedAt: undefined,
            lastPlayed: !!item.lastPlayed,
          })),
        });
        const orderChanged = nextFingerprint === fingerprint && state.page === 'library' && !state.selectedId &&
          (state.category === 'recent' || (state.category !== 'added' && state.sort === 'recent')) &&
          queryMedia(next.items, state, next.collections)
            .map((item) => item.id)
            .join('|') !==
          queryMedia(state.data.items, state, state.data.collections)
            .map((item) => item.id)
            .join('|');
        const changed = nextFingerprint !== fingerprint || orderChanged;
        const detailChanged = state.selectedId && mediaViewSignature(mediaDetailScope(next.items, state.selectedId)) !==
          mediaViewSignature(mediaDetailScope(state.data.items, state.selectedId));
        const filesChanged = state.category === 'files' && state.sourceId && mediaViewSignature([next.sources, next.items.filter(item => item.resourceCount > 0)]) !==
          mediaViewSignature([state.data.sources, state.data.items.filter(item => item.resourceCount > 0)]);
        fingerprint = nextFingerprint;
        state.data = next;
        itemContext = mediaItemContext(next.items);
        itemsById = itemContext.byId;
        applyMediaPlayback(state, state.playerStatus);
        state.error = '';
        state.loading = false;
        playback.update(playerStatus);
        const ids = new Set(next.items.map((item) => item.id));
        for (const id of state.selection) if (!ids.has(id)) state.selection.delete(id);
        if (state.selectedId && !ids.has(state.selectedId)) {
          state.selectedId = '';
          state.detail = null;
          state.page = 'library';
        }
        if (
          state.collectionId &&
          !next.collections.some((collection) => collection.id === state.collectionId)
        )
          state.collectionId = '';
        for (const callback of subscribers) callback(next);
        if (active) {
          if (changed) {
            if (state.selectedId) {
              if (detailChanged) await fetchDetail(state.selectedId, navigation, false);
            } else if (state.page === 'search') {
              startSearch(true);
              render();
            } else if (state.page === 'library' && state.category === 'files' && state.sourceId) {
              if (filesChanged) {
                const version = navigation;
                try {
                  const value = await run(
                    'media_browse',
                    { sourceId: state.sourceId, path: state.browsePath },
                    false
                  );
                  if (version === navigation && active) {
                    state.browse = value;
                    render();
                  }
                } catch {
                  render();
                }
              }
            } else render();
            sidebar?.update();
          }
          patchPlayback();
        }
        if (
          active &&
          state.page === 'library' &&
          state.category === 'history' &&
          Date.now() - lastStatisticsAt > 15000
        )
          await loadStatistics(false);
      } catch (error) {
        if (disposed) return;
        state.error = String(error);
        state.loading = false;
        if (active) {
          if (!state.data.items.length) render();
          else toast('影视库暂未更新：' + String(error), true);
        }
      }
    })().finally(() => (refreshTask = null));
    return refreshTask;
  }
  async function ensurePeople() {
    if (peopleLoaded && Date.now() - peopleLoadedAt < 60000) return;
    if (!peopleTask) {
      peopleTask = run('media_people_snapshot', {}, false).then((response) => {
        if (disposed) return;
        state.data = { ...state.data, people: response.people || [], peopleIndex: response.peopleIndex || state.data.peopleIndex };
        peopleLoaded = true;
        peopleLoadedAt = Date.now();
      }).finally(() => { peopleTask = null; });
    }
    await peopleTask;
  }
  async function fetchDetail(id, version, showLoading = true) {
    const request = ++detailRequest;
    if (showLoading) {
      state.detail = null;
      render();
    }
    try {
      const detail = await run('media_detail', { itemId: id });
      if (disposed || request !== detailRequest || version !== navigation || state.selectedId !== id) return;
      const changed = mediaViewSignature(detail) !== mediaViewSignature(state.detail);
      state.detail = detail;
      detailCache.delete(id);
      detailCache.set(id, { detail, signature: mediaViewSignature(mediaDetailScope(state.data.items, id)) });
      if (detailCache.size > 12) detailCache.delete(detailCache.keys().next().value);
      applyMediaPlayback(state, state.playerStatus);
      if (changed) render();
      else patchPlayback();
      presentations.load(detail.item);
    } catch {
      if (!disposed && request === detailRequest && version === navigation) {
        if (state.detail) toast('影视详情暂未更新', true);
        else back();
      }
    }
  }
  async function refreshDetail() {
    const request = detailRequest;
    await refresh();
    if (state.selectedId && request === detailRequest) await fetchDetail(state.selectedId, navigation, false);
  }
  async function open(item) {
    contextMenu.close();
    cancelSearch();
    history.push();
    state.page = 'detail';
    state.detailResourceId = '';
    state.detailOnlyResources = false;
    const current = state.data.items.find((value) => value.id === item.id) || item;
    const context = seriesContext(current, state.data.items);
    state.detailEpisodeId = current.kind === 'episode' ? current.id : '';
    state.detailSeasonId =
      current.kind === 'season' ? current.id : current.kind === 'episode' ? current.parentId : '';
    item = context;
    state.selectedId = item.id;
    state.batch = false;
    state.selection.clear();
    const version = ++navigation;
    const cached = detailCache.get(item.id);
    const valid = cached && cached.signature === mediaViewSignature(mediaDetailScope(state.data.items, item.id));
    state.detail = valid ? cached.detail : null;
    scrollToTop();
    onNavigate(state.category, state.collectionId);
    if (valid) render();
    await fetchDetail(item.id, version, !valid);
    if (disposed || version !== navigation || state.selectedId !== item.id) return;
  }
  function back() {
    contextMenu.close();
    cancelSearch();
    searchRunner.cancel();
    navigation++;
    if (!history.back()) {
      state.page = 'library';
      state.selectedId = '';
      state.detail = null;
      state.person = null;
    }
    state.browsePending = false;
    state.browseError = '';
    if (state.page === 'search') startSearch(true);
    else {
      searchRunner.clear();
      state.searchResults = null;
      state.searchPending = false;
      state.searchError = '';
    }
    render();
    if (state.selectedId) fetchDetail(state.selectedId, navigation, false);
    else if (state.page === 'library' && state.category === 'files' && state.sourceId) {
      const stale = state.browse?.entries?.some(entry => entry.item &&
        mediaViewSignature(entry.item) !== mediaViewSignature(itemsById.get(entry.item.id)));
      if (!state.browse || stale) browse(state.sourceId, state.browsePath, { remember: false });
    }
    sidebar?.update();
    onNavigate(state.category, state.collectionId);
  }
  function openPerson(person, item = null) {
    contextMenu.close();
    cancelSearch();
    history.push();
    navigation++;
    state.page = 'person';
    state.person = state.data.people.find((value) => value.id === person.id) || {
      ...person,
      departments: person.departments || [person.department],
      credits:
        item && ['movie', 'season'].includes(item.kind)
          ? [
              {
                itemId: item.id,
                roles: person.role ? [person.role] : [],
                departments: [person.department],
              },
            ]
          : [],
    };
    state.personQuery = {
      kind: 'all',
      watchedFilter: 'all',
      onlyPlayable: false,
      sort: 'az',
      view: state.view,
    };
    state.selectedId = '';
    state.detail = null;
    state.batch = false;
    state.selection.clear();
    render();
    scrollToTop();
    ensurePeople().then(() => {
      if (!disposed && state.page === 'person' && state.person?.id === person.id) render();
    }).catch(() => toast('演职员资料暂未加载', true));
  }
  function search(query) {
    contextMenu.close();
    cancelSearch();
    query = query.trim().slice(0, 200);
    if (!query) {
      if (state.page === 'search') back();
      return;
    }
    if (state.page !== 'search') history.push();
    navigation++;
    state.page = 'search';
    state.searchQuery = query;
    state.searchPeoplePage = 0;
    state.searchItemsPage = 0;
    state.selectedId = '';
    state.detail = null;
    state.batch = false;
    state.selection.clear();
    startSearch();
    render();
    scrollToTop();
  }
  function startSearch(keepResults = false) {
    const request = ++searchRequest;
    const query = state.searchQuery;
    state.searchPending = true;
    state.searchError = '';
    if (!keepResults) state.searchResults = null;
    ensurePeople().then(() => searchRunner.search(state.data, query, () => disposed || state.page !== 'search' || state.searchQuery !== query))
      .then((result) => {
        if (!result || request !== searchRequest || disposed || state.page !== 'search' || state.searchQuery !== query) return;
        state.searchResults = result;
        state.searchPending = false;
        render();
      })
      .catch(() => {
        if (request !== searchRequest || disposed || state.page !== 'search' || state.searchQuery !== query) return;
        state.searchPending = false;
        state.searchError = '搜索失败';
        render();
      });
  }
  function navigate(category = 'all', collectionId = '') {
    contextMenu.close();
    cancelSearch();
    searchRunner.clear();
    state.searchResults = null;
    state.searchPending = false;
    state.searchError = '';
    history.clear();
    state.page = 'library';
    state.person = null;
    state.detailResourceId = '';
    navigation++;
    pendingUi = null;
    state.browsePending = false;
    state.browseError = '';
    state.category = mediaCategories.some(([value]) => value === category) ? category : 'all';
    state.collectionId = collectionId;
    if (['watched', 'unwatched'].includes(state.category)) state.watchedFilter = 'all';
    if (state.category === 'other') state.onlyRecognized = false;
    state.selectedId = '';
    state.detail = null;
    state.batch = false;
    state.selection.clear();
    state.search = '';
    render();
    scrollToTop();
    sidebar?.update();
    onNavigate(state.category, state.collectionId);
    if (state.category === 'history') loadStatistics();
    else if (state.category === 'files' && state.sourceId) browse(state.sourceId, state.browsePath, { remember: false });
  }
  async function browse(sourceId, path = '', { remember = true } = {}) {
    contextMenu.close();
    const targetPath = path || state.data.sources.find((source) => source.id === sourceId)?.path;
    const sameDirectory = state.page === 'library' && state.category === 'files' && state.sourceId === sourceId && state.browsePath === targetPath;
    if (sameDirectory && state.browsePending) return;
    if (!sameDirectory && remember) history.push();
    if (!sameDirectory) {
      state.search = '';
      cancelSearch();
    }
    const version = ++navigation;
    state.category = 'files';
    state.page = 'library';
    state.collectionId = '';
    state.selectedId = '';
    state.detail = null;
    state.sourceId = sourceId;
    state.browsePath = targetPath || path;
    state.browseError = '';
    state.browsePending = true;
    if (!sameDirectory) state.browse = null;
    if (!sourceId) {
      state.browsePath = '';
      state.browse = null;
      state.browsePending = false;
    }
    if (!sameDirectory) scrollToTop();
    render();
    sidebar?.update();
    onNavigate('files', '');
    if (!sourceId) return;
    try {
      const response = await run('media_browse', { sourceId, path });
      if (disposed || version !== navigation) return;
      state.browse = response;
      state.browsePath = response.path ?? path;
      state.browsePending = false;
      render();
    } catch (error) {
      if (!disposed && version === navigation) {
        state.browsePending = false;
        state.browseError = String(error);
        render();
      }
    }
  }
  async function play(item, resource = null, resume = true) {
    const result = await run('media_play', {
      itemId: item.id || resource?.itemIds?.[0] || '',
      resourceId: resource?.id || (resume ? item.resumeResourceId : undefined),
      resume,
    });
    toast(result.message || '已打开播放器');
    await refreshDetail();
  }
  function receivePlayback(value, renderSwitch = true) {
    const switched = applyMediaPlayback(state, value);
    playerStatus = state.playerStatus;
    playback.update(playerStatus);
    if (
      renderSwitch &&
      switched &&
      active &&
      !state.loading &&
      !document.querySelector('dialog[open]')
    )
      render();
    if (active) patchPlayback();
  }
  async function loadStatistics(show = true) {
    const version = ++statisticsGeneration;
    state.statisticsError = '';
    if (show) {
      state.statistics = null;
      render();
    }
    try {
      const value = await run('media_statistics', { ...state.historyQuery }, false);
      if (disposed || version !== statisticsGeneration) return;
      state.statistics = value;
      lastStatisticsAt = Date.now();
      if (state.page === 'library' && state.category === 'history') render();
    } catch (error) {
      if (version !== statisticsGeneration) return;
      state.statisticsError = String(error);
      if (state.page === 'library' && state.category === 'history') render();
    }
  }
  function patchPlayback() {
    for (const node of element.querySelectorAll('[data-playback-resource]')) {
      if (node.dataset.playbackResource === state.playerStatus.resourceId)
        node.textContent = secondsLabel(state.playerStatus.positionSeconds);
    }
    const byId = new Map(state.data.items.map((item) => [item.id, item]));
    for (const node of element.querySelectorAll('[data-playback-item-position]')) {
      const item = byId.get(node.dataset.playbackItemPosition);
      if (item) node.textContent = secondsLabel(item.positionSeconds);
    }
    for (const tile of element.querySelectorAll('[data-media-item-id]')) {
      const item = byId.get(tile.dataset.mediaItemId);
      if (!item) continue;
      const art = tile.querySelector('.media-art'),
        copy = tile.querySelector('.media-card-copy,.media-continue-copy');
      if (!art || !copy) continue;
      let progress = art.querySelector('.media-card-progress'),
        position = copy.querySelector('.media-card-position');
      if (Number(item.positionSeconds) > 0) {
        if (!progress) {
          progress = el('div', 'media-card-progress');
          progress.append(el('i'));
          art.append(progress);
        }
        progress.firstElementChild.style.width = progressPercent(item) + '%';
        progress.setAttribute('role', 'img');
        progress.setAttribute('aria-label', '播放位置 ' + secondsLabel(item.positionSeconds));
        if (!position) {
          position = el('small', 'media-card-position');
          copy.append(position);
        }
        position.textContent = '上次看到 ' + secondsLabel(item.positionSeconds);
      } else {
        progress?.remove();
        position?.remove();
      }
    }
  }
  function canRefresh() {
    return (
      !disposed &&
      ((active && element.isConnected) || subscribers.size > 0) &&
      !(active && state.page === 'search' && !state.selectedId && subscribers.size === 0) &&
      !document.hidden &&
      !document.querySelector('dialog[open]')
    );
  }
  function scheduleRefresh() {
    if (!canRefresh()) {
      clearTimeout(refreshTimer);
      refreshTimer = null;
      return;
    }
    if (refreshTimer) return;
    refreshTimer = setTimeout(
      () => {
        refreshTimer = null;
        if (canRefresh()) refresh();
      },
      Math.max(2000, 5000 - (Date.now() - lastRefreshAt))
    );
  }
  function contextDialog(item, extraActions = [], event) {
    contextMenu.open(item.title, [[
      [
        item.playable ? '播放／续播' : '查看详情',
        item.playable ? 'play' : 'info',
        () => (item.playable ? play(item) : open(item)),
      ],
    ], [
      ['编辑观影记录', 'edit', () => recordDialog(controller, item)],
      ['整理收藏夹和标签', 'tags', () => organizeDialog(controller, [item])],
      [item.favorite ? '取消收藏' : '收藏', 'star', () => controller.favorite(item)],
    ], [
      ['识别／更正资料', 'search', () => identifyDialog(controller, item)],
      ...extraActions,
    ], [['清理条目', 'trash', () => cleanupDialog(controller, currentItem(item))]]], event);
  }
  function captureView() {
    const detail = pageHost.querySelector('[data-detail-id]');
    const scrolls = [];
    for (let node = element; node; node = node.parentElement) scrolls.push([node, node.scrollTop]);
    const focused = element.contains(document.activeElement) ? document.activeElement : null;
    const viewportTop = Math.max(0, ...scrolls.filter(([node]) => node.scrollHeight > node.clientHeight && getComputedStyle(node).overflowY.match(/auto|scroll/)).map(([node]) => node.getBoundingClientRect().top));
    const anchor = [...pageHost.querySelectorAll('.media-tile')].find(node => {
      const rect = node.getBoundingClientRect();
      return rect.bottom > viewportTop && rect.top < window.innerHeight;
    });
    const anchorKey = anchor?.dataset.mediaGridKey != null ? ['data-media-grid-key', anchor.dataset.mediaGridKey]
      : anchor?.dataset.mediaItemId ? ['data-media-item-id', anchor.dataset.mediaItemId]
        : anchor?.querySelector('[data-focus-key]') ? ['data-focus-key', anchor.querySelector('[data-focus-key]').dataset.focusKey] : null;
    return {
      anchor: anchorKey && { key: anchorKey, top: (anchorKey[0] === 'data-focus-key' ? anchor.querySelector('[data-focus-key]') : anchor).getBoundingClientRect().top },
      scrolls,
      windowY: window.scrollY,
      focusKey: focused?.dataset.focusKey,
      selectionStart: focused?.selectionStart,
      selectionEnd: focused?.selectionEnd,
      detail: detail
        ? {
            id: detail.dataset.detailId,
            rail: detail.querySelector('.media-episode-rail')?.scrollLeft || 0,
            credits: detail.querySelector('.media-credits-rail')?.scrollLeft || 0,
            opened: [...detail.querySelectorAll('details[open][data-disclosure]')].map(
              (node) => node.dataset.disclosure
            ),
          }
        : null,
    };
  }
  function restoreView(ui) {
    if (!ui) return;
    for (const grid of [libraryGrid, filesGrid, personGrid]) grid.restoreAnchor(ui.anchor?.key[0] === 'data-media-grid-key' ? ui.anchor.key[1] : null);
    if (
      ui.detail &&
      ui.detail.id === pageHost.querySelector('[data-detail-id]')?.dataset.detailId
    ) {
      const rail = pageHost.querySelector('.media-episode-rail');
      const credits = pageHost.querySelector('.media-credits-rail');
      if (rail) rail.scrollLeft = ui.detail.rail;
      if (credits) credits.scrollLeft = ui.detail.credits;
      for (const node of pageHost.querySelectorAll('[data-disclosure]'))
        node.open = ui.detail.opened.includes(node.dataset.disclosure);
    }
    for (const [node, top] of ui.scrolls) if (node.isConnected) node.scrollTop = top;
    window.scrollTo(0, ui.windowY);
    if (ui.anchor) {
      const [attribute, key] = ui.anchor.key;
      const anchor = pageHost.querySelector(`[${attribute}="${CSS.escape(key)}"]`);
      if (anchor) {
        const delta = anchor.getBoundingClientRect().top - ui.anchor.top;
        const container = ui.scrolls.find(([node]) => node.isConnected && node !== document.documentElement && node !== document.body && node.scrollHeight > node.clientHeight && /auto|scroll/.test(getComputedStyle(node).overflowY))?.[0];
        if (container) container.scrollTop += delta;
        else window.scrollBy(0, delta);
      }
    }
    if (ui.focusKey) {
      const focus = element.querySelector(`[data-focus-key="${CSS.escape(ui.focusKey)}"]`);
      focus?.focus({ preventScroll: true });
      if (ui.selectionStart != null) {
        try {
          focus?.setSelectionRange?.(ui.selectionStart, ui.selectionEnd);
        } catch {}
      }
    }
  }
  function scrollToTop() {
    for (let node = element; node; node = node.parentElement) node.scrollTop = 0;
    window.scrollTo(0, 0);
  }
  function cancelSearch() {
    searchDraft = null;
    searchComposing = false;
  }
  function render() {
    // 后台补齐索引时也可能刷新；中文输入法组词期间保留正在编辑的节点。
    if (disposed) return;
    if (state.loading || state.selectedId || state.page !== 'library' || ['files', 'history'].includes(state.category)) libraryGrid.hide();
    if (state.loading || state.selectedId || state.page !== 'library' || state.category !== 'files') filesGrid.hide();
    if (state.loading || state.selectedId || state.page !== 'person') personGrid.hide();
    if (searchComposing) {
      renderDeferred = true;
      return;
    }
    renderDeferred = false;
    const ui = pendingUi || captureView();
    pendingUi = null;
    const root = views.use('page', null, () => el('div', 'media-page'));
    const children = [];
    const detail = !state.loading && state.selectedId ? detailPage(controller) : null;
    const detailNavigation = detail?.dataset.navigationTitle ? views.use('detail-navigation', detail.dataset.navigationTitle, () => {
      const node = el('div', 'media-detail-navigation');
      node.append(button('返回上一页', 'media-text-button media-back', back, 'back'), el('span', '', detail.dataset.navigationTitle));
      return node;
    }) : null;
    const header = views.use('header', [state.page, state.category, state.selectedId,
      detail?.dataset.navigationTitle, state.search, state.searchQuery, state.person?.name], () => {
      const contextual = state.page !== 'library' || state.category === 'history';
      const local = state.page === 'library' && state.category === 'files';
      const header = el('div', `media-page-header ${contextual ? 'media-context-header' : ''}`);
      const copy = el('div', 'media-page-heading');
      if (detailNavigation) {
        header.classList.add('media-detail-header');
        copy.append(detailNavigation);
      } else if (['search', 'person'].includes(state.page)) {
        const backButton = button('返回', 'media-text-button media-back', back, 'back');
        backButton.dataset.focusKey = 'media-page-back';
        backButton.setAttribute('aria-label', '返回上一页');
        copy.append(
          backButton,
          el('h1', '', state.page === 'search' ? '搜索结果' : state.person?.name || '人员作品')
        );
      } else if (!contextual) {
        const title = el('div');
        title.append(
          el('div', 'eyebrow', '你的私人放映室'),
          el('h1', '', local ? '本地影视' : '影视库')
        );
        copy.append(title);
      }
      const tools = el('div', 'media-page-tools');
      const searchBox = el('form', 'search-box media-search-box');
      const input = el('input');
      input.type = 'search';
      input.maxLength = 200;
      input.placeholder = local ? '输入后搜索当前目录' : '输入后搜索影视、演职员';
      input.value =
        searchDraft ?? (local ? state.search : state.page === 'search' ? state.searchQuery : '');
      input.setAttribute('aria-label', local ? '搜索当前目录' : '搜索全影视库的影视和演职员');
      input.dataset.focusKey = 'media-search';
      input.oninput = () => { searchDraft = input.value; };
      input.addEventListener('compositionstart', () => {
        searchComposing = true;
      });
      input.addEventListener('compositionend', () => {
        searchComposing = false;
        searchDraft = input.value;
        if (renderDeferred) render();
      });
      searchBox.onsubmit = (event) => {
        event.preventDefault();
        if (searchComposing) return;
        if (local) {
          state.search = input.value;
          searchDraft = null;
          render();
        } else search(input.value);
      };
      input.onkeydown = (event) => {
        if (event.isComposing || event.keyCode === 229) {
          if (event.key === 'Enter') event.preventDefault();
          return;
        }
        if (event.key === 'Escape' && state.page === 'search') {
          event.preventDefault();
          back();
        }
      };
      const searchButton = el('button', 'media-search-submit');
      searchButton.type = 'submit';
      searchButton.title = '搜索';
      searchButton.setAttribute('aria-label', '搜索');
      searchButton.append(icon('search'));
      searchBox.append(input, searchButton);
      tools.append(searchBox);
      if (state.page === 'library' && state.category !== 'files' && state.category !== 'history')
        tools.append(
          button('添加目录', 'secondary', () => controller.addSource(), 'folder'),
          button('添加影视剧', 'primary', () => newItemDialog(controller), 'plus')
        );
      header.append(copy, tools);
      return header;
    });
    children.push(header);
    if (state.loading) {
      children.push(emptyState('正在打开影视库', '正在读取本地资料…'));
      reconcileMediaChildren(root, children);
      reconcileMediaChildren(pageHost, [root]);
      return;
    }
    if (state.error) children.push(views.use('error', state.error, () => el('div', 'media-error-banner', state.error)));
    if (state.selectedId) children.push(detail || detailPage(controller));
    else if (state.page === 'search') children.push(searchPage(controller));
    else if (state.page === 'person') children.push(personPage(controller));
    else if (state.category === 'files') children.push(filesPage(controller));
    else if (state.category === 'history') children.push(views.use('history', [state.statistics, state.statisticsError, state.historyQuery, state.historyDay, state.historyMonth], () => historyPage(controller)));
    else children.push(libraryPage(controller));
    reconcileMediaChildren(root, children);
    reconcileMediaChildren(pageHost, [root]);
    views.prune();
    patchPlayback();
    restoreView(ui);
    sidebar?.update();
  }
  const listeners = [];
  if (isTauri())
    for (const event of ['media-changed', 'media-job-changed', 'media-playback-changed'])
      listen(
        event,
        event === 'media-playback-changed'
          ? ({ payload }) => receivePlayback(payload)
          : scheduleRefresh
      )
        .then((unlisten) => {
          if (disposed) unlisten();
          else listeners.push(unlisten);
        })
        .catch(() => {});
  const timer = setInterval(() => {
    if (canRefresh()) {
      clearTimeout(refreshTimer);
      refreshTimer = null;
      refresh();
    }
  }, 5000);
  render();
  refresh();
  return controller;
}
