import { el, button, icon, artwork, sourceName } from '../lib/dom.js';
import { duration, percent, localDate } from '../lib/statistics.js';
import { modal } from './modal.js';

export function segment(options, value, change, key) {
  const group=el('div','stats-segment');group.dataset.kind=key;group.setAttribute('role','group');group.setAttribute('aria-label',key==='period'?'统计周期':key==='source'?'统计平台':'显示方式');
  for(const [id,label] of options) {
    const control=button(label,id===value?'active':'',()=>change(id));
    control.setAttribute('aria-pressed',String(id===value));control.dataset.focusKey='stats-'+key+'-'+id;group.append(control);
  }
  return group;
}
export function panel(title, subtitle, glyph) {
  const node=el('section','stats-panel');
  const heading=el('div','stats-panel-heading');const copy=el('div');
  copy.append(el('h2','',title),el('p','',subtitle));heading.append(copy);
  if(glyph){const mark=el('span','stats-panel-mark');mark.append(icon(glyph));heading.prepend(mark);}node.append(heading);return node;
}
export function emptyState(title, description, glyph) {
  const node=el('div','stats-empty-state');node.append(icon(glyph),el('strong','',title),el('p','',description));return node;
}
export function gameCover(game, actions, variant='') {
  const cover=el('span','stats-game-cover');
  artwork(cover,actions.game(game.gameId)||{id:game.gameId,title:game.title,metadataJson:game.metadataJson});
  cover.dataset.artworkKey=JSON.stringify([variant,cover.dataset.artworkKey]);return cover;
}
export function gameRow(game, actions, suffix, index) {
  const row=button('','stats-game-row'+(index===0?' leading':''),()=>actions.selectStatistic(game.gameId));
  row.dataset.focusKey='stats-game-'+suffix+'-'+game.gameId;
  row.setAttribute('aria-label','查看 '+game.title);
  row.append(el('span','stats-rank-index',String(index+1).padStart(2,'0')),gameCover(game,actions,suffix));
  const copy=el('span','stats-game-copy');copy.append(el('strong','',game.title),el('small','',sourceName(game)));row.append(copy);return row;
}
export function completionLabel(game) {
  return ({partial:'定义未完整',none:'暂无成就',unknown:'尚未获取'})[game.definitionState] || percent(game.completionRate);
}
export function unlockList(events, actions) {
  const list=el('div','stats-unlocks');
  if(!events.length)list.append(emptyState('新的突破，总会到来','本期还没有新的自动解锁，已有成就仍保留在累计进度中。','trophy'));
  for(const item of events) {
    const row=button('','stats-unlock-row',()=>actions.selectStatistic(item.gameId));
    row.dataset.focusKey='stats-unlock-'+item.gameId+'-'+item.apiName;
    row.setAttribute('aria-label',item.name+'，'+item.gameTitle);
    const image=el('span','stats-unlock-icon');image.append(icon('trophy'));
    if(item.icon){const img=el('img');img.src=item.icon;img.alt='';img.loading='lazy';img.onerror=()=>img.remove();image.append(img);}
    const text=el('span');text.append(el('strong','',item.name),el('small','',item.gameTitle));
    row.append(image,text,el('time','',localDate(item.recordedAt)));list.append(row);
  }
  return list;
}
export function sessionList(page, actions, onPage, offset=0) {
  const root=el('div','stats-sessions');
  if(!page?.items.length)root.append(emptyState('留下一段属于你的游戏时光','所选范围还没有游玩会话，开始游戏后会自动记录。','game'));
  else {
    const wrap=el('div','stats-table-wrap');const table=el('table','stats-table');
    const head=el('thead');const titles=el('tr');
    for(const text of ['游戏','开始','结束','时长','状态'])titles.append(el('th','',text));head.append(titles);table.append(head);
    const body=el('tbody');
    for(const item of page.items){
      const row=el('tr');const game=el('td');
      const link=button(item.gameTitle,'stats-session-game',()=>actions.selectStatistic(item.gameId));
      link.dataset.focusKey='stats-session-'+item.id;
      game.append(link,el('small','',sourceName(item)));
      const state=el('td');state.append(el('span',item.endedAt?'stats-record-badge':'stats-record-badge running',item.endedAt?'已结束':'运行中'));
      if(!item.dailyRecorded)state.append(el('small','','无逐日记录'));
      row.append(game,el('td','',localDate(item.startedAt)),el('td','',localDate(item.endedAt)),el('td','',duration(item.seconds)),state);body.append(row);
    }table.append(body);wrap.append(table);root.append(wrap);
  }
  const footer=el('div','stats-pagination');
  const previous=button('上一页','stats-small-button',()=>onPage(Math.max(0,offset-20)),'back');
  const next=button('下一页','stats-small-button',()=>onPage(offset+20),'arrow');
  previous.disabled=offset===0;next.disabled=offset+20>=(page?.total||0);
  previous.dataset.focusKey='stats-page-prev';next.dataset.focusKey='stats-page-next';
  footer.append(el('span','', '共 '+(page?.total||0)+' 条 · 第 '+(Math.floor(offset/20)+1)+' 页'),previous,next);root.append(footer);return root;
}
export function showStatisticsDay(controller,date,actions) {
  const content=el('div','stats-day-detail');content.append(el('p','stats-empty','正在读取当天记录…'));
  const dialog=modal(date+' · 活动详情','时长按本机逐日记录统计；会话列表保留整次游玩的累计时长。',content);
  dialog.classList.add('stats-day-modal');
  async function load(){
    try {
      const {data,sessions,query}=await controller.day(date);
      if(!dialog.open)return;
      const day=data.daily.find(item=>item.date===date);
      const summary=el('div','stats-day-summary');summary.append(el('strong','',duration(day?.seconds||0)),el('span','',(day?.unlocks||0)+' 项新增解锁'));
      const records=el('div');
      function show(page,offset) {records.replaceChildren(sessionList(page,actions,async next=>show(await controller.dayPage(query,next),next),offset));}
      show(sessions,0);content.replaceChildren(summary,el('h3','','游玩会话'),records,el('h3','','新增解锁'),unlockList(data.dayUnlocks,actions));
    }catch(error){content.replaceChildren(el('p','stats-empty','读取失败：'+String(error)),button('重试','secondary',load));}
  }
  load();
}
