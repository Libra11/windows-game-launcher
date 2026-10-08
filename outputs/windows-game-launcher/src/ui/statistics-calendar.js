import { el, button } from '../lib/dom.js';
import { monthDays, shiftMonth, duration } from '../lib/statistics.js';
import { panel, segment } from './statistics-components.js';

export function activityCalendar(state,controller,onDay) {
  const node=panel('活动日历','把每一天的热爱，留在日历里。','clock');
  node.append(segment([['seconds','游玩时长'],['unlocks','新增解锁']],state.metric,value=>controller.change('metric',value),'calendar-metric'));
  const header=el('div','stats-calendar-header');
  const previous=button('上个月','stats-small-button',()=>controller.change('month',shiftMonth(state.month,-1)),'back');
  const next=button('下个月','stats-small-button',()=>controller.change('month',shiftMonth(state.month,1)),'arrow');
  previous.disabled=state.month<=state.data.from.slice(0,7);next.disabled=state.month>=state.data.to.slice(0,7);
  previous.dataset.focusKey='stats-month-prev';next.dataset.focusKey='stats-month-next';
  header.append(previous,el('strong','',state.month.replace('-',' 年 ')+' 月'),next);node.append(header);
  const grid=el('div','stats-calendar-grid');grid.setAttribute('aria-label','活动日历');
  ['一','二','三','四','五','六','日'].forEach(day=>grid.append(el('span','stats-weekday',day)));
  const daysByDate=new Map(state.data.daily.map(day=>[day.date,day]));
  const {offset,dates}=monthDays(state.month);
  for(let i=0;i<offset;i++)grid.append(el('span'));
  const max=Math.max(1,...state.data.daily.map(day=>day[state.metric]));
  for(const date of dates) {
    const day=daysByDate.get(date);const value=day?.[state.metric]||0;
    const control=button(String(Number(date.slice(-2))),'stats-calendar-day',()=>onDay(date));
    control.disabled=!day?.recorded;control.dataset.focusKey='stats-day-'+date;
    const level=day?.recorded ? value>0 ? Math.max(1,Math.ceil(value/max*4)) : 0 : 'unknown';
    control.dataset.level=String(level);control.classList.toggle('today',date===state.data.to);
    const label=day?.recorded ? state.metric==='seconds' ? duration(value) : value+' 项新增解锁' : day?'未记录':'不在所选周期';
    control.title=date+' · '+label;control.setAttribute('aria-label',control.title);
    grid.append(control);
  }
  node.append(grid);
  const legend=el('div','stats-heat-legend');legend.append(el('span','','较少'));
  for(let i=0;i<5;i++){const swatch=el('i');swatch.dataset.level=i;legend.append(swatch);}
  legend.append(el('span','','较多'),el('span','stats-unrecorded-key','斜纹 · 未记录'));node.append(legend);return node;
}
