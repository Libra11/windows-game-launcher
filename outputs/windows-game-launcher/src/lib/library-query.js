export const platformCategories = [
  ['steam', 'Steam 游戏', 'steam'], ['epic', 'Epic 游戏', 'epic'], ['local', '本地游戏', 'folder'],
];
export const bigScreenCategories = [['all', '全部游戏', 'library'], ...platformCategories];
export const categories = [
  ['all', '全部游戏', 'library'], ['recent', '最近游玩', 'clock'], ['favorites', '我的收藏', 'star'],
  ...platformCategories,
];

export function inCategory(game, category) {
  if (category === 'recent') return !!game.lastPlayed;
  if (category === 'favorites') return !!game.favorite;
  return category === 'all' || game.source === category;
}

export function queryGames(games, { category = 'all', collection = 'all', search = '', sort = 'az', installedOnly = false } = {}) {
  const term = search.trim().toLocaleLowerCase();
  return games.filter(game => inCategory(game, category) && inCategory(game, collection)
    && (!installedOnly || game.installation?.state === 'installed')
    && game.title.toLocaleLowerCase().includes(term)).sort((a, b) => {
    if (sort === 'time') {
      const total = game => game.source === 'steam' ? game.playtime?.seconds ?? -1 : game.playtime?.seconds ?? game.playedSeconds ?? 0;
      const order = total(b) - total(a); if (order) return order;
      return a.title.localeCompare(b.title, 'zh-CN');
    }
    if (sort === 'recent' || category === 'recent' || collection === 'recent') {
      const order = (b.lastPlayed || '').localeCompare(a.lastPlayed || '');
      if (order) return order;
    } else if (a.favorite !== b.favorite) return Number(!!b.favorite) - Number(!!a.favorite);
    return sort === 'za' ? b.title.localeCompare(a.title, 'zh-CN') : a.title.localeCompare(b.title, 'zh-CN');
  });
}

export function playTime(seconds = 0) {
  if (seconds < 60) return seconds > 0 ? '不足 1 分钟' : '暂无游玩记录';
  const minutes = Math.floor(seconds / 60);
  return minutes < 60 ? `${minutes} 分钟` : `${Math.floor(minutes / 60)} 小时 ${minutes % 60} 分钟`;
}

export function playedDate(time) {
  if (!time) return '尚未游玩';
  return new Date(time).toLocaleString('zh-CN', { month:'numeric', day:'numeric', hour:'2-digit', minute:'2-digit' });
}
