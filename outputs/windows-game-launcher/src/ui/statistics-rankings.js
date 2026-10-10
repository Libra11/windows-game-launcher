import { el, button } from '../lib/dom.js';
import { duration, localDate, rankedGames } from '../lib/statistics.js';
import { segment, panel, gameRow, gameCover, completionLabel, emptyState } from './statistics-components.js';

export function statisticsRankings(controller, actions) {
  const state=controller.state, data=state.data;
  const root=el('div','stats-rankings-grid');
  const time=panel('常玩游戏','按游玩时长排序','clock');
  time.append(segment([['period','本期本机'],['local','本机累计'],['steam','Steam 累计']],state.rankBasis,value=>controller.change('rankBasis',value),'rank'));
  time.classList.add('stats-favorite-panel');
  const ranked=rankedGames(data.games,state.rankBasis), timeList=el('div','stats-favorite-shelf');
  root.classList.toggle('stats-rankings-compact',ranked.length>0&&ranked.length<=4);
  ranked.slice(0,state.rankExpanded?ranked.length:6).forEach((game,index)=>{
    const card=button('','stats-favorite-game',()=>actions.selectStatistic(game.gameId));
    card.dataset.focusKey='stats-game-time-'+game.gameId;
    card.setAttribute('aria-label','查看 '+game.title+'，'+duration(game.rankSeconds));
    const cover=gameCover(game,actions,'time');
    cover.append(el('span','stats-favorite-rank',String(index+1).padStart(2,'0')));
    card.append(cover,el('strong','',game.title),el('span','stats-favorite-time',duration(game.rankSeconds)));
    timeList.append(card);
  });
  if(!ranked.length)timeList.append(state.rankBasis==='steam'
    ?emptyState('还没有可用的 Steam 时长','同步 Steam 游戏资料后，官方累计时长会显示在这里。','steam')
    :emptyState('还没有这类游玩记录','从启动器开始一场游戏，投入的时光会记录在这里。','clock'));
  time.append(timeList);
  if(ranked.length>6)time.append(button(state.rankExpanded?'收起排行':'查看全部 '+ranked.length+' 款','stats-text-button',()=>controller.change('rankExpanded',!state.rankExpanded),'arrow'));
  if(state.rankBasis==='steam')time.append(el('p','stats-panel-note','官方累计更新于 '+(data.summary.steamCheckedAt?localDate(data.summary.steamCheckedAt):'尚未同步')+' · 未获取时长的游戏不参与排行'));

  const achievements=panel('成就进度','按完成度排序','trophy');
  const games=[...data.games].sort((a,b)=>(b.completionRate??-1)-(a.completionRate??-1)||b.unlocked-a.unlocked||a.title.localeCompare(b.title,'zh-CN'));
  const list=el('div','stats-ranking stats-achievement-ranking');
  games.slice(0,state.achievementExpanded?games.length:6).forEach((game,index)=>{
    const row=gameRow(game,actions,'achievement',index),value=el('span','stats-rank-value',completionLabel(game));
    value.append(el('small','',game.unlocked+' / '+game.totalAchievements+' 项'+(game.manualUnlocked?' · 含手动':'')));
    if(game.completionRate!=null){const track=el('span','stats-rank-track'),fill=el('i');fill.style.width=game.completionRate+'%';track.append(fill);value.append(track);}
    row.append(value);list.append(row);
  });
  if(!games.length)list.append(emptyState('下一段冒险，等你开启','添加游戏后，这里会展现你的成就进度。','trophy'));
  achievements.append(list);
  if(games.length>6)achievements.append(button(state.achievementExpanded?'收起排行':'查看全部 '+games.length+' 款','stats-text-button',()=>controller.change('achievementExpanded',!state.achievementExpanded),'arrow'));
  root.append(time,achievements);return root;
}
