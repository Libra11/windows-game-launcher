// 取消后的标记不按时间失效：在手势的最终点击被消费或下一次独立按下时清除。
export function createDragClickGuard() {
  let suppressed;
  return {
    begin: () => { suppressed = undefined; },
    suppress: (kind, id) => { suppressed = { kind, id }; },
    matches: (kind, id) => suppressed?.kind === kind && suppressed.id === id,
    consumeClick: detail => {
      if (!detail || !suppressed) return false;
      suppressed = undefined;
      return true;
    },
  };
}

export function collectionDragPayload(state, gameId) {
  const game = state.games.find(item => item.id === gameId);
  if (!game) return null;
  const selected = state.organizationBatchMode && state.organizationSelection.has(gameId);
  const known = new Set(state.games.map(item => item.id));
  const gameIds = selected
    ? [...state.organizationSelection].filter(id => known.has(id))
    : [gameId];
  return { gameIds, title: gameIds.length > 1 ? `${gameIds.length} 款游戏` : game.title };
}

export function collectionDrop(state, gameIds, collectionId) {
  const collection = state.organization.collections.find(item => item.id === collectionId);
  if (!collection) throw new Error('收藏夹已不存在，请刷新后重试');
  const known = new Set(state.games.map(item => item.id));
  if (!gameIds.length || gameIds.some(id => !known.has(id))) {
    throw new Error('拖动的游戏已变化，请重新拖动');
  }
  const existing = state.organization.byCollection.get(collectionId);
  return { collection, gameIds: [...new Set(gameIds)].filter(id => !existing?.has(id)) };
}
