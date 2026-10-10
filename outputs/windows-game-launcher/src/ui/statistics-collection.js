import { el, button, icon } from '../lib/dom.js';
import { panel, gameCover, unlockList, emptyState } from './statistics-components.js';

export function statisticsCollection(data, actions) {
  const root=el('div','stats-two-columns stats-collection-grid');
  const completed=panel('全成就收藏','已完成的游戏','star');
  const shelf=el('div','stats-completed-shelf'),finished=data.games.filter(game=>game.completionRate===100);
  finished.forEach(game=>{
    const card=button('','stats-completed-game',()=>actions.selectStatistic(game.gameId));
    card.dataset.focusKey='stats-completed-'+game.gameId;card.setAttribute('aria-label',game.title+'，全成就');
    const cover=gameCover(game,actions,'completed');const mark=el('span','stats-completed-mark');mark.append(icon('check'));cover.append(mark);
    card.append(cover,el('strong','',game.title),el('span','',game.totalAchievements+' 项成就'+(game.manualUnlocked?' · 含手动':'')));shelf.append(card);
  });
  if(!finished.length)shelf.append(emptyState('下一座里程碑，正在路上','全部成就解锁后，游戏会留在这份收藏里。','star'));
  completed.append(shelf);
  const recent=panel('最近解锁','本期新增的自动解锁记录。','trophy');recent.append(unlockList(data.recentUnlocks,actions));
  root.append(completed,recent);return root;
}
