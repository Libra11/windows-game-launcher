import { organizationMatches } from './library-organization.js';
import { isFamilyGame } from './family-library.js';

export const platformCategories = [
  ['steam', 'Steam 游戏', 'steam'], ['epic', 'Epic 游戏', 'epic'], ['local', '本地游戏', 'folder'],
];
export const bigScreenCategories = [['all', '全部游戏', 'library'], ...platformCategories];
export const categories = [
  ['all', '全部游戏', 'library'], ['recent', '最近游玩', 'clock'], ['favorites', '我的收藏', 'star'],
  ['steam-family','家庭共享','family'],
  ...platformCategories,
];
const titleCollator=new Intl.Collator('zh-CN');
const playedDateFormat=new Intl.DateTimeFormat('zh-CN',{month:'numeric',day:'numeric',hour:'2-digit',minute:'2-digit'});

export function inCategory(game, category) {
  if(category==='steam-family')return isFamilyGame(game);
  if (category === 'recent') return !!game.lastPlayed;
  if (category === 'favorites') return !!game.favorite;
  return category === 'all' || game.source === category;
}

export function queryGames(games, { category = 'all', collection = 'all', search = '', sort = 'az', installedOnly = false, organization, collectionId = '', tagIds = [], tagMatch = 'all' } = {}) {
  const term = search.trim().toLocaleLowerCase();
  return games.filter(game => inCategory(game, category) && inCategory(game, collection)
    && organizationMatches(game,organization,collectionId,tagIds,tagMatch)
    && (!installedOnly || game.installation?.state === 'installed')
    && (!term || game.title.toLocaleLowerCase().includes(term))).sort((a, b) => {
    if (sort === 'time') {
      const total = game => game.source === 'steam' ? game.playtime?.seconds ?? -1 : game.playtime?.seconds ?? game.playedSeconds ?? 0;
      const order = total(b) - total(a); if (order) return order;
      return titleCollator.compare(a.title,b.title);
    }
    if (sort === 'recent' || category === 'recent' || collection === 'recent') {
      const left=b.lastPlayed||'',right=a.lastPlayed||'';
      const order=left===right?0:left>right?1:-1;
      if (order) return order;
    } else if (a.favorite !== b.favorite) return Number(!!b.favorite) - Number(!!a.favorite);
    return sort === 'za' ? titleCollator.compare(b.title,a.title) : titleCollator.compare(a.title,b.title);
  });
}

export function playTime(seconds = 0) {
  if (seconds < 60) return seconds > 0 ? '不足 1 分钟' : '暂无游玩记录';
  const minutes = Math.floor(seconds / 60);
  return minutes < 60 ? `${minutes} 分钟` : `${Math.floor(minutes / 60)} 小时 ${minutes % 60} 分钟`;
}

export function playedDate(time) {
  if (!time) return '尚未游玩';
  const date=new Date(time);
  return Number.isNaN(date.getTime())?'尚未游玩':playedDateFormat.format(date);
}

export function featuredGame(games) {
  let recent;
  for(const game of games){
    if(game.lastPlayed&&(!recent||game.lastPlayed>recent.lastPlayed
      ||(game.lastPlayed===recent.lastPlayed&&titleCollator.compare(game.title,recent.title)<0)))recent=game;
  }
  return recent||games.find(game=>game.favorite)||games.find(game=>game.source==='local')||games[0];
}
