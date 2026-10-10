import { el, button, icon } from '../lib/dom.js';
import { duration } from '../lib/statistics.js';

let chartSequence=0;

export function trendChart(days, metric, onDay, state) {
  const root=el('div','stats-trend');
  if(!days.length)return root;
  // 折叠记录开始之前的日期，避免用大片空图挤压实际活动；明确保留缺失区间说明。
  const firstRecorded=days.findIndex(day=>day.recorded);
  if(firstRecorded>0){
    root.append(el('p','stats-chart-history-note',days[firstRecorded].date.replaceAll('-','.')+' 起有逐日记录 · 更早日期未记录'));
    days=days.slice(firstRecorded);
  }
  const total=days.reduce((sum,day)=>sum+(day.recorded?day[metric]:0),0);
  const summary=el('div','stats-trend-total');
  if(metric==='unlocks'){summary.append(el('strong','',total+' 项'),el('span','','本期新增解锁'));root.append(summary);}
  const max=Math.max(metric==='seconds'?3600:3,...days.map(day=>day[metric]));
  const plot=el('div','stats-trend-plot'),scale=el('div','stats-chart-scale'),dots=[];
  const svg=document.createElementNS('http://www.w3.org/2000/svg','svg');
  svg.setAttribute('viewBox','0 0 640 190');svg.setAttribute('preserveAspectRatio','none');svg.setAttribute('role','img');
  svg.setAttribute('aria-label',metric==='seconds'?'每日游玩时长趋势，使用下方日期按钮查看详细数值':'每日新增解锁趋势，使用下方日期按钮查看详细数值');
  const create=(tag,attrs,parent=svg)=>{const node=document.createElementNS(svg.namespaceURI,tag);for(const [key,value] of Object.entries(attrs))node.setAttribute(key,String(value));parent.append(node);return node;};
  // 同页可能保留普通视图与大屏视图，渐变标识不能互相引用。
  const fillId='stats-trend-fill-'+chartSequence++,defs=create('defs',{});
  const gradient=create('linearGradient',{id:fillId,x1:0,x2:0,y1:0,y2:1},defs);
  create('stop',{offset:'0%','stop-color':'var(--stats-accent)','stop-opacity':.2},gradient);
  create('stop',{offset:'100%','stop-color':'var(--stats-accent)','stop-opacity':0},gradient);
  for(let i=0;i<4;i++){
    const y=20+i*46;
    create('line',{x1:54,x2:624,y1:y,y2:y,class:'stats-chart-grid'});
    const label=el('span');label.style.top=y/190*100+'%';scale.append(label);
    label.textContent=metric==='seconds'?Math.round(max*(1-i/3)/360)/10+'h':String(Math.round(max*(1-i/3)));
  }
  let points=[];
  const draw=()=>{
    if(!points.length)return;
    const path=points.map(([x,y],i)=>(i?'L':'M')+x+','+y).join(' ');
    create('path',{d:path+' L'+points.at(-1)[0]+',158 L'+points[0][0]+',158 Z',class:'stats-chart-area',fill:'url(#'+fillId+')'});
    create('path',{d:path,class:'stats-chart-line'});
    if(days.length<15||points.length===1)points.forEach(([x,y])=>dots.push([x,y]));
    points=[];
  };
  const position=index=>[54+(days.length===1?285:index/(days.length-1)*570),158-days[index][metric]/max*138];
  days.forEach((day,index)=>{if(day.recorded)points.push(position(index));else draw();});draw();
  const cursor=create('line',{y1:20,y2:158,class:'stats-chart-cursor'});
  const selected=el('span','stats-chart-selected');
  plot.append(svg,scale);
  for(const [x,y] of dots){const dot=el('span','stats-chart-dot');dot.style.left=x/640*100+'%';dot.style.top=y/190*100+'%';plot.append(dot);}
  plot.append(selected);
  if(!days.some(day=>day.recorded&&day[metric]>0)){
    const empty=el('div','stats-chart-empty');
    empty.append(el('span','',days.some(day=>day.recorded)?metric==='seconds'?'本期还没有游玩记录':'本期还没有新增解锁':'所选日期尚未开始记录'));plot.append(empty);
  }
  root.append(plot);
  const axis=el('div','stats-chart-axis'),ticks=Math.min(5,days.length);
  for(let tick=0;tick<ticks;tick++){
    const dayIndex=ticks===1?0:Math.round(tick/(ticks-1)*(days.length-1));
    const label=el('span','',days[dayIndex].date.slice(5).replace('-','/'));
    label.style.left=position(dayIndex)[0]/640*100+'%';axis.append(label);
  }
  root.append(axis);
  const controls=el('div','stats-chart-inspector');
  let index=days.findIndex(day=>day.date===state.inspectDate);if(index<0)index=days.length-1;
  const label=el('span'),open=button('查看当天','stats-text-button',()=>onDay(days[index].date),'arrow');
  function step(text,glyph,delta){
    const control=el('button','stats-small-button');control.type='button';control.setAttribute('aria-label',text);
    control.append(icon(glyph),el('span','',text));control.onclick=()=>{index=Math.min(days.length-1,Math.max(0,index+delta));update();};return control;
  }
  const previous=step('前一天','back',-1),next=step('后一天','arrow',1);
  for(const [node,key] of [[previous,'prev'],[next,'next'],[open,'open']])node.dataset.focusKey='trend-'+metric+'-'+key;
  function update(){
    const day=days[index],[x,y]=position(index);label.textContent=day.date+' · '+(day.recorded?metric==='seconds'?duration(day.seconds):day.unlocks+' 项解锁':'未记录');
    cursor.setAttribute('x1',x);cursor.setAttribute('x2',x);selected.style.left=x/640*100+'%';selected.style.top=y/190*100+'%';selected.hidden=!day.recorded;
    state.inspectDate=day.date;previous.disabled=index===0;next.disabled=index===days.length-1;open.disabled=!day.recorded;
  }
  svg.onpointerdown=svg.onpointermove=event=>{const rect=svg.getBoundingClientRect();index=Math.min(days.length-1,Math.max(0,Math.round(((event.clientX-rect.left)*640/rect.width-54)/570*(days.length-1))));update();};
  svg.onclick=()=>{if(days[index].recorded)onDay(days[index].date);};
  controls.append(previous,label,next,open);root.append(controls);update();return root;
}

export function platformChart(platforms,total) {
  const root=el('div','stats-platform-chart'),bar=el('div','stats-platform-bar');
  bar.setAttribute('role','img');bar.setAttribute('aria-label','游戏库平台分布，共 '+total+' 款游戏');
  const colors={steam:'var(--stats-steam)',epic:'var(--stats-epic)',local:'var(--stats-local)'};
  const legend=el('div','stats-platform-legend');
  for(const entry of platforms){
    const part=el('i');part.style.background=colors[entry.source];part.style.flex=entry.games+' 1 0';part.hidden=!entry.games;bar.append(part);
    const row=el('div'),dot=el('i');dot.style.background=colors[entry.source];
    row.append(dot,el('span','',({steam:'Steam',epic:'Epic',local:'本地'})[entry.source]),el('strong','',entry.games));legend.append(row);
  }
  root.append(bar,legend);return root;
}
