import { collectionDragPayload } from './collection-drag.js';
import { launchState, installState } from './launch-state.js';

export function gameMenuScope(state, gameId) {
  const payload = collectionDragPayload(state, gameId);
  return payload && { gameId, gameIds: payload.gameIds, batch: payload.gameIds.length > 1 };
}

const item = (id, label, glyph, extra = {}) => ({ id, label, glyph, ...extra });

export function gameMenuModel(state, scope) {
  const game = state.games.find(value => value.id === scope.gameId);
  if (!game || scope.gameIds.some(id => !state.games.some(value => value.id === id))) return null;
  const current = state.organization.collections.find(value => value.id === state.collectionId);
  const memberIds = state.organization.byCollection.get(current?.id);
  const removeCurrent = current && scope.gameIds.some(id => memberIds?.has(id))
    ? item('remove-current', `从「${current.name}」移出`, 'folder', { collectionId: current.id }) : null;
  if (scope.batch) {
    return {
      title: `整理已选 ${scope.gameIds.length} 款游戏`,
      groups: [[
        item('add-collection', '加入收藏夹…', 'folder'),
        item('remove-collection', '移出收藏夹…', 'folder'),
        item('add-tag', '添加标签…', 'plus'),
        item('remove-tag', '移除标签…', 'list'),
      ], removeCurrent ? [removeCurrent] : []],
    };
  }
  const launch = launchState(game);
  const play = [item('launch', launch.label, launch.action === 'install' ? 'download' : 'play', {
    disabled: launch.disabled, reason: launch.reason,
  })];
  if (game.source === 'epic') {
    const install = installState(game);
    play.push(item('install', install.label, 'download', { disabled: install.disabled, reason: install.reason }));
  }
  play.push(item('details', '详情与成就', 'info'));
  const organize = [
    item('favorite', game.favorite ? '取消收藏' : '收藏置顶', 'star'),
    item('collections', '收藏夹', 'folder', { submenu: true }),
    item('tags', '标签…', 'list'),
  ];
  if (removeCurrent) organize.push(removeCurrent);
  return {
    title: game.title,
    groups: [play, organize, [item('manage', '管理', 'settings', { submenu: true })],
      game.source === 'local' ? [item('remove', '从游戏库移除…', 'trash', { danger: true })] : []],
  };
}

export function gameManagementItems(game) {
  const items = [item('sync', '更新资料', 'refresh'),
    item('edit', game.source === 'local' ? '编辑游戏…' : '启动检测设置…', 'settings')];
  if (game.source === 'local') items.push(item('scan', '扫描本地记录', 'refresh'), item('guide', '成就检测引导…', 'info'));
  return items;
}

export function gameCollectionItems(state, scope, search = '') {
  const term = search.trim().toLocaleLowerCase();
  return state.organization.collections.filter(value => value.name.toLocaleLowerCase().includes(term)).map(value => ({
    ...item(`collection-${value.id}`, value.name, 'folder'), collectionId: value.id,
    checked: state.organization.byCollection.get(value.id)?.has(scope.gameId) || false,
  }));
}
