import { el, button, icon } from '../lib/dom.js';
import { statisticsPeriods, statisticsSources, localDate } from '../lib/statistics.js';
import { segment, panel, sessionList, showStatisticsDay } from './statistics-components.js';
import { statisticsHeader, statisticsOverview, periodOverview } from './statistics-overview.js';
import { statisticsRankings } from './statistics-rankings.js';
import { statisticsCollection } from './statistics-collection.js';
import { trendChart } from './statistics-charts.js';
import { activityCalendar } from './statistics-calendar.js';
import './statistics.css';

export function statisticsView(controller, actions) {
  const state=controller.state, data=state.data;
  const root=el('div','statistics-view');
  root.append(statisticsHeader(data));
  const controls=el('div','stats-filter-bar');
  controls.append(segment(statisticsSources,state.query.source,value=>controller.filter('source',value),'source'),segment(statisticsPeriods,state.query.period,value=>controller.filter('period',value),'period'));
  root.append(controls);
  if(state.error){
    const error=el('div','stats-error');error.setAttribute('role','status');
    error.append(el('span','','统计暂未更新：'+state.error),button('重试','stats-small-button',()=>controller.refresh(true),'refresh'));root.append(error);
  }
  if(!data){
    const skeleton=el('div','stats-skeleton');skeleton.setAttribute('aria-label',state.error?'统计尚未读取':'正在加载统计');
    for(let i=0;i<3;i++)skeleton.append(el('div'));root.append(skeleton);return root;
  }
  root.append(statisticsOverview(data.summary,data.platforms),periodOverview(data.period));
  const onDay=date=>showStatisticsDay(controller,date,actions);
  const activity=el('div','stats-activity-grid');
  const trend=panel('游玩趋势','游玩与解锁，汇成自己的节奏。','chart');
  trend.classList.add('stats-trend-panel');
  trend.querySelector('.stats-panel-heading').append(segment([['seconds','游玩时长'],['unlocks','新增解锁']],state.metric,value=>controller.change('metric',value),'trend-metric'));
  trend.append(trendChart(data.daily,state.metric,onDay,state));
  activity.append(trend,activityCalendar(state,controller,onDay));root.append(activity);
  const note=el('p','stats-data-note');
  note.append(icon('info'),el('span','','逐日记录自 '+localDate(data.startedAt)+' 开始；此前日期未记录，历史累计时长未补算为日趋势。'));
  root.append(note,statisticsRankings(controller,actions),statisticsCollection(data,actions));
  const records=panel('游玩记录','每一次开始与结束，都有迹可循。','list');
  records.append(sessionList(state.sessions,actions,controller.page,state.offset));root.append(records);
  root.append(el('p','stats-updated','最近更新 '+localDate(data.generatedAt)+' · 数据保存在本机'));
  return root;
}
