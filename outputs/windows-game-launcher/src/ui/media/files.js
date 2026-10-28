import { el, button, icon } from '../../lib/dom.js';
import { mediaItemTitle, resourceTechnical } from '../../lib/media-query.js';
import { displayMediaPath } from '../../lib/media-path.js';
import { emptyState } from './components.js';
import './files.css';

function bindPathContext(tile, controller, target) {
  const open = event => { event.preventDefault(); controller.fileContext(target, event); };
  tile.oncontextmenu = open;
  tile.addEventListener('keydown', event => {
    if (event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10')) open(event);
  });
}

function directoryCard({ name, path, status = '文件夹', error = '', focusKey, open }) {
  const tile = el('article', 'media-tile media-directory-tile');
  const card = button('', 'media-card', open);
  card.setAttribute('aria-label', `打开目录 ${name}`);
  card.dataset.focusKey = focusKey;
  const art = el('div', 'media-art media-directory-art');
  const placeholder = el('div', 'media-art-placeholder');
  placeholder.append(icon('folder'), el('span', '', '文件夹'));
  art.append(placeholder);
  const copy = el('div', 'media-card-copy');
  const title = el('h3', '', name);
  title.title = name;
  copy.append(title, el('span', 'media-card-meta', status));
  const displayPath = displayMediaPath(path);
  const location = el('small', 'media-directory-path', displayPath);
  location.title = displayPath;
  copy.append(location);
  if (error) {
    const message = el('small', 'media-error media-directory-error', error);
    message.title = error;
    copy.append(message);
  }
  card.append(art, copy);
  tile.append(card);
  return tile;
}

export function filesPage(controller) {
  const { state } = controller;
  const byId = controller.itemContext().byId;
  const titles = (state.browse?.entries || []).map(entry => entry.item && [mediaItemTitle(entry.item, state.data.items, byId), controller.posterKey(entry.item)]);
  const root = controller.views.use('files-page', [state.sourceId, state.browse, state.browsePath, state.browseError, state.search, state.data.sources, titles, controller.canGoBack()], () => buildFilesPage(controller));
  const refresh = root.querySelector('[data-media-browse-refresh]');
  if (refresh) {
    refresh.disabled = state.browsePending;
    refresh.lastChild.textContent = state.browsePending ? '正在刷新…' : '刷新目录';
  }
  root.setAttribute('aria-busy', String(state.browsePending));
  return root;
}

function buildFilesPage(controller) {
  const { state } = controller;
  const byId = controller.itemContext().byId;
  const root = el('div', 'media-files');
  const ignoredCount = state.data.sources.reduce(
    (sum, source) => sum + (source.ignoredCount || 0),
    0
  );
  if (!state.sourceId) {
    controller.filesGrid.hide();
    const heading = el('div', 'collection-toolbar');
    if (controller.canGoBack()) heading.append(button('返回', 'secondary', () => controller.back(), 'back'));
    heading.append(
      el('h2', '', '媒体目录'),
      button(`忽略列表${ignoredCount ? ' · ' + ignoredCount : ''}`, 'secondary', () =>
        controller.ignoredPaths()
      ),
      button('添加媒体目录', 'primary', () => controller.addSource(), 'plus')
    );
    root.append(heading);
    const grid = el('div', 'media-grid');
    for (const source of state.data.sources) {
      const tile = directoryCard({
        name: source.label || displayMediaPath(source.path),
        path: source.path,
        status: source.available ? '媒体目录 · 可访问' : '媒体目录 · 离线',
        error: source.error,
        focusKey: 'media-source-' + source.id,
        open: () => controller.browse(source.id, ''),
      });
      bindPathContext(tile, controller, { sourceId: source.id, path: source.path, directory: true, sourceRoot: true });
      grid.append(tile);
    }
    root.append(grid);
    if (!state.data.sources.length)
      root.append(
        emptyState(
          '添加你的第一个媒体目录',
          '扫描只读取文件，不移动或修改媒体资源。',
          button('选择媒体目录', 'primary', () => controller.addSource(), 'plus')
        )
      );
    return root;
  }
  const browse = state.browse;
  const source = state.data.sources.find((source) => source.id === state.sourceId);
  const navigation = el('div', 'media-file-navigation');
  if (controller.canGoBack()) navigation.append(button('返回', 'secondary', () => controller.back(), 'back'));
  navigation.append(
    button(
      '媒体目录',
      'secondary',
      () => controller.browse(''),
      'back'
    )
  );
  if (browse?.parentPath != null)
    navigation.append(
      button(
        '上一级',
        'secondary',
        () => controller.browseParent(state.sourceId, browse.parentPath),
        'folder'
      )
    );
  const path = displayMediaPath(browse?.path || state.browsePath || source?.path || '');
  const location = el('span', 'media-path', path);
  location.title = path;
  const refresh = button('刷新目录', 'secondary', () => controller.browse(state.sourceId, state.browsePath), 'refresh');
  refresh.dataset.mediaBrowseRefresh = '';
  navigation.append(
    location,
    refresh,
    button('匹配／更正本季剧集', 'secondary', () => controller.matchSeason({
      sourceId: state.sourceId, path: state.browse?.path || state.browsePath,
    }), 'scan-check'),
    button('扫描媒体', 'secondary', () => controller.scan(state.sourceId), 'refresh'),
    button(`忽略列表${source?.ignoredCount ? ' · ' + source.ignoredCount : ''}`, 'secondary', () =>
      controller.ignoredPaths(state.sourceId)
    )
  );
  if (browse?.parentPath != null)
    navigation.append(
      button('忽略此目录', 'media-text-button', () =>
        controller.ignorePath({
          sourceId: state.sourceId,
          path: browse.path,
          directory: true,
          parentPath: browse.parentPath,
        })
      )
    );
  root.append(navigation);
  if (state.browseError) {
    root.append(emptyState('目录暂时无法读取', state.browseError,
      button('重试', 'secondary', () => controller.browse(state.sourceId, state.browsePath), 'refresh')));
  }
  if (!browse) {
    controller.filesGrid.hide();
    if (!state.browseError) root.append(emptyState('正在打开目录', '请稍候…'));
    return root;
  }
  const query = state.search.trim().toLocaleLowerCase();
  const entries = (browse.entries || []).filter(
    (entry) =>
      !query ||
      `${entry.name} ${entry.path.split(/[\\/]/).pop()} ${entry.item ? mediaItemTitle(entry.item, state.data.items, byId) : ''}`
        .toLocaleLowerCase()
        .includes(query)
  );
  const makeCard = (entry) => {
    const target = {
      sourceId: state.sourceId,
      path: entry.path,
      directory: entry.directory,
      parentPath: browse.path,
      resource: entry.resource,
    };
    let tile;
    if (entry.kind === 'folder')
      tile = directoryCard({
        name: entry.name,
        path: entry.path,
        focusKey: 'media-directory-' + entry.path,
        open: () => controller.browse(state.sourceId, entry.path),
      });
    else if (entry.item)
      tile = controller.card(entry.item,
        { displayTitle: mediaItemTitle(entry.item, state.data.items, byId) },
        'file:' + entry.path,
        { ...controller.cardActions, context: (item, event) => controller.pathContext(item, target, event) }
      );
    else {
      const resource = entry.resource;
      tile = el('article', 'media-tile');
      const card = button('', 'media-card', () => controller.openResource(resource));
      const art = el('div', 'media-art');
      const placeholder = el('div', 'media-art-placeholder');
      placeholder.append(icon('play'));
      art.append(placeholder);
      const copy = el('div', 'media-card-copy');
      const title = el('h3', '', entry.name || resource.name);
      title.title = title.textContent;
      copy.append(
        title,
        el('small', 'media-card-meta', resourceTechnical(resource).summary || '未识别视频')
      );
      card.setAttribute('aria-label', `打开视频 ${title.textContent}`);
      card.append(art, copy);
      tile.append(card);
    }
    if (!tile.querySelector('.media-file-ignore')) {
    const ignore = button(
      '忽略',
      'media-file-ignore',
      () => controller.ignorePath(target),
      'close'
    );
    ignore.title = target.directory ? '忽略此目录及其内容' : '忽略此文件';
    ignore.setAttribute('aria-label', `${ignore.title} ${entry.name}`);
    tile.append(ignore);
    if (!entry.item) bindPathContext(tile, controller, target);
    }
    return tile;
  };
  let grid;
  if (entries.length > 120) grid = controller.filesGrid.show(entries, makeCard);
  else {
    controller.filesGrid.hide();
    grid = el('div', 'media-grid');
    grid.append(...entries.map(makeCard));
  }
  if (!entries.length)
    root.append(query ? emptyState('没有匹配的文件或目录', '可以按作品名或文件名搜索。',
      button('清除搜索', 'secondary', () => { state.search = ''; controller.render(); }))
      : emptyState('这个目录没有可展示的视频', '可以返回上一级或重新扫描。'));
  root.append(grid);
  return root;
}
