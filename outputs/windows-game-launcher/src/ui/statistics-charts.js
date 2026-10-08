import { el, button, icon } from '../lib/dom.js';
import { duration } from '../lib/statistics.js';

export function trendChart(days, metric, onDay, state) {
  const root = el('div','stats-trend');
  if (!days.length) return root;
  const max = Math.max(1,...days.map(day=>day[metric]));
  const svg = document.createElementNS('http://www.w3.org/2000/svg','svg');
  svg.setAttribute('viewBox','0 0 640 190'); svg.setAttribute('role','img');
  svg.setAttribute('aria-label',metric === 'seconds' ? '每日游玩时长趋势，使用下方日期按钮查看详细数值' : '每日新增解锁趋势，使用下方日期按钮查看详细数值');
  const create = (tag,attrs) => {const node=document.createElementNS(svg.namespaceURI,tag); for(const [key,value] of Object.entries(attrs)) node.setAttribute(key,String(value)); svg.append(node); return node;};
  for (let i=0;i<4;i++) {
    const y=20+i*46;
    create('line',{x1:54,x2:624,y1:y,y2:y,class:'stats-chart-grid'});
    const label=create('text',{x:0,y:y+4,class:'stats-chart-label'});
    label.textContent=metric==='seconds' ? Math.round(max*(1-i/3)/360)/10+'h' : String(Math.round(max*(1-i/3)));
  }
  let points=[];
  const draw=()=>{
    if (!points.length) return;
    const path=points.map(([x,y],i)=>(i?'L':'M')+x+','+y).join(' ');
    create('path',{d:path+' L'+points.at(-1)[0]+',158 L'+points[0][0]+',158 Z',class:'stats-chart-area'});
    create('path',{d:path,class:'stats-chart-line'});
    points.forEach(([x,y])=>create('circle',{cx:x,cy:y,r:days.length<35?3:1.5,class:'stats-chart-dot'}));
    points=[];
  };
  days.forEach((day,i)=>{if(day.recorded) points.push([54+(days.length===1?285:i/(days.length-1)*570),158-day[metric]/max*138]); else draw();}); draw();
  root.append(svg);
  const axis=el('div','stats-chart-axis'); axis.append(el('span','',days[0].date),el('span','','新版启用后的记录'),el('span','',days.at(-1).date)); root.append(axis);
  const controls=el('div','stats-chart-inspector');
  let index=days.findIndex(day=>day.date===state.inspectDate);
  if(index<0)index=days.length-1;
  const label=el('span');
  const open=button('查看当天','stats-text-button',()=>onDay(days[index].date),'arrow');
  function step(label,glyph,delta) {
    const control=el('button','stats-small-button');control.type='button';control.setAttribute('aria-label',label);
    control.append(icon(glyph),el('span','',label));
    control.onclick=()=>{index=Math.min(days.length-1,Math.max(0,index+delta));update();};
    return control;
  }
  const previous=step('前一天','back',-1);
  const next=step('后一天','arrow',1);
  for(const [node,key] of [[previous,'prev'],[next,'next'],[open,'open']]) node.dataset.focusKey='trend-'+metric+'-'+key;
  function update() {
    const day=days[index]; label.textContent=day.date+' · '+(day.recorded ? metric==='seconds' ? duration(day.seconds) : day.unlocks+' 项解锁' : '未记录');
    state.inspectDate=day.date;
    previous.disabled=index===0; next.disabled=index===days.length-1; open.disabled=!day.recorded;
  }
  svg.onpointerdown=svg.onpointermove=event=>{const rect=svg.getBoundingClientRect();index=Math.min(days.length-1,Math.max(0,Math.round(((event.clientX-rect.left)*640/rect.width-54)/570*(days.length-1))));update();};
  svg.onclick=()=>{if(days[index].recorded)onDay(days[index].date);};
  controls.append(previous,label,next,open); root.append(controls); update(); return root;
}

export function platformChart(platforms,total) {
  const wrap=el('div','stats-platform-chart');
  const ring=el('div','stats-donut'); ring.setAttribute('role','img'); ring.setAttribute('aria-label','游戏库平台分布，共 '+total+' 款游戏');
  let cursor=0; const colors=['var(--stats-steam)','var(--stats-epic)','var(--stats-local)'];
  const segments=platforms.map((entry,index)=>{const start=cursor;cursor+=total?entry.games/total*100:0;return colors[index]+' '+start+'% '+cursor+'%';});
  ring.style.background=total?'conic-gradient('+segments.join(',')+')':'var(--track)';
  const center=el('div','stats-donut-center');center.append(el('strong','',total),el('span','','款游戏'));ring.append(center);
  const legend=el('div','stats-platform-legend');
  platforms.forEach((entry,index)=>{const row=el('div');const dot=el('i');dot.style.background=colors[index];row.append(dot,el('span','',({steam:'Steam',epic:'Epic',local:'本地游戏'})[entry.source]),el('strong','',entry.games+' 款'));legend.append(row);});
  wrap.append(ring,legend); return wrap;
}
