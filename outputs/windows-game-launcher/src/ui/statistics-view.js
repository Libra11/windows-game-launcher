import { el, button, icon } from '../lib/dom.js';
import { statisticsPeriods, statisticsSources, duration, percent, localDate, rankedGames } from '../lib/statistics.js';
import { segment, panel, metricCard, gameRow, gameCover, completionLabel, unlockList, sessionList, showStatisticsDay } from './statistics-components.js';
import { trendChart, platformChart } from './statistics-charts.js';
import { activityCalendar } from './statistics-calendar.js';
import './statistics.css';

export function statisticsView(controller,actions) {
  const state=controller.state, data=state.data;
  const root=el('div','statistics-view');
  const header=el('div','stats-page-header');const copy=el('div');
  copy.append(el('div','eyebrow','热爱发生的每一天'),el('h1','','游戏统计'),el('p','','每一段旅程，都值得回望。'));
  const decoration=el('div','stats-header-mark');decoration.append(icon('chart'));header.append(copy,decoration);root.append(header);
  const controls=el('div','stats-filter-bar');
  controls.append(segment(statisticsSources,state.query.source,value=>controller.filter('source',value),'source'),segment(statisticsPeriods,state.query.period,value=>controller.filter('period',value),'period'));
  root.append(controls);
  if(state.error){
    const error=el('div','stats-error');error.setAttribute('role','status');
    error.append(el('span','', '统计暂未更新：'+state.error),button('重试','stats-small-button',()=>controller.refresh(true),'refresh'));root.append(error);
  }
  if(!data){
    const skeleton=el('div','stats-skeleton');skeleton.setAttribute('aria-label',state.error?'统计尚未读取':'正在加载统计');
    for(let i=0;i<6;i++)skeleton.append(el('div'));root.append(skeleton);return root;
  }
  const summary=data.summary;
  const overview=el('div','stats-overview');
  const hours=value=>value==null?'未获取':value<3600?duration(value):(Math.round(value/360)/10).toLocaleString('zh-CN')+' 小时';
  const steamHint=summary.steamGames ? '已获取 '+summary.steamKnown+'/'+summary.steamGames+' 款'+(summary.steamCached?' · 缓存':'') : '当前分类没有 Steam 游戏';
  [
    ['游戏收藏',summary.gameCount.toLocaleString('zh-CN'),'属于你的游戏库','library'],
    ['Steam 累计时长',hours(summary.steamSeconds),steamHint,'steam'],
    ['本机累计时长',hours(summary.localSeconds),'含历史记录 · 与 Steam 分开展示','clock'],
    ['已解锁成就',summary.unlocked.toLocaleString('zh-CN'),'其中 '+summary.manualUnlocked+' 项手动记录','trophy'],
    ['全成就游戏',summary.completedGames+' 款','仅统计完整且非空的成就定义','star'],
    ['整体成就完成度',percent(summary.completionRate),'按完整定义中的成就数量计算','chart'],
  ].forEach(args=>overview.append(metricCard(...args)));root.append(overview);
  const period=el('div','stats-period-strip');
  for(const [value,label] of [[duration(data.period.seconds),'本期本机游玩'],[data.period.activeDays+' 天','活跃天数'],[data.period.sessions+' 次','游玩会话'],[duration(data.period.averageSeconds),'平均单次'],[data.period.newUnlocks+' 项','新增解锁']]){
    const item=el('div');item.append(el('strong','',value),el('span','',label));period.append(item);
  }root.append(period);
  const note=el('p','stats-data-note');note.append(icon('info'),el('span','','逐日记录自 '+localDate(data.startedAt)+' 开始；此前日期未记录。Steam 累计与历史本机时长保留，未补算日趋势。'));root.append(note);
  const onDay=date=>showStatisticsDay(controller,date,actions);
  const activity=el('div','stats-activity-grid');
  const trend=panel('时光轨迹','在游玩与解锁之间，看到自己的节奏。','chart');
  trend.append(segment([['seconds','游玩时长'],['unlocks','新增解锁']],state.metric,value=>controller.change('metric',value),'trend-metric'),trendChart(data.daily,state.metric,onDay,state));
  activity.append(trend,activityCalendar(state,controller,onDay));root.append(activity);
  const rankings=el('div','stats-two-columns');
  const time=panel('最投入的旅程','打开游戏详情，继续下一段故事。','clock');
  time.append(segment([['period','本期本机'],['local','本机累计'],['steam','Steam 累计']],state.rankBasis,value=>controller.change('rankBasis',value),'rank'));
  const ranked=rankedGames(data.games,state.rankBasis);
  const timeList=el('div','stats-ranking');
  ranked.slice(0,state.rankExpanded?ranked.length:6).forEach((game,index)=>{
    const row=gameRow(game,actions,'time',index);
    const value=el('span','stats-rank-value',duration(game.rankSeconds));
    const track=el('span','stats-rank-track');const fill=el('i');fill.style.width=game.rankSeconds/ranked[0].rankSeconds*100+'%';track.append(fill);value.append(track);row.append(value);timeList.append(row);
  });
  if(!ranked.length)timeList.append(el('p','stats-empty','当前口径暂时没有可排行的游玩时长。'));
  time.append(timeList);
  if(ranked.length>6)time.append(button(state.rankExpanded?'收起排行':'展开全部 '+ranked.length+' 款','stats-text-button',()=>controller.change('rankExpanded',!state.rankExpanded),'arrow'));
  if(state.rankBasis==='steam')time.append(el('p','stats-panel-note','官方累计更新时间：'+(summary.steamCheckedAt?localDate(summary.steamCheckedAt):'尚未同步')+'。未获取时长的游戏不参与排行。'));
  const achievements=panel('成就旅程','每个里程碑，都有属于你的进度。','trophy');
  const achievementGames=[...data.games].sort((a,b)=>(b.completionRate??-1)-(a.completionRate??-1)||b.unlocked-a.unlocked||a.title.localeCompare(b.title,'zh-CN'));
  const achievementList=el('div','stats-ranking');
  achievementGames.slice(0,state.achievementExpanded?achievementGames.length:6).forEach((game,index)=>{
    const row=gameRow(game,actions,'achievement',index);
    const value=el('span','stats-rank-value',completionLabel(game));
    value.append(el('small','',game.unlocked+'/'+game.totalAchievements+' 项'+(game.manualUnlocked?' · 含手动':'')));
    if(game.completionRate!=null){const track=el('span','stats-rank-track');const fill=el('i');fill.style.width=game.completionRate+'%';track.append(fill);value.append(track);}
    row.append(value);achievementList.append(row);
  });
  if(!achievementGames.length)achievementList.append(el('p','stats-empty','添加游戏后，这里会显示你的成就进度。'));
  achievements.append(achievementList);
  if(achievementGames.length>6)achievements.append(button(state.achievementExpanded?'收起排行':'展开全部 '+achievementGames.length+' 款','stats-text-button',()=>controller.change('achievementExpanded',!state.achievementExpanded),'arrow'));
  rankings.append(time,achievements);root.append(rankings);
  const secondary=el('div','stats-two-columns');
  const platform=panel('游戏库组成','按游戏的导入来源统计，本地游戏可关联 Xbox 成就。','library');
  platform.append(platformChart(data.platforms,summary.gameCount));
  const recent=panel('最近的里程碑','本期最近 20 项新的自动解锁，不包含历史导入。','trophy');recent.append(unlockList(data.recentUnlocks,actions));
  secondary.append(platform,recent);root.append(secondary);
  const completed=panel('完整的冒险','所有成就，都已留下足迹。','star');
  const shelf=el('div','stats-completed-shelf');
  const finished=data.games.filter(game=>game.completionRate===100);
  finished.forEach(game=>{
    const card=button('','stats-completed-game',()=>actions.selectStatistic(game.gameId));
    card.dataset.focusKey='stats-completed-'+game.gameId;card.setAttribute('aria-label',game.title+'，全成就');
    card.append(gameCover(game,actions,'completed'),el('strong','',game.title),el('span','',game.totalAchievements+' 项成就'+(game.manualUnlocked?' · 含手动记录':'')));shelf.append(card);
  });
  if(!finished.length)shelf.append(el('p','stats-empty','还没有全成就游戏。每一步进展都值得记录。'));
  completed.append(shelf);root.append(completed);
  const records=panel('游玩记录','每次确认游戏运行后开始记录，历史会话继续保留。','list');
  records.append(sessionList(state.sessions,actions,controller.page,state.offset));root.append(records);
  root.append(el('p','stats-updated','最近更新 '+localDate(data.generatedAt)+' · 数据保存在本机'));
  return root;
}
